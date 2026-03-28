# UMMS — Universal Multimodal Memory System (个人版)

## 项目概述

UMMS 是一个面向个人 AI Agent 生态的类人记忆系统。作为本地记忆中间件运行，支持多模态输入（文本/图像/音频）的编码、存储、检索、联想和自进化。核心设计原则：**交互体验优先 · 记忆隔离与复用并存 · 单人可维护**。

- **规模定位**: <100 用户（通常 1 人），不考虑大规模部署
- **技术栈**: Rust (tokio async runtime) 单进程单体，PyO3 桥接 Python ML
- **部署形态**: 单二进制 / Docker，无 GPU 可运行

## 架构

### 七大模块

| 模块 | Crate | 职责 |
|------|-------|------|
| M1 存储引擎 | `umms-core` + `umms-storage` | 四级记忆层次 (L0 moka → L1 moka → L2 LanceDB → L3 CozoDB → L4 FS)，Agent 隔离，快照/恢复 |
| M2 编码服务 | `umms-encoder` + `umms-model` | 多模态→3072 维向量 (Gemini API)，统一 LLM 服务层 (ModelPool) |
| M3 检索分析 | `umms-retriever` + `umms-analyzer` | 三阶段检索 (BM25+ANN 混合召回 → 残差精排 → LIF 认知扩散)，LGSRR 语义分解，EPA 嵌入投影分析 |
| M4 巩固引擎 | `umms-consolidation` + `umms-scheduler` | WKD 压缩蒸馏，知识图谱演化，预测编码优化，四级遗忘衰减，定时调度 |
| M5 交互层 | `umms-api` + `umms-server` | HTTP REST (axum) + MCP Server (rmcp) + WebSocket + Chat Client (Vue 3) |
| M6 可观测性 | `umms-observe` | metrics (prometheus-client) + tracing + Dashboard (Vue 3 + Naive UI) |
| M7 Persona | `umms-persona` | Agent 身份卡 TOML 管理，VCP 三模式 Prompt 系统，模板渲染 |

### 依赖拓扑

```
M1 (存储) ← 基础层
  ↑
M7 (Persona) ← 依赖 M1
  ↑
M2 (编码) ← 依赖 M1
  ↑
M3 (检索) ← 依赖 M1 + M2 + M7
  ↑
M4 (巩固) ← 依赖 M1 + M2 + M3
  ↑
M5 (交互) ← 依赖 M1-M4 + M7
M6 (可观测) ← 横切所有模块
```

### 记忆隔离模型

三层隔离：
- **Layer 1 Private Context**: 每个 Agent 的对话历史、L0/L1 缓存、执行状态、预测模型 — 完全隔离，切换时快照冻结
- **Layer 2 Shared Knowledge**: 用户偏好、通用知识、知识图谱 (agent_id=NULL) — 所有 Agent 可读，写入需巩固服务提升
- **Layer 3 External Memory**: 知识库文档、文件附件 — 按权限配置选择性共享

Agent 切换流程: snapshot(A) → clean(L0/L1) → restore_or_coldstart(B)

## 技术选型

| 组件 | 选型 | 理由 |
|------|------|------|
| 主语言 | Rust (tokio, Edition 2024, MSRV 1.80+) | 单进程高性能，内存安全 |
| 向量数据库 | LanceDB (Rust native) | 嵌入式零部署，100万向量 <500MB 内存 |
| 知识图谱 | CozoDB (Rust native, RocksDB) | Datalog 查询，内置图算法+HNSW。ADR-003 |
| 缓存 | moka | 进程内并发缓存，TTI/TTL |
| HTTP 框架 | axum | tokio 原生，tower 中间件 |
| MCP | rmcp | Rust native MCP SDK |
| 文本编码 | Gemini Embedding API (3072d) | 在线 API，无本地 GPU 依赖 |
| LLM 服务 | umms-model (ModelPool) | 统一 Gemini/本地模型管理，trace 系统 |
| 前端 | Vue 3 + TypeScript + Naive UI | Dashboard 和 Chat Client 独立前端应用 |
| 测试 | cargo nextest + criterion.rs | Rust 全覆盖 |

## 目录结构

```
UMMS/
├── Cargo.toml                    # workspace
├── crates/
│   ├── umms-core/                # 核心类型、trait、错误类型
│   ├── umms-storage/             # 存储抽象层 (LanceDB + CozoDB)
│   ├── umms-encoder/             # 编码服务
│   ├── umms-model/               # 统一 LLM 服务层 (ModelPool)
│   ├── umms-retriever/           # 检索管道
│   ├── umms-analyzer/            # 语义分析 (EPA + LGSRR)
│   ├── umms-consolidation/       # 巩固引擎
│   ├── umms-scheduler/           # 定时调度服务
│   ├── umms-persona/             # Agent Persona
│   ├── umms-api/                 # API 路由定义
│   ├── umms-server/              # 独立 HTTP 服务 (headless)
│   ├── umms-observe/             # 可观测性
│   └── umms-python/              # PyO3 桥接
├── chat/                         # Chat Client (Vue 3 + TypeScript)
├── dashboard/                    # Dashboard (Vue 3 + Naive UI)
├── configs/                      # TOML 配置文件 + Agent Persona
├── migrations/                   # SQLite 迁移
├── tests/                        # 集成测试 + fixtures
├── benches/                      # criterion.rs benchmarks
└── docs/                         # 编码标准、ADR、架构反思
```

## 工程原则

完整原则见 `docs/CODING_STANDARDS.md`。核心摘要：

1. **第一性原理** — 先问本质，再写代码
2. **一次做对** — 接口先于实现，画状态图再编码
3. **模块隔离** — 换 LanceDB→Qdrant 只改 umms-storage；Rule of Three 才抽象
4. **类型安全** — Newtype ID 防混淆，Typestate 防状态越迁
5. **显式优于隐式** — agent_id 是必填构造参数，非可选字段
6. **错误是第一公民** — 携带诊断上下文，区分可恢复/不可恢复
7. **测行为不测实现** — 测 "B 看不到 A 的数据"，每个 bugfix 附带回归测试
8. **代码写给三个月后的自己** — 注释解释 WHY，用领域语言命名

## 性能目标

| 操作 | P99 目标 |
|------|---------|
| L0/L1 缓存读写 | <1μs |
| LanceDB 插入 | <5ms |
| LanceDB ANN 查询 (100K) | <30ms |
| 完整检索管道 (100K) | <100ms |
| Agent 切换 | <500ms |
| HTTP API 端到端 | <200ms |
| 系统冷启动 | <3s |

## 常用命令

```bash
# 开发
cargo build                          # 构建
cargo nextest run                    # 测试
cargo clippy -- -D warnings          # lint
cargo fmt --check                    # 格式检查
cargo bench --bench '*'              # 性能基准

# 运行
cargo run -- serve                   # 启动服务 (开发模式)
cargo run -- serve --config ./configs/default.toml

# Python
cd python && pytest tests/ -v        # Python 测试
ruff check umms_ml/                  # Python lint

# 发布
cargo build --release                # Release 构建
```

## 文档索引

### 项目内文档 (`docs/`)

| 文档 | 类型 | 说明 |
|------|------|------|
| `CODING_STANDARDS.md` | 工程规范 | 八大原则 + 检验清单（必读） |
| `AUDIT_2026-03-28.md` | 审计报告 | 全量代码审计，3 P0 + 5 P1 + 12 P2 问题 |
| `OPT-001-vcp-prompt-system-migration.md` | 优化计划 | VCP Prompt 系统移植分析，7 项待移植能力 |
| `OPT-002-epa-algorithm-evolution.md` | 优化计划 | EPA 算法演进 6 个 Sprint (E1-E6) |
| `ADR-003-cozodb-graph-storage.md` | 架构决策 | CozoDB 替代 SQLite+petgraph |
| `ADR-014-frontend-backend-separation.md` | 架构决策 | 三进程架构 |
| `ADR-015-vcp-lessons-learned.md` | 架构决策 | VCP 项目 6 个借鉴点 |
| `ADR-016-tagmem-wave-algorithm-reference.md` | 架构决策 | Tagmem 浪潮算法借鉴分析 |
| `architecture/LGSRR_EPA架构反思.md` | 架构反思 | LGSRR 与 EPA 职责打通 |

### 外部计划文档 (`F:\Research\大模型记忆\`)

- `UMMS_project_master_plan.md` — 总体项目计划 (v2.1.0)
- `UMMS_M1_storage_engine.md` ~ `UMMS_M7_persona_engine.md` — 各模块详细计划

## 开发顺序

```
Phase 1 (W1-4):   M1 存储引擎 + M6 可观测性基础       ✅ 完成 (55 tests)
Phase 1.5 (W3-5): M7 Persona 引擎                     ✅ 完成 (VCP 三模式 Prompt)
Phase 2 (W5-8):   M2 编码服务 + umms-model             ✅ 完成 (ModelPool + 多模态编码)
Phase 3 (W9-14):  M3 检索与分析                        ✅ 完成 (EPA + LGSRR + LIF 扩散)
Phase 4 (W15-18): M4 巩固与自进化                      🔧 进行中 (WKD 蒸馏 + 遗忘衰减 + 图谱演化)
Phase 5 (W19-24): M5 接入与交互层                      🔧 进行中 (Chat + Session + Prompt 编辑器 + 统一 API 错误处理)
Phase 6 (W25-28): M6 完善 + 集成测试 + 发布            ⏳ 待开始
```

## 系统不变量（违反任何一条都是 bug）

1. **agent_id 是隔离的唯一保证** — 每个接触存储的函数签名中必须有 agent_id。没有 agent_id 的存储操作必须是明确操作共享层的，且函数名体现这一点。
2. **记忆层级晋升是单向阀门** — L0→L1→L2→L3 不可逆。可以删除，不能降级。
3. **巩固服务是共享层的唯一自动写入者** — 只有巩固服务（自动）或 promote API（手动）可以写入共享层。任何绕过的代码都是 bug。
4. **编码降级必须对调用方透明** — 上层不关心当前用的 Gemini 还是 ONNX。维度适配是编码模块内部的事。
5. **切换 Agent ≠ 杀死 Agent** — 切换是"挂起+恢复"，不是"销毁+重建"。设计任何 Agent 功能时问：切换后恢复，状态还在吗？
6. **图谱后端可替换** — 上层模块只依赖 KnowledgeGraphStore trait，图谱实现更换（SQLite→CozoDB→其他）对 M3/M4/M5 零影响。

## 开发约定

### Dashboard 同步更新规则

每个模块开发时**必须同步更新** Dashboard（`dashboard/` Vue 3 应用）和后端 API（`umms-server`）。

**验收标准**：新模块的 PR 如果没有附带 dashboard 更新，视为未完成。

### 三进程架构（ADR-014）

```
进程 1: umms-server (Rust Axum) — 核心服务，headless 可运行
进程 2: dashboard/ (Vue 3 + Naive UI) — 管理/监控/配置
进程 3: chat/ (Vue 3) — 用户日常交互入口
```

```bash
# 启动核心服务
cargo run -p umms-server
# Dashboard 开发
cd dashboard && npm run dev
# Chat Client 开发
cd chat && npm run dev
```

## 写代码前的自检

详细清单见 `docs/CODING_STANDARDS.md` 末尾。最关键的两条：
- 换存储后端，存储层之外的代码需要改吗？（不应该）
- **这个模块的状态能在 dashboard 上看到吗？**

## 已知问题（2026-03-28 审计）

详见 `docs/AUDIT_2026-03-28.md`。关键待修复项：
- **P0-1**: 3 个 API 端点 agent_id 默认 "coder"，绕过隔离（违反不变量 #1）
- **P0-2/P0-3**: 巩固 API 和图谱查询返回 500
- **P1-4**: Handler 错误类型为 String，需统一 ApiError 枚举
