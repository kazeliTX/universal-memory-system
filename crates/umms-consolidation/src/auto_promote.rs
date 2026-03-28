//! Automatic memory promotion — scan private memories and promote qualifying
//! entries to the shared scope.
//!
//! This module bridges the consolidation scheduler with the promotion logic
//! in `umms-storage`. It scans an agent's private memories, evaluates each
//! against [`PromotionCriteria`], and calls [`promote`] for those that qualify.

use std::time::Instant;

use chrono::Utc;
use serde::Serialize;
use tracing::{debug, info, instrument, warn};

use umms_core::config::PromotionConfig;
use umms_core::error::Result;
use umms_core::traits::VectorStore;
use umms_core::types::{AgentId, IsolationScope};
use umms_storage::promotion::{self, PromotionCriteria};

/// Result of an auto-promotion scan.
#[derive(Debug, Clone, Serialize)]
pub struct PromoteResult {
    /// Total private entries scanned.
    pub scanned: usize,
    /// Entries that were promoted to shared scope.
    pub promoted: usize,
    /// Wall-clock time in milliseconds.
    pub elapsed_ms: u64,
}

/// Automatic promoter that scans private memories and promotes those
/// meeting the configured criteria.
pub struct AutoPromoter {
    config: PromotionConfig,
}

impl AutoPromoter {
    /// Create a new auto-promoter with the given promotion config.
    pub fn new(config: PromotionConfig) -> Self {
        Self { config }
    }

    /// Build [`PromotionCriteria`] from the stored config.
    fn criteria(&self) -> PromotionCriteria {
        PromotionCriteria {
            min_importance: self.config.min_importance,
            min_age_hours: self.config.min_age_hours,
        }
    }

    /// Scan private memories for an agent and promote those meeting criteria.
    ///
    /// Uses paginated `list()` to walk all entries, filters for `Private` scope,
    /// checks promotion criteria, then calls [`promotion::promote`] for qualifying
    /// entries.
    #[instrument(skip(self, vector_store), fields(agent_id = %agent_id))]
    pub async fn scan_and_promote(
        &self,
        vector_store: &dyn VectorStore,
        agent_id: &AgentId,
    ) -> Result<PromoteResult> {
        let start = Instant::now();
        let criteria = self.criteria();

        let total = vector_store.count(agent_id, false).await?;
        info!(total_entries = total, "Starting auto-promotion scan");

        let mut scanned: usize = 0;
        let mut promoted: usize = 0;

        let page_size: u64 = 500;
        let mut offset: u64 = 0;
        let now = Utc::now();

        loop {
            let entries = vector_store
                .list(agent_id, offset, page_size, false)
                .await?;
            if entries.is_empty() {
                break;
            }

            let page_len = entries.len();
            scanned += page_len;

            for entry in &entries {
                // Only consider private entries.
                if entry.scope != IsolationScope::Private {
                    continue;
                }

                let created_hours_ago = (now - entry.created_at).num_seconds() as f64 / 3600.0;

                if promotion::meets_promotion_criteria(
                    entry.importance,
                    created_hours_ago,
                    &criteria,
                ) {
                    debug!(
                        memory_id = %entry.id,
                        importance = entry.importance,
                        created_hours_ago,
                        "Memory meets promotion criteria"
                    );

                    match promotion::promote(vector_store, &entry.id, &[]).await {
                        Ok(_result) => {
                            info!(memory_id = %entry.id, "Memory promoted to shared scope");
                            promoted += 1;
                        }
                        Err(e) => {
                            warn!(
                                memory_id = %entry.id,
                                error = %e,
                                "Failed to promote memory, skipping"
                            );
                        }
                    }
                }
            }

            if (page_len as u64) < page_size {
                break;
            }
            offset += page_size;
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        info!(
            scanned,
            promoted, elapsed_ms, "Auto-promotion scan complete"
        );

        Ok(PromoteResult {
            scanned,
            promoted,
            elapsed_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn criteria_from_config() {
        let config = PromotionConfig {
            min_importance: 0.8,
            min_access_count: 5,
            min_age_hours: 48,
        };
        let promoter = AutoPromoter::new(config);
        let criteria = promoter.criteria();

        assert!((criteria.min_importance - 0.8).abs() < f32::EPSILON);
        assert_eq!(criteria.min_age_hours, 48);
    }

    #[test]
    fn default_config_criteria() {
        let promoter = AutoPromoter::new(PromotionConfig::default());
        let criteria = promoter.criteria();

        assert!((criteria.min_importance - 0.7).abs() < f32::EPSILON);
        assert_eq!(criteria.min_age_hours, 24);
    }

    #[test]
    fn promotion_criteria_check() {
        let criteria = PromotionCriteria {
            min_importance: 0.7,
            min_age_hours: 24,
        };

        // Meets both criteria.
        assert!(promotion::meets_promotion_criteria(0.8, 48.0, &criteria));

        // Below importance.
        assert!(!promotion::meets_promotion_criteria(0.5, 48.0, &criteria));

        // Too young.
        assert!(!promotion::meets_promotion_criteria(0.8, 12.0, &criteria));

        // Exactly at thresholds.
        assert!(promotion::meets_promotion_criteria(0.7, 24.0, &criteria));
    }
}
