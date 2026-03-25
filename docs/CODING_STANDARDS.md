# UMMS 编码标准

本文档定义 UMMS 项目的编码规范、架构约束和质量要求。所有贡献代码必须遵循此标准。

---

## 1. Rust 编码规范

### 1.1 编译器与工具链

- **Rust Edition**: 2024
- **MSRV (最低支持版本)**: 1.80+
- **Async Runtime**: 统一 `tokio`，禁止混用 `async-std`
- **格式化**: `rustfmt` (项目根目录 `rustfmt.toml` 配置)
- **Lint**: `clippy` 严格模式

每个 crate 的 `lib.rs` 或 `main.rs` 顶部必须包含：
```rust
#![deny(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]  // 待项目稳定后移除
```

### 1.2 命名规范

| 元素 | 风格 | 示例 |
|------|------|------|
| 函数/方法 | snake_case | `fn encode_text()` |
| 变量/参数 | snake_case | `let agent_id` |
| 类型/结构体/枚举 | CamelCase | `struct MemoryEntry` |
| 枚举变体 | CamelCase | `Modality::Text` |
| 常量 | SCREAMING_SNAKE_CASE | `const MAX_RETRIES: u32` |
| Trait | CamelCase + 动词/形容词 | `trait Encodable`, `trait MemoryStorage` |
| Crate | kebab-case | `umms-storage` |
| 模块 | snake_case | `mod lance_store` |
| Feature flag | kebab-case | `local-encoder` |

### 1.3 错误处理

**统一错误类型**：所有错误定义在 `umms-core` 中，使用 `thiserror` 派生：

```rust
// umms-core/src/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UmmsError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Encoding error: {0}")]
    Encoding(#[from] EncodingError),

    #[error("Retrieval error: {0}")]
    Retrieval(#[from] RetrievalError),

    // ... 各模块子错误
}
```

**规则**：
- 所有可失败操作返回 `Result<T, UmmsError>` 或模块级子错误
- **禁止** 在非测试代码中使用 `unwrap()` 或 `expect()`
- 使用 `?` 操作符传播错误，不手动 match + return Err
- 外部库错误通过 `#[from]` 或 `map_err` 转换为内部错误类型
- 测试代码中可使用 `unwrap()` / `expect()` / `#[should_panic]`

### 1.4 异步编程

```rust
// 正确：使用 tokio 原语
use tokio::sync::{mpsc, RwLock, Mutex};
use tokio::time::{sleep, timeout};

// 错误：不要使用 std 同步原语在 async 上下文
// use std::sync::Mutex; // 会阻塞 tokio 线程

// CPU 密集任务必须使用 spawn_blocking
let result = tokio::task::spawn_blocking(move || {
    heavy_computation()
}).await?;

// PyO3 调用必须释放 GIL
Python::with_gil(|py| {
    py.allow_threads(|| {
        // Rust 代码在这里运行，GIL 已释放
    })
});
```

**async trait 规则**：
- 使用 `#[async_trait]` 宏（来自 `async-trait` crate）
- 所有 async trait 要求 `Send + Sync` bound

### 1.5 日志与追踪

统一使用 `tracing` crate，不使用 `println!`、`eprintln!` 或 `log` crate：

```rust
use tracing::{info, warn, error, debug, trace, instrument};

// 函数级追踪（自动记录参数和返回值）
#[tracing::instrument(skip(self, vector), fields(agent_id = %query.agent_id))]
async fn search(&self, query: &MemoryQuery, vector: &[f32]) -> Result<Vec<MemoryEntry>> {
    debug!(top_k = query.top_k, "Starting memory search");

    // 业务逻辑...

    info!(result_count = results.len(), "Search completed");
    Ok(results)
}
```

**日志级别指南**：
| 级别 | 用途 | 示例 |
|------|------|------|
| `error!` | 需要关注的错误 | 存储连接失败、API 调用异常 |
| `warn!` | 非致命但异常 | fallback 到本地模型、缓存未命中率高 |
| `info!` | 重要业务事件 | Agent 切换、巩固完成、服务启动 |
| `debug!` | 开发调试信息 | 查询参数、中间结果 |
| `trace!` | 详细执行追踪 | 每次缓存访问、每条记忆评分 |

### 1.6 文件组织

- **单文件不超过 500 行**：超过则拆分为子模块
- **每个 crate 的 `lib.rs`**：只做 re-export 和模块声明，不放业务逻辑
- **模块文件结构**：

```
crate-name/src/
├── lib.rs          # pub mod 声明 + re-export
├── traits.rs       # 公共 trait 定义
├── types.rs        # 公共类型定义
├── feature_a/
│   ├── mod.rs      # 子模块入口
│   ├── impl_x.rs   # 具体实现
│   └── impl_y.rs
└── feature_b/
    └── ...
```

### 1.7 文档注释

```rust
/// 所有 pub 项必须有 doc comment
///
/// 包含：
/// - 功能描述（一句话总结）
/// - 参数说明（如果不显而易见）
/// - 返回值说明
/// - 错误条件
/// - 使用示例（复杂 API 必须有）
///
/// # Examples
///
/// ```rust
/// let store = MemoryStore::new(config).await?;
/// let id = store.write(entry).await?;
/// ```
///
/// # Errors
///
/// Returns `StorageError::ConnectionFailed` if the database is unreachable.
pub async fn write(&self, entry: MemoryEntry) -> Result<MemoryId> {
    // ...
}

// 内部函数可以省略 doc comment，但复杂逻辑需要行内注释
fn calculate_decay_score(importance: f32, lambda: f32, hours: f32) -> f32 {
    // Exponential decay: score = importance * e^(-λ * t)
    importance * (-lambda * hours).exp()
}
```

---

## 2. 架构约束

### 2.1 模块依赖规则

严格遵循依赖拓扑，**禁止循环依赖**：

```
umms-core         → 无依赖（纯类型定义）
umms-storage      → umms-core
umms-persona      → umms-core, umms-storage
umms-encoder      → umms-core, umms-storage
umms-retriever    → umms-core, umms-storage, umms-encoder
umms-analyzer     → umms-core
umms-consolidation→ umms-core, umms-storage, umms-encoder, umms-retriever
umms-api          → 所有 crate
umms-observe      → umms-core (trait 层)
umms-python       → umms-core, umms-encoder
```

**规则**：
- 下层模块不可依赖上层模块
- 模块间通过 trait（定义在 `umms-core`）解耦
- 具体实现通过泛型或 `dyn Trait` 注入

### 2.2 Agent 隔离不变量

以下不变量在任何代码路径中都必须成立：

1. **所有存储写入必须携带 `agent_id`**
2. **所有存储查询必须过滤 `agent_id`**（除非显式设置 `include_shared = true`）
3. **Agent A 的私有数据永远不可被 Agent B 直接访问**
4. **共享层写入只允许通过巩固服务（自动）或显式 promote API（手动）**
5. **Agent 切换时必须执行 snapshot → clean → restore 三步骤**

### 2.3 Panic 防护

每个模块的顶层入口点必须 `catch_unwind`：

```rust
// 正确：模块入口有 panic 防护
pub async fn handle_request(req: Request) -> Response {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // 实际处理逻辑
    })) {
        Ok(result) => result,
        Err(_) => {
            error!("Panic caught in request handler");
            Response::internal_error()
        }
    }
}
```

### 2.4 配置管理

- 所有可配置参数通过 `configs/default.toml` 管理
- 支持环境变量覆盖：`UMMS_SERVER_PORT=8720`（前缀 `UMMS_`，`_` 分隔层级）
- 配置优先级：命令行参数 > 环境变量 > 配置文件 > 硬编码默认值
- **禁止**在代码中硬编码魔法数字，必须定义为常量或配置项

---

## 3. 测试规范

### 3.1 测试组织

```
crates/umms-xxx/
├── src/
│   └── feature.rs          # 底部可放 #[cfg(test)] mod tests
└── tests/
    └── integration_test.rs  # 集成测试

tests/                       # 项目级集成测试
├── fixtures/                # 测试数据
│   ├── memory_entries.json
│   └── agent_configs/
└── integration/
    └── e2e_test.rs
```

### 3.2 测试命名

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // 命名格式: test_{被测功能}_{场景}_{期望结果}
    #[test]
    fn test_decay_score_with_zero_hours_returns_full_importance() { ... }

    #[tokio::test]
    async fn test_agent_switch_cleans_l0_cache() { ... }

    #[test]
    #[should_panic(expected = "agent_id must not be empty")]
    fn test_write_with_empty_agent_id_panics() { ... }
}
```

### 3.3 覆盖率要求

| 层级 | 目标 | 工具 |
|------|------|------|
| 单元测试 | ≥ 80% 行覆盖 | cargo-llvm-cov |
| 集成测试 | ≥ 60% | cargo nextest |
| 性能基准 | 每模块 ≥ 3 个 benchmark | criterion.rs |

### 3.4 性能测试

```rust
// benches/storage_bench.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_l0_cache_write(c: &mut Criterion) {
    c.bench_function("l0_cache_write", |b| {
        b.iter(|| {
            // benchmark body
        })
    });
}

criterion_group!(benches, bench_l0_cache_write);
criterion_main!(benches);
```

---

## 4. Git 工作流

### 4.1 分支策略

```
main              ← 稳定分支，CI 全绿才能合入
├── feature/M1-xx ← 功能开发（按看板任务 ID）
├── fix/issue-xx  ← Bug 修复
├── refactor/xxx  ← 重构
└── perf/xxx      ← 性能优化
```

### 4.2 Commit 规范

遵循 [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

| Type | 用途 |
|------|------|
| `feat` | 新功能 |
| `fix` | Bug 修复 |
| `refactor` | 重构（不改变外部行为） |
| `perf` | 性能优化 |
| `test` | 测试相关 |
| `docs` | 文档 |
| `chore` | 构建/工具/依赖变更 |

Scope 使用模块标识：`storage`, `encoder`, `retriever`, `analyzer`, `consolidation`, `persona`, `api`, `observe`, `core`

示例：
```
feat(storage): implement LanceDB vector insert and ANN query
fix(encoder): handle Gemini API timeout with local fallback
refactor(core): extract MemoryEntry builder pattern
perf(retriever): optimize BM25 index update for incremental writes
test(storage): add agent isolation integration tests
```

### 4.3 PR 检查清单

合入 main 前必须满足：
- [ ] `cargo fmt --check` 通过
- [ ] `cargo clippy -- -D warnings` 通过
- [ ] `cargo nextest run` 全部通过
- [ ] 新增 pub API 有 doc comment
- [ ] 涉及存储操作的代码包含 agent_id 隔离
- [ ] 无 `unwrap()` 在非测试代码中
- [ ] 性能敏感路径有 benchmark

---

## 5. 依赖管理

### 5.1 核心依赖版本锁定

在 workspace `Cargo.toml` 中统一管理依赖版本：

```toml
[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
anyhow = "1"              # 仅用于 bin/tests，库代码用 thiserror
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
async-trait = "0.1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
toml = "0.8"
```

### 5.2 依赖引入原则

- 优先使用 Rust 生态已验证的 crate，不重复造轮子
- 引入新依赖前评估：维护活跃度、依赖树大小、是否 `unsafe`
- 尽量使用 feature flag 控制可选依赖（如 `local-encoder` 控制 ONNX runtime）
- 禁止引入仅用于一处的大型依赖

---

## 6. 安全规范

### 6.1 数据安全

- API Key 等敏感信息只通过环境变量传入，禁止硬编码或写入配置文件
- 用户数据存储在 `~/.umms/` 下，目录权限 700
- SQLite 数据库文件权限 600
- 不在日志中输出完整向量数据或用户原始内容

### 6.2 输入校验

- 所有外部输入（HTTP 请求、MCP 工具参数、CLI 参数）必须校验
- agent_id 只允许 `[a-zA-Z0-9_-]`，长度 1-64
- 文本输入长度上限 100KB（可配置）
- 文件上传大小上限 50MB（可配置）

### 6.3 依赖安全

- 定期运行 `cargo audit` 检查已知漏洞
- CI 中集成 `cargo deny` 检查 license 和安全
