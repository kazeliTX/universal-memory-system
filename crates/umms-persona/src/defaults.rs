//! Pre-defined persona templates — seeded on first run.

use std::str::FromStr;

use chrono::Utc;

use umms_core::types::AgentId;

use crate::persona::{AgentPersona, AgentRetrievalConfig};

/// Returns the default set of personas to seed on first run.
pub fn default_personas() -> Vec<AgentPersona> {
    let now = Utc::now();

    vec![
        AgentPersona {
            agent_id: AgentId::from_str("coder").unwrap(),
            name: "Coder".to_owned(),
            role: "Software Engineer".to_owned(),
            description: "Handles programming, code analysis, and software architecture"
                .to_owned(),
            expertise: vec![
                "programming",
                "code",
                "rust",
                "python",
                "javascript",
                "api",
                "database",
                "testing",
                "debug",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            system_prompt: String::new(),
            retrieval_config: AgentRetrievalConfig::default(),
            created_at: now,
            updated_at: now,
        },
        AgentPersona {
            agent_id: AgentId::from_str("researcher").unwrap(),
            name: "Researcher".to_owned(),
            role: "Research Analyst".to_owned(),
            description: "Handles research, analysis, and knowledge synthesis".to_owned(),
            expertise: vec![
                "research",
                "analysis",
                "paper",
                "study",
                "data",
                "statistics",
                "methodology",
                "literature",
                "review",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            system_prompt: String::new(),
            retrieval_config: AgentRetrievalConfig::default(),
            created_at: now,
            updated_at: now,
        },
        AgentPersona {
            agent_id: AgentId::from_str("writer").unwrap(),
            name: "Writer".to_owned(),
            role: "Content Writer".to_owned(),
            description: "Handles writing, editing, and content creation".to_owned(),
            expertise: vec![
                "writing",
                "editing",
                "prose",
                "blog",
                "documentation",
                "article",
                "grammar",
                "style",
                "narrative",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            system_prompt: String::new(),
            retrieval_config: AgentRetrievalConfig::default(),
            created_at: now,
            updated_at: now,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_personas_are_valid() {
        let personas = default_personas();
        assert_eq!(personas.len(), 3);

        let ids: Vec<&str> = personas.iter().map(|p| p.agent_id.as_str()).collect();
        assert!(ids.contains(&"coder"));
        assert!(ids.contains(&"researcher"));
        assert!(ids.contains(&"writer"));

        for persona in &personas {
            assert!(!persona.name.is_empty());
            assert!(!persona.role.is_empty());
            assert!(!persona.description.is_empty());
            assert!(!persona.expertise.is_empty());
        }
    }
}
