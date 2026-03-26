# ADR-015: VCP 项目经验借鉴

## 状态

已记录（2026-03-26），分散到各模块实施计划中。

## 背景

VCP (VCPChat) 是一个成熟的 AI 聊天客户端，其 DeepMemo 插件实现了聊天记忆检索。
通过分析 VCP 的实现，提取可借鉴的经验和需要避免的问题。

## VCP 架构概述

```
用户输入/文件上传
  ↓
文件处理: pdf-parse / mammoth / 直接读取
  ↓
聊天记忆: Jieba 分词 → Tantivy/FlexSearch 全文索引
  ↓
检索: 关键词匹配 → 滑动窗口取上下文(±N轮) → 去重
  ↓
可选: 外部 Rerank API (Qwen3-Reranker-8B)
  ↓
格式化输出 → 注入 LLM prompt
```

## 借鉴点与落地计划

### 1. 中文分词 → M3 检索优化

**VCP 做法**: 使用 jieba-rs 实现自定义 Tantivy Tokenizer，对中文进行精确分词。

**UMMS 当前问题**: BM25 使用 tantivy 默认分词器，对中文效果差（按字切分，无法识别词组）。

**落地**:
- 在 `umms-retriever/recall/bm25.rs` 中集成 jieba-rs 作为 tantivy tokenizer
- 参考 VCP 的 `JiebaTokenizer` 实现（F:/vcp/.../DeepMemo/src/main.rs:20-74）
- 阶段: M3 优化迭代

### 2. 滑动窗口上下文扩展 → M3 检索返回策略

**VCP 做法**: 检索命中第 N 条消息后，自动取 N±window_size 条作为上下文返回。

**UMMS 借鉴**: 检索命中 chunk_5 后，自动合并 chunk_4 + chunk_5 + chunk_6 返回。
等效于 Parent-Child 层级检索的简化版——不需要多层存储，只需要在返回时扩展。

**落地**:
- 在 `RetrievalPipeline` 中增加 `context_window` 参数
- 检索结果返回时，按 chunk index 向前后扩展
- 需要在 MemoryEntry 的 tags 中记录 `chunk:{index}` 和 `doc:{title}`（已实现）
- 阶段: M3 Sprint 3 或 M5 交互层

### 3. 外部 Rerank 服务 → M3 精排 Layer 2

**VCP 做法**: 对接外部 Rerank API（Qwen3-Reranker-8B），支持递归批量精排和锦标赛排序。

**UMMS 借鉴**: 当前 Rerank 只有 Layer 1（cosine 重算分），Layer 2 可以直接对接外部 Rerank 服务，
比自己训练/部署 cross-encoder 更快落地。

**落地**:
- 在 `umms.toml` 中增加 rerank 配置段
- 实现 `ExternalReranker` 调用外部 API
- 参考 VCP 的 token-aware batching 逻辑（处理大文档集时按 token 预算分批）
- 阶段: M3 优化迭代

### 4. 扫描型 PDF 处理 → M3 文档摄入

**VCP 做法**: 扫描型 PDF 用 pdf-poppler 转 JPEG，作为多模态内容发送给 LLM。

**UMMS 借鉴**: 扫描型 PDF 转图片后，可用 gemini-embedding-002（支持多模态）直接编码图片。
不需要 OCR 中间步骤。

**落地**:
- 在文档摄入管线中增加 PDF 类型检测（文本型 vs 扫描型）
- 文本型: pdf-parse 提取文字 → 分块 → 编码
- 扫描型: 转图片 → gemini-embedding-002 多模态编码
- 阶段: M3 Sprint 3

### 5. 去重逻辑 → M3/M4

**VCP 做法**: 滑动窗口重叠时用 HashSet 去重，防止同一条消息出现在多个回忆片段中。

**UMMS 已实现**: RRF 融合时已有去重（按 memory_id）。LIF 扩散也有去重（existing_ids）。
可进一步优化: 内容级去重（两条不同 ID 但内容相似的记忆只保留一条）。

**落地**:
- M4 巩固引擎中实现内容级去重（相似度 > 0.95 的记忆自动合并）
- 阶段: M4

### 6. 高级查询语法 → M5 交互层

**VCP 做法**: 支持精确短语 `"xxx"`、加权词 `(重要:1.5)`、排除词 `[闲聊]`、OR 组 `{A|B|C}`。

**UMMS 借鉴**: 当前只支持自然语言查询，可增加高级语法支持。

**落地**:
- 在查询解析层增加语法支持
- 用户可以混合自然语言和结构化查询
- 阶段: M5

## 不借鉴的点

| VCP 做法 | 不借鉴原因 |
|----------|-----------|
| 文档全文塞进 prompt | UMMS 需要持久化存储和长期检索，不能用完即弃 |
| FlexSearch 内存索引 | UMMS 已用 tantivy（更强大），不需要 FlexSearch |
| 无文档分块 | UMMS 必须分块才能做向量检索 |
| SHA256 文件去重 | UMMS 用 MemoryId（UUID），语义级去重比文件级更有价值 |

## 优先级排序

| 序号 | 借鉴项 | 价值 | 成本 | 阶段 |
|------|--------|------|------|------|
| 1 | Jieba 中文分词 | 高（中文检索质量翻倍） | 低（加一个 crate） | M3 优化 |
| 2 | 滑动窗口上下文扩展 | 高（解决 chunk 断裂问题） | 低 | M3 Sprint 3 |
| 3 | 外部 Rerank 服务 | 中（精排质量提升） | 中 | M3 优化 |
| 4 | 扫描型 PDF | 中（扩展输入类型） | 中 | M3 Sprint 3 |
| 5 | 高级查询语法 | 低（当前用户量少） | 中 | M5 |
| 6 | 内容级去重 | 中（数据质量） | 中 | M4 |
