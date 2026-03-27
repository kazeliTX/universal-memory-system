//! Three-mode prompt construction engine (VCP-inspired).
//!
//! Replaces the old template-based engine with a flexible three-mode system:
//! - **Original**: raw text with `{{variable}}` substitution
//! - **Modular**: ordered blocks, each with variants, joined with `\n\n`
//! - **Preset**: load from template file, then substitute variables
//!
//! The legacy `build(template_name, vars)` method is retained for backward
//! compatibility — it delegates to the modular mode with a default config.

use std::collections::HashMap;

use super::types::*;

/// Errors from prompt building.
#[derive(Debug)]
pub enum PromptError {
    TemplateNotFound(String),
    MissingVariable { section: String, variable: String },
    BuildError(String),
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemplateNotFound(name) => write!(f, "Template not found: {name}"),
            Self::MissingVariable { section, variable } => {
                write!(f, "Required variable '{variable}' missing in section '{section}'")
            }
            Self::BuildError(msg) => write!(f, "Prompt build error: {msg}"),
        }
    }
}

impl std::error::Error for PromptError {}

/// A prompt template engine that supports three editing modes.
pub struct PromptEngine {
    /// Legacy templates for backward compatibility.
    templates: HashMap<String, PromptTemplate>,
}

/// A single prompt template with ordered sections (legacy support).
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    pub name: String,
    pub sections: Vec<PromptSection>,
}

/// One section of a prompt template (legacy support).
#[derive(Debug, Clone)]
pub struct PromptSection {
    pub name: String,
    pub template: String,
    pub required: bool,
    pub max_chars: Option<usize>,
}

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

    /// Register a legacy template.
    pub fn register(&mut self, template: PromptTemplate) {
        self.templates.insert(template.name.clone(), template);
    }

    // -----------------------------------------------------------------------
    // New three-mode prompt building
    // -----------------------------------------------------------------------

    /// Build the final prompt string from an agent's config + runtime variables.
    pub fn build_prompt(
        config: &AgentPromptConfig,
        vars: &HashMap<String, String>,
    ) -> Result<String, PromptError> {
        match config.mode {
            PromptMode::Original => Self::replace_variables(&config.original_prompt, vars),
            PromptMode::Modular => {
                let mut enabled_blocks: Vec<&PromptBlock> =
                    config.blocks.iter().filter(|b| b.enabled).collect();
                enabled_blocks.sort_by_key(|b| b.order);

                let mut parts = Vec::new();
                for block in enabled_blocks {
                    let content = if block.selected_variant < block.variants.len() {
                        &block.variants[block.selected_variant]
                    } else {
                        &block.content
                    };
                    let resolved = Self::replace_variables(content, vars)?;
                    if !resolved.trim().is_empty() {
                        parts.push(resolved);
                    }
                }
                Ok(parts.join("\n\n"))
            }
            PromptMode::Preset => {
                let content = config.preset_content.as_deref().unwrap_or("");
                Self::replace_variables(content, vars)
            }
        }
    }

    /// Replace `{{Variable}}` placeholders in text.
    pub fn replace_variables(
        text: &str,
        vars: &HashMap<String, String>,
    ) -> Result<String, PromptError> {
        let mut result = text.to_owned();
        for (key, value) in vars {
            result = result.replace(&format!("{{{{{key}}}}}"), value);
        }
        Ok(result)
    }

    /// Create default modular blocks for a new agent.
    pub fn default_blocks(_agent_name: &str) -> Vec<PromptBlock> {
        vec![
            PromptBlock {
                id: new_block_id(),
                name: "\u{8EAB}\u{4EFD}\u{8BBE}\u{5B9A}".into(), // 身份设定
                block_type: BlockType::System,
                content: "\u{4F60}\u{662F} {{AgentName}}\u{FF0C}{{AgentRole}}\u{3002}".into(),
                variants: vec![
                    "\u{4F60}\u{662F} {{AgentName}}\u{FF0C}{{AgentRole}}\u{3002}".into(),
                    "\u{4F60}\u{662F}\u{4E00}\u{4E2A}\u{540D}\u{4E3A} {{AgentName}} \u{7684}AI\u{52A9}\u{624B}\u{FF0C}\u{4E13}\u{6CE8}\u{4E8E} {{AgentRole}} \u{9886}\u{57DF}\u{3002}".into(),
                ],
                selected_variant: 0,
                enabled: true,
                order: 0,
            },
            PromptBlock {
                id: new_block_id(),
                name: "\u{8BB0}\u{5FC6}\u{89C4}\u{5219}".into(), // 记忆规则
                block_type: BlockType::Memory,
                content: "\u{3010}\u{8BB0}\u{5FC6}\u{7CFB}\u{7EDF}\u{3011}\u{4EE5}\u{4E0B}\u{662F}\u{4ECE}\u{8BB0}\u{5FC6}\u{5E93}\u{4E2D}\u{68C0}\u{7D22}\u{5230}\u{7684}\u{5185}\u{5BB9}\u{FF08}\u{6309}\u{76F8}\u{5173}\u{5EA6}\u{6392}\u{5E8F}\u{FF09}\u{3002}\n\u{89C4}\u{5219}\u{FF1A}\n- \u{5982}\u{679C}\u{8BB0}\u{5FC6}\u{4E0E}\u{7528}\u{6237}\u{5F53}\u{524D}\u{95EE}\u{9898}\u{9AD8}\u{5EA6}\u{76F8}\u{5173}\u{FF08}\u{76F8}\u{5173}\u{5EA6} > 60%\u{FF09}\u{FF0C}\u{8BF7}\u{81EA}\u{7136}\u{5730}\u{878D}\u{5165}\u{56DE}\u{7B54}\u{4E2D}\n- \u{5982}\u{679C}\u{8BB0}\u{5FC6}\u{4E0E}\u{5F53}\u{524D}\u{95EE}\u{9898}\u{65E0}\u{5173}\u{FF0C}\u{8BF7}\u{5B8C}\u{5168}\u{5FFD}\u{7565}\u{5B83}\u{4EEC}\n- \u{5F15}\u{7528}\u{8BB0}\u{5FC6}\u{65F6}\u{50CF}\u{662F}\u{4F60}\u{672C}\u{6765}\u{5C31}\u{77E5}\u{9053}\u{7684}\u{4E00}\u{6837}\u{81EA}\u{7136}\u{8868}\u{8FBE}\n{{memory_content}}".into(),
                variants: vec![
                    "\u{3010}\u{8BB0}\u{5FC6}\u{7CFB}\u{7EDF}\u{3011}\u{4EE5}\u{4E0B}\u{662F}\u{4ECE}\u{8BB0}\u{5FC6}\u{5E93}\u{4E2D}\u{68C0}\u{7D22}\u{5230}\u{7684}\u{5185}\u{5BB9}\u{FF08}\u{6309}\u{76F8}\u{5173}\u{5EA6}\u{6392}\u{5E8F}\u{FF09}\u{3002}\n\u{89C4}\u{5219}\u{FF1A}\n- \u{5982}\u{679C}\u{8BB0}\u{5FC6}\u{4E0E}\u{7528}\u{6237}\u{5F53}\u{524D}\u{95EE}\u{9898}\u{9AD8}\u{5EA6}\u{76F8}\u{5173}\u{FF08}\u{76F8}\u{5173}\u{5EA6} > 60%\u{FF09}\u{FF0C}\u{8BF7}\u{81EA}\u{7136}\u{5730}\u{878D}\u{5165}\u{56DE}\u{7B54}\u{4E2D}\n- \u{5982}\u{679C}\u{8BB0}\u{5FC6}\u{4E0E}\u{5F53}\u{524D}\u{95EE}\u{9898}\u{65E0}\u{5173}\u{FF0C}\u{8BF7}\u{5B8C}\u{5168}\u{5FFD}\u{7565}\u{5B83}\u{4EEC}\n- \u{5F15}\u{7528}\u{8BB0}\u{5FC6}\u{65F6}\u{50CF}\u{662F}\u{4F60}\u{672C}\u{6765}\u{5C31}\u{77E5}\u{9053}\u{7684}\u{4E00}\u{6837}\u{81EA}\u{7136}\u{8868}\u{8FBE}\n{{memory_content}}".into(),
                ],
                selected_variant: 0,
                enabled: true,
                order: 1,
            },
            PromptBlock {
                id: new_block_id(),
                name: "\u{7528}\u{6237}\u{6863}\u{6848}".into(), // 用户档案
                block_type: BlockType::Diary,
                content: "\u{3010}\u{7528}\u{6237}\u{6863}\u{6848}\u{3011}\u{4EE5}\u{4E0B}\u{662F}\u{4F60}\u{5BF9}\u{5F53}\u{524D}\u{7528}\u{6237}\u{7684}\u{4E86}\u{89E3}\u{FF1A}\n{{diary_content}}".into(),
                variants: vec![
                    "\u{3010}\u{7528}\u{6237}\u{6863}\u{6848}\u{3011}\u{4EE5}\u{4E0B}\u{662F}\u{4F60}\u{5BF9}\u{5F53}\u{524D}\u{7528}\u{6237}\u{7684}\u{4E86}\u{89E3}\u{FF1A}\n{{diary_content}}".into(),
                ],
                selected_variant: 0,
                enabled: true,
                order: 2,
            },
            PromptBlock {
                id: new_block_id(),
                name: "\u{5BF9}\u{8BDD}\u{5386}\u{53F2}".into(), // 对话历史
                block_type: BlockType::History,
                content: "\u{5BF9}\u{8BDD}\u{5386}\u{53F2}:\n{{history_content}}".into(),
                variants: vec![
                    "\u{5BF9}\u{8BDD}\u{5386}\u{53F2}:\n{{history_content}}".into(),
                ],
                selected_variant: 0,
                enabled: true,
                order: 3,
            },
            PromptBlock {
                id: new_block_id(),
                name: "\u{7528}\u{6237}\u{6D88}\u{606F}".into(), // 用户消息
                block_type: BlockType::User,
                content: "\u{7528}\u{6237}: {{user_message}}".into(),
                variants: vec!["\u{7528}\u{6237}: {{user_message}}".into()],
                selected_variant: 0,
                enabled: true,
                order: 4,
            },
            PromptBlock {
                id: new_block_id(),
                name: "\u{56DE}\u{7B54}\u{6307}\u{4EE4}".into(), // 回答指令
                block_type: BlockType::Instruction,
                content: "\u{8BF7}\u{7528}\u{4E2D}\u{6587}\u{56DE}\u{7B54}\u{3002}".into(), // 请用中文回答。
                variants: vec![
                    "\u{8BF7}\u{7528}\u{4E2D}\u{6587}\u{56DE}\u{7B54}\u{3002}".into(),
                    "\u{8BF7}\u{7528}\u{4E2D}\u{6587}\u{56DE}\u{7B54}\u{3002}\u{56DE}\u{7B54}\u{8981}\u{7B80}\u{6D01}\u{6709}\u{529B}\u{FF0C}\u{907F}\u{514D}\u{5197}\u{4F59}\u{3002}".into(),
                    "\u{8BF7}\u{7528}\u{4E2D}\u{6587}\u{56DE}\u{7B54}\u{3002}\u{56DE}\u{7B54}\u{8981}\u{8BE6}\u{7EC6}\u{5168}\u{9762}\u{FF0C}\u{5305}\u{542B}\u{4EE3}\u{7801}\u{793A}\u{4F8B}\u{3002}".into(),
                ],
                selected_variant: 0,
                enabled: true,
                order: 5,
            },
        ]
    }

    /// List all available variables with descriptions.
    pub fn available_variables() -> Vec<PromptVariable> {
        vec![
            PromptVariable {
                name: "AgentName".into(),
                description: "\u{667A}\u{80FD}\u{4F53}\u{540D}\u{79F0}".into(), // 智能体名称
                resolver: "runtime".into(),
            },
            PromptVariable {
                name: "AgentRole".into(),
                description: "\u{667A}\u{80FD}\u{4F53}\u{89D2}\u{8272}".into(), // 智能体角色
                resolver: "runtime".into(),
            },
            PromptVariable {
                name: "DateTime".into(),
                description: "\u{5F53}\u{524D}\u{65E5}\u{671F}\u{65F6}\u{95F4}".into(), // 当前日期时间
                resolver: "runtime".into(),
            },
            PromptVariable {
                name: "MemoryCount".into(),
                description: "\u{8BB0}\u{5FC6}\u{6761}\u{76EE}\u{603B}\u{6570}".into(), // 记忆条目总数
                resolver: "runtime".into(),
            },
            PromptVariable {
                name: "SessionTitle".into(),
                description: "\u{5F53}\u{524D}\u{4F1A}\u{8BDD}\u{6807}\u{9898}".into(), // 当前会话标题
                resolver: "runtime".into(),
            },
            PromptVariable {
                name: "DiaryEntries".into(),
                description: "\u{7528}\u{6237}\u{884C}\u{4E3A}\u{6863}\u{6848}".into(), // 用户行为档案
                resolver: "runtime".into(),
            },
            PromptVariable {
                name: "memory_content".into(),
                description: "\u{68C0}\u{7D22}\u{5230}\u{7684}\u{8BB0}\u{5FC6}\u{5185}\u{5BB9}".into(), // 检索到的记忆内容
                resolver: "runtime".into(),
            },
            PromptVariable {
                name: "diary_content".into(),
                description: "\u{7528}\u{6237}\u{6863}\u{6848}\u{5185}\u{5BB9}".into(), // 用户档案内容
                resolver: "runtime".into(),
            },
            PromptVariable {
                name: "history_content".into(),
                description: "\u{5BF9}\u{8BDD}\u{5386}\u{53F2}\u{5185}\u{5BB9}".into(), // 对话历史内容
                resolver: "runtime".into(),
            },
            PromptVariable {
                name: "user_message".into(),
                description: "\u{7528}\u{6237}\u{5F53}\u{524D}\u{6D88}\u{606F}".into(), // 用户当前消息
                resolver: "runtime".into(),
            },
        ]
    }

    // -----------------------------------------------------------------------
    // Legacy: backward-compatible build()
    // -----------------------------------------------------------------------

    /// Build a prompt by filling a legacy template with the given variables.
    ///
    /// Retained for backward compatibility with existing chat handler code.
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

    /// The default chat template used by the UMMS chat handler (legacy).
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

/// Render a single section by substituting `{{var}}` placeholders (legacy).
fn render_section(
    section: &PromptSection,
    vars: &HashMap<String, String>,
) -> Result<Option<String>, PromptError> {
    let template = &section.template;

    let placeholders = extract_placeholders(template);

    if placeholders.is_empty() {
        return Ok(Some(template.clone()));
    }

    let mut any_non_empty = false;
    let mut rendered = template.clone();

    for var_name in &placeholders {
        match vars.get(var_name.as_str()) {
            Some(value) if !value.is_empty() => {
                any_non_empty = true;
                rendered = rendered.replace(&format!("{{{{{var_name}}}}}"), value);
            }
            Some(_empty) => {
                rendered = rendered.replace(&format!("{{{{{var_name}}}}}"), "");
            }
            None if section.required => {
                return Err(PromptError::MissingVariable {
                    section: section.name.clone(),
                    variable: var_name.clone(),
                });
            }
            None => {
                rendered = rendered.replace(&format!("{{{{{var_name}}}}}"), "");
            }
        }
    }

    if !any_non_empty && !section.required {
        return Ok(None);
    }

    if let Some(max) = section.max_chars {
        if rendered.len() > max {
            let truncated = &rendered[..max];
            let break_point = truncated.rfind('\n').unwrap_or(max);
            rendered = format!("{}...", &rendered[..break_point]);
        }
    }

    Ok(Some(rendered))
}

/// Extract placeholder names from a template string.
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
    use chrono::Utc;

    fn make_vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // --- Legacy tests (backward compatibility) ---

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
        assert!(result.len() < 55);
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

    // --- New three-mode tests ---

    #[test]
    fn build_prompt_original_mode() {
        let config = AgentPromptConfig {
            agent_id: "test".into(),
            mode: PromptMode::Original,
            original_prompt: "Hello {{AgentName}}, you are {{AgentRole}}.".into(),
            blocks: vec![],
            preset_path: None,
            preset_content: None,
            updated_at: Utc::now(),
        };

        let vars = make_vars(&[("AgentName", "Alice"), ("AgentRole", "helper")]);
        let result = PromptEngine::build_prompt(&config, &vars).unwrap();
        assert_eq!(result, "Hello Alice, you are helper.");
    }

    #[test]
    fn build_prompt_modular_mode() {
        let config = AgentPromptConfig {
            agent_id: "test".into(),
            mode: PromptMode::Modular,
            original_prompt: String::new(),
            blocks: vec![
                PromptBlock {
                    id: "b1".into(),
                    name: "system".into(),
                    block_type: BlockType::System,
                    content: "You are {{AgentName}}.".into(),
                    variants: vec!["You are {{AgentName}}.".into()],
                    selected_variant: 0,
                    enabled: true,
                    order: 0,
                },
                PromptBlock {
                    id: "b2".into(),
                    name: "disabled".into(),
                    block_type: BlockType::Custom,
                    content: "SHOULD NOT APPEAR".into(),
                    variants: vec!["SHOULD NOT APPEAR".into()],
                    selected_variant: 0,
                    enabled: false,
                    order: 1,
                },
                PromptBlock {
                    id: "b3".into(),
                    name: "instruction".into(),
                    block_type: BlockType::Instruction,
                    content: "Answer in English.".into(),
                    variants: vec![
                        "Answer in English.".into(),
                        "Answer in Chinese.".into(),
                    ],
                    selected_variant: 0,
                    enabled: true,
                    order: 2,
                },
            ],
            preset_path: None,
            preset_content: None,
            updated_at: Utc::now(),
        };

        let vars = make_vars(&[("AgentName", "Bob")]);
        let result = PromptEngine::build_prompt(&config, &vars).unwrap();
        assert!(result.contains("You are Bob."));
        assert!(result.contains("Answer in English."));
        assert!(!result.contains("SHOULD NOT APPEAR"));
    }

    #[test]
    fn build_prompt_modular_respects_variant_selection() {
        let config = AgentPromptConfig {
            agent_id: "test".into(),
            mode: PromptMode::Modular,
            original_prompt: String::new(),
            blocks: vec![PromptBlock {
                id: "b1".into(),
                name: "instruction".into(),
                block_type: BlockType::Instruction,
                content: "variant 0".into(),
                variants: vec!["variant 0".into(), "variant 1".into()],
                selected_variant: 1,
                enabled: true,
                order: 0,
            }],
            preset_path: None,
            preset_content: None,
            updated_at: Utc::now(),
        };

        let vars = make_vars(&[]);
        let result = PromptEngine::build_prompt(&config, &vars).unwrap();
        assert_eq!(result, "variant 1");
    }

    #[test]
    fn build_prompt_preset_mode() {
        let config = AgentPromptConfig {
            agent_id: "test".into(),
            mode: PromptMode::Preset,
            original_prompt: String::new(),
            blocks: vec![],
            preset_path: Some("template.md".into()),
            preset_content: Some("Welcome, {{AgentName}}! Date: {{DateTime}}".into()),
            updated_at: Utc::now(),
        };

        let vars = make_vars(&[("AgentName", "Charlie"), ("DateTime", "2026-01-01")]);
        let result = PromptEngine::build_prompt(&config, &vars).unwrap();
        assert_eq!(result, "Welcome, Charlie! Date: 2026-01-01");
    }

    #[test]
    fn build_prompt_modular_respects_order() {
        let config = AgentPromptConfig {
            agent_id: "test".into(),
            mode: PromptMode::Modular,
            original_prompt: String::new(),
            blocks: vec![
                PromptBlock {
                    id: "b1".into(),
                    name: "second".into(),
                    block_type: BlockType::Custom,
                    content: "SECOND".into(),
                    variants: vec!["SECOND".into()],
                    selected_variant: 0,
                    enabled: true,
                    order: 2,
                },
                PromptBlock {
                    id: "b2".into(),
                    name: "first".into(),
                    block_type: BlockType::Custom,
                    content: "FIRST".into(),
                    variants: vec!["FIRST".into()],
                    selected_variant: 0,
                    enabled: true,
                    order: 1,
                },
            ],
            preset_path: None,
            preset_content: None,
            updated_at: Utc::now(),
        };

        let vars = make_vars(&[]);
        let result = PromptEngine::build_prompt(&config, &vars).unwrap();
        let first_pos = result.find("FIRST").unwrap();
        let second_pos = result.find("SECOND").unwrap();
        assert!(first_pos < second_pos, "FIRST should come before SECOND");
    }

    #[test]
    fn variable_replacement() {
        let vars = make_vars(&[("name", "World"), ("count", "42")]);
        let result =
            PromptEngine::replace_variables("Hello {{name}}, you have {{count}} items.", &vars)
                .unwrap();
        assert_eq!(result, "Hello World, you have 42 items.");
    }

    #[test]
    fn default_blocks_are_valid() {
        let blocks = PromptEngine::default_blocks("test-agent");
        assert_eq!(blocks.len(), 6);
        assert!(blocks[0].enabled);
        assert_eq!(blocks[0].block_type, BlockType::System);
        assert_eq!(blocks[5].block_type, BlockType::Instruction);
        // Verify ordering is sequential
        for (i, block) in blocks.iter().enumerate() {
            assert_eq!(block.order, i);
        }
    }

    #[test]
    fn available_variables_not_empty() {
        let vars = PromptEngine::available_variables();
        assert!(vars.len() >= 6);
        assert!(vars.iter().any(|v| v.name == "AgentName"));
        assert!(vars.iter().any(|v| v.name == "user_message"));
    }

    #[test]
    fn build_prompt_modular_skips_empty_resolved_blocks() {
        let config = AgentPromptConfig {
            agent_id: "test".into(),
            mode: PromptMode::Modular,
            original_prompt: String::new(),
            blocks: vec![
                PromptBlock {
                    id: "b1".into(),
                    name: "system".into(),
                    block_type: BlockType::System,
                    content: "I am here".into(),
                    variants: vec!["I am here".into()],
                    selected_variant: 0,
                    enabled: true,
                    order: 0,
                },
                PromptBlock {
                    id: "b2".into(),
                    name: "empty".into(),
                    block_type: BlockType::Memory,
                    content: "{{memory_content}}".into(),
                    variants: vec!["{{memory_content}}".into()],
                    selected_variant: 0,
                    enabled: true,
                    order: 1,
                },
            ],
            preset_path: None,
            preset_content: None,
            updated_at: Utc::now(),
        };

        // memory_content is empty, so its block should be skipped
        let vars = make_vars(&[("memory_content", "")]);
        let result = PromptEngine::build_prompt(&config, &vars).unwrap();
        assert_eq!(result, "I am here");
    }
}
