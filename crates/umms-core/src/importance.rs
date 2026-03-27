//! Multi-dimensional importance scoring (ADR-013).
//!
//! Computes an effective importance score from multiple signals at retrieval
//! time, replacing the old single-`f32` manual approach.
//!
//! Formula:
//! ```text
//! effective_importance =
//!     base_importance                      // LLM initial assessment (0-1)
//!     × recency_boost(last_accessed_at)    // exponential decay, recent = higher
//!     × frequency_boost(access_count)      // log scale, more accessed = higher
//!     × graph_centrality(in_degree)        // graph hub bonus
//!     + cross_agent_bonus                  // multi-agent reference bonus
//!     + user_feedback_adjustment           // explicit user rating
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Configuration for importance scoring weights.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ImportanceConfig {
    /// Importance halves every N days of no access.
    pub recency_half_life_days: f64,
    /// Logarithmic base for access-count frequency boost.
    pub frequency_log_base: f64,
    /// Weight of the frequency signal in the final score.
    pub frequency_weight: f32,
    /// Weight of the graph centrality signal.
    pub centrality_weight: f32,
    /// Bonus per additional agent that references this memory.
    pub cross_agent_bonus: f32,
    /// Maximum cumulative cross-agent bonus.
    pub cross_agent_max: f32,
    /// Weight of the user feedback signal.
    pub feedback_weight: f32,
}

impl Default for ImportanceConfig {
    fn default() -> Self {
        Self {
            recency_half_life_days: 7.0,
            frequency_log_base: 10.0,
            frequency_weight: 0.15,
            centrality_weight: 0.10,
            cross_agent_bonus: 0.05,
            cross_agent_max: 0.15,
            feedback_weight: 0.10,
        }
    }
}

/// Compute effective importance from multiple signals.
///
/// Returns a value clamped to `[0.0, 1.0]`.
///
/// # Arguments
///
/// * `base_importance` — LLM-assigned importance at ingestion time (0.0..=1.0).
/// * `access_count` — how many times this memory has been retrieved.
/// * `last_accessed_at` — timestamp of the most recent access (`None` = never).
/// * `graph_in_degree` — number of incoming edges in the knowledge graph.
/// * `cross_agent_count` — number of distinct agents that reference this memory.
/// * `user_rating` — explicit user feedback in `[-1.0, 1.0]`, or `None`.
/// * `config` — tuning knobs.
pub fn compute_effective_importance(
    base_importance: f32,
    access_count: u64,
    last_accessed_at: Option<DateTime<Utc>>,
    graph_in_degree: usize,
    cross_agent_count: usize,
    user_rating: Option<f32>,
    config: &ImportanceConfig,
) -> f32 {
    // Recency boost: exponential decay based on time since last access.
    let recency = if let Some(last) = last_accessed_at {
        let days_ago = (Utc::now() - last).num_seconds() as f64 / 86_400.0;
        let half_life = config.recency_half_life_days;
        (0.5_f64.powf(days_ago / half_life)) as f32
    } else {
        0.5 // never accessed -> neutral
    };

    // Frequency boost: logarithmic (diminishing returns).
    let frequency = if access_count > 0 {
        let log_val = (access_count as f64 + 1.0).log(config.frequency_log_base);
        (log_val as f32).min(1.0) * config.frequency_weight
    } else {
        0.0
    };

    // Graph centrality: bonus for being a hub node.
    let centrality = (graph_in_degree as f32 * 0.05).min(config.centrality_weight);

    // Cross-agent bonus: referenced by multiple agents.
    let cross_agent = if cross_agent_count > 1 {
        ((cross_agent_count - 1) as f32 * config.cross_agent_bonus)
            .min(config.cross_agent_max)
    } else {
        0.0
    };

    // User feedback adjustment.
    let feedback = user_rating.unwrap_or(0.0) * config.feedback_weight;

    // Combine: multiplicative core + additive bonuses.
    let raw = base_importance * recency + frequency + centrality + cross_agent + feedback;

    raw.clamp(0.0, 1.0)
}

/// Convenience wrapper that pulls signals directly from a [`MemoryEntry`].
///
/// `graph_in_degree` and `cross_agent_count` are not stored on the entry
/// itself, so they must be supplied externally (pass 0 if unavailable).
pub fn score_entry(
    entry: &crate::memory::MemoryEntry,
    graph_in_degree: usize,
    cross_agent_count: usize,
    config: &ImportanceConfig,
) -> f32 {
    compute_effective_importance(
        entry.importance,
        entry.access_count,
        Some(entry.accessed_at),
        graph_in_degree,
        cross_agent_count,
        entry.user_rating,
        config,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn cfg() -> ImportanceConfig {
        ImportanceConfig::default()
    }

    #[test]
    fn default_config_is_sane() {
        let c = cfg();
        assert!((c.recency_half_life_days - 7.0).abs() < f64::EPSILON);
        assert!((c.frequency_weight - 0.15).abs() < f32::EPSILON);
        assert!((c.centrality_weight - 0.10).abs() < f32::EPSILON);
        assert!((c.cross_agent_bonus - 0.05).abs() < f32::EPSILON);
        assert!((c.cross_agent_max - 0.15).abs() < f32::EPSILON);
        assert!((c.feedback_weight - 0.10).abs() < f32::EPSILON);
    }

    #[test]
    fn high_base_recent_access_yields_high_score() {
        let now = Utc::now();
        let score = compute_effective_importance(
            0.9,             // high base
            10,              // accessed 10 times
            Some(now),       // just accessed
            0,
            0,
            None,
            &cfg(),
        );
        // base * recency(~1.0) + frequency bonus => should be close to 0.9+
        assert!(score > 0.8, "score was {score}");
    }

    #[test]
    fn old_access_decays_recency() {
        let old = Utc::now() - Duration::days(28); // 4 half-lives at 7-day half-life
        let score = compute_effective_importance(
            1.0,
            0,
            Some(old),
            0,
            0,
            None,
            &cfg(),
        );
        // recency ~ 0.5^4 = 0.0625, so score ~ 0.0625
        assert!(score < 0.15, "score was {score}");
    }

    #[test]
    fn never_accessed_gets_neutral_recency() {
        let score = compute_effective_importance(
            1.0,
            0,
            None, // never accessed
            0,
            0,
            None,
            &cfg(),
        );
        // base * 0.5 = 0.5
        assert!((score - 0.5).abs() < 0.01, "score was {score}");
    }

    #[test]
    fn frequency_adds_bonus() {
        let now = Utc::now();
        let score_no_access = compute_effective_importance(
            0.5, 0, Some(now), 0, 0, None, &cfg(),
        );
        let score_many = compute_effective_importance(
            0.5, 100, Some(now), 0, 0, None, &cfg(),
        );
        assert!(
            score_many > score_no_access,
            "many={score_many} should be > none={score_no_access}"
        );
    }

    #[test]
    fn cross_agent_adds_bonus() {
        let now = Utc::now();
        let score_single = compute_effective_importance(
            0.5, 0, Some(now), 0, 1, None, &cfg(),
        );
        let score_multi = compute_effective_importance(
            0.5, 0, Some(now), 0, 4, None, &cfg(),
        );
        assert!(
            score_multi > score_single,
            "multi={score_multi} should be > single={score_single}"
        );
    }

    #[test]
    fn negative_feedback_lowers_score() {
        let now = Utc::now();
        let score_neutral = compute_effective_importance(
            0.5, 0, Some(now), 0, 0, None, &cfg(),
        );
        let score_negative = compute_effective_importance(
            0.5, 0, Some(now), 0, 0, Some(-1.0), &cfg(),
        );
        assert!(
            score_negative < score_neutral,
            "negative={score_negative} should be < neutral={score_neutral}"
        );
    }

    #[test]
    fn result_is_clamped_to_unit_range() {
        // Extremely high signals
        let high = compute_effective_importance(
            1.0, 1_000_000, Some(Utc::now()), 100, 100, Some(1.0), &cfg(),
        );
        assert!(high <= 1.0, "score {high} exceeds 1.0");

        // Extremely negative feedback with low base
        let low = compute_effective_importance(
            0.0, 0, None, 0, 0, Some(-1.0), &cfg(),
        );
        assert!(low >= 0.0, "score {low} below 0.0");
    }
}
