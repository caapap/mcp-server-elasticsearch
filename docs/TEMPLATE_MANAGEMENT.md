# 索引模板管理功能说明

## 概述

索引模板（Index Template）是 Elasticsearch 中用于自动配置新索引的重要机制。当创建新索引时，ES 会自动应用匹配的模板配置，包括：
- 索引设置（分片数、副本数、刷新间隔等）
- 字段映射（Mappings）
- 索引别名（Aliases）

本 MCP Server 新增的 `get_templates` 工具提供了强大的模板查询能力，是实现 AIOps 智能运维的基础。

## 功能特性

### 1. 全量模板查询

获取集群中所有索引模板的完整定义。

**使用场景**：
- 模板审计：检查集群中所有模板配置
- 配置备份：导出模板配置用于迁移或备份
- 问题排查：全局了解模板配置情况

**示例**：
```json
{
  "name": "get_templates",
  "arguments": {}
}
```

### 2. 按名称过滤（支持通配符）

使用通配符 `*` 过滤模板名称，快速定位目标模板。

**使用场景**：
- 按业务线查询：`logs-*` 查看所有日志相关模板
- 按环境查询：`*-prod` 查看所有生产环境模板
- 精确查询：`app-template-v2` 查看特定模板

**示例**：
```json
{
  "name": "get_templates",
  "arguments": {
    "name": "logs-*"
  }
}
```

### 3. 索引反查模板（核心功能）

输入索引名称，自动查找并返回所有匹配的模板，按优先级排序。

**使用场景**：
- 配置溯源：某个索引的分片数为什么是 10？
- 故障诊断：索引写入失败，检查是否模板配置有问题
- 变更影响分析：修改模板会影响哪些索引？

**示例**：
```json
{
  "name": "get_templates",
  "arguments": {
    "matching_index": "logs-app-202501"
  }
}
```

**返回结果特点**：
- ✅ 自动匹配 `index_patterns`（支持通配符）
- ✅ 按 `order` 字段降序排列（优先级高的在前）
- ✅ 返回完整的模板定义

## 技术实现细节

### 模板匹配算法

工具在 Rust 侧实现了 ES 的模板匹配逻辑，确保结果准确性：

```rust
fn matches_pattern(&self, index_name: &str, pattern: &str) -> bool {
    // 将 ES 通配符转换为正则表达式
    // logs-* → ^logs-.*$
    // *-prod → ^.*-prod$
    let escaped = regex::escape(pattern).replace(r"\*", ".*");
    let regex_pattern = format!("^{}$", escaped);
    
    Regex::new(&regex_pattern).unwrap().is_match(index_name)
}
```

### 优先级排序

当多个模板匹配同一索引时，按 `order` 字段排序：

```rust
matching.sort_by(|a, b| {
    let order_a = a.1.order.unwrap_or(0);
    let order_b = b.1.order.unwrap_or(0);
    order_b.cmp(&order_a)  // 降序：高优先级在前
});
```

**排序规则**：
- `order` 值越高，优先级越高
- 未设置 `order` 的模板默认为 0
- 相同 `order` 的模板，保持原始顺序

### API 调用

工具封装了 ES 的 `GET /_template` API：

```rust
es_client
    .indices()
    .get_template(IndicesGetTemplateParts::Name(&[template_name]))
    .send()
    .await
```

## 实际应用案例

### 案例 1: 配置溯源

**问题**：索引 `app-prod-202501` 的分片数为什么是 10？

**AI 交互流程**：

1. 用户提问：
   ```
   索引 app-prod-202501 的分片数为什么是 10？
   ```

2. AI 调用工具：
   ```json
   {
     "name": "get_templates",
     "arguments": {
       "matching_index": "app-prod-202501"
     }
   }
   ```

3. 工具返回：
   ```json
   {
     "app-prod-template": {
       "index_patterns": ["app-prod-*"],
       "order": 5,
       "settings": {
         "number_of_shards": 10,
         "number_of_replicas": 2
       }
     }
   }
   ```

4. AI 分析回答：
   ```
   索引 app-prod-202501 的分片数配置来自模板 "app-prod-template"：
   
   - 匹配模式: app-prod-*
   - 分片数: 10
   - 副本数: 2
   - 模板优先级: 5
   
   该模板专门为生产环境的 app 索引设计，使用较多的分片数以支持高吞吐量。
   ```

### 案例 2: 模板冲突检测

**问题**：索引 `logs-system-202501` 匹配了多个模板，最终生效的是哪个？

**AI 交互流程**：

1. 用户提问：
   ```
   索引 logs-system-202501 匹配了哪些模板？
   ```

2. AI 调用工具：
   ```json
   {
     "name": "get_templates",
     "arguments": {
       "matching_index": "logs-system-202501"
     }
   }
   ```

3. 工具返回（已按优先级排序）：
   ```json
   {
     "logs-system-template": {
       "index_patterns": ["logs-system-*"],
       "order": 10,
       "settings": {"number_of_shards": 3}
     },
     "logs-template": {
       "index_patterns": ["logs-*"],
       "order": 5,
       "settings": {"number_of_shards": 5}
     },
     "default-template": {
       "index_patterns": ["*"],
       "order": 0,
       "settings": {"number_of_shards": 1}
     }
   }
   ```

4. AI 分析回答：
   ```
   索引 logs-system-202501 匹配了 3 个模板（按优先级排序）：
   
   1. **logs-system-template** (order: 10) ✅ 最终生效
      - 匹配模式: logs-system-*
      - 分片数: 3
   
   2. **logs-template** (order: 5)
      - 匹配模式: logs-*
      - 分片数: 5
   
   3. **default-template** (order: 0)
      - 匹配模式: *
      - 分片数: 1
   
   最终生效的是 **logs-system-template**，因为它的 order 值最高（10）。
   该模板专门为系统日志设计，使用 3 个分片以平衡性能和资源消耗。
   ```

### 案例 3: 模板审计

**问题**：检查集群中是否有配置不合理的模板

**AI 交互流程**：

1. 用户提问：
   ```
   检查所有模板，找出分片数超过 10 的模板
   ```

2. AI 调用工具：
   ```json
   {
     "name": "get_templates",
     "arguments": {}
   }
   ```

3. AI 分析所有模板，输出：
   ```
   发现 2 个模板的分片数配置超过 10：
   
   1. **heavy-logs-template**
      - 匹配模式: heavy-logs-*
      - 分片数: 20 ⚠️
      - 建议: 20 个分片可能导致资源浪费，建议评估实际数据量后调整
   
   2. **metrics-prod-template**
      - 匹配模式: metrics-prod-*
      - 分片数: 15 ⚠️
      - 建议: 除非数据量特别大，否则建议降低到 10 以下
   
   其他 5 个模板的分片数配置合理（1-10 个分片）。
   ```

## 与开发计划的关系

本功能是 [AIOps 增强规划](./AIOPS_ENHANCEMENT_PLAN.md) 中 **场景一：模板深度探测** 的实现。

### 已实现（阶段 1）

- ✅ **get_templates** 工具
  - 支持全量查询
  - 支持按名称过滤（通配符）
  - 支持索引反查模板
  - 实现模板匹配算法（`index_patterns` + `order` 排序）

### 待实现（阶段 2）

- ⏳ **upsert_template** 工具
  - 创建/更新模板
  - 风险预检（字段冲突、性能风险、配置降级）
  - 安全约束（分片限制、副本强制、防爆保护）

- ⏳ **check_ingestion_vitality** 工具
  - 监控索引数据接入活性
  - 自动识别时间戳字段
  - 计算数据延迟并诊断

## API 参考

### get_templates

获取索引模板，支持过滤和反查。

**参数**：

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `name` | string | 否 | 模板名称过滤（支持 `*` 通配符），默认 `*`（所有模板） |
| `matching_index` | string | 否 | 索引名称，返回匹配该索引的所有模板（按优先级排序） |

**注意**：`name` 和 `matching_index` 不能同时使用。

**返回值**：

```typescript
{
  [templateName: string]: {
    index_patterns: string[];      // 索引匹配模式
    order?: number;                // 优先级（默认 0）
    settings?: object;             // 索引设置
    mappings?: object;             // 字段映射
    aliases?: object;              // 索引别名
    version?: number;              // 模板版本
  }
}
```

**示例**：

```json
// 查询所有模板
{"name": "get_templates", "arguments": {}}

// 按名称过滤
{"name": "get_templates", "arguments": {"name": "logs-*"}}

// 索引反查
{"name": "get_templates", "arguments": {"matching_index": "logs-app-202501"}}
```

## 最佳实践

### 1. 模板命名规范

建议使用清晰的命名规范，便于过滤和管理：

```
<业务线>-<环境>-<类型>-template
```

示例：
- `logs-prod-app-template`
- `metrics-dev-system-template`
- `traces-prod-template`

### 2. 优先级设置

合理设置 `order` 值，避免冲突：

- **0-10**：默认/通用模板（如 `*`）
- **10-50**：业务线级别模板（如 `logs-*`）
- **50-100**：具体业务模板（如 `logs-app-prod-*`）

### 3. 定期审计

建议定期检查模板配置：

```
每月检查一次所有模板，确保：
1. 分片数配置合理（通常 1-10 个）
2. 副本数至少为 1（生产环境）
3. 没有冲突的模板（相同 index_patterns 但不同 order）
```

## 故障排查

### 问题 1: 工具返回空结果

**可能原因**：
- 集群中没有模板
- 名称过滤条件不匹配
- 索引名称不匹配任何模板

**解决方法**：
1. 先调用 `get_templates` 不带参数，查看所有模板
2. 检查通配符是否正确（ES 使用 `*`，不是 `%`）

### 问题 2: 索引反查返回多个模板

**说明**：这是正常现象。ES 允许多个模板匹配同一索引。

**理解方式**：
- 返回结果已按优先级排序
- 第一个模板是最终生效的
- 其他模板可能提供了部分配置（会被高优先级覆盖）

### 问题 3: 模板配置与实际索引不符

**可能原因**：
- 索引创建时未应用模板（手动创建）
- 索引创建后模板被修改（不影响已存在的索引）
- 使用了组件模板（Component Template）或索引模板 v2

**解决方法**：
- 使用 `get_mappings` 工具查看索引实际配置
- 检查索引创建时间与模板修改时间

## 相关文档

- [AIOps 增强规划](./AIOPS_ENHANCEMENT_PLAN.md) - 完整的功能规划
- [测试用例 2](../tests/case2_template_management.md) - 详细的测试场景
- [Elasticsearch 官方文档 - Index Templates](https://www.elastic.co/guide/en/elasticsearch/reference/current/index-templates.html)
