//! Auto-diary generation — analyses conversation turns and extracts
//! diary-worthy observations about user behavior patterns.
//!
//! This runs asynchronously *after* the chat response is sent to the user,
//! so it adds zero latency to the chat flow.

use chrono::Utc;
use umms_model::ModelPool;
use umms_persona::{DiaryCategory, DiaryEntry};

/// Generates diary entries from conversation analysis.
pub struct DiaryGenerator;

impl DiaryGenerator {
    /// Analyze a conversation turn and generate diary entries if warranted.
    ///
    /// Returns entries to add to the diary. The caller decides whether to save them.
    /// Errors are logged and returned as empty vec — diary generation should never
    /// crash the application.
    pub async fn analyze_turn(
        pool: &ModelPool,
        agent_id: &str,
        user_message: &str,
        assistant_response: &str,
        existing_diary: &[DiaryEntry],
    ) -> Vec<DiaryEntry> {
        let existing = existing_diary
            .iter()
            .map(|e| format!("[{}] {}", e.category, e.content))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            "\u{4F60}\u{662F}\u{4E00}\u{4E2A}\u{89C2}\u{5BDF}\u{8005}\u{3002}\
             \u{5206}\u{6790}\u{4EE5}\u{4E0B}\u{5BF9}\u{8BDD}\u{FF0C}\
             \u{5224}\u{65AD}\u{662F}\u{5426}\u{6709}\u{503C}\u{5F97}\u{8BB0}\u{5F55}\u{7684}\
             \u{7528}\u{6237}\u{884C}\u{4E3A}\u{6A21}\u{5F0F}\u{3002}\n\n\
             \u{7528}\u{6237}\u{8BF4}: {user_message}\n\
             \u{52A9}\u{624B}\u{7B54}: {assistant_response}\n\n\
             \u{5DF2}\u{6709}\u{8BB0}\u{5F55}:\n{existing}\n\n\
             \u{5982}\u{679C}\u{53D1}\u{73B0}\u{65B0}\u{7684}\u{7528}\u{6237}\u{504F}\u{597D}\u{3001}\
             \u{4E13}\u{4E1A}\u{9886}\u{57DF}\u{3001}\u{6C9F}\u{901A}\u{98CE}\u{683C}\u{7B49}\
             \u{503C}\u{5F97}\u{8BB0}\u{5F55}\u{7684}\u{6A21}\u{5F0F}\u{FF0C}\n\
             \u{4EE5}JSON\u{6570}\u{7EC4}\u{683C}\u{5F0F}\u{8FD4}\u{56DE}\u{FF08}\
             \u{6BCF}\u{6761}\u{5305}\u{542B} category, content, confidence\u{FF09}\u{3002}\n\
             category \u{53EF}\u{9009}\u{503C}: preference, expertise, style, pattern, context, feedback\n\
             \u{5982}\u{679C}\u{6CA1}\u{6709}\u{503C}\u{5F97}\u{8BB0}\u{5F55}\u{7684}\u{FF0C}\
             \u{8FD4}\u{56DE}\u{7A7A}\u{6570}\u{7EC4} []\u{3002}\n\
             \u{53EA}\u{8FD4}\u{56DE}JSON\u{FF0C}\u{4E0D}\u{8981}\u{89E3}\u{91CA}\u{3002}"
        );

        let response = match pool.generate(&prompt).await {
            Ok(text) => text,
            Err(e) => {
                tracing::warn!(error = %e, "diary generation LLM call failed");
                return Vec::new();
            }
        };

        match parse_diary_entries(&response, agent_id) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::debug!(error = %e, raw = %response, "failed to parse diary entries from LLM response");
                Vec::new()
            }
        }
    }
}

/// Parse the LLM's JSON response into diary entries.
fn parse_diary_entries(
    response: &str,
    agent_id: &str,
) -> Result<Vec<DiaryEntry>, serde_json::Error> {
    #[derive(serde::Deserialize)]
    struct RawEntry {
        category: String,
        content: String,
        confidence: f32,
    }

    // The LLM may wrap the JSON in markdown code fences, so strip them.
    let cleaned = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let raw: Vec<RawEntry> = serde_json::from_str(cleaned)?;

    let now = Utc::now();
    let entries = raw
        .into_iter()
        .filter_map(|r| {
            let category: DiaryCategory = r.category.parse().ok()?;
            Some(DiaryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                agent_id: agent_id.to_owned(),
                category,
                content: r.content,
                confidence: r.confidence.clamp(0.0, 1.0),
                source_session_id: None,
                created_at: now,
                updated_at: now,
            })
        })
        .collect();

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_json() {
        let json = r#"[
            {"category": "preference", "content": "User prefers Rust", "confidence": 0.85},
            {"category": "expertise", "content": "Expert in ML", "confidence": 0.7}
        ]"#;

        let entries = parse_diary_entries(json, "agent-1").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].category, DiaryCategory::Preference);
        assert_eq!(entries[1].category, DiaryCategory::Expertise);
    }

    #[test]
    fn parse_empty_array() {
        let entries = parse_diary_entries("[]", "agent-1").unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_json_with_code_fences() {
        let response = "```json\n[{\"category\": \"style\", \"content\": \"Likes brief answers\", \"confidence\": 0.6}]\n```";
        let entries = parse_diary_entries(response, "agent-1").unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn parse_invalid_category_is_filtered() {
        let json = r#"[
            {"category": "unknown_cat", "content": "Something", "confidence": 0.5},
            {"category": "style", "content": "Good stuff", "confidence": 0.8}
        ]"#;
        let entries = parse_diary_entries(json, "agent-1").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].category, DiaryCategory::Style);
    }

    #[test]
    fn confidence_clamped() {
        let json = r#"[{"category": "pattern", "content": "Test", "confidence": 1.5}]"#;
        let entries = parse_diary_entries(json, "agent-1").unwrap();
        assert!((entries[0].confidence - 1.0).abs() < f32::EPSILON);
    }
}
