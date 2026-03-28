# LGSRR & EPA 架构反思

**日期**: 2026-03-27
**参与人**: kazeli + AI

---

## 现状梳理

Query 处理管道当前顺序：

```
原始文本 → LGSRR ──────────────────────────────→ recall (策略生效)
原始文本 → encode ──────────────────────────→ recall (向量输入)
                   ↓
               EPA ──────────────────────────→ recall (reshape参数)
```

- **LGSRR**：纯规则，<1ms，输出 query_type / specificity / retrieval_hints
- **EPA**：向量空间分析，输出 logic_depth / cross_domain_resonance / alpha / reshape 决策
- 两者**并行**运行，**互不感知**

---

## 发现的问题

### 1. LGSRR 的策略调度没有传给 encode

encode 是盲的，同一文本无论 LGSRR 给出什么 hints，encode 都生成相同的向量。如果想针对不同 query 类型做自适应编码，当前架构做不到。

### 2. LGSRR 和 EPA 存在职责重叠

两者都在"理解 Query"：
- LGSRR 从文本推断 query 类型和检索策略
- EPA 从向量推断逻辑深度和是否 reshape

没有统一的仲裁层，各自独立决策。

### 3. EPA 可以静默推翻 LGSRR 的决策

LGSRR 输出 `enable_reshape: true` 的 hints，但 EPA 的 alpha 阈值独立判断，可以跳过 reshape 而 LGSRR 完全不知情。策略意图被静默否定。

---

## 改进方向

**LGSRR 作为热插拔的策略先验，EPA 作为最终仲裁者：**

```
LGSRR hints ──→ EPA ──→ 最终策略 ──→ recall
              ↑
           向量
```

EPA 拿到 LGSRR 的先验建议后：
- **采纳**：LGSRR 与 EPA 结论一致 → 策略生效
- **否决**：EPA 向量分析不支持 LGSRR 建议 → 该维度跳过，但其他 hints（top_k、diffusion_hops）仍生效
- **增强**：EPA 发现强信号时主动开启 LGSRR 未建议的策略

LGSRR 可设计为热插拔策略插件（轻量规则 / 复杂推理规则按场景切换），EPA 作为统一仲裁层避免规则系统的蛮力误判。

---

## 待办

- [ ] 在 `umms-analyzer/src/epa/` 中设计 hints 接收接口
- [ ] 明确 EPA 对 LGSRR hints 的采纳/否决/增强三种决策模式
- [ ] 考虑让 encode 阶段接收 EPA 的最终策略（自适应编码方向）

### EPA 维度优化（2026-03-27 补充）

**现状**：3072d 向量直接用于 K-Means 和 Power Iteration PCA，开销随 num_clusters 和 num_axes 线性增长。

**优化方向**：EPA 内部增加降维步骤（在 K-Means/PCA 前），仅对 EPA 分析管道降维，不影响 LanceDB 主索引的 3072d 存储。可选 384d 或 128d，收益是 EPA 阶段延迟进一步降低。需权衡工程复杂度与实际收益。

### semantic_axes 未被使用（2026-03-27 补充）

**发现**：Power Iteration PCA 提取的 `semantic_axes`（语义轴）在 Dashboard 中显示，但实际 reshape 逻辑中完全未使用。

**当前 reshape 融合公式**：
```
fused = (1 - alpha) * original_query_vector + alpha * context_vector
```
其中 `context_vector` 来自 activated_tags 的 L0/L1/L2 三级加权质心，没有用到 PCA 语义轴的方向向量。

**确认**：是预留接口。语义轴本意是在 context 构建时提供方向指导（比如沿语义轴方向增强/削弱某些维度），但当前 reshape 逻辑仅用质心融合，尚未接入语义轴。后续如需实现更精细的向量调整，可沿此方向扩展。

---

## Reshape 机制详解

### 它在解决什么问题

用户的 query 向量可能和记忆中真正相关的内容存在"语义偏移"——比如用户说"那个关于图的算法"，但记忆中存的是"Dijkstra"。原始 query 向量的方向可能偏向"图"这个词，而记忆中的 Dijkstra 向量方向完全不同。

Reshape 的目标是：**把 query 向量的方向往相关记忆的语义区域"拉"一下**，让它更容易命中真正相关的内容。

### 怎么做的 — 三级残差金字塔

`QueryReshaper` 构建了一个三层 context 向量：

**L0（细节层）**：top-N 激活 tag（相似度最高），反映 query 当前最直接的语义焦点

**L1（上下文层）**：next-N 激活 tag，反映相邻语义区域，是对 L0 的扩展

**L2（联想层）**：L0 中每个 tag 的共现 tag（通过 PMI 筛选），反映跨记忆关联

三层加权合并为 context 向量，然后：
```
reshaped_vector = (1 - alpha) * original + alpha * context
```
alpha 越大，说明 tag 语义空间越"可信"，query 向量被拉得越远。

### 为什么 semantic_axes 没用上

语义轴（PCA 方向）是理论上有价值的信息——它描述了 tag 分布的主要变异方向，理论上可以指导 context 往哪个方向调整 query。但如果当前实现里 reshape 只用质心不做方向修正，那 PCA 的计算就完全浪费了。这个点值得和 semantic_axes 是否废弃一起确认。

---

## 优化建议（待落地）

### 1. K-Means 簇心几何信息未参与 Reshape

**现状**：EPA 的 K-Means 聚类输出簇心位置（`clusters[].centroid`），但 reshape 的 context 构建完全不使用簇心——用的是 individual activated_tags 的加权质心，簇的几何信息被丢弃。

**问题**：K-Means 聚类本可以描述 tag 的语义区域结构，比如 query 落在哪个簇的质心附近、不同簇之间的语义距离等。如果用簇心代替 individual tags 或将簇心作为额外 context 层级，可以让 reshape 更语义化。

**改进方向**：在 L0/L1/L2 基础上增加 L_cluster 层，用簇心向量作为 context 的一部分；或用簇间距离调整 alpha 的作用方向。

### 2. Tag Importance 始终为 0.5 常数

**现状**：Tag 创建时 `importance` 硬编码为 0.5，后续从不更新。`decay.rs` 只对记忆条目进行 importance 衰退，tag 不在管辖范围内。

**后果**：EPA 的 alpha 计算中 `avg_importance` 项对所有 query 基本相同（约 0.5），不起任何区分作用。`alpha_importance_weight` 参数实际上形同虚设。

**改进方向**：
- 方案 A：将 tag importance 接入 consolidation 引擎，仿照 memory decay 机制，让高频 tag 的 importance 随使用量增长（类似 page rank）
- 方案 B：直接用 `frequency` 替代 `importance`，frequency 是实际增长的有意义数值
- 方案 C：删除 alpha_importance_weight 项，简化 alpha 公式

### 3. EPA 内部 3072d 向量未降维

（见上文"EPA 维度优化"节）

### 4. LGSRR 与 EPA 职责未打通

（见上文"改进方向"节）
