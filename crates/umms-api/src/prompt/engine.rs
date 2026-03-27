//! Template-based prompt construction engine.
//!
//! The engine manages named templates, each composed of ordered sections with
//! `{{variable}}` placeholders. At build time, variables are substituted and
//! optional sections with missing variables are silently omitted.

use std::collections::HashMap;

/// A prompt template engine that manages multiple named templates.
pub struct PromptEngine {
    templates: HashMap<String, PromptTemplate>,
}

/// A single prompt template with ordered sections.
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    pub name: String,
    pub sections: Vec<PromptSection>,
}

/// One section of a prompt template.
#[derive(Debug, Clone)]
pub struct PromptSection {
    /// Section name (e.g. "system", "memory", "diary").
    pub name: String,
    /// Template text with `{{variable}}` placeholders.
    pub template: String,
    /// If true, building fails when referenced variables are not provided.
    pub required: bool,
    /// Optional truncation limit (character count, not tokens — a reasonable proxy).
    pub max_chars: Option<usize>,
}

/// Errors from prompt building.
#[derive(Debug)]
pub enum PromptError {
    TemplateNotFound(String),
    MissingVariable { section: String, variable: String },
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemplateNotFound(name) => write!(f, "Template not found: {name}"),
            Self::MissingVariable { section, variable } => {
                write!(f, "Required variable '{variable}' missing in section '{section}'")
            }
        }
    }
}

impl std::error::Error for PromptError {}

impl PromptEngine {
    /// Create an empty engine.
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    /// Create an engine pre-loaded with the default chat template.
    pub fn with_defaults() -> Self {
        let mut engine = Self::new();
        engine.register(Self::default_chat_template());
        engine
    }

    /// Register a template. Overwrites any existing template with the same name.
    pub fn register(&mut self, template: PromptTemplate) {
        self.templates.insert(template.name.clone(), template);
    }

    /// Build a prompt by filling a template with the given variables.
    ///
    /// Sections whose variables are all empty or missing (and not required) are
    /// silently omitted. Required sections with missing variables return an error.
    pub fn build(
        &self,
        template_name: &str,
        vars: &HashMap<String, String>,
    ) -> Result<String, PromptError> {
        let template = self
            .templates
            .get(template_name)
            .ok_or_else(|| PromptError::TemplateNotFound(template_name.to_owned()))?;

        let mut parts = Vec::new();

        for section in &template.sections {
            let rendered = render_section(section, vars)?;
            if let Some(text) = rendered {
                parts.push(text);
            }
        }

        Ok(parts.join("\n\n"))
    }

    /// The default chat template used by the UMMS chat handler.
    pub fn default_chat_template() -> PromptTemplate {
        PromptTemplate {
            name: "chat".to_owned(),
            sections: vec![
                PromptSection {
                    name: "system".to_owned(),
                    template: "{{system_prompt}}".to_owned(),
                    required: true,
                    max_chars: None,
                },
                PromptSection {
                    name: "memory".to_owned(),
                    template: concat!(
                        "\u{3010}\u{8BB0}\u{5FC6}\u{7CFB}\u{7EDF}\u{3011}",
                        "\u{4EE5}\u{4E0B}\u{662F}\u{4ECE}\u{8BB0}\u{5FC6}\u{5E93}",
                        "\u{4E2D}\u{68C0}\u{7D22}\u{5230}\u{7684}\u{5185}\u{5BB9}",
                        "\u{FF08}\u{6309}\u{76F8}\u{5173}\u{5EA6}\u{6392}\u{5E8F}",
                        "\u{FF09}\u{3002}\n",
                        "\u{89C4}\u{5219}\u{FF1A}\n",
                        "- \u{5982}\u{679C}\u{8BB0}\u{5FC6}\u{4E0E}\u{7528}\u{6237}",
                        "\u{5F53}\u{524D}\u{95EE}\u{9898}\u{9AD8}\u{5EA6}\u{76F8}",
                        "\u{5173}\u{FF08}\u{76F8}\u{5173}\u{5EA6} > 60%\u{FF09}",
                        "\u{FF0C}\u{8BF7}\u{81EA}\u{7136}\u{5730}\u{878D}\u{5165}",
                        "\u{56DE}\u{7B54}\u{4E2D}\n",
                        "- \u{5982}\u{679C}\u{8BB0}\u{5FC6}\u{4E0E}\u{5F53}\u{524D}",
                        "\u{95EE}\u{9898}\u{65E0}\u{5173}\u{FF0C}\u{8BF7}\u{5B8C}",
                        "\u{5168}\u{5FFD}\u{7565}\u{5B83}\u{4EEC}\n",
                        "- \u{5F15}\u{7528}\u{8BB0}\u{5FC6}\u{65F6}\u{50CF}\u{662F}",
                        "\u{4F60}\u{672C}\u{6765}\u{5C31}\u{77E5}\u{9053}\u{7684}",
                        "\u{4E00}\u{6837}\u{81EA}\u{7136}\u{8868}\u{8FBE}\n",
                        "{{memory_content}}",
                    )
                    .to_owned(),
                    required: false,
                    max_chars: Some(8000),
                },
                PromptSection {
                    name: "diary".to_owned(),
                    template: "\u{3010}\u{7528}\u{6237}\u{6863}\u{6848}\u{3011}\
                               \u{4EE5}\u{4E0B}\u{662F}\u{4F60}\u{5BF9}\u{5F53}\u{524D}\
                               \u{7528}\u{6237}\u{7684}\u{4E86}\u{89E3}\u{FF1A}\n\
                               {{diary_content}}"
                        .to_owned(),
                    required: false,
                    max_chars: Some(2000),
                },
                PromptSection {
                    name: "history".to_owned(),
                    template: "\u{5BF9}\u{8BDD}\u{5386}\u{53F2}:\n{{history_content}}"
                        .to_owned(),
                    required: false,
                    max_chars: Some(6000),
                },
                PromptSection {
                    name: "user".to_owned(),
                    template: "\u{7528}\u{6237}: {{user_message}}".to_owned(),
                    required: true,
                    max_chars: None,
                },
                PromptSection {
                    name: "instruction".to_owned(),
                    template: "\u{8BF7}\u{7528}\u{4E2D}\u{6587}\u{56DE}\u{7B54}\u{3002}"
                        .to_owned(),
                    required: false,
                    max_chars: None,
                },
            ],
        }
    }
}

impl Default for PromptEngine {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Render a single section by substituting `{{var}}` placeholders.
///
/// Returns `None` if no variables were found (section is purely variable-driven
/// and all vars are empty). Returns `Err` if a required variable is missing.
fn render_section(
    section: &PromptSection,
    vars: &HashMap<String, String>,
) -> Result<Option<String>, PromptError> {
    let template = &section.template;

    // Find all {{variable}} placeholders
    let placeholders = extract_placeholders(template);

    if placeholders.is_empty() {
        // Static section with no variables — always include.
        return Ok(Some(template.clone()));
    }

    // Check for missing required variables
    let mut any_non_empty = false;
    let mut rendered = template.clone();

    for var_name in &placeholders {
        match vars.get(var_name.as_str()) {
            Some(value) if !value.is_empty() => {
                any_non_empty = true;
                rendered = rendered.replace(&format!("{{{{{var_name}}}}}"), value);
            }
            Some(_empty) => {
                // Variable present but empty
                rendered = rendered.replace(&format!("{{{{{var_name}}}}}"), "");
            }
            None if section.required => {
                return Err(PromptError::MissingVariable {
                    section: section.name.clone(),
                    variable: var_name.clone(),
                });
            }
            None => {
                // Optional missing variable — replace with empty
                rendered = rendered.replace(&format!("{{{{{var_name}}}}}"), "");
            }
        }
    }

    // If no variable had a non-empty value, skip this section entirely
    // (unless it's required, in which case we keep it even if empty).
    if !any_non_empty && !section.required {
        return Ok(None);
    }

    // Apply truncation if configured
    if let Some(max) = section.max_chars {
        if rendered.len() > max {
            // Truncate to max chars, trying to break at a newline
            let truncated = &rendered[..max];
            let break_point = truncated.rfind('\n').unwrap_or(max);
            rendered = format!("{}...", &rendered[..break_point]);
        }
    }

    Ok(Some(rendered))
}

/// Extract placeholder names from a template string.
/// E.g. `"Hello {{name}}, you have {{count}} items"` -> `["name", "count"]`
fn extract_placeholders(template: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        let after_open = &rest[start + 2..];
        if let Some(end) = after_open.find("}}") {
            let var_name = after_open[..end].trim().to_owned();
            if !var_name.is_empty() && !result.contains(&var_name) {
                result.push(var_name);
            }
            rest = &after_open[end + 2..];
        } else {
            break;
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn extract_placeholders_basic() {
        let placeholders = extract_placeholders("Hello {{name}}, you have {{count}} items");
        assert_eq!(placeholders, vec!["name", "count"]);
    }

    #[test]
    fn extract_placeholders_no_duplicates() {
        let placeholders = extract_placeholders("{{a}} and {{a}} again");
        assert_eq!(placeholders, vec!["a"]);
    }

    #[test]
    fn build_simple_template() {
        let mut engine = PromptEngine::new();
        engine.register(PromptTemplate {
            name: "test".to_owned(),
            sections: vec![
                PromptSection {
                    name: "greeting".to_owned(),
                    template: "Hello {{name}}!".to_owned(),
                    required: true,
                    max_chars: None,
                },
                PromptSection {
                    name: "body".to_owned(),
                    template: "You said: {{message}}".to_owned(),
                    required: true,
                    max_chars: None,
                },
            ],
        });

        let vars = make_vars(&[("name", "Alice"), ("message", "hi there")]);
        let result = engine.build("test", &vars).unwrap();
        assert!(result.contains("Hello Alice!"));
        assert!(result.contains("You said: hi there"));
    }

    #[test]
    fn optional_section_omitted_when_empty() {
        let mut engine = PromptEngine::new();
        engine.register(PromptTemplate {
            name: "test".to_owned(),
            sections: vec![
                PromptSection {
                    name: "required".to_owned(),
                    template: "Always here".to_owned(),
                    required: true,
                    max_chars: None,
                },
                PromptSection {
                    name: "optional".to_owned(),
                    template: "Context: {{context}}".to_owned(),
                    required: false,
                    max_chars: None,
                },
            ],
        });

        // Without context variable
        let vars = make_vars(&[]);
        let result = engine.build("test", &vars).unwrap();
        assert!(result.contains("Always here"));
        assert!(!result.contains("Context:"));
    }

    #[test]
    fn missing_required_variable_errors() {
        let mut engine = PromptEngine::new();
        engine.register(PromptTemplate {
            name: "test".to_owned(),
            sections: vec![PromptSection {
                name: "sys".to_owned(),
                template: "{{system_prompt}}".to_owned(),
                required: true,
                max_chars: None,
            }],
        });

        let vars = make_vars(&[]);
        let result = engine.build("test", &vars);
        assert!(result.is_err());
    }

    #[test]
    fn template_not_found_errors() {
        let engine = PromptEngine::new();
        let vars = make_vars(&[]);
        let result = engine.build("nonexistent", &vars);
        assert!(result.is_err());
    }

    #[test]
    fn max_chars_truncates() {
        let mut engine = PromptEngine::new();
        engine.register(PromptTemplate {
            name: "test".to_owned(),
            sections: vec![PromptSection {
                name: "content".to_owned(),
                template: "{{data}}".to_owned(),
                required: true,
                max_chars: Some(20),
            }],
        });

        let vars = make_vars(&[("data", "This is a very long string that should be truncated")]);
        let result = engine.build("test", &vars).unwrap();
        assert!(result.len() < 55); // original is ~51 chars
        assert!(result.ends_with("..."));
    }

    #[test]
    fn default_chat_template_builds() {
        let engine = PromptEngine::with_defaults();
        let vars = make_vars(&[
            ("system_prompt", "You are a helpful assistant."),
            ("user_message", "Hello!"),
        ]);
        let result = engine.build("chat", &vars).unwrap();
        assert!(result.contains("You are a helpful assistant."));
        assert!(result.contains("Hello!"));
    }

    #[test]
    fn default_chat_template_with_all_sections() {
        let engine = PromptEngine::with_defaults();
        let vars = make_vars(&[
            ("system_prompt", "You are helpful."),
            ("memory_content", "[Memory 1] Rust is great"),
            ("diary_content", "User prefers concise answers"),
            ("history_content", "User: hi\nAssistant: hello"),
            ("user_message", "Tell me about Rust"),
        ]);
        let result = engine.build("chat", &vars).unwrap();
        assert!(result.contains("You are helpful."));
        assert!(result.contains("Rust is great"));
        assert!(result.contains("concise answers"));
        assert!(result.contains("Tell me about Rust"));
    }
}
