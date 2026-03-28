# OPT-002: EPA 算法演进——VCP 对比分析与迭代计划

## 状态

已记录（2026-03-28），待实施。依赖 OPT-001（Prompt 系统移植）中的 P0 项完成后启动。

## 背景

UMMS 的 EPA（Embedding Projection Analysis）系统在 Phase 3 中实现，核心功能包括标签激活、K-Means 聚类、Power Iteration PCA、三级残差金字塔 Reshaping。VCP 项目在同一领域有约 3000 行 JavaScript 实现（7 个模块），部分算法更成熟。本文档对比两套实现的差异，给出有优先级的迭代计划。

---

## 一、架构对照

### 1.1 模块映射

```
VCP (JavaScript, ~3000 行)                  UMMS (Rust, ~800 行)
──────────────────────────                  ──────────────────────
EPAModule.js (485 行)                       epa/analyzer.rs
  加权 PCA + Gram-Schmidt 正交              K-Means++ + Power Iteration PCA
  投影 + 能量分布 + 共振检测                 聚类指标 + alpha 计算

ResidualPyramid.js (392 行)                 reshaping/reshape.rs
  Gram-Schmidt 正交残差分析                  三级加权质心金字塔
  能量追踪 + handshake 分析                  直接向量融合

ContextVectorManager.js (455 行)            ❌ 无对应
  时间衰减对话上下文聚合
  语义宽度 / 逻辑深度

SemanticGroupManager.js (461 行)            TagStore（部分等价）
  手动语义组定义 + 向量增强                   自动标签聚类

MetaThinkingManager.js (396 行)             LIF 扩散（功能正交）
  多阶段递归推理链                           知识图谱 BFS 遍历
  向量迭代融合                               衰减评分

EmbeddingUtils.js (234 行)                  umms-encoder ✅ 已有
DeepMemo.js (608 行)                        umms-retriever ✅ 已有
```

### 1.2 数据流对照

```
              VCP 数据流                                 UMMS 数据流
              ─────────                                 ──────────

query ──→ Embed ──→ ContextVector(时间衰减)     query ──→ Embed ──→ [无上下文聚合]
                         │                                        │
                         ▼                                        ▼
              SemanticGroup 向量增强              ──→ EPA Analyzer (K-Means + PCA)
                         │                                        │
                         ▼                                        ▼
              EPAModule (PCA 投影)                ──→ Reshaping (三级金字塔)
                         │                                        │
                    ┌────┴────┐                                   │
                    ▼         ▼                                   ▼
           logicDepth    resonanceBridges         ──→ effective_vector
                    │         │                           │
                    ▼         ▼                           ▼
          ResidualPyramid (能量分析)              ──→ Hybrid Recall
                    │                                     │
               ┌────┴────┐                                ▼
               ▼         ▼                          ──→ Rerank
          coverage    novelty                             │
               │         │                                ▼
               ▼         ▼                          ──→ LIF Diffusion
         expansionSignal                                  │
               │                                          ▼
               ▼                                    ──→ Final Results
         MetaThinking(递归搜索)
```

**关键差异**：VCP 是"先分析后决策"（分析出 coverage/novelty 再决定是否扩展搜索），UMMS 是"先改向量后搜索"（直接改 effective_vector 然后搜索）。两者理念不同，可以互补。

---

## 二、算法逐项对比

### 2.1 PCA 主成分分析

| 维度 | VCP | UMMS | 差异影响 |
|------|-----|------|---------|
| 正交保证 | Gram-Schmidt 每次迭代重正交 | Deflation（减去已提取成分） | UMMS 在 num_axes ≤ 3 时误差可忽略 |
| 居中 | 加权均值居中 | 加权均值居中 | 一致 |
| 输出 | 正交基 + 能量 + **语义标签** | 方向 + explained_variance | VCP 多了可读的轴名 |
| 投影 | 查询投影到每个轴 → 能量分布 | 不投影查询，仅输出轴 | VCP 能用能量分布算 logicDepth |

**结论**：PCA 核心算法等价。差异在于 VCP 多做了一步"查询投影"来算能量分布。

### 2.2 聚类

| 维度 | VCP | UMMS |
|------|-----|------|
| 初始化 | 随机或简单启发 | **K-Means++ D² 加权** |
| 权重 | 等权或简单加权 | **相似度分数加权质心** |
| 确定性 | 非确定性 | **确定性**（首个质心 = 最大权重点） |

**结论**：UMMS 的聚类实现更优，无需移植 VCP 的版本。

### 2.3 Logic Depth

| 维度 | VCP | UMMS |
|------|-----|------|
| 公式 | `1 - H(energy) / H_max` | `max_weight / total_weight` |
| 输入 | PCA 各轴的能量占比 | K-Means 各聚类的权重占比 |
| 区分度 | 高——区分"2 个均匀聚类"(0.0) vs "10 个均匀聚类"(0.0) 都是 0，但过程中 H_max 不同 | 低——只看最大的一个 |

**示例**：
```
场景 A: 3 个聚类，权重 [0.5, 0.3, 0.2]
  VCP:  H = -0.5*ln(0.5) - 0.3*ln(0.3) - 0.2*ln(0.2) = 1.03
        H_max = ln(3) = 1.10
        depth = 1 - 1.03/1.10 = 0.06   (很分散)
  UMMS: depth = 0.5 / 1.0 = 0.50       (看起来很集中)

场景 B: 3 个聚类，权重 [0.9, 0.05, 0.05]
  VCP:  H = 0.33, depth = 1 - 0.33/1.10 = 0.70  (较集中)
  UMMS: depth = 0.9 / 1.0 = 0.90                  (非常集中)
```

**结论**：VCP 的熵公式对分散场景的区分度更好，应移植。

### 2.4 Cross-Domain Resonance

| 维度 | VCP | UMMS |
|------|-----|------|
| 公式 | `√(E_i × E_j)` 两两轴共激活 | `significant_count / k` |
| 输出 | `ResonanceBridge[]`（哪两个轴在共振 + 强度 + 平衡度） | 单个标量 |
| 阈值 | 共激活 > 0.15 | 聚类权重占比 > 0.1 |

**结论**：VCP 的成对检测更有诊断价值，应增强。

### 2.5 残差分析 / Reshaping

| 维度 | VCP (`ResidualPyramid`) | UMMS (`reshape.rs`) |
|------|------------------------|---------------------|
| **核心方法** | Gram-Schmidt 正交投影，逐层计算残差 | 加权质心插值 |
| **能量追踪** | ✅ 逐层 `energyExplained`，有 `coverage` 总量 | ❌ 无 |
| **提前终止** | ✅ 残差 < `minEnergyRatio` (10%) 时停止 | ❌ 固定三级 |
| **方向分析** | ✅ Handshake: `coherence` / `novelty` / `noise` / `tension` | ❌ 无 |
| **输出** | 分析指标（coverage, coherence, novelty, expansionSignal） | `effective_vector`（改写后的向量） |
| **向量改写** | ❌ 不改向量 | ✅ `fused = (1-α)*original + α*context` |

**结论**：这是**最大的差距**。VCP 做分析但不改向量，UMMS 改向量但不做分析。两者应合并。

### 2.6 对话上下文聚合

| 维度 | VCP (`ContextVectorManager`) | UMMS |
|------|----------------------------|------|
| 时间衰减 | `weight = 0.85^age`，最多回溯 10 轮 | ❌ 每轮查询独立 |
| 上下文分割 | 基于余弦相似度阈值自动断句 | ❌ 无 |
| 语义宽度 | `entropy(||v||²_normalized)` | ❌ 无 |
| 缓存 | SHA256 内容哈希 + 模糊匹配 (Dice 系数) | ❌ 无 |

**结论**：多轮对话场景下，缺少上下文聚合会导致"每轮都从零开始"。应移植核心的时间衰减聚合。

### 2.7 递归推理链

| 维度 | VCP (`MetaThinkingManager`) | UMMS |
|------|---------------------------|------|
| 多阶段搜索 | 搜索 → 结果向量融合 → 再搜索 | ❌ 单次搜索 |
| 主题自动切换 | 查询匹配预设主题链 | ❌ 无 |
| 向量迭代 | `0.8*original + 0.2*avg(results)` | ❌ 无 |

**结论**：UMMS 的 LIF 扩散在图空间完成了类似的"发现关联"功能，但在嵌入空间的迭代探索是 VCP 独有的。P3 优先级——需要 benchmark 验证收益。

---

## 三、迭代计划

### Sprint E1: Reshaping 质量可观测（P0）

**目标**：让 reshaping 从"盲改"变为"可量化"。

**当前问题**：`reshape()` 直接返回 `Vec<f32>`，调用方无法判断改写质量。在陌生领域（标签覆盖率低），reshaping 可能把向量拉向无关方向。

**实现**：

1. 在 `reshaping/reshape.rs` 中增加残差能量计算：

```rust
/// Reshaping 结果，包含改写后的向量和质量指标。
pub struct ReshapingResult {
    /// 改写后的查询向量（L2 归一化）。
    pub effective_vector: Vec<f32>,
    /// 标签对查询的能量覆盖率 (0.0-1.0)。
    /// 低覆盖率意味着标签几乎不认识这个查询。
    pub coverage: f32,
    /// 各级标签的方向一致性 (0.0-1.0)。
    /// 低一致性意味着激活的标签指向不同方向。
    pub coherence: f32,
    /// 未被标签解释的残差能量占比 (0.0-1.0)。
    pub residual_ratio: f32,
}
```

2. 能量计算算法（从 VCP `ResidualPyramid` 移植）：

```
输入: query_vector, activated_tag_vectors[]
输出: coverage, coherence, residual_ratio

Step 1: original_energy = ||query||²
Step 2: 对 L0 标签做 Gram-Schmidt 正交化 → 正交基 B0
Step 3: 投影 query 到 B0 → projection_0
        residual_0 = query - projection_0
        energy_explained_0 = (original_energy - ||residual_0||²) / original_energy
Step 4: 对 L1 标签做 Gram-Schmidt → B1（与 B0 正交）
        投影 residual_0 到 B1 → projection_1
        residual_1 = residual_0 - projection_1
        energy_explained_1 = (||residual_0||² - ||residual_1||²) / original_energy
Step 5: coverage = energy_explained_0 + energy_explained_1
        residual_ratio = ||residual_1||² / original_energy
Step 6: coherence = 各级标签内部方向一致性（成对余弦均值）
```

3. 在 pipeline 中利用 coverage 做自适应：

```rust
// pipeline.rs Stage 0.6
let reshaping_result = reshaper.reshape_with_quality(...);
let effective_vector = if reshaping_result.coverage < 0.2 {
    // 标签几乎不认识这个查询，不改向量
    tracing::info!(coverage = reshaping_result.coverage, "reshaping skipped: low coverage");
    query_vector.clone()
} else {
    reshaping_result.effective_vector
};
```

**文件变更**：

| 文件 | 变更 |
|------|------|
| `crates/umms-analyzer/src/reshaping/reshape.rs` | 新增 `ReshapingResult`、`reshape_with_quality()`、Gram-Schmidt 正交投影、能量计算 (~60 行) |
| `crates/umms-analyzer/src/reshaping/mod.rs` | 导出新类型 |
| `crates/umms-retriever/src/pipeline.rs` | 使用 `reshape_with_quality()`，coverage 自适应逻辑 (~10 行) |
| `crates/umms-core/src/tag.rs` | `EpaResult` 增加 `reshaping_coverage`、`reshaping_coherence` 字段 |

**验收标准**：
- `coverage` 在标签密集领域 > 0.5，在全新领域 < 0.2
- Pipeline 在 `coverage < 0.2` 时自动跳过 reshaping
- Dashboard EPA 面板展示 coverage / coherence 指标
- 全部现有测试通过 + 3 个新增测试

---

### Sprint E2: Logic Depth 熵公式升级（P1）

**目标**：用信息熵替代 max/sum，提升聚焦度度量的区分度。

**实现**：

在 `epa/analyzer.rs` 的 `analyze()` 方法中替换 logic_depth 计算：

```rust
// 替换前
let logic_depth = dominant_weight / total_weight;

// 替换后
let probs: Vec<f32> = clusters.iter()
    .map(|c| c.total_weight / total_weight)
    .filter(|&p| p > 0.0)
    .collect();
let entropy: f32 = probs.iter()
    .map(|&p| -p * p.ln())
    .sum();
let max_entropy = (probs.len() as f32).ln().max(f32::EPSILON);
let logic_depth = (1.0 - entropy / max_entropy).clamp(0.0, 1.0);
```

**文件变更**：

| 文件 | 变更 |
|------|------|
| `crates/umms-analyzer/src/epa/analyzer.rs` | 替换 logic_depth 计算 (~10 行) |

**验收标准**：
- 单聚类场景：`logic_depth ≈ 1.0`
- 均匀分布场景：`logic_depth ≈ 0.0`
- 现有 EPA 测试更新后通过
- alpha 计算兼容（可能需要微调 `alpha_depth_weight`）

**注意**：此变更会影响 alpha 计算。旧公式下 `logic_depth` 值域偏高（0.3-0.9），新公式值域更对称（0.0-1.0）。可能需要将 `alpha_depth_weight` 从 0.15 调到 0.20，通过 benchmark 确认。

---

### Sprint E3: 共振桥检测（P1）

**目标**：从"有几个聚类显著"升级为"哪些语义轴在共振"。

**实现**：

1. 在 `umms-core/src/tag.rs` 中新增类型：

```rust
/// 两个语义轴之间的共振关系。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResonanceBridge {
    /// 第一个语义轴索引。
    pub axis_a: usize,
    /// 第二个语义轴索引。
    pub axis_b: usize,
    /// 共激活强度: √(variance_a × variance_b)。
    pub co_activation: f32,
    /// 平衡度: min(var_a, var_b) / max(var_a, var_b)。
    /// 接近 1.0 表示两个轴同等重要。
    pub balance: f32,
}
```

2. 在 `epa/analyzer.rs` 的 `analyze()` 末尾，从 `semantic_axes` 计算共振桥：

```rust
let mut bridges = Vec::new();
for i in 0..axes.len() {
    for j in (i + 1)..axes.len() {
        let co_act = (axes[i].explained_variance * axes[j].explained_variance).sqrt();
        if co_act > 0.15 {
            let (hi, lo) = if axes[i].explained_variance >= axes[j].explained_variance {
                (axes[i].explained_variance, axes[j].explained_variance)
            } else {
                (axes[j].explained_variance, axes[i].explained_variance)
            };
            bridges.push(ResonanceBridge {
                axis_a: i,
                axis_b: j,
                co_activation: co_act,
                balance: lo / hi,
            });
        }
    }
}
```

3. `EpaResult` 增加 `pub resonance_bridges: Vec<ResonanceBridge>` 字段。

**文件变更**：

| 文件 | 变更 |
|------|------|
| `crates/umms-core/src/tag.rs` | 新增 `ResonanceBridge` (~10 行) |
| `crates/umms-analyzer/src/epa/analyzer.rs` | 计算共振桥 (~20 行) |
| `crates/umms-api/src/handlers/epa.rs` | Response 增加 bridges 字段 (~5 行) |

**验收标准**：
- 查询激活单一领域时：`resonance_bridges` 为空
- 查询跨两个领域时：产生 1 个 bridge，`co_activation > 0.15`
- Dashboard EPA 面板可视化共振桥

---

### Sprint E4: 对话上下文时间衰减聚合（P2）

**目标**：多轮对话中，查询向量融入近期对话上下文，提升检索连贯性。

**当前问题**：用户连续问"Rust 的所有权是什么"→"那借用呢"，第二轮查询"那借用呢"缺乏"Rust 所有权"的上下文，检索可能偏离。

**实现**：

1. 新增 `crates/umms-retriever/src/context.rs`：

```rust
use std::collections::VecDeque;

/// 对话级上下文聚合器。
///
/// 维护最近 N 轮查询向量，用指数时间衰减聚合为上下文向量。
/// 生命周期跟随 Session，Agent 切换时 reset。
pub struct ContextAggregator {
    /// 衰减率，每多一轮历史权重乘以此值。
    decay_rate: f32,
    /// 最大回溯轮数。
    max_window: usize,
    /// 聚合后上下文向量占比（0.0-1.0）。
    context_weight: f32,
    /// 最近 N 轮的查询向量。
    history: VecDeque<Vec<f32>>,
}

impl ContextAggregator {
    pub fn new(decay_rate: f32, max_window: usize, context_weight: f32) -> Self { ... }

    /// 记录一轮查询向量。
    pub fn push(&mut self, query_vector: Vec<f32>) { ... }

    /// 用时间衰减聚合历史，与当前查询混合。
    ///
    /// 返回 `(1 - context_weight) * current + context_weight * decayed_history`。
    /// 如果没有历史，原样返回 current。
    pub fn blend(&self, current: &[f32]) -> Vec<f32> { ... }

    /// Session 切换 / Agent 切换时清空。
    pub fn reset(&mut self) { ... }
}
```

2. 在 `umms-core/src/config.rs` 增加配置：

```rust
pub struct ContextConfig {
    /// 是否启用对话上下文聚合。
    pub enabled: bool,          // default: true
    /// 时间衰减率（每轮）。
    pub decay_rate: f32,        // default: 0.85
    /// 最大回溯轮数。
    pub max_window: usize,      // default: 10
    /// 上下文向量在融合中的占比。
    pub context_weight: f32,    // default: 0.15
}
```

3. 集成点：`pipeline.rs` Stage 0（编码后、EPA 前）。`ContextAggregator` 作为 `RetrievalPipeline` 的字段（需要 `&mut self` 或内部可变性）。

**文件变更**：

| 文件 | 变更 |
|------|------|
| `crates/umms-retriever/src/context.rs` | 新增 (~80 行) |
| `crates/umms-retriever/src/lib.rs` | 导出 context 模块 |
| `crates/umms-core/src/config.rs` | 新增 `ContextConfig` (~20 行) |
| `crates/umms-retriever/src/pipeline.rs` | 集成 ContextAggregator (~15 行) |
| `umms.toml` | 新增 `[context]` 注释段 |

**验收标准**：
- 单轮对话：blend 输出 = 原始向量
- 3 轮同主题对话：blend 向量与主题中心更近
- 3 轮后切换主题：decay 自然减弱旧主题影响
- Agent 切换时 reset，不泄露上一个 Agent 的对话上下文

**注意**：`ContextAggregator` 有状态，需要考虑并发访问。建议用 `Arc<Mutex<ContextAggregator>>` 或 per-session 实例。

---

### Sprint E5: 语义宽度指标（P2）

**目标**：补充 logic_depth 的"互补指标"——logic_depth 衡量聚焦度，semantic_width 衡量分散度。

**实现**：

在 `epa/analyzer.rs` 中增加：

```rust
/// 计算向量在高维空间中的语义宽度。
///
/// 使用归一化熵衡量能量分布的分散程度。
/// 高宽度 = 跨多个维度分散（泛化查询）。
/// 低宽度 = 集中在少数维度（精确查询）。
fn semantic_width(vector: &[f32]) -> f32 {
    let norm_sq: f32 = vector.iter().map(|v| v * v).sum();
    if norm_sq < 1e-12 {
        return 0.0;
    }
    let entropy: f32 = vector.iter()
        .map(|v| {
            let p = v * v / norm_sq;
            if p > 1e-12 { -p * p.ln() } else { 0.0 }
        })
        .sum();
    let max_entropy = (vector.len() as f32).ln();
    if max_entropy < 1e-12 { 0.0 } else { (entropy / max_entropy).clamp(0.0, 1.0) }
}
```

在 `EpaResult` 中增加 `pub semantic_width: f32`。

**文件变更**：

| 文件 | 变更 |
|------|------|
| `crates/umms-analyzer/src/epa/analyzer.rs` | 新增 `semantic_width()` (~15 行) |
| `crates/umms-core/src/tag.rs` | `EpaResult` 增加字段 |
| `crates/umms-api/src/handlers/epa.rs` | Response 增加字段 |

**验收标准**：
- 全零向量：width = 0.0
- 单维度非零：width ≈ 0.0
- 均匀分布向量：width ≈ 1.0

---

### Sprint E6: 递归向量融合探索（P3）

**目标**：评估 VCP MetaThinking 的"搜索→融合→再搜索"策略是否在 UMMS 中带来增量收益。

**当前状态**：UMMS 已有 LIF 扩散（图空间探索），但缺少嵌入空间的迭代探索。

**实现方案**（实验性）：

在 `pipeline.rs` 的 Stage 1（recall）之后，可选地执行第二轮 recall：

```rust
// Stage 1.5: Optional recursive vector fusion
if config.recursive_recall_enabled && !recall_results.is_empty() {
    let top_n = &recall_results[..5.min(recall_results.len())];
    let avg_vector = weighted_centroid(
        top_n.iter().map(|h| h.memory.vector.as_slice()),
        top_n.iter().map(|h| h.score),
    );
    let fused = blend(0.8, &effective_vector, 0.2, &avg_vector);
    let round2_results = self.hybrid.recall(query, agent_id, &fused).await?;
    // 合并去重
    merge_dedup(&mut recall_results, round2_results);
}
```

**评估方式**：
1. 在 10 个标准查询上运行 A/B 测试
2. 比较单轮 vs 双轮的 Recall@20 和 Precision@10
3. 记录额外延迟（预计 +40-80ms）
4. 仅当 Recall@20 提升 > 10% 时纳入正式管线

**优先级**：P3 — 不确定收益是否覆盖双倍延迟成本。先完成 E1-E5，再用 benchmark 数据决定。

---

## 四、不移植项及理由

| VCP 模块 | 不移植原因 |
|----------|-----------|
| **SemanticGroupManager** | UMMS TagStore + K-Means 自动完成等价功能。手动定义语义组在个人版中维护成本高于收益。如果未来需要，可通过 tag 的 `agent_id` 分组模拟 |
| **Vexus-Lite Rust 加速** | UMMS 已是纯 Rust，原生性能即等价于 VCP 的 FFI 加速层 |
| **EmbeddingUtils 批量容错** | `umms-encoder` 已有 `max_retries` + `timeout_ms` 机制 |
| **Gram-Schmidt 迭代内正交** | 在 `num_axes ≤ 3` 时 Deflation 与 Gram-Schmidt 数值差异 < 1e-6，不值得增加复杂度 |

---

## 五、依赖与风险

### 依赖关系

```
E2 (熵公式) ←── 独立，可随时做
E3 (共振桥) ←── 独立，可随时做
E5 (语义宽度) ←── 独立，可随时做
E1 (残差能量) ←── 独立，最高优先级
E4 (上下文衰减) ←── 需要 Session 状态管理（依赖 M5 Chat 完善）
E6 (递归融合) ←── 需要 E1 的 benchmark 数据做决策
```

### 风险

| 风险 | 影响 | 缓解 |
|------|------|------|
| E2 熵公式改变 logic_depth 值域，影响 alpha 计算 | alpha 偏移导致 reshaping 过强/过弱 | 改后 benchmark alpha 分布，必要时调 `alpha_depth_weight` |
| E1 Gram-Schmidt 正交投影在高维稀疏向量上数值不稳定 | coverage 计算不准 | 增加 epsilon 兜底，单元测试覆盖边界 |
| E4 ContextAggregator 引入有状态组件 | 并发场景竞争 | 使用 per-session 实例 + `Arc<Mutex<>>` |
| E6 递归融合增加 40-80ms 延迟 | 超出 200ms P99 目标 | 仅在 P3 评估后、收益明确时纳入 |

---

## 六、迭代顺序总览

```
Phase 4 迭代（当前）:
  ┌─ E1 残差能量分析 (P0)  ──── 约 60 行新增
  ├─ E2 熵公式升级 (P1)    ──── 约 10 行替换
  └─ E3 共振桥检测 (P1)    ──── 约 30 行新增

Phase 5 迭代:
  ├─ E4 上下文时间衰减 (P2) ── 约 95 行新增（依赖 Session 管理）
  └─ E5 语义宽度 (P2)      ──── 约 15 行新增

Phase 6 评估:
  └─ E6 递归向量融合 (P3)  ──── 实验性，benchmark 驱动
```
