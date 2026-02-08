# Elasticsearch MCP Server 优化实施方案

## 1. 背景与目标

当前 Elasticsearch MCP Server 存在以下核心问题：

| 维度 | 现状 | 风险等级 |
|------|------|----------|
| 稳定性 | `unwrap()` 可导致服务 panic 崩溃；ES 请求无超时 | 致命 |
| 安全性 | Query DSL 零验证，无危险操作拦截 | 高 |
| 数据上限 | `size`/响应体/索引列表均无限制 | 高 |
| Prompt | 冗余内容多，缺少安全红线声明，Token 占用过高 | 中 |

**策略**：Prompt 能解决的不动代码，代码必须解决的用最小改动量覆盖。

**目标**：以 ~30 行 Rust 改动 + 1 次 Prompt 重构，覆盖 85% 的高危问题。

---

## 2. 限制值决策依据

### 2.1 Token 预算模型

所有与"数据量"相关的限制值，均基于以下 Token 预算模型推导，而非拍脑袋：

**目标模型**：Qwen3-32B (32K context) / Qwen3-235B-A22B (128K context)

```
单次会话 Token 预算分配（以 32K 模型为基准）：
┌─────────────────────────┬───────────┬──────────┐
│ 组成部分                │ Token 预算 │ 约字符数  │
├─────────────────────────┼───────────┼──────────┤
│ System Prompt           │ 2,500     │ 5,000    │
│ 用户消息（累计）         │ 1,500     │ 3,000    │
│ Agent 推理 + 输出       │ 3,000     │ 6,000    │
│ 安全余量 (20%)          │ 6,400     │ -        │
│ ─── 剩余：工具响应累计 ──│ 18,600    │ ~37,000  │
├─────────────────────────┼───────────┼──────────┤
│ 典型会话 5-8 次工具调用  │           │          │
│ 单次工具响应预算         │ ~2,300    │ ~4,600   │
│ 单次工具响应上限（峰值） │ ~5,000    │ ~10,000  │
└─────────────────────────┴───────────┴──────────┘

注：中文约 1.5-2 字符/token，英文约 4 字符/token，
    ES JSON 响应通常为中英混合，取 ~2 字符/token。
```

**128K 模型的情况**：预算按比例放大 4 倍，单次工具响应峰值可达 ~40,000 字符。

### 2.2 各限制值推导

| 限制项 | 值 | 推导过程 |
|--------|-----|---------|
| **`size` 硬上限** | **200** | 单条 ES 文档（含 `_source`）典型 200-1000 字符。200 × 500 = 100,000 字符，远超任何模型预算，但有响应截断兜底。此值主要保护 ES 集群（避免拉取海量数据），而非 Token 管理。ES 默认 `max_result_window=10,000`，200 远低于此值，对集群无压力。 |
| **响应截断** | **默认 15,000 字符** | 15,000 字符 ≈ 7,500 token = 32K 模型的 23%。作为**单次响应峰值上限**可接受——实际会话中 `get_cluster_health`（~300 字符）、`get_mappings`（~2,000 字符）等小响应会拉低均值。对 128K 模型非常宽裕（仅 6%）。建议通过环境变量 `MCP_MAX_RESPONSE_CHARS` 可配置，32K 模型可调低至 8,000-10,000。 |
| **索引列表上限** | **100 条** | 100 条 × ~150 字符/条 = 15,000 字符，刚好命中截断阈值。实际生产集群通常 50-500 索引，100 条覆盖多数场景。超出时提示用 `index_pattern` 过滤。 |
| **ES 请求超时** | **30 秒** | ES 官方建议 search timeout 不超过 60s。30s 是运维查询的合理上限——正常查询 <1s，慢查询 1-10s，超过 30s 基本是异常（如全集群扫描）。此值保护 MCP Server 不被挂起，与 Token 无关。 |
| **Prompt 建议 `size` 默认值** | **10** | 10 × 500 = 5,000 字符 ≈ 2,500 token，恰好在单次响应预算内。Agent 查看数据通常只需 5-20 条样本即可分析模式。需要更多时 Agent 可主动增大。 |
| **Prompt 建议聚合桶数** | **≤ 50** | 50 桶 × ~100 字符/桶 = 5,000 字符，在预算内。ES `terms` 聚合默认 10 桶，50 已经是 5 倍放大。 |

### 2.3 不同模型的建议配置

| 环境变量 | 32K 模型 | 128K 模型 | 说明 |
|----------|----------|-----------|------|
| `MCP_MAX_RESPONSE_CHARS` | 10,000 | 30,000 | 响应截断阈值 |
| `MCP_MAX_SEARCH_SIZE` | 200 | 200 | size 硬上限（保护 ES，与模型无关） |
| `MCP_MAX_INDEX_LIST` | 100 | 200 | 索引列表上限 |
| `MCP_REQUEST_TIMEOUT_SECS` | 30 | 30 | ES 请求超时（与模型无关） |

> **原则**：保护 ES 集群的限制值（size、timeout）与模型无关，固定即可；管控 Token 的限制值（响应截断、列表上限）应随模型上下文窗口调整。

---

## 3. 编码侧实施（Rust）— 4 处必改项

以下 4 项是 **Prompt 无法替代的**，因为它们发生在 MCP Server 内部，Agent 既看不见也控制不了。

### 3.1 [P0] 修复 `unwrap()` panic — 消除服务崩溃

**文件**：`src/servers/elasticsearch/base_tools.rs` L226

**问题**：`get_mappings` 在 mapping 响应为空时（如索引不存在、通配符无匹配）直接 `unwrap()` 导致整个 MCP Server 进程 panic 崩溃。

**改动**（~2 行）：
```rust
// 改前
let mapping = response.values().next().unwrap();

// 改后
let mapping = response.values().next()
    .ok_or_else(|| rmcp::Error::internal_error(
        "No mapping found for the specified index. Please verify the index name exists.",
        None,
    ))?;
```

**为什么 Prompt 不行**：Prompt 可以要求 Agent "先 list_indices 确认索引存在"，但 Agent 可能遗忘或用户直接给了错误的索引名。`unwrap()` 是确定性崩溃，只有代码能修。

### 3.2 [P0] 响应体截断 — 防止上下文溢出

**文件**：`src/servers/elasticsearch/base_tools.rs`

**问题**：`search` 和 `list_indices_detailed` 返回的 JSON 可能达到数十万字符，直接撑爆 Agent 上下文窗口。

**改动**（~15 行）：
```rust
/// 辅助函数：截断超长响应
fn truncate_response(json_str: String, max_chars: usize) -> String {
    if json_str.len() <= max_chars {
        return json_str;
    }
    let truncated = &json_str[..json_str.floor_char_boundary(max_chars)];
    format!(
        "{}\n\n[truncated: response too large, displaying first {} / total about {} characters. Please reduce time range, add filters, or reduce limit to get more precise results]",
        truncated, max_chars, json_str.len()
    )
}
```

在 `search` 函数返回前、`Content::json` 序列化后应用截断。

**截断阈值**：默认 15,000 字符，通过环境变量 `MCP_MAX_RESPONSE_CHARS` 可配置（见 2.3）。

**为什么 Prompt 不行**：响应数据从 ES → MCP Server → Agent，Agent 无法控制自己接收到多少数据。截断只能在 MCP Server 内部完成。

### 3.3 [P0] ES 请求超时 — 防止请求挂起

**文件**：`src/servers/elasticsearch/mod.rs`（构建 ES 客户端处）

**问题**：当前所有 ES HTTP 请求无超时配置，如果 ES 响应缓慢或网络异常，MCP Server 线程将无限期挂起。

**改动**（~5 行）：在构建 `reqwest::Client`（ES 客户端底层 HTTP 客户端）时设置：
```rust
// 在 TransportBuilder 或 reqwest::Client 构建处
.timeout(std::time::Duration::from_secs(30))
```

**超时值**：30 秒（见 2.2 推导）。

**为什么 Prompt 不行**：HTTP 超时是客户端级别的网络配置，Prompt 完全无法触及。

### 3.4 [P1] `search` size 硬上限 + 索引列表截断

**文件**：`src/servers/elasticsearch/base_tools.rs`

**改动**（~10 行）：

```rust
// === search 函数中，构造 query_body 后 ===
// 强制 size 上限
if let Some(Value::Number(n)) = query_body.get("size") {
    if let Some(size) = n.as_u64() {
        if size > 200 {
            query_body.insert("size".to_string(), json!(200));
        }
    }
}

// === list_indices_detailed 函数中，read_json 之后 ===
let mut response: Vec<serde_json::Value> = read_json(response).await?;
let total_count = response.len();
let truncated = total_count > 100;
if truncated {
    response.truncate(100);
}
// 在返回的 Content::text 中追加截断提示
let summary = if truncated {
    format!("Found {} indices (showing first 100, use index_pattern to filter):", total_count)
} else {
    format!("Found {} indices:", total_count)
};
```

**为什么 Prompt 不行**：
- **size 上限**：Prompt 说"默认 size=10"Agent 大多会听，但用户可以要求"给我查 5000 条"，Agent 会照做。硬上限 200 是最后防线。
- **索引列表截断**：Agent 无法干预 MCP Server 返回多少条数据。

### 3.5 编码侧改动汇总

| 编号 | 优先级 | 改动位置 | 改动量 | 效果 |
|------|--------|----------|--------|------|
| 3.1 | P0 | `base_tools.rs` L226 | ~2 行 | 消除 panic 崩溃 |
| 3.2 | P0 | `base_tools.rs` 新增辅助函数 + 2 处调用 | ~15 行 | 防止上下文溢出 |
| 3.3 | P0 | `mod.rs` 客户端构建处 | ~5 行 | 防止请求挂起 |
| 3.4 | P1 | `base_tools.rs` search + list_indices_detailed | ~10 行 | 限制查询量和列表量 |
| **合计** | | **2 个文件，4 处改动** | **~32 行** | 覆盖所有致命+高危问题 |

---

## 4. Prompt 侧实施 — 重构 `ElasticSearchBot-ES7.x-latest.md`

以下问题通过 Prompt 指令即可有效解决（有效率 ~90-95%），无需改 Rust 代码。

### 4.1 [P0] 新增安全红线章节

**当前缺失**：Prompt 未禁止 Agent 构造危险 DSL 操作。

**新增内容**：
```markdown
<SecurityRedLines>
### 绝对禁止的操作（安全红线）
以下操作可能破坏集群数据或耗尽资源，**严禁在 query_body 中使用**：
- `script` / `script_fields` / `scripted_metric`（任意代码执行风险）
- `scroll` / `scroll_id`（长期占用集群资源）
- `delete_by_query` / `update_by_query`（数据破坏）
- `reindex`（大规模数据搬运）
- `_all` 或单独 `*` 作为 index 参数（全集群扫描）
- `from` + `size` 总和超过 10,000（深度分页导致 OOM）

如果用户要求执行上述操作，应拒绝并解释风险。
</SecurityRedLines>
```

**为什么不硬编码**：递归遍历 JSON 检测黑名单关键字、维护白名单列表，代码复杂度高且容易误伤正常查询（如字段名恰好包含 `script`）。Agent 对明确禁令的遵从率 >95%，且本项目是内部运维工具（非对抗场景），Prompt 禁令性价比远高于代码实现。

### 4.2 [P0] 新增系统限制声明

Agent 需要知道代码层已经施加了哪些限制，才能在截断发生时正确引导用户：

```markdown
<SystemLimits>
### 系统硬限制（MCP Server 已强制执行）
- **查询上限**: 单次 search `size` 强制 ≤ 200，超出自动覆盖。
- **响应截断**: 工具响应超过 15,000 字符自动截断（可能更少，取决于部署配置）。
- **请求超时**: 所有 ES 请求 30 秒超时，超时返回错误。
- **索引列表上限**: list_indices_detailed 最多返回 100 条，超出需用 index_pattern 过滤。

**遇到截断提示时**：引导用户缩小时间范围、添加过滤条件、指定 `_source` 字段或改用聚合统计。
**遇到超时时**：建议用户缩小查询范围，检查 ES 集群负载。
</SystemLimits>
```

### 4.3 [P1] 新增查询规范（软限制）

```markdown
<QueryGuidelines>
### 查询默认值（推荐遵守）
- **默认 size**: 未指定时使用 `size: 10`。分析场景最多 `size: 50`，仅在用户明确要求时增大。
- **必须指定 `_source`**: 查询时始终通过 `_source` 或 `fields` 参数限定返回字段，避免返回全部字段。
- **聚合桶数**: `terms` 聚合的 `size` 默认 ≤ 50，除非用户明确要求更多。
- **时间范围**: 日志类索引查询必须带 `range` 过滤，避免全量扫描。
</QueryGuidelines>
```

### 4.4 [P1] 删除冗余章节

| 删除/精简内容 | 原因 | 预计节省 |
|---------------|------|----------|
| "不可用工具"章节（L57-59） | Agent 看不到 `esql` 工具定义就不会调用 | ~5 行 |
| "工具参数格式"章节（L62-68） | MCP 协议层已保证 JSON 序列化正确性 | ~7 行 |
| "ES 7.x 版本特性说明 - 支持的功能"列表（L247-258） | 罗列支持的功能对 Agent 行为无约束力 | ~12 行 |
| "版本升级建议"章节（L266-271） | Agent 无法执行升级操作，属于噪音 | ~6 行 |
| 重复的字段类型提醒（出现 3 次） | 合并为一处 | ~10 行 |
| **合计** | | **~40 行，约 15% Token** |

### 4.5 [P2] XML 标签结构化

将 Prompt 整体改为 XML 标签包裹结构，最终结构：

```
<Role>          — 1 句话角色定义
<CriticalRule>  — 工具失败处理（保留，已有且质量好）
<SecurityRedLines> — 【新增】安全红线
<SystemLimits>  — 【新增】硬限制声明
<QueryGuidelines> — 【新增】软限制/默认值
<Goals>         — 精简为 3-4 条
<KnowledgeBase> — 保留，精简引用要求部分
<Tools>         — 仅列工具名 + 一句话描述
<Strategies>    — 保留核心策略，合并重复内容
<Examples>      — 保留 3 个最具代表性场景（删除场景 4、5）
<ErrorHandling> — 保留，精简排查步骤
<Constraints>   — 精简为 5-6 条核心约束
```

**预计效果**：从 271 行压缩至 ~160-180 行，Token 减少 ~30-35%。

---

## 5. 不做的事项及理由

以下事项经评估**性价比低**，当前阶段不实施：

| 事项 | 不做的理由 | 替代方案 |
|------|-----------|----------|
| Query DSL 黑名单代码过滤 | 需递归遍历 JSON + 维护黑名单，复杂度高，易误伤正常查询 | Prompt 安全红线（4.1），有效率 ~95% |
| 速率限制 | 需引入状态管理（计数器/令牌桶），侵入性大 | 在基础设施层（Nginx/API Gateway）实施 |
| 重试机制 | 需封装所有 ES 调用，改动面广 | ES 客户端自带一定容错；Prompt 已有错误处理引导 |
| 连接池管理 | 需重构 `EsClientProvider` | 当前单节点/少量并发场景足够 |
| `matches_pattern` ReDoS 防护 | 模板数量有限（通常 <50），实际风险极低 | 暂不处理 |
| 请求体大小限制 | 需在 HTTP 协议层加 middleware | 可在反向代理层做 |
| Docker 非 root 用户 | 需测试文件权限兼容性 | 低优先级，有空再做 |
| Mapping 嵌套深度截断 | 大多数索引 Mapping ≤3 层；截断后 Agent 可能丢失关键字段信息 | 由响应截断（3.2）兜底 |

---

## 6. 执行计划

### Phase 1：Prompt 重构（预计 30 min，零风险）

1. 新增 `<SecurityRedLines>`、`<SystemLimits>`、`<QueryGuidelines>` 三个章节
2. 删除冗余内容（4.4 清单）
3. 整体改为 XML 标签结构
4. 调整思维链示例：删除场景 4（业务聚合）和场景 5（模板溯源），保留场景 1-3

### Phase 2：Rust 必改项（预计 30 min，低风险）

按以下顺序逐项修改并编译验证：

1. **修复 `unwrap()`**（3.1）→ `cargo build` 验证
2. **添加 ES 请求超时**（3.3）→ `cargo build` 验证
3. **实现 `truncate_response` 辅助函数**（3.2）→ 应用到 `search` 和 `list_indices_detailed`
4. **添加 `size` 硬上限 + 索引列表截断**（3.4）→ `cargo build` 验证

### Phase 3：验证（预计 15 min）

1. 编译通过
2. 手动测试：正常查询、空索引查询（验证 unwrap 修复）、大结果集（验证截断）
3. 确认 Prompt Token 数量下降

---

## 7. 预期收益

| 指标 | 改善前 | 改善后 |
|------|--------|--------|
| 服务崩溃风险 | `unwrap()` 可导致进程退出 | 安全错误返回 |
| 上下文溢出风险 | 无限制，大查询直接撑爆 | 15,000 字符截断兜底 |
| 请求挂起风险 | 无超时，可无限等待 | 30 秒强制超时 |
| Prompt Token | ~271 行 / ~4,000 token | ~170 行 / ~2,600 token（-35%） |
| 危险操作防护 | 无 | Prompt 红线 + size 硬上限 |
| 代码改动量 | - | ~32 行 Rust，2 个文件 |
