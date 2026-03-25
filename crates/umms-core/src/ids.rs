//! Newtype ID wrappers — compile-time safety against parameter mixups.
//!
//! Each ID type is a distinct type wrapping a String. Passing an `AgentId`
//! where a `SessionId` is expected is a compile error, not a runtime bug.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            /// Generate a new random ID.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4().to_string())
            }

            /// Wrap an existing string as this ID type.
            /// Validates: non-empty, only `[a-zA-Z0-9_-]`, max 128 chars.
            pub fn from_str(s: &str) -> std::result::Result<Self, &'static str> {
                if s.is_empty() {
                    return Err("ID must not be empty");
                }
                if s.len() > 128 {
                    return Err("ID must not exceed 128 characters");
                }
                if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                    return Err("ID must only contain [a-zA-Z0-9_-]");
                }
                Ok(Self(s.to_owned()))
            }

            /// Access the inner string. Use sparingly — prefer passing the typed ID.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

define_id!(
    /// Unique identifier for an Agent.
    /// This is the isolation key — every storage operation must carry one.
    AgentId
);

define_id!(
    /// Unique identifier for a memory entry.
    MemoryId
);

define_id!(
    /// Unique identifier for a user session.
    SessionId
);

define_id!(
    /// Unique identifier for a knowledge graph node.
    NodeId
);

define_id!(
    /// Unique identifier for a knowledge graph edge.
    EdgeId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_id_generates_unique_ids() {
        let id1 = MemoryId::new();
        let id2 = MemoryId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn agent_id_rejects_invalid_input() {
        assert!(AgentId::from_str("").is_err());
        assert!(AgentId::from_str("has spaces").is_err());
        assert!(AgentId::from_str("has/slash").is_err());
        assert!(AgentId::from_str(&"x".repeat(129)).is_err());
    }

    #[test]
    fn agent_id_accepts_valid_input() {
        assert!(AgentId::from_str("coding_assistant").is_ok());
        assert!(AgentId::from_str("agent-01").is_ok());
        assert!(AgentId::from_str("A").is_ok());
    }
}
