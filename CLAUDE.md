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
| M1 存储引擎 | `umms-core` + `umms-storage` | 四级记忆层次 (L0 moka → L1 moka → L2 LanceDB → L3 SQLite+petgraph → L4 FS)，Agent 隔离，快照/恢复 |
| M2 编码服务 | `umms-encoder` + `umms-python` | 多模态→3072 维向量 (Gemini API 主路径 / ONNX 降级)，PyO3 FFI |
| M3 检索分析 | `umms-retriever` + `umms-analyzer` | 三阶段检索 (BM25+ANN 混合召回 → 残差精排 → LIF 认知扩散)，LGSRR 语义分解 |
| M4 巩固引擎 | `umms-consolidation` | WKD 压缩蒸馏，知识图谱演化，预测编码优化，四级遗忘衰减 |
| M5 交互层 | `umms-api` | HTTP REST (axum) + MCP Server (rmcp) + WebSocket + CLI (clap) |
| M6 可观测性 | `umms-observe` | metrics (prometheus-client) + tracing + dashboard |
| M7 Persona | `umms-persona` | Agent 身份卡 TOML 管理，模板渲染，权限校验 |

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
| 知识图谱 | CozoDB (Rust native, RocksDB) → 迁移中 | Datalog 查询，内置图算法+HNSW。当前 SQLite 过渡实现。ADR-003 |
| 缓存 | moka | 进程内并发缓存，TTI/TTL |
| HTTP 框架 | axum | tokio 原生，tower 中间件 |
| MCP | rmcp | Rust native MCP SDK |
| 文本编码 | Gemini Embedding API (3072d) | 在线 API，无本地 GPU 依赖 |
| Python 桥接 | PyO3 FFI (~5μs) | 远优于 HTTP 桥接 |
| 测试 | cargo nextest + criterion.rs + pytest | Rust + Python 全覆盖 |

## 目录结构

```
UMMS/
├── Cargo.toml                    # workspace
├── crates/
│   ├── umms-core/                # 核心类型、trait、错误类型
│   ├── umms-storage/             # 存储抽象层
│   ├── umms-encoder/             # 编码服务
│   ├── umms-retriever/           # 检索管道
│   ├── umms-analyzer/            # 语义分析
│   ├── umms-consolidation/       # 巩固引擎
│   ├── umms-persona/             # Agent Persona
│   ├── umms-api/                 # 接入层
│   ├── umms-observe/             # 可观测性
│   └── umms-python/              # PyO3 桥接
├── python/umms_ml/               # Python ML 模块
├── configs/                      # TOML 配置文件
├── migrations/                   # SQLite 迁移
├── dashboard/                    # Web Dashboard (可选)
├── tests/                        # 集成测试 + fixtures
├── benches/                      # criterion.rs benchmarks
└── docs/                         # 编码标准、ADR 等
```

## 工程原则（详见 docs/CODING_STANDARDS.md）

**原则零：第一性原理** — 写代码前先问：需求的本质是什么？最简单的正确解法是什么？如果我错了代价多大？

**原则一：一次做对** — 不写"先让它跑起来"的代码。写之前先画状态图，接口先于实现，假设你没有第二次修改机会。

**原则二：每个模块只知道它该知道的** — 接口即契约，实现即秘密。检验方法：把 LanceDB 换成 Qdrant，除 umms-storage 外应零修改。三个相似的东西才是抽象的时机（Rule of Three）。

**原则三：让错误的代码无法编译** — Newtype 防止参数混淆（AgentId ≠ SessionId ≠ String），Typestate 强制状态机转换顺序，`#[must_use]` 防止忽略重要返回值。

**原则四：显式优于隐式** — agent_id 是构造参数不是可选字段，副作用体现在函数名中，配置项有类型约束和边界检查。

**原则五：错误处理是第一公民** — 错误类型携带诊断上下文，区分可恢复和不可恢复，永远不吞掉错误。

**原则六：测试行为不测实现** — 测 "Agent B 不能看到 Agent A 的数据" 而不是 "调用了某个 mock 函数 1 次"。每个 bug fix 附带回归测试。

**原则七：代码写给三个月后的自己** — 注释解释 WHY 不解释 WHAT，用领域语言命名（`apply_forgetting_decay` 不是 `process_items`），ADR 记录放弃了什么。

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

## 计划文档位置

总体计划和各模块详细计划位于 `F:\Research\大模型记忆\`:
- `UMMS_project_master_plan.md` — 总体项目计划 (v2.1.0)
- `UMMS_M1_storage_engine.md` ~ `UMMS_M7_persona_engine.md` — 各模块详细计划
- `technical_architecture_universal_memory_system.md` — 企业版技术架构 (参考)
- `llm_memory_engineering_implementation.md` — 认知科学理论基础
- `research_report_universal_memory_system.md` — 研究报告

## 开发顺序

```
Phase 1 (W1-4):   M1 存储引擎 + M6 可观测性基础  ← 核心完成 (55 tests)
Phase 1.5 (W3-5): M7 Persona 引擎 (与 M1 后期并行)
Phase 2 (W5-8):   M2 编码服务
Phase 3 (W9-14):  M3 检索与分析
Phase 4 (W15-18): M4 巩固与自进化
Phase 5 (W19-24): M5 接入与交互层
Phase 6 (W25-28): M6 完善 + 集成测试 + 发布
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

每个模块开发时**必须同步更新** dashboard（`crates/umms-devserver` + `dashboard/index.html`）：

| 模块 | Dashboard 需要新增的内容 |
|------|------------------------|
| M2 编码服务 | 编码统计面板：API/本地调用次数、平均延迟、fallback 触发次数 |
| M3 检索分析 | 检索面板：QPS、平均延迟、召回/精排/扩散各阶段耗时 |
| M4 巩固引擎 | 巩固面板：上次运行时间、压缩/衰减/提升数量、下次触发时间 |
| M5 交互层 | 会话面板：活跃会话数、Agent 切换次数；替代 devserver 成为正式服务 |
| M7 Persona | Agent 面板：每个 Agent 的身份卡摘要、记忆策略、权限配置 |
| CozoDB 集成 | 图谱面板升级：Datalog 查询耗时、图演化统计 |

**验收标准**：新模块的 PR 如果没有附带 dashboard 更新，视为未完成。

### Dev Server

```bash
# 启动（需要 PROTOC 环境变量）
cargo run -p umms-devserver
# 访问 http://127.0.0.1:8720
# Seed 测试数据: GET /api/demo/seed
```

## 写代码前的自检

- 如果删掉所有注释，新人仅通过类型签名和函数名能理解它在做什么吗？
- 如果底层存储换了，存储层之外的代码需要改吗？（不应该）
- 有没有接受 String 参数而它其实应该是更具体的类型？
- 输入极端值（空串、空数组、MAX）会发生什么？
- 有没有隐式顺序依赖？能用类型系统强制吗？
- 三个月后看到这段代码，30 秒内能理解它为什么存在吗？
- **这个模块的状态能在 dashboard 上看到吗？**
