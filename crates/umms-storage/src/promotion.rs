//! Memory promotion and demotion between Private and Shared scopes.
//!
//! Invariant: the shared layer can only be written to through:
//! 1. The consolidation service (automatic, M4) — checks importance/cross-refs/age
//! 2. This module's explicit promote/demote API (manual, user-triggered)
//!
//! Any other code path that writes `scope = Shared` is a bug.

use tracing::{info, warn};
use umms_core::error::{Result, StorageError, UmmsError};
use umms_core::traits::VectorStore;
use umms_core::types::{AgentId, IsolationScope, MemoryId};

/// Criteria for automatic promotion (used by M4 consolidation service).
pub struct PromotionCriteria {
    /// Minimum importance score to be eligible for promotion.
    pub min_importance: f32,
    /// Minimum hours since creation (avoid promoting ephemeral data).
    pub min_age_hours: u32,
}

impl Default for PromotionCriteria {
    fn default() -> Self {
        Self {
            min_importance: 0.7,
            min_age_hours: 24,
        }
    }
}

/// Result of a promote or demote operation.
#[derive(Debug)]
pub struct PromotionResult {
    pub memory_id: MemoryId,
    pub previous_scope: IsolationScope,
    pub new_scope: IsolationScope,
}

/// Promote a private memory to the shared layer.
///
/// This makes the memory visible to all agents. The entry's `scope` is changed
/// from `Private` to `Shared`, and agent-specific tags (if any) are stripped.
///
/// Returns error if:
/// - The memory doesn't exist
/// - The memory is already shared
/// - The memory belongs to the External scope (not promotable)
pub async fn promote(
    store: &dyn VectorStore,
    memory_id: &MemoryId,
    strip_tags: &[String],
) -> Result<PromotionResult> {
    let entry = store
        .get(memory_id)
        .await?
        .ok_or_else(|| UmmsError::Storage(StorageError::NotFound(memory_id.clone())))?;

    match entry.scope {
        IsolationScope::Shared => {
            warn!(memory_id = %memory_id, "Memory is already in shared scope");
            return Err(UmmsError::Storage(StorageError::WriteFailed {
                memory_id: memory_id.clone(),
                agent_id: entry.agent_id.clone(),
                reason: "Memory is already shared".into(),
            }));
        }
        IsolationScope::External => {
            return Err(UmmsError::Storage(StorageError::WriteFailed {
                memory_id: memory_id.clone(),
                agent_id: entry.agent_id.clone(),
                reason: "External memories cannot be promoted".into(),
            }));
        }
        IsolationScope::Private => {} // proceed
    }

    // Strip agent-specific tags if requested
    let new_tags: Option<Vec<String>> = if strip_tags.is_empty() {
        None
    } else {
        Some(
            entry
                .tags
                .iter()
                .filter(|t| !strip_tags.contains(t))
                .cloned()
                .collect(),
        )
    };

    store
        .update_metadata(
            memory_id,
            None,
            new_tags,
            Some(IsolationScope::Shared),
            None,
        )
        .await?;

    info!(
        memory_id = %memory_id,
        agent_id = %entry.agent_id,
        "Memory promoted to shared scope"
    );

    Ok(PromotionResult {
        memory_id: memory_id.clone(),
        previous_scope: IsolationScope::Private,
        new_scope: IsolationScope::Shared,
    })
}

/// Demote a shared memory back to a specific agent's private scope.
///
/// This restricts the memory to be visible only to the specified agent.
pub async fn demote(
    store: &dyn VectorStore,
    memory_id: &MemoryId,
    target_agent_id: &AgentId,
) -> Result<PromotionResult> {
    let entry = store
        .get(memory_id)
        .await?
        .ok_or_else(|| UmmsError::Storage(StorageError::NotFound(memory_id.clone())))?;

    if entry.scope != IsolationScope::Shared {
        return Err(UmmsError::Storage(StorageError::WriteFailed {
            memory_id: memory_id.clone(),
            agent_id: entry.agent_id.clone(),
            reason: format!("Cannot demote a {:?} memory, only Shared", entry.scope),
        }));
    }

    store
        .update_metadata(
            memory_id,
            None,
            None,
            Some(IsolationScope::Private),
            Some(target_agent_id.clone()),
        )
        .await?;

    info!(
        memory_id = %memory_id,
        target_agent = %target_agent_id,
        "Memory demoted to private scope"
    );

    Ok(PromotionResult {
        memory_id: memory_id.clone(),
        previous_scope: IsolationScope::Shared,
        new_scope: IsolationScope::Private,
    })
}

/// Check if a memory entry meets the criteria for automatic promotion.
/// Used by the M4 consolidation service to decide which memories to promote.
pub fn meets_promotion_criteria(
    importance: f32,
    created_hours_ago: f64,
    criteria: &PromotionCriteria,
) -> bool {
    importance >= criteria.min_importance && created_hours_ago >= f64::from(criteria.min_age_hours)
}

#[cfg(test)]
mod tests {
    use super::*;
    use umms_core::types::{MemoryEntryBuilder, Modality};

    #[test]
    fn meets_criteria_checks_importance_and_age() {
        let criteria = PromotionCriteria::default();

        // High importance, old enough → eligible
        assert!(meets_promotion_criteria(0.8, 48.0, &criteria));

        // High importance, too young → not eligible
        assert!(!meets_promotion_criteria(0.8, 12.0, &criteria));

        // Low importance, old enough → not eligible
        assert!(!meets_promotion_criteria(0.3, 48.0, &criteria));

        // Exactly at threshold
        assert!(meets_promotion_criteria(0.7, 24.0, &criteria));
    }

    #[test]
    fn default_criteria_values() {
        let c = PromotionCriteria::default();
        assert!((c.min_importance - 0.7).abs() < f32::EPSILON);
        assert_eq!(c.min_age_hours, 24);
    }
}
