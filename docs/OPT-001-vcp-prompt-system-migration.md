# OPT-001: VCP Prompt 系统移植到 UMMS 的优化方案

## 状态

已记录（2026-03-28），待实施。

## 背景

VCP（VCPChat）项目拥有成熟的三模式 Prompt 管理系统，UMMS 在 Phase 5 中已参考 VCP 实现了基础框架（`umms-api/src/prompt/`）。本文档对比两个系统的差异，识别 VCP 中值得移植但 UMMS 尚未覆盖的能力，并给出具体的优化计划。

## 一、现状对比

### 1.1 三模式核心（已移植）

| 能力 | VCP | UMMS | 状态 |
|------|-----|------|------|
| 原始文本模式 | `originalSystemPrompt` | `PromptMode::Original` | **已移植** |
| 模块化积木模式 | `advancedSystemPrompt` | `PromptMode::Modular` | **已移植** |
| 预制模板模式 | `presetSystemPrompt` | `PromptMode::Preset` | **已移植** |
| 模式切换 | `prompt-manager.js` | `PUT /api/prompts/:id/mode` | **已移植** |
| `{{Variable}}` 替换 | 简单正则 | `PromptEngine::replace_variables` | **已移植** |
| Block 变体系统 | `variants[]` + `selectedVariant` | `PromptBlock.variants` | **已移植** |
| Block 增删改排序 | Drag-drop UI | REST API 完整 | **已移植** |
| 仓库系统（全局/私有） | `hiddenBlocks` + warehouse | `PromptWarehouse` + SQLite | **已移植** |
| Preset 文件扫描 | `promptHandlers.js` | `GET /api/prompts/presets` | **已移植** |
| 10 个模板变量 | `{{AgentName}}` 等 | 10 个 runtime 变量 | **已移植** |
| 6 个默认积木块 | 无（VCP 不预设块） | 身份/记忆/档案/历史/用户/指令 | **UMMS 超越** |
| 语义化 BlockType | 无（VCP 只有 text/newline） | System/Memory/Diary/History/User/Instruction/Custom/Separator | **UMMS 超越** |

### 1.2 VCP 已有但 UMMS 未移植的能力

| # | VCP 能力 | 描述 | 价值 |
|---|----------|------|------|
| G1 | **DASP 协议** | 动态流式渲染——LLM 输出中嵌入控制指令，前端实时更新/隐藏/高亮内容 | 高 |
| G2 | **Block 去重检测** | `areBlocksEqual()` — 拖入仓库时防止重复 | 中 |
| G3 | **自定义模式名称** | 用户可重命名三个模式的显示名 | 低 |
| G4 | **Prompt Sponsor 远程 API** | 外部进程通过 STDIN/STDOUT JSON 协议远程操作 Prompt | 中 |
| G5 | **预制模板目录浏览** | 用户可切换 preset 目录路径，不限于默认位置 | 低 |
| G6 | **群聊 Prompt 隔离** | 多 Agent 群聊时各自独立 Prompt + fallback 机制 | 高 |
| G7 | **翻译专用 Prompt** | 独立的翻译模式，与主对话 Prompt 互不干扰 | 低 |

### 1.3 UMMS 已有但 VCP 没有的能力

| 能力 | 说明 |
|------|------|
| 语义化 BlockType (8 种) | VCP 只有 text/newline，UMMS 可区分 System/Memory/Diary 等，支持按类型过滤 |
| 6 个预设默认积木 | 新 Agent 开箱即有结构化 Prompt，VCP 新建 Agent 只有一句话 |
| 变量分类 (static/runtime/config) | `PromptVariable.resolver` 标记变量来源，VCP 没有 |
| Prompt 预览 API | `POST /api/prompts/preview` 可在保存前预览渲染结果 |
| Diary 系统集成 | `{{diary_content}}` 自动注入用户行为档案，VCP 无此概念 |
| 记忆内容注入 | `{{memory_content}}` 由检索管道自动填充，VCP 记忆是独立插件 |
| Legacy 模板兼容层 | `PromptEngine::build()` 保留旧版接口，平滑过渡 |

---

## 二、待移植项详细方案

### G1: DASP 动态流式渲染协议

**VCP 实现**:
- `modules/DASP.txt` 定义协议规范
- LLM 在 streaming 输出中嵌入 HTML 注释指令：
  ```html
  <span class="provisional-content" id="step1">分析中...</span>
  <!--AI_INSTRUCTIONS:{"update":[{"id":"step1","newContent":"分析完成"}]}-->
  <!--AI_INSTRUCTIONS:{"hide":["step1"]}-->
  ```
- 前端解析器实时执行：hide / show / update / highlight / unhighlight / hideAll
- 三个角色变体：编程助手（req-analysis → algo-choice → testing）、数据分析、问答

**UMMS 移植方案**:

1. **后端**：DASP 对后端透明 — 只是 system prompt 中的一段文本，后端无需解析。
   - 在 `PromptEngine::default_blocks()` 中新增一个可选的 `BlockType::Custom` 块，内容为 DASP 协议文本
   - 或作为预制模板文件放入 `presets/dasp_programmer.md`、`presets/dasp_analyst.md`、`presets/dasp_qa.md`
   - **推荐**: 作为预制模板文件，用户按需启用，不强制注入

2. **前端**（`chat/`）：需要实现 DASP 解析器
   - 在 Vue chat 组件的 markdown 渲染管线中增加后处理步骤
   - 监听 streaming chunks，检测 `<!--AI_INSTRUCTIONS:{...}-->` 模式
   - 执行 DOM 操作：`document.getElementById(id).style.display = 'none'` 等
   - 支持 `provisional-content` class 的特殊渲染样式

3. **文件变更**:
   ```
   新增 configs/presets/dasp_programmer.md       # DASP 编程助手模板
   新增 configs/presets/dasp_analyst.md          # DASP 数据分析模板
   新增 configs/presets/dasp_qa.md               # DASP 问答模板
   修改 chat/src/components/MessageBubble.vue    # DASP 解析器
   ```

4. **优先级**: P1（高价值，前端改动中等）

---

### G2: Block 去重检测

**VCP 实现**:
```javascript
areBlocksEqual(a, b) {
  if (a.type !== b.type) return false;
  if (a.type === 'newline') return true;
  if (a.name !== b.name) return false;
  if (a.variants && b.variants) return JSON.stringify(a.variants) === JSON.stringify(b.variants);
  return a.content === b.content;
}
```
拖入仓库时检测重复，提示用户。

**UMMS 移植方案**:

在 `umms-api/src/prompt/engine.rs` 或 `types.rs` 中新增：

```rust
impl PromptBlock {
    pub fn content_eq(&self, other: &PromptBlock) -> bool {
        if self.block_type != other.block_type {
            return false;
        }
        if self.block_type == BlockType::Separator {
            return true;
        }
        if self.name != other.name {
            return false;
        }
        self.variants == other.variants
    }
}
```

在仓库 `POST` 端点中调用检测，返回 `409 Conflict` 或 warning 字段。

**优先级**: P2（防止数据膨胀，改动小）

---

### G3: 自定义模式名称

**VCP 实现**: 用户可以把"原始富文本"改名为"我的简单模式"等，双击编辑，右键长按重置。

**UMMS 移植方案**: 在 `PromptConfig`（`umms-core/config.rs`）中增加：

```rust
pub struct PromptConfig {
    // ... existing fields ...
    /// Custom display names for the three modes.
    pub mode_names: ModeNames,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ModeNames {
    pub original: String,   // default: "原始文本"
    pub modular: String,    // default: "模块化"
    pub preset: String,     // default: "预制模板"
}
```

**优先级**: P3（纯 UI 偏好，低价值）

---

### G4: Prompt Sponsor 远程控制 API

**VCP 实现**: 独立进程通过 STDIN/STDOUT JSON 协议操控 Prompt 系统，32 个命令覆盖全部操作。

**UMMS 移植方案**: UMMS 已有完整的 REST API（`handlers/prompts.rs` 提供 15+ 端点），覆盖了 VCP Prompt Sponsor 的全部功能。**无需移植**。

如果未来需要进程间通信（非 HTTP），可考虑通过 MCP Server（`rmcp`）暴露 prompt 工具：

```
Tool: get_prompt_config(agent_id) → AgentPromptConfig
Tool: set_prompt_mode(agent_id, mode) → Ok
Tool: add_block(agent_id, block_type, content) → block_id
Tool: preview_prompt(agent_id, vars) → rendered_string
```

**优先级**: P3（REST API 已覆盖，MCP 是增值项）

---

### G5: 预制模板目录可配置

**VCP 实现**: 用户可以在 UI 中切换 preset 文件扫描目录。

**UMMS 现状**: `PromptConfig.presets_dir` 已可通过 `umms.toml` 配置，但不支持运行时切换。

**移植方案**: 在 `AgentPromptConfig` 中增加 `preset_dir_override: Option<String>`，优先级高于全局配置。前端 preset 面板增加目录选择器。

**优先级**: P3（低频需求）

---

### G6: 群聊 Prompt 隔离

**VCP 实现**:
- 群聊中每个 Agent 保持独立的 system prompt
- 没有自定义 prompt 时 fallback 到 `你是${agentName}。`
- `{{AgentName}}` 在群聊调度时动态替换

**UMMS 移植方案**:

UMMS 架构天然支持——每个 Agent 有独立的 `AgentPromptConfig`（按 `agent_id` 隔离）。群聊场景需要：

1. **Chat Handler 支持 multi-agent 轮询**: 当前 `chat handler` 假设单 Agent 对话。群聊需要一个调度器决定哪个 Agent 回复。
2. **每轮构建对应 Agent 的 Prompt**: 调用 `PromptEngine::build_prompt(agent_config, vars)` 时传入当前 Agent 的 config。
3. **Fallback**: `PromptEngine::build_prompt` 在 `blocks` 为空时已 fallback 到空串，需改为 fallback 到 persona 的 `system_prompt`。

**文件变更**:
```
修改 crates/umms-api/src/handlers/chat.rs      # 多 Agent 调度
修改 crates/umms-api/src/prompt/engine.rs       # Fallback 逻辑
新增 crates/umms-api/src/handlers/group_chat.rs # 群聊专用 handler
```

**优先级**: P2（架构已支持，需要 handler 层实现）

---

### G7: 翻译专用 Prompt

**VCP 实现**: 独立模块，硬编码翻译 system prompt，与主对话 Prompt 互不干扰。

**UMMS 移植方案**: 不单独移植。UMMS 的模块化积木系统可以用一个 `Custom` 类型的 block 实现翻译指令，或创建一个专用的 "Translator" Agent persona。

**优先级**: P4（不移植，已有替代方案）

---

## 三、UMMS 独有优化项（非 VCP 移植）

### U1: Prompt 版本历史

**问题**: 当前 `PromptStore` 覆盖保存，无法回滚。

**方案**: 在 `agent_prompts` 表增加 `version` 列，保存时 INSERT 新行而非 UPDATE。提供 `GET /api/prompts/:id/history` 和 `PUT /api/prompts/:id/rollback/:version` 端点。

**优先级**: P2

---

### U2: Persona ↔ Prompt 同步

**问题**: `AgentPersona.system_prompt`（personas.sqlite）和 `AgentPromptConfig.blocks[0].content`（prompts.sqlite）可以脱节。

**方案**:
- 写入 Prompt 的 `System` block 时，同步更新 `AgentPersona.system_prompt`
- 或废弃 `AgentPersona.system_prompt` 字段，统一由 PromptEngine 管理
- **推荐后者**: 让 `PersonaStore` 不再存储 system_prompt，persona 只保留身份信息（name, role, expertise），prompt 全部由 `PromptStore` 管理

**优先级**: P1

---

### U3: 条件渲染 Block

**问题**: 当前 block 要么启用要么禁用，无法根据运行时条件动态决定。

**方案**: 在 `PromptBlock` 中增加 `condition: Option<String>` 字段，例如：
- `"memory_count > 0"` — 仅在有检索到记忆时启用
- `"diary_count > 0"` — 仅在有档案时启用
- `"agent_role == 'coder'"` — 仅对特定角色启用

PromptEngine 在构建时评估条件，跳过不满足条件的 block。

**优先级**: P2

---

### U4: Block Token 预算控制

**问题**: 各 block 的内容长度不受控，memory_content 或 history_content 可能撑爆 context window。

**方案**: 利用 `PromptSection.max_chars`（legacy 模板已有此字段）的思路，在 `PromptBlock` 中增加 `max_tokens: Option<usize>`。PromptEngine 在构建时截断超长 block 内容。

**优先级**: P1

---

## 四、实施优先级总览

| 优先级 | 编号 | 名称 | 来源 | 改动范围 | 预估工作量 |
|--------|------|------|------|----------|-----------|
| **P0** | U2 | Persona ↔ Prompt 同步 | UMMS 独有 | persona + prompt 模块 | 小 |
| **P1** | G1 | DASP 动态流式渲染 | VCP 移植 | 预制模板 + chat 前端 | 中 |
| **P1** | U4 | Block Token 预算控制 | UMMS 独有 | prompt engine | 小 |
| **P2** | G2 | Block 去重检测 | VCP 移植 | prompt types + handler | 小 |
| **P2** | G6 | 群聊 Prompt 隔离 | VCP 移植 | chat handler | 中 |
| **P2** | U1 | Prompt 版本历史 | UMMS 独有 | prompt store + API | 中 |
| **P2** | U3 | 条件渲染 Block | UMMS 独有 | prompt engine | 小 |
| **P3** | G3 | 自定义模式名称 | VCP 移植 | config + dashboard | 小 |
| **P3** | G4 | MCP Prompt 工具 | VCP 启发 | MCP server | 中 |
| **P3** | G5 | Preset 目录可配置 | VCP 移植 | config + API | 小 |
| **P4** | G7 | 翻译专用 Prompt | VCP | — | 不移植 |

## 五、VCP DASP 协议完整规范（附录）

### 5.1 指令格式

嵌入在 LLM streaming 输出的 HTML 注释中：

```
<!--AI_INSTRUCTIONS:{"action": [...]}-->
```

### 5.2 支持的 Action

| Action | 参数 | 效果 |
|--------|------|------|
| `hide` | `string[]` (element IDs) | 隐藏元素 (`display: none`) |
| `show` | `string[]` (element IDs) | 显示元素 (`display: block`) |
| `update` | `[{id, newContent}]` | 替换元素 innerHTML |
| `highlight` | `string[]` (element IDs) | 添加高亮 CSS class |
| `unhighlight` | `string[]` (element IDs) | 移除高亮 CSS class |
| `hideAll` | `boolean` | 隐藏所有 `.provisional-content` 元素 |

### 5.3 内容标记

```html
<span class="provisional-content" id="unique-id">初始内容</span>
```

### 5.4 典型使用模式

```
1. 输出带 ID 的临时内容
2. 执行实际工作
3. update 替换临时内容为最终结果
4. hide 隐藏不再需要的中间步骤
```

### 5.5 向后兼容

`<!--HIDE_PROVISIONAL_CONTENT-->` 等价于 `{"hideAll": true}`。

---

## 六、结论

UMMS 的三模式 Prompt 系统已经覆盖了 VCP 的核心功能，且在语义化 BlockType、默认积木预设、Diary 集成、记忆注入等方面超越了 VCP。主要差距在于：

1. **DASP 动态渲染**（VCP 的杀手级特性，需要前端配合）
2. **数据一致性**（Persona ↔ Prompt 脱节）
3. **运行时控制**（Token 预算、条件渲染）

这些优化项可以在 Phase 5（交互层）和 Phase 6（集成完善）中逐步实施。
