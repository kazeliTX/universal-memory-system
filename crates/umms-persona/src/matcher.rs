//! Agent matcher — auto-match documents to the most relevant agent based on content.

use umms_core::types::AgentId;

use crate::persona::AgentPersona;

/// Stateless agent matcher that scores documents against persona expertise.
pub struct AgentMatcher;

impl AgentMatcher {
    /// Given document text and available personas, return the best matching agent.
    ///
    /// Uses keyword overlap between document content and agent expertise.
    /// Case-insensitive matching. Returns `None` if no persona has any expertise
    /// keyword match in the text.
    pub fn match_agent(text: &str, personas: &[AgentPersona]) -> Option<AgentId> {
        if personas.is_empty() {
            return None;
        }

        let text_lower = text.to_lowercase();

        let mut best_id: Option<AgentId> = None;
        let mut best_score: usize = 0;

        for persona in personas {
            let score = persona
                .expertise
                .iter()
                .filter(|keyword| text_lower.contains(&keyword.to_lowercase()))
                .count();

            if score > best_score {
                best_score = score;
                best_id = Some(persona.agent_id.clone());
            }
        }

        // Only return a match if at least one keyword was found
        if best_score > 0 { best_id } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::{AgentPersona, AgentRetrievalConfig};
    use chrono::Utc;
    use std::str::FromStr;

    fn make_persona(id: &str, expertise: &[&str]) -> AgentPersona {
        AgentPersona {
            agent_id: AgentId::from_str(id).unwrap(),
            name: id.to_owned(),
            role: String::new(),
            description: String::new(),
            expertise: expertise.iter().map(|s| s.to_string()).collect(),
            system_prompt: String::new(),
            retrieval_config: AgentRetrievalConfig::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn matches_best_persona() {
        let personas = vec![
            make_persona("coder", &["rust", "python", "code", "api"]),
            make_persona("writer", &["writing", "prose", "blog"]),
        ];

        let result = AgentMatcher::match_agent(
            "Here is some Rust code that calls an API endpoint",
            &personas,
        );
        assert_eq!(result.unwrap().as_str(), "coder");
    }

    #[test]
    fn returns_none_when_no_match() {
        let personas = vec![make_persona("coder", &["rust", "python"])];
        let result = AgentMatcher::match_agent("The weather is nice today", &personas);
        assert!(result.is_none());
    }

    #[test]
    fn returns_none_for_empty_personas() {
        let result = AgentMatcher::match_agent("Hello world", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn case_insensitive_matching() {
        let personas = vec![make_persona("coder", &["Rust", "Python"])];
        let result = AgentMatcher::match_agent("I love rust and python programming", &personas);
        assert_eq!(result.unwrap().as_str(), "coder");
    }

    #[test]
    fn tiebreak_goes_to_first_highest() {
        let personas = vec![
            make_persona("alpha", &["rust", "code"]),
            make_persona("beta", &["rust", "code"]),
        ];
        // Both score 2, first one wins
        let result = AgentMatcher::match_agent("rust code example", &personas);
        assert_eq!(result.unwrap().as_str(), "alpha");
    }
}
