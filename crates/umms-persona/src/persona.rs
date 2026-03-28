//! Agent persona data model — identity, expertise, and behavioral configuration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use umms_core::types::AgentId;

/// An Agent's persona — its identity, expertise, and behavioral configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPersona {
    pub agent_id: AgentId,
    pub name: String,
    pub role: String,
    pub description: String,
    pub expertise: Vec<String>,
    pub system_prompt: String,
    pub retrieval_config: AgentRetrievalConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Per-agent retrieval parameter overrides.
///
/// When set, these override the global retriever config for this agent.
/// `None` means "use the global default".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct AgentRetrievalConfig {
    pub bm25_weight: Option<f32>,
    pub min_score: Option<f32>,
    pub top_k_final: Option<usize>,
    pub lif_hops: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn persona_serialization_roundtrip() {
        let persona = AgentPersona {
            agent_id: AgentId::from_str("test-agent").unwrap(),
            name: "Test Agent".to_owned(),
            role: "Tester".to_owned(),
            description: "A test persona".to_owned(),
            expertise: vec!["testing".to_owned(), "qa".to_owned()],
            system_prompt: "You are a test agent.".to_owned(),
            retrieval_config: AgentRetrievalConfig {
                bm25_weight: Some(0.5),
                min_score: None,
                top_k_final: Some(5),
                lif_hops: None,
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let json = serde_json::to_string(&persona).unwrap();
        let deserialized: AgentPersona = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.agent_id.as_str(), "test-agent");
        assert_eq!(deserialized.name, "Test Agent");
        assert_eq!(deserialized.expertise.len(), 2);
        assert_eq!(deserialized.retrieval_config.bm25_weight, Some(0.5));
        assert!(deserialized.retrieval_config.min_score.is_none());
    }

    #[test]
    fn retrieval_config_default_is_all_none() {
        let cfg = AgentRetrievalConfig::default();
        assert!(cfg.bm25_weight.is_none());
        assert!(cfg.min_score.is_none());
        assert!(cfg.top_k_final.is_none());
        assert!(cfg.lif_hops.is_none());
    }
}
