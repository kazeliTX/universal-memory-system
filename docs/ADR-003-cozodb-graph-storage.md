# ADR-003: 用 CozoDB 替代 SQLite+petgraph 作为知识图谱存储

**状态**: 已决策（待实施）
**日期**: 2026-03-25
**决策者**: kazeli

## 背景

UMMS 的知识图谱（L3 语义记忆）当前使用 SQLite + petgraph 实现。随着 M4 巩固引擎的需求明确（节点合并、社区检测、关系推断等），手写图操作的维护成本和 bug 风险越来越高。

## 决策

用 CozoDB（纯 Rust 嵌入式图数据库，Datalog 查询）替代当前的 SQLite 图谱实现。

## 选择 CozoDB 的理由

- **纯 Rust**：`cargo add cozo` 直接使用，无 C++ FFI
- **Datalog 查询**：多跳遍历、模式匹配、递归查询天然支持，不用手写 BFS
- **内置图算法**：PageRank、社区检测、最短路径、BFS 等 20+ 算法
- **内置 HNSW 向量搜索**：未来可能统一图谱+向量存储，减少一个依赖 (LanceDB)
- **API 极简**：`DbInstance::new()` + `run_script()` 两个方法
- **性能**：10K-100K 节点范围内 2 跳遍历亚毫秒级

## 放弃的方案

| 方案 | 放弃原因 |
|------|---------|
| 保持 SQLite+petgraph | M4 会被迫绕过 trait 直接操作 SQL，破坏抽象，提高迁移成本 |
| Kùzu 嵌入式图数据库 | **2025.10 已归档 (archived)**，不建议新项目引入 |
| Neo4j / SurrealDB | 需要独立服务进程，不符合单进程单体的设计约束 |
| 去掉独立图谱用向量+LLM 替代 | 牺牲结构化推理能力，LIF 认知扩散失去图拓扑基础 |

## 已知风险

| 风险 | 缓解 |
|------|------|
| CozoDB 最后发版 2023.12，单人维护者活跃度下降 | 纯 Rust 代码库，fork 维护成本可控 |
| Datalog 学习曲线 | 2-3 天可上手，语法逻辑性强 |
| SQLite 后端写性能差 (<100 QPS) | 使用 RocksDB 后端 (100K+ QPS) |
| 现有 SQLite 图谱数据不兼容 | 需要一次性数据迁移脚本 |
| HNSW 增量索引行为不明确 | 先仅用于图谱，向量存储继续用 LanceDB，后续评估统一 |

## 实施计划

1. **Phase A**: 在 `umms-storage` 中新增 `graph/cozo_store.rs`，实现 `KnowledgeGraphStore` trait
2. **Phase B**: 编写数据迁移脚本（SQLite → CozoDB）
3. **Phase C**: 切换默认实现，保留 SQLite 实现作为 fallback (feature flag)
4. **Phase D**: 评估 CozoDB HNSW 是否可替代 LanceDB

## 回退条件

如果 CozoDB 在使用中遇到以下情况，回退到 SQLite+petgraph：
- Datalog 查询引擎存在无法绕过的 bug
- RocksDB 后端在 Windows 上编译/运行问题严重
- 性能在目标规模下不达标 (>10ms per 2-hop traversal)
