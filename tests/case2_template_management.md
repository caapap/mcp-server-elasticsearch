# 测试用例 2: 索引模板管理

本测试用例演示如何使用新增的 `get_templates` 工具进行索引模板管理。

## 场景描述

运维人员需要查看和管理 Elasticsearch 集群中的索引模板，包括：
1. 查看所有模板
2. 按名称过滤模板（支持通配符）
3. 查询特定索引匹配的模板

## 测试步骤

### 1. 查看所有索引模板

**用户输入**：
```
请列出所有的索引模板
```

**预期 AI 行为**：
调用 `get_templates` 工具，不传递任何参数（或传递 `name: "*"`）

**工具调用**：
```json
{
  "name": "get_templates",
  "arguments": {}
}
```

**预期返回**：
```json
{
  "template1": {
    "index_patterns": ["logs-*", "metrics-*"],
    "order": 1,
    "settings": {
      "number_of_shards": 3,
      "number_of_replicas": 1
    },
    "mappings": {
      "properties": {
        "timestamp": {"type": "date"},
        "message": {"type": "text"}
      }
    }
  },
  "template2": {
    "index_patterns": ["app-*"],
    "order": 0,
    "settings": {
      "number_of_shards": 5
    }
  }
}
```

### 2. 按名称过滤模板（支持通配符）

**用户输入**：
```
查看所有以 "logs-" 开头的模板
```

**预期 AI 行为**：
调用 `get_templates` 工具，传递 `name: "logs-*"`

**工具调用**：
```json
{
  "name": "get_templates",
  "arguments": {
    "name": "logs-*"
  }
}
```

**预期返回**：
只返回名称匹配 `logs-*` 模式的模板

### 3. 查询特定索引匹配的模板（核心功能）

**用户输入**：
```
索引 "logs-2025-01" 使用的是哪个模板？
```

**预期 AI 行为**：
调用 `get_templates` 工具，传递 `matching_index: "logs-2025-01"`

**工具调用**：
```json
{
  "name": "get_templates",
  "arguments": {
    "matching_index": "logs-2025-01"
  }
}
```

**预期返回**：
返回所有 `index_patterns` 匹配该索引的模板，按 `order` 降序排列（优先级高的在前）

```json
{
  "logs-template-v2": {
    "index_patterns": ["logs-*"],
    "order": 10,
    "settings": {...}
  },
  "general-template": {
    "index_patterns": ["*"],
    "order": 0,
    "settings": {...}
  }
}
```

**AI 分析输出示例**：
```
索引 "logs-2025-01" 匹配到 2 个模板（按优先级排序）：

1. **logs-template-v2** (order: 10) - 最高优先级
   - 匹配模式: logs-*
   - 分片数: 3
   - 副本数: 1

2. **general-template** (order: 0) - 默认模板
   - 匹配模式: *
   - 分片数: 5

最终生效的模板是 **logs-template-v2**，因为它的 order 值最高。
```

## 技术实现要点

### 模板匹配算法

工具内部实现了 ES 的模板匹配逻辑：

1. **通配符匹配**：将 ES 的 `*` 通配符转换为正则表达式
   - `logs-*` → `^logs-.*$`
   - `*-prod` → `^.*-prod$`

2. **优先级排序**：按 `order` 字段降序排列
   - 未设置 `order` 的模板默认为 0
   - `order` 值越高，优先级越高

3. **多模板合并**：当多个模板匹配同一索引时
   - 按优先级从高到低应用
   - 后应用的模板会覆盖前面的设置

## 使用场景

### 场景 1: 模板审计
```
列出所有模板，检查是否有配置不合理的模板
```

### 场景 2: 故障排查
```
索引 "app-prod-202501" 的分片数为什么是 10？
```
AI 会查询匹配的模板，分析配置来源

### 场景 3: 模板冲突检测
```
检查索引 "metrics-system-202501" 匹配了哪些模板，是否存在冲突
```

## 下一步扩展

根据开发计划，后续将实现：

1. **模板更新工具** (`upsert_template`)
   - 支持创建和更新模板
   - 内置安全检查（分片数限制、副本数强制等）

2. **风险预检工具** (`validate_template_change`)
   - 对比新旧配置
   - 生成风险报告（字段冲突、性能风险、配置降级）

3. **数据活性检查** (`check_ingestion_vitality`)
   - 监控索引数据接入是否正常
   - 自动识别时间戳字段并计算延迟
