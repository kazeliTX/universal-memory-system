# ADR-016: VCP Tagmem 浪潮算法借鉴分析

## 状态

已分析（2026-03-26），分阶段引入。

## 背景

VCP 项目的 Tagmem（TagMemo Wave Algorithm）是一套物理学启发的 RAG 增强系统，
将向量空间视为被 Tag "引力源"扭曲的非欧几何空间，通过多层分解和向量重塑
来突破传统 cosine 相似度的信息压缩瓶颈。

核心代码位于:
- `F:/vcp/VCPToolBox/KnowledgeBaseManager.js` — 主编排器
- `F:/vcp/VCPToolBox/EPAModule.js` — 嵌入投影分析
- `F:/vcp/VCPToolBox/ResidualPyramid.js` — 残差金字塔分解
- `F:/vcp/VCPToolBox/ResultDeduplicator.js` — SVD 去重

## Tagmem 五层架构

```
Layer 1: EPA 嵌入投影分析
  加权 K-Means(32簇) + PCA(power iteration)
  → 逻辑深度 L = 1 - 归一化熵 (0=发散, 1=聚焦)
  → 跨域共振 R = 多轴共激活几何均值
  → 主导语义轴 Top 10%

Layer 2: 残差金字塔 (最多3层)
  每层: tag搜索 → Gram-Schmidt正交投影 → 提取已解释能量 → 传递残差
  → 覆盖率 = 累计解释能量%
  → 新颖度 = 残差方向强度 (tag体系未覆盖的语义)
  → 一致性 = tag差异向量方向是否对齐
  终止条件: 残差能量 < 原始10%

Layer 3: LIF 脉冲传播网络 (2跳)
  种子节点(EPA+残差发现的tag) → 沿共现关系扩散
  衰减因子 0.3, 激活阈值 0.10
  → 发现间接关联的涌现tag

Layer 4: 语义去重
  按 adjustedWeight 排序 → cosine > 0.88 的合并
  合并时权重转移20%, Core状态OR保留

Layer 5: 向量融合
  contextVec = Σ(tag_vec × weight) / Σ(weight)
  fusedVec = (1-α) × originalVec + α × contextVec
  α = min(1.0, effectiveTagBoost) 动态计算
```

## 动态参数公式 (V3.7)

```
L = logicDepth, R = resonance, S = semanticWidth

β = sigmoid(L · log(1+R) - S · noise_penalty)
finalTagWeight = range[0] + β × (range[1] - range[0])
                 // 默认 [0.05, 0.45]

dynamicK = clamp(k_base + L×3 + log(1+R)×2, 3, 10)

dynamicBoostFactor = L × (1 + log(1+R)) / (1 + entropy×0.5) × activationMultiplier
effectiveTagBoost = baseBoost × clamp(dynamicBoostFactor, 0.3, 2.0)
```

解读: 聚焦查询(高L)且多域共振(高R) → 强化tag增益; 发散查询(高S) → 抑制tag增益。

## 与 UMMS 的映射关系

| Tagmem 组件 | UMMS 对应物 | 差异 |
|---|---|---|
| Tag 锚点 | 知识图谱实体节点 | Tagmem 用独立 tag 索引，UMMS 用图谱 |
| EPA 投影分析 | 无 | UMMS 直接用原始向量搜索，无投影分析 |
| 残差金字塔 | 残差精排(计划中) | Tagmem 用于查询分析，UMMS 用于结果排序 |
| LIF 脉冲传播 | LIF 图谱扩散(已实现) | 思路一致: 种子→扩散→衰减→涌现 |
| 向量融合 | 无 | UMMS 不修改查询向量 |
| 动态参数 | umms.toml 静态配置 | Tagmem 每次查询动态计算 |
| 共现网络 | 知识图谱边权重 | Tagmem 用统计共现，UMMS 用实体关系 |
| SVD 去重 | RRF 去重(已实现) | Tagmem 用残差选择最大化多样性 |

## 借鉴计划

### Phase 1: 动态参数调整（M3 优化迭代）

**最低成本最快见效的改进。**

当前 `umms.toml` 的 `bm25_weight`、`min_score`、`top_k_final` 是固定值，
对所有查询一视同仁。短查询（"Neural"）和长查询（"如何优化Rust异步运行时的内存布局"）
应该用不同的参数。

实现思路:
```rust
struct DynamicParams {
    bm25_weight: f32,    // 短查询提高BM25权重（关键词更重要）
    min_score: f32,      // 短查询降低阈值（避免全过滤）
    top_k_recall: usize, // 模糊查询增加召回量
}

fn compute_params(query: &str, query_vec: &[f32]) -> DynamicParams {
    let word_count = query.split_whitespace().count();
    let char_count = query.chars().count();

    // 短查询: 降低min_score，提高BM25权重
    // 长查询: 提高min_score，降低BM25权重（语义更重要）
    // 具体公式参考 Tagmem V3.7 的 β 计算
}
```

### Phase 2: 残差能量分解（M3 精排优化）

引入 Tagmem 的残差金字塔思想到精排阶段:

```
当前: 粗排(cosine) → 直接返回
改进: 粗排(cosine) → 分析"tag能解释的能量"vs"残差能量"
      → 覆盖率高 = 结果可信
      → 新颖度高 = 查询包含未知概念，需要扩大检索范围
```

这与 ADR-012 的自动升级策略互补:
残差能量高 → 触发 LIF 扩散 → 尝试发现间接关联。

### Phase 3: 查询向量重塑（M4 巩固引擎之后）

**最有价值但依赖图谱质量的改进。**

前提: M4 巩固引擎已经在图谱中积累了高质量的实体节点和关系。

实现思路:
```rust
fn reshape_query(
    query_vec: &[f32],
    graph: &dyn KnowledgeGraphStore,
    agent_id: &AgentId,
) -> Vec<f32> {
    // 1. 从图谱中找到与 query 最相关的 N 个实体节点
    // 2. 获取这些节点的 embedding（如果有）
    // 3. 构建 context_vector = 加权平均
    // 4. 融合: fused = (1-α) × query_vec + α × context_vec
    // 5. α 根据查询的"逻辑深度"动态计算
}
```

**为什么要等 M4**: 如果图谱中的实体质量差（错误的实体抽取、噪声关系），
重塑后的向量会被引向错误方向，比原始向量更差。
Tagmem 的 tag 质量靠人工维护 + 统计共现保证，UMMS 靠 M4 巩固引擎保证。

### Phase 4: SVD 多样性选择（M3/M4 优化）

替代当前的简单 truncate，引入 Tagmem 的残差选择:

```
当前: rerank 后 truncate(top_k_final)
改进: rerank 后 SVD 分析候选向量
      → 贪心选择: 每次选信息增量最大的（残差能量最高）
      → 结果集多样性显著提升
```

## 不借鉴的部分

| Tagmem 特性 | 不借鉴原因 |
|---|---|
| 世界观门控（语言惩罚） | UMMS 通过 Agent 隔离实现类似效果，不需要语言级过滤 |
| Core Tag 特权系统 | UMMS 用 importance 权重体系，不需要二元特权 |
| 上下文向量聚合（对话历史衰减） | UMMS 的 L0/L1 缓存已实现类似功能 |
| FlexSearch 备选索引 | UMMS 已用 tantivy，不需要备选 |

## 风险评估

| 借鉴项 | 风险 | 缓解措施 |
|---|---|---|
| 动态参数 | 公式调参复杂 | 先用简单的线性映射，不用 sigmoid |
| 残差分解 | 计算开销增加 | 只对 top-50 候选做，不是全量 |
| 查询重塑 | 图谱质量差时反效果 | α 上限 clamp，保证原始向量主导 |
| SVD 去重 | 小数据量时意义不大 | 仅在候选 >50 条时启用 |

## 总结

Tagmem 的核心洞察: **cosine 相似度把高维信息压缩成一个标量是巨大的信息损失，
应该利用已知的语义锚点（tag/实体）来恢复被压缩掉的维度信息。**

这个洞察对 UMMS 完全适用。但实现路径不同:
- Tagmem 用独立的 tag 索引 + 统计共现
- UMMS 用知识图谱 + 实体关系

两者的 LIF 扩散思路已经趋同，后续差异主要在查询侧的向量重塑。
等 M4 把图谱质量做好后，这将是 UMMS 检索能力的重大升级。
