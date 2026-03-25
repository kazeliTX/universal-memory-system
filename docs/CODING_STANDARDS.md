# UMMS 工程原则

这不是一份风格指南（rustfmt 和 clippy 会管那些事）。这是一份**思维约束**——在写每一行代码之前，用来检验自己有没有在做正确的事。

---

## 原则零：第一性原理

写代码前先问三个问题：
1. **这个需求的本质是什么？** — 不是"调用方让我加个参数"，而是"他到底想解决什么问题"。
2. **最简单的正确解法是什么？** — 不是最炫的、最通用的，而是刚好解决问题、不多不少的。
3. **如果我错了，代价有多大？** — 如果代价高，就多花时间确认；如果代价低，就快速试错。

**反面案例**：用户说"检索需要支持按时间范围过滤"。表面需求是加个 `created_after` / `created_before` 参数。第一性原理思考：用户真正想做的是"只看最近的相关记忆"。也许更好的方案是在相关性评分中加入时间衰减因子，而不是硬过滤。

---

## 原则一：一次做对，拒绝"先实现再优化"

"先让它跑起来"这句话杀死了无数代码库。临时方案一旦上线，就永远不会被替换——因为总有更紧急的事。

**实践**：
- **写之前先画状态图**。UMMS 的核心复杂度在状态转换：记忆从 L0→L1→L2→L3 的晋升、Agent 切换时的快照→清空→恢复。如果你画不清楚状态图，说明你还没理解要做什么，这时候写出来的代码一定会改。
- **接口先于实现**。先写 trait 签名和 doc comment，让调用方 review，确认这是他们需要的契约。实现可以慢慢来，但接口一旦发布就很难改。
- **写代码时假设你不会有第二次机会修改它**。这不是说追求完美，而是说每个函数、每个数据结构，在写的时候就要想清楚它的边界和职责。

**信号（你可能做错了）**：
- 代码里有 `// TODO: fix later` — 大概率不会 fix。要么现在解决，要么开 issue 跟踪。
- 写了一个函数超过 50 行 — 它可能在做不止一件事。
- 测试需要 mock 超过 3 个依赖 — 说明被测代码耦合太紧。

---

## 原则二：每个模块只知道它该知道的

耦合的根源不是 `use` 语句太多，而是**知识泄漏**——一个模块知道了另一个模块的内部细节。

### 2.1 接口即契约，实现即秘密

```rust
// 错误：检索模块知道了 LanceDB 的过滤语法
fn search(query: &str, lance_filter: &str) -> Vec<Memory> { ... }

// 正确：检索模块只知道"我需要过滤条件"，不知道底层怎么实现
fn search(query: &str, filter: MetadataFilter) -> Vec<Memory> { ... }
// MetadataFilter 是 umms-core 中的领域类型，与任何存储后端无关
```

**核心检验方法**：假设明天要把 LanceDB 换成 Qdrant——除了 `umms-storage` 内部，其他所有 crate 的代码应该**零修改**。如果做不到，说明存储细节泄漏了。

### 2.2 数据流向决定依赖方向

数据应该从上层流向下层，而不是反过来。如果你发现下层模块需要"回调"上层，说明抽象层次划分有问题。

```
用户请求 → M5(交互层) → M3(检索) → M1(存储)
                                         ↓
                                     返回数据
                                         ↓
              M5 ← M3 ← M1（数据原路返回）
```

**禁止**：M1 存储层主动通知 M3 "我的数据变了"。如果需要这种模式，用事件通道 (tokio mpsc) 解耦，而不是直接依赖。

### 2.3 谨慎使用"通用"和"可扩展"

你觉得未来可能需要的灵活性，90% 不会被用到，但它带来的复杂度 100% 会留下。

```rust
// 过度设计：为了"未来可能的多种编码策略"搞了 4 层抽象
trait EncoderFactory: EncoderFactoryProvider { ... }
trait EncoderFactoryProvider { ... }
trait EncoderStrategy { ... }
trait EncoderStrategyAdapter { ... }

// 正确：当前只有 Gemini 和 Local 两种，一个 enum match 就够了
enum EncoderBackend {
    Gemini(GeminiClient),
    Local(OnnxEncoder),
}
// 等真的需要第三种时，再重构。三个相似的东西才是抽象的时机。
```

**The Rule of Three**：代码重复两次可以忍，重复三次再抽象。过早抽象比重复更有害。

---

## 原则三：让错误的代码无法编译

Rust 的类型系统是你最强的防线。把业务规则编码到类型中，让违规操作在编译期就被拒绝。

### 3.1 新类型（Newtype）防止参数混淆

```rust
// 危险：两个 String 参数，调用时很容易传反
fn query(agent_id: String, session_id: String) { ... }

// 安全：类型不同，编译器帮你检查
fn query(agent_id: AgentId, session_id: SessionId) { ... }
// query(session_id, agent_id)  ← 编译错误！
```

在 UMMS 中，至少以下 ID 必须是独立类型：`AgentId`, `SessionId`, `MemoryId`, `NodeId`, `EdgeId`。

### 3.2 用类型状态（Typestate）表达状态机

```rust
// Agent 上下文的生命周期：通过类型系统强制正确的状态转换
struct AgentContext<S: ContextState> { ... }
struct Active;      // 当前活跃
struct Snapshotted; // 已快照，等待清理
struct Suspended;   // 已挂起

impl AgentContext<Active> {
    fn snapshot(self) -> AgentContext<Snapshotted> { ... }
    // 注意：self 被 move，Active 状态被消费，不可能再用
}
impl AgentContext<Snapshotted> {
    fn clean(self) -> AgentContext<Suspended> { ... }
}
// 编译器保证：不可能在未 snapshot 的情况下 clean
// 编译器保证：不可能对已 snapshot 的 context 再次 snapshot
```

### 3.3 用 `#[must_use]` 防止忽略重要返回值

```rust
#[must_use = "snapshot must be persisted, dropping it loses agent state"]
pub struct Snapshot { ... }

// 如果调用方忽略了 snapshot 返回值，编译器会警告
```

---

## 原则四：显式优于隐式

隐式行为是 bug 的温床。当代码"自动"做了某件事，维护者很难追踪"为什么会这样"。

### 4.1 不要用默认值掩盖必须的决策

```rust
// 危险：agent_id 有默认值，调用方可能忘记设置
pub struct MemoryQuery {
    pub agent_id: String,  // 默认 ""
    ...
}

// 安全：builder 模式强制必填字段
impl MemoryQuery {
    pub fn new(agent_id: AgentId) -> Self { ... }
    // agent_id 是构造参数，不提供就无法创建 Query
}
```

在 UMMS 中，`agent_id` 是**隔离的命脉**。任何可以不带 `agent_id` 就执行的存储操作都是安全漏洞。

### 4.2 副作用必须在函数名中体现

```rust
// 不清楚：这个函数只是读取，还是会修改什么？
fn process_memory(entry: &MemoryEntry) -> Score { ... }

// 清楚：
fn calculate_importance_score(entry: &MemoryEntry) -> Score { ... }  // 纯计算
fn promote_to_shared_layer(&self, entry: &MemoryEntry) -> Result<()> { ... }  // 有副作用
fn write_and_index(&self, entry: MemoryEntry) -> Result<MemoryId> { ... }  // 写入+索引
```

### 4.3 配置要有边界，不要成为"万能后门"

```rust
// 危险：什么都能配，等于什么都没约束
pub struct Config {
    pub arbitrary_options: HashMap<String, Value>,
}

// 正确：每个配置项都有类型、范围和默认值
pub struct CacheConfig {
    /// L1 容量。认知科学研究表明人类工作记忆约 7±2 项。
    /// 范围: 3-15, 默认: 9
    pub l1_capacity: BoundedU32<3, 15>,
    ...
}
```

配置项必须问自己：**如果用户把它设成一个极端值（0、负数、MAX），系统会崩溃吗？** 如果会，就用类型约束它。

---

## 原则五：错误处理是第一公民，不是事后补丁

错误路径和正常路径一样重要。大部分生产 bug 都发生在"不应该发生"的错误路径上。

### 5.1 错误类型要携带诊断信息

```rust
// 没用的错误：告诉你出错了，但你不知道为什么
#[error("write failed")]
WriteFailed,

// 有用的错误：包含诊断上下文
#[error("Failed to write memory {memory_id} for agent {agent_id}: {source}")]
WriteFailed {
    memory_id: MemoryId,
    agent_id: AgentId,
    source: Box<dyn std::error::Error + Send + Sync>,
},
```

### 5.2 区分"可恢复"和"不可恢复"

```rust
// 可恢复：API 暂时不可用，切换到本地模型
EncodingError::ApiTimeout { .. } => fallback_to_local(input).await,

// 可恢复：单条记忆写入失败，跳过并记录
StorageError::WriteFailed { .. } => {
    warn!(?error, "Skipping failed write, will retry in next consolidation");
    continue;
}

// 不可恢复：数据库文件损坏，必须停止并通知用户
StorageError::DatabaseCorrupted { .. } => {
    error!(?error, "Database corruption detected");
    return Err(error);  // 向上传播，让顶层处理
}
```

### 5.3 永远不要吞掉错误

```rust
// 禁止：错误消失了，将来出 bug 时你找不到原因
let _ = storage.write(entry).await;

// 如果你确实想忽略错误，至少记录下来
if let Err(e) = storage.write(entry).await {
    debug!(?e, "Non-critical write failed, continuing");
}
```

---

## 原则六：为可测试性而设计，而不是为了覆盖率

80% 覆盖率毫无意义——如果测的都是 getter/setter。重要的是测**状态转换**和**边界条件**。

### 6.1 测试行为，不测实现

```rust
// 无价值的测试：只是验证了你调用了某个函数
#[test]
fn test_write_calls_lance_db() {
    let mock = MockLanceDb::new();
    mock.expect_insert().times(1);
    storage.write(entry, mock);
}

// 有价值的测试：验证了一个完整的业务不变量
#[tokio::test]
async fn agent_b_cannot_see_agent_a_private_memories() {
    let store = create_test_store().await;
    store.write(entry_for_agent("A")).await.unwrap();

    let results = store.query(query_as_agent("B")).await.unwrap();
    assert!(results.is_empty(), "Agent B saw Agent A's private data!");
}
```

### 6.2 每个 bug fix 都要附带回归测试

修 bug 之前先写一个能复现 bug 的测试（红灯），然后修复直到测试通过（绿灯）。这样这个 bug 永远不会再出现。

### 6.3 性能测试要测真实场景

```rust
// 无价值：在空库上测插入
fn bench_insert_empty_db() { ... }

// 有价值：在已有 100K 条记忆的库上测插入（这才是真实场景）
fn bench_insert_with_100k_existing() {
    let store = create_store_with_n_entries(100_000);
    // 现在测插入，这个数字才有参考意义
}
```

---

## 原则七：代码是写给下一个读者的

下一个读者大概率是三个月后的你自己。到时候你已经忘了当初为什么这么写。

### 7.1 注释解释 WHY，代码本身表达 WHAT

```rust
// 无价值的注释（重复代码已经说的话）
// 将 importance 乘以衰减因子
let score = importance * decay_factor;

// 有价值的注释（解释为什么选择这个公式）
// 指数衰减函数参考 Ebbinghaus 遗忘曲线。
// λ 参数的四个档位来自认知科学的实证研究：
// task_context 半衰期 ~1.4 天对应人类对临时任务的遗忘速度，
// domain_knowledge 半衰期 ~693 天对应长期知识的保持率。
// 参见: llm_memory_engineering_implementation.md Section 1.2
let score = importance * (-lambda * hours).exp();
```

### 7.2 用领域语言命名，不用技术术语

```rust
// 技术化命名（要看实现才知道在做什么）
fn process_items(vec: &[Item], map: &HashMap<String, f32>) -> Vec<Item> { ... }

// 领域化命名（读名字就知道业务含义）
fn apply_forgetting_decay(memories: &[MemoryEntry], decay_rates: &DecayRateTable) -> Vec<ScoredMemory> { ... }
```

### 7.3 ADR（架构决策记录）记录你放弃了什么

做决策时，不只记录你选了什么，还要记录你**没选什么以及为什么**。三个月后当你想"要不换个方案试试"时，ADR 会告诉你当初为什么排除了它。

```markdown
## ADR-001: 向量数据库选型 LanceDB

### 背景
需要嵌入式向量数据库，个人场景，<1M 向量。

### 选择
LanceDB

### 放弃的方案
- Qdrant Embedded: 性能更好，但 Rust binding 不够成熟，需要额外的 gRPC 层
- Chroma: Python-first，与 Rust 集成需要 FFI 开销
- 纯 SQLite + 暴力搜索: 10K 以下可行，但无法扩展到 100K+

### 回退条件
如果 LanceDB 在 >500K 向量时 P99 >100ms，切换到 Qdrant Embedded。
```

---

## 原则八：对 UMMS 专属的不变量

以下是这个项目独有的、必须刻进 DNA 里的规则。不是通用规则，而是**从 UMMS 的本质推导出来的**。

### 8.1 agent_id 是隔离的唯一保证

UMMS 存在的意义之一就是让多个 Agent 的记忆不互相污染。`agent_id` 不是一个"可选参数"，它是系统正确性的**前提条件**。

- 每个接触存储的函数，签名中必须有 `agent_id`（或包含 `agent_id` 的类型）
- 如果一个函数操作存储但没有 `agent_id`，它**必须**是操作共享层的，且函数名中要体现这一点（如 `read_shared_knowledge`）
- 查询结果中的每条记忆都必须携带 `agent_id` 和 `scope`，调用方可以验证

### 8.2 记忆层级晋升是单向阀门

L0→L1→L2→L3 的晋升是**不可逆的单向流**。一条记忆可以被删除，但不能从 L3 降级回 L0。这个不变量简化了所有状态推理。

- 例外：共享层的 `demote`（降级到私有层）不是层级降级，是**归属权变更**。

### 8.3 巩固服务是共享层的唯一自动写入者

防止共享知识层被任何 Agent 随意写入导致污染。只有两种途径：
1. 巩固服务自动提升（满足 importance/跨Agent引用/存活时间 三个条件）
2. 用户通过 promote API 手动提升

任何绕过这两个路径写入共享层的代码都是 bug。

### 8.4 编码降级必须对调用方透明

当 Gemini API 不可用时，系统自动切到本地 ONNX 编码。但上层模块**不应该关心**当前用的是哪个编码器。返回的向量格式、维度、后续处理流程都应该一致。如果本地模型的向量维度不同，适配是编码模块内部的事。

### 8.5 切换 Agent ≠ 杀死 Agent

切换 Agent 是"挂起 + 恢复"，不是"销毁 + 重建"。前一个 Agent 的状态完整保留在快照中，下次切回来应该**无感恢复**到离开时的状态。设计任何 Agent 相关功能时都要问：**切换后恢复，这个功能的状态还在吗？**

---

## 检验清单

写完一段代码后，问自己：

- [ ] 如果删掉这段代码的所有注释，一个新人仅通过类型签名和函数名能理解它在做什么吗？
- [ ] 如果把底层存储从 LanceDB 换成 Qdrant，这段代码需要改吗？（存储层之外的代码应该不需要改）
- [ ] 这段代码有没有接受一个 `String` 参数，而这个参数其实应该是一个更具体的类型？
- [ ] 如果这个函数的输入是一个极端值（空字符串、空数组、MAX_INT），会发生什么？
- [ ] 这段代码有没有"先做A再做B"的隐式顺序依赖？如果有，能不能通过类型系统强制这个顺序？
- [ ] 三个月后看到这段代码，你能在 30 秒内理解它为什么存在吗？
