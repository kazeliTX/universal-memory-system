//! Agent diary data model — per-agent observations about user behavior patterns.
//!
//! The diary is a notebook where an agent records observations about users:
//! preferences, expertise levels, communication style, recurring patterns,
//! active context, and corrections. These entries are injected into prompts
//! to provide personalised, context-aware responses.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A diary entry recorded by an agent about a user interaction pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiaryEntry {
    /// Unique identifier (UUID).
    pub id: String,
    /// Which agent recorded this observation.
    pub agent_id: String,
    /// What kind of observation this is.
    pub category: DiaryCategory,
    /// The observation text.
    pub content: String,
    /// Confidence level (0.0 = guess, 1.0 = certain).
    pub confidence: f32,
    /// Which session triggered this observation (if any).
    pub source_session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Categories of diary observations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiaryCategory {
    /// User prefers X over Y.
    Preference,
    /// User is expert in X, beginner in Y.
    Expertise,
    /// User likes concise/detailed answers.
    Style,
    /// User often asks about X after Y.
    Pattern,
    /// User is working on project X.
    Context,
    /// User corrected agent on X.
    Feedback,
}

impl fmt::Display for DiaryCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Preference => write!(f, "preference"),
            Self::Expertise => write!(f, "expertise"),
            Self::Style => write!(f, "style"),
            Self::Pattern => write!(f, "pattern"),
            Self::Context => write!(f, "context"),
            Self::Feedback => write!(f, "feedback"),
        }
    }
}

impl std::str::FromStr for DiaryCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "preference" => Ok(Self::Preference),
            "expertise" => Ok(Self::Expertise),
            "style" => Ok(Self::Style),
            "pattern" => Ok(Self::Pattern),
            "context" => Ok(Self::Context),
            "feedback" => Ok(Self::Feedback),
            other => Err(format!("Unknown diary category: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_display_roundtrip() {
        let categories = [
            DiaryCategory::Preference,
            DiaryCategory::Expertise,
            DiaryCategory::Style,
            DiaryCategory::Pattern,
            DiaryCategory::Context,
            DiaryCategory::Feedback,
        ];
        for cat in &categories {
            let s = cat.to_string();
            let parsed: DiaryCategory = s.parse().unwrap();
            assert_eq!(&parsed, cat);
        }
    }

    #[test]
    fn category_serde_roundtrip() {
        let entry = DiaryEntry {
            id: "test-id".to_owned(),
            agent_id: "agent-1".to_owned(),
            category: DiaryCategory::Expertise,
            content: "User is expert in Rust".to_owned(),
            confidence: 0.85,
            source_session_id: Some("session-1".to_owned()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: DiaryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.category, DiaryCategory::Expertise);
        assert_eq!(deserialized.content, "User is expert in Rust");
    }
}
