//! Memory Decay Engine — exponential importance decay with archival.
//!
//! Implements the forgetting curve: memories that are not accessed lose
//! importance over time. Memories that fall below the archive threshold
//! for long enough are marked for archival.
//!
//! The decay formula is: `new_importance = max(importance * (1 - rate)^days, floor)`
//!
//! # Known limitation
//!
//! The `VectorStore` trait does not expose a `list_all` method. We use the
//! `list()` method with pagination to scan all entries. This is acceptable
//! at personal scale (<10K entries) but would need a streaming iterator for
//! production workloads.

use std::time::Instant;

use chrono::Utc;
use serde::Serialize;
use tracing::{debug, info, instrument};

use umms_core::config::DecayConfig;
use umms_core::error::Result;
use umms_core::traits::VectorStore;
use umms_core::types::AgentId;

/// Result of a decay pass over an agent's memories.
#[derive(Debug, Clone, Serialize)]
pub struct DecayResult {
    /// Total entries scanned.
    pub scanned: usize,
    /// Entries whose importance was updated.
    pub updated: usize,
    /// Entries that were archived (importance below threshold + old enough).
    pub archived: usize,
    /// Wall-clock time for the decay pass in milliseconds.
    pub elapsed_ms: u64,
}

/// Engine that applies exponential importance decay to memory entries.
pub struct DecayEngine {
    config: DecayConfig,
}

impl DecayEngine {
    /// Create a new decay engine with the given configuration.
    pub fn new(config: DecayConfig) -> Self {
        Self { config }
    }

    /// Calculate the new importance after decay.
    ///
    /// Formula: `max(importance * (1 - rate)^days_since_access, floor)`
    ///
    /// - `current_importance`: the entry's current importance score (0.0..=1.0)
    /// - `days_since_access`: fractional days since the entry was last accessed
    pub fn calculate_decay(&self, current_importance: f32, days_since_access: f64) -> f32 {
        if days_since_access <= 0.0 {
            return current_importance;
        }
        let base = 1.0 - f64::from(self.config.rate);
        let decayed = f64::from(current_importance) * base.powf(days_since_access);
        (decayed as f32).max(self.config.floor)
    }

    /// Check if a memory should be archived based on importance and age.
    ///
    /// A memory is archived when:
    /// 1. Its importance is at or below `archive_threshold`, AND
    /// 2. It has not been accessed for at least `archive_after_days` days.
    pub fn should_archive(&self, importance: f32, days_since_access: f64) -> bool {
        importance <= self.config.archive_threshold
            && days_since_access >= f64::from(self.config.archive_after_days)
    }

    /// Run decay on all memories for an agent.
    ///
    /// Scans all entries via paginated `list()`, applies the decay formula,
    /// and updates entries whose importance changed. Entries meeting archive
    /// criteria are flagged (importance set to 0.0).
    ///
    /// Returns a [`DecayResult`] summarizing the pass.
    #[instrument(skip(self, vector_store), fields(agent_id = %agent_id))]
    pub async fn run_decay(
        &self,
        vector_store: &dyn VectorStore,
        agent_id: &AgentId,
    ) -> Result<DecayResult> {
        let start = Instant::now();

        if !self.config.enabled {
            debug!("Decay is disabled, skipping");
            return Ok(DecayResult {
                scanned: 0,
                updated: 0,
                archived: 0,
                elapsed_ms: 0,
            });
        }

        let total = vector_store.count(agent_id, false).await? as u64;
        info!(total_entries = total, "Starting decay pass");

        let mut scanned: usize = 0;
        let mut updated: usize = 0;
        let mut archived: usize = 0;

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
                let days_since_access = (now - entry.accessed_at).num_seconds() as f64 / 86_400.0;

                let new_importance = self.calculate_decay(entry.importance, days_since_access);

                // Check for archival condition.
                let archive = self.should_archive(new_importance, days_since_access);

                // Only update if importance actually changed (avoid pointless writes).
                let importance_changed = (new_importance - entry.importance).abs() > f32::EPSILON;

                if archive {
                    // Archive by setting importance to 0.0 (marks as fully decayed).
                    debug!(
                        memory_id = %entry.id,
                        old_importance = entry.importance,
                        days_since_access,
                        "Archiving memory"
                    );
                    vector_store
                        .update_metadata(&entry.id, Some(0.0), None, None, None)
                        .await?;
                    archived += 1;
                    updated += 1;
                } else if importance_changed {
                    debug!(
                        memory_id = %entry.id,
                        old = entry.importance,
                        new = new_importance,
                        "Decaying memory importance"
                    );
                    vector_store
                        .update_metadata(&entry.id, Some(new_importance), None, None, None)
                        .await?;
                    updated += 1;
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
            updated, archived, elapsed_ms, "Decay pass complete"
        );

        Ok(DecayResult {
            scanned,
            updated,
            archived,
            elapsed_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_engine() -> DecayEngine {
        DecayEngine::new(DecayConfig {
            enabled: true,
            rate: 0.05,
            floor: 0.01,
            archive_threshold: 0.05,
            archive_after_days: 90,
            delete_originals: false,
        })
    }

    #[test]
    fn decay_formula_basic() {
        let engine = default_engine();
        // After 0 days, importance is unchanged.
        let result = engine.calculate_decay(1.0, 0.0);
        assert!((result - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn decay_formula_one_day() {
        let engine = default_engine();
        // After 1 day at 5% rate: 1.0 * (0.95)^1 = 0.95
        let result = engine.calculate_decay(1.0, 1.0);
        assert!((result - 0.95).abs() < 0.001);
    }

    #[test]
    fn decay_formula_multiple_days() {
        let engine = default_engine();
        // After 10 days at 5% rate: 1.0 * (0.95)^10 ≈ 0.5987
        let result = engine.calculate_decay(1.0, 10.0);
        let expected = 0.95_f64.powi(10) as f32;
        assert!((result - expected).abs() < 0.001);
    }

    #[test]
    fn decay_respects_floor() {
        let engine = default_engine();
        // After 1000 days, should clamp to floor (0.01).
        let result = engine.calculate_decay(1.0, 1000.0);
        assert!((result - 0.01).abs() < f32::EPSILON);
    }

    #[test]
    fn decay_with_low_initial_importance() {
        let engine = default_engine();
        // Starting at 0.1, after 50 days: 0.1 * (0.95)^50 ≈ 0.00769
        // Should clamp to floor 0.01.
        let result = engine.calculate_decay(0.1, 50.0);
        assert!((result - 0.01).abs() < f32::EPSILON);
    }

    #[test]
    fn decay_negative_days_returns_unchanged() {
        let engine = default_engine();
        let result = engine.calculate_decay(0.8, -5.0);
        assert!((result - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn should_archive_below_threshold_and_old() {
        let engine = default_engine();
        // Below threshold (0.05) and older than 90 days → archive.
        assert!(engine.should_archive(0.03, 100.0));
    }

    #[test]
    fn should_not_archive_above_threshold() {
        let engine = default_engine();
        // Above threshold → don't archive, even if old.
        assert!(!engine.should_archive(0.1, 100.0));
    }

    #[test]
    fn should_not_archive_too_recent() {
        let engine = default_engine();
        // Below threshold but too recent → don't archive.
        assert!(!engine.should_archive(0.03, 30.0));
    }

    #[test]
    fn should_archive_at_exact_threshold() {
        let engine = default_engine();
        // Exactly at threshold and exactly at day limit → archive.
        assert!(engine.should_archive(0.05, 90.0));
    }
}
