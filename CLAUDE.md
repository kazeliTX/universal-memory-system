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
| 知识图谱 | SQLite WAL + petgraph (内存图) | 持久化+热数据，2-3 跳遍历 <1ms |
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

## 编码约定

### Rust
- Edition 2024, MSRV 1.80+
- `#![deny(clippy::all, clippy::pedantic)]` — 所有 crate
- `#![allow(clippy::module_name_repetitions)]` — 允许模块名重复
- 所有 `pub` API 必须有 doc comment
- 错误类型统一使用 `thiserror` 定义在 `umms-core`
- async 运行时统一 `tokio`，不混用 `async-std`
- 使用 `tracing` 进行结构化日志，不用 `println!` 或 `log`
- 所有可失败操作返回 `Result<T, UmmsError>`，禁止 `unwrap()` 在非测试代码中
- 命名：snake_case 函数/变量，CamelCase 类型，SCREAMING_SNAKE_CASE 常量
- 文件不超过 500 行，超过则拆分子模块

### Python (python/umms_ml/)
- Python 3.10+
- 类型注解 (type hints) 全覆盖
- 使用 ruff 格式化和 lint
- 使用 pytest 测试

### Git
- 分支策略: main (稳定) + feature/* + fix/* + refactor/*
- Commit message: Conventional Commits (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `perf:`, `chore:`)
- PR 合入前必须 CI 通过 (clippy + fmt + test)

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
Phase 1 (W1-4):   M1 存储引擎 + M6 可观测性基础
Phase 1.5 (W3-5): M7 Persona 引擎 (与 M1 后期并行)
Phase 2 (W5-8):   M2 编码服务
Phase 3 (W9-14):  M3 检索与分析
Phase 4 (W15-18): M4 巩固与自进化
Phase 5 (W19-24): M5 接入与交互层
Phase 6 (W25-28): M6 完善 + 集成测试 + 发布
```

## 关键设计决策速查

- **隔离键**: 所有存储操作必须携带 `agent_id`，查询时必须过滤
- **共享层写入**: 只有巩固服务可自动写入共享层 (importance > 0.7 + 跨 Agent 引用 ≥ 2 + 存活 > 24h)
- **编码降级**: Gemini API 超时 3s → 自动切本地 ONNX
- **GIL 处理**: PyO3 调用必须用 `Python::allow_threads` 释放 GIL
- **panic 防护**: 各模块入口 `catch_unwind` 隔离
- **遗忘衰减**: 4 级 (task λ=0.5 / session λ=0.1 / preference λ=0.01 / domain λ=0.001)
