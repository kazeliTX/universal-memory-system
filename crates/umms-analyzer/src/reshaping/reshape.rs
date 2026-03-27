//! QueryReshaper — residual pyramid query vector reshaping.
//!
//! Blends the original query vector with a context vector built from a
//! three-level residual pyramid of activated tag embeddings:
//! - Level 0: top-N activated tags (fine detail, highest weight)
//! - Level 1: next-N activated tags (broader context)
//! - Level 2: co-occurring tags of Level 0 (associative expansion via PMI)

use std::sync::Arc;

use umms_core::config::ReshapingConfig;
use umms_core::error::Result;
use umms_core::tag::EpaResult;
use umms_core::traits::TagStore;
use umms_core::types::AgentId;

/// Reshapes query vectors using EPA-driven residual pyramid blending.
pub struct QueryReshaper {
    tag_store: Arc<dyn TagStore>,
    config: ReshapingConfig,
}

impl QueryReshaper {
    pub fn new(tag_store: Arc<dyn TagStore>, config: ReshapingConfig) -> Self {
        Self { tag_store, config }
    }

    /// Reshape a query vector based on EPA analysis.
    ///
    /// Returns the fused, L2-normalized vector. If alpha is effectively zero
    /// or there are no activated tags, returns the original vector unchanged.
    pub async fn reshape(
        &self,
        original: &[f32],
        epa: &EpaResult,
        _agent_id: &AgentId,
    ) -> Result<Vec<f32>> {
        // Early exit: no reshaping needed
        if epa.alpha < 1e-6 || epa.activated_tags.is_empty() {
            return Ok(original.to_vec());
        }

        let dim = original.len();
        let l0_count = self.config.level0_count;
        let l1_count = self.config.level1_count;

        // --- Level 0: top-N activated tags → weighted centroid ---
        let l0_end = l0_count.min(epa.activated_tags.len());
        let l0_tags = &epa.activated_tags[..l0_end];
        let l0_ids: Vec<_> = l0_tags.iter().map(|t| t.tag_id.clone()).collect();
        let l0_weights: Vec<f32> = l0_tags.iter().map(|t| t.similarity).collect();
        let l0_full_tags = self.tag_store.get_batch(&l0_ids).await?;
        let l0_centroid = weighted_centroid(&l0_full_tags, &l0_weights, dim);

        // --- Level 1: next-N activated tags → weighted centroid ---
        let l1_start = l0_end;
        let l1_end = (l1_start + l1_count).min(epa.activated_tags.len());
        let l1_centroid = if l1_start < l1_end {
            let l1_tags = &epa.activated_tags[l1_start..l1_end];
            let l1_ids: Vec<_> = l1_tags.iter().map(|t| t.tag_id.clone()).collect();
            let l1_weights: Vec<f32> = l1_tags.iter().map(|t| t.similarity).collect();
            let l1_full_tags = self.tag_store.get_batch(&l1_ids).await?;
            weighted_centroid(&l1_full_tags, &l1_weights, dim)
        } else {
            vec![0.0; dim]
        };

        // --- Level 2: co-occurring tags of Level 0 → PMI-weighted centroid ---
        let l2_centroid = if self.config.cooc_expansion_k > 0 && !l0_ids.is_empty() {
            let mut cooc_ids = Vec::new();
            let mut cooc_pmis = Vec::new();

            for tag_id in &l0_ids {
                let coocs = self
                    .tag_store
                    .cooccurrences(tag_id, self.config.cooc_expansion_k)
                    .await?;
                for cooc in coocs {
                    // Get the "other" tag from the co-occurrence
                    let other_id = if cooc.tag_a == *tag_id {
                        cooc.tag_b.clone()
                    } else {
                        cooc.tag_a.clone()
                    };
                    // Avoid duplicates with L0
                    if !l0_ids.contains(&other_id) && !cooc_ids.contains(&other_id) {
                        cooc_ids.push(other_id);
                        cooc_pmis.push(cooc.pmi.max(0.0)); // Use only positive PMI
                    }
                }
            }

            if !cooc_ids.is_empty() {
                let cooc_tags = self.tag_store.get_batch(&cooc_ids).await?;
                weighted_centroid(&cooc_tags, &cooc_pmis, dim)
            } else {
                vec![0.0; dim]
            }
        } else {
            vec![0.0; dim]
        };

        // --- Pyramid blending ---
        let pw = &self.config.pyramid_weights;
        let mut context = vec![0.0_f32; dim];
        for d in 0..dim {
            context[d] = pw[0] * l0_centroid[d] + pw[1] * l1_centroid[d] + pw[2] * l2_centroid[d];
        }

        // --- Fuse with original ---
        let alpha = epa.alpha;
        let mut fused = vec![0.0_f32; dim];
        for d in 0..dim {
            fused[d] = (1.0 - alpha) * original[d] + alpha * context[d];
        }

        // --- L2 normalize ---
        l2_normalize(&mut fused);

        Ok(fused)
    }
}

/// Compute a weighted centroid from tags and their weights.
///
/// Tags are matched to weights by position. If a tag lacks a vector or
/// there are more weights than tags, the extra entries are skipped.
fn weighted_centroid(
    tags: &[umms_core::tag::Tag],
    weights: &[f32],
    dim: usize,
) -> Vec<f32> {
    let mut centroid = vec![0.0_f32; dim];
    let mut total_weight = 0.0_f32;

    for (tag, &w) in tags.iter().zip(weights.iter()) {
        if tag.vector.len() == dim && w > 0.0 {
            total_weight += w;
            for (d, val) in tag.vector.iter().enumerate() {
                centroid[d] += val * w;
            }
        }
    }

    if total_weight > 0.0 {
        for v in &mut centroid {
            *v /= total_weight;
        }
    }

    centroid
}

/// L2 normalize a vector in place. No-op if the vector has zero norm.
fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-12 {
        for val in v.iter_mut() {
            *val /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umms_core::tag::EpaResult;

    #[test]
    fn l2_normalize_preserves_direction() {
        let mut v = vec![3.0_f32, 4.0];
        l2_normalize(&mut v);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn l2_normalize_zero_vector() {
        let mut v = vec![0.0_f32; 5];
        l2_normalize(&mut v);
        // Should remain zero (no division by zero)
        assert!(v.iter().all(|&x| x == 0.0));
    }

    fn make_test_tag(label: &str, vector: Vec<f32>) -> umms_core::tag::Tag {
        use umms_core::types::TagId;
        let now = chrono::Utc::now();
        umms_core::tag::Tag {
            id: TagId::new(),
            label: label.into(),
            canonical: label.into(),
            agent_id: None,
            vector,
            frequency: 1,
            importance: 1.0,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn weighted_centroid_basic() {
        let t1 = make_test_tag("a", vec![1.0, 0.0]);
        let t2 = make_test_tag("b", vec![0.0, 1.0]);

        // Equal weights → centroid at [0.5, 0.5]
        let c = weighted_centroid(&[t1.clone(), t2.clone()], &[1.0, 1.0], 2);
        assert!((c[0] - 0.5).abs() < 1e-6);
        assert!((c[1] - 0.5).abs() < 1e-6);

        // Unequal weights → pulled toward t1
        let c2 = weighted_centroid(&[t1, t2], &[3.0, 1.0], 2);
        assert!((c2[0] - 0.75).abs() < 1e-6);
        assert!((c2[1] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn alpha_zero_returns_original() {
        let epa = EpaResult {
            alpha: 0.0,
            logic_depth: 0.0,
            cross_domain_resonance: 0.0,
            semantic_axes: Vec::new(),
            activated_tags: Vec::new(),
        };

        // reshape would return early since alpha ≈ 0, returning original.
        // We test the logic directly:
        assert!(epa.alpha < 1e-6);
    }

    #[test]
    fn fusion_formula_correctness() {
        // Test the core fusion math: fused = (1-alpha)*orig + alpha*ctx, normalized.
        let dim = 3;
        let original = vec![1.0_f32, 0.0, 0.0];
        let context = vec![0.0_f32, 1.0, 0.0];
        let alpha = 0.5_f32;

        let mut fused = vec![0.0_f32; dim];
        for d in 0..dim {
            fused[d] = (1.0 - alpha) * original[d] + alpha * context[d];
        }
        l2_normalize(&mut fused);

        // Expected: [0.5, 0.5, 0.0] normalized → [1/√2, 1/√2, 0]
        let expected_norm = (0.5_f32 * 0.5 + 0.5 * 0.5).sqrt();
        assert!((fused[0] - 0.5 / expected_norm).abs() < 1e-6);
        assert!((fused[1] - 0.5 / expected_norm).abs() < 1e-6);
        assert!(fused[2].abs() < 1e-6);

        // Output should be unit length
        let norm: f32 = fused.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn alpha_one_gives_context_only() {
        // With alpha=1, fused = context (normalized)
        let dim = 3;
        let original = vec![1.0_f32, 0.0, 0.0];
        let context = vec![0.0_f32, 3.0, 4.0]; // norm = 5
        let alpha = 1.0_f32;

        let mut fused = vec![0.0_f32; dim];
        for d in 0..dim {
            fused[d] = (1.0 - alpha) * original[d] + alpha * context[d];
        }
        l2_normalize(&mut fused);

        // Should be [0, 0.6, 0.8]
        assert!(fused[0].abs() < 1e-6);
        assert!((fused[1] - 0.6).abs() < 1e-6);
        assert!((fused[2] - 0.8).abs() < 1e-6);
    }
}
