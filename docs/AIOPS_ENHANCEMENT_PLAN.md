# AIOps Enhancement Plan for Elasticsearch MCP Server
# Elasticsearch MCP Server - AIOps 增强计划

> **文档版本**: v1.0  
> **创建日期**: 2026-01-20  
> **目标**: 将 Ansible Playbook 中的 ES 操作迁移到 MCP，提升 AIOps Agent 的灵活性与智能化水平

---

## 📋 Executive Summary | 执行摘要

### 背景 (Background)
当前 AIOps 系统中，Elasticsearch 相关操作分散在 Ansible Playbooks 中，存在以下问题：
1. **缺乏灵活性**: Playbook 是静态脚本，无法根据上下文动态调整参数
2. **诊断能力弱**: 需要运行完整 Playbook 才能获取状态信息，效率低下
3. **Agent 理解困难**: Ansible 输出格式复杂，Agent 难以解析和决策
4. **操作粒度粗**: 无法执行精细化的单点操作（如删除单个索引）

### 目标 (Goals)
通过将 **数据层** 和 **API 交互层** 操作迁移到 MCP，实现：
- ✅ Agent 可以实时查询 ES 集群状态
- ✅ Agent 可以智能决策索引管理操作（创建/删除/备份）
- ✅ Agent 可以验证 Playbook 执行结果
- ✅ 保留 Ansible 用于基础设施层操作（安装/启停）

### 架构原则 (Architecture Principles)
```
┌─────────────────────────────────────────────────────────────┐
│                    AIOps Agent (Dify)                       │
└─────────────────┬───────────────────────────┬───────────────┘
                  │                           │
        ┌─────────▼─────────┐       ┌────────▼─────────┐
        │  ES MCP Server    │       │ Ansible MCP      │
        │  (轻量级 API 操作)  │       │ (重量级基础设施)  │
        └─────────┬─────────┘       └────────┬─────────┘
                  │                           │
        ┌─────────▼─────────┐       ┌────────▼─────────┐
        │  Elasticsearch    │       │  Target Hosts    │
        │  (数据查询/索引管理) │       │  (安装/启停/配置)  │
        └───────────────────┘       └──────────────────┘
```

**职责划分**:
- **ES MCP Server**: 索引管理、数据查询、集群状态监控、数据验证
- **Ansible MCP Server**: ES 软件部署、服务启停、OS 配置、文件系统操作

---

## 🎯 Phase 1: 只读诊断能力 (Read-Only Diagnostics)
**优先级**: 🔴 P0 (Critical)  
**预计工期**: 2-3 天  
**目标**: 替代 `elasticsearch_status.yml` 的核心功能

### 1.1 新增 Tools

#### Tool 1: `get_cluster_health`
**功能**: 获取集群健康状态  
**替代**: `elasticsearch_status.yml` Phase 4 (Cluster Health)

```rust
#[tool(
    description = "Get Elasticsearch cluster health status",
    annotations(title = "Get cluster health", read_only_hint = true)
)]
async fn get_cluster_health(
    &self,
    req_ctx: RequestContext<RoleServer>,
    Parameters(params): Parameters<ClusterHealthParams>,
) -> Result<CallToolResult, rmcp::Error>
```

**参数**:
```json
{
  "wait_for_status": "green|yellow|red (optional)",
  "timeout": "30s (optional)"
}
```

**返回示例**:
```json
{
  "cluster_name": "es7-cluster",
  "status": "green",
  "number_of_nodes": 3,
  "number_of_data_nodes": 3,
  "active_primary_shards": 120,
  "active_shards": 240,
  "relocating_shards": 0,
  "initializing_shards": 0,
  "unassigned_shards": 0
}
```

**Agent 使用场景**:
```
User: "ES 集群现在健康吗？"
Agent: 调用 get_cluster_health() 
       -> 返回 status: "green"
       -> 回复: "集群状态正常，所有分片已分配"
```

---

#### Tool 2: `get_nodes_info`
**功能**: 获取节点详细信息  
**替代**: `elasticsearch_status.yml` Phase 5 (Cluster Nodes)

```rust
#[tool(
    description = "Get detailed information about Elasticsearch cluster nodes",
    annotations(title = "Get nodes info", read_only_hint = true)
)]
async fn get_nodes_info(
    &self,
    req_ctx: RequestContext<RoleServer>,
    Parameters(params): Parameters<NodesInfoParams>,
) -> Result<CallToolResult, rmcp::Error>
```

**参数**:
```json
{
  "node_id": "node-1,node-2 (optional, 默认 _all)",
  "metrics": "heap,cpu,load (optional)"
}
```

**返回示例**:
```json
{
  "nodes": {
    "es7-node1": {
      "name": "es7-node1",
      "ip": "172.30.137.172",
      "heap_percent": 45,
      "ram_percent": 60,
      "cpu": 12,
      "load_1m": 2.5,
      "node_role": "dim",
      "master": "*"
    }
  }
}
```

**Agent 使用场景**:
```
User: "哪个节点是 Master？"
Agent: 调用 get_nodes_info()
       -> 解析 master: "*" 字段
       -> 回复: "es7-node1 是当前 Master 节点"
```

---

#### Tool 3: `list_indices_detailed`
**功能**: 增强版索引列表（包含健康状态、文档数、大小）  
**替代**: 现有 `list_indices` 的增强版

```rust
#[tool(
    description = "List Elasticsearch indices with detailed health and size information",
    annotations(title = "List indices (detailed)", read_only_hint = true)
)]
async fn list_indices_detailed(
    &self,
    req_ctx: RequestContext<RoleServer>,
    Parameters(params): Parameters<ListIndicesDetailedParams>,
) -> Result<CallToolResult, rmcp::Error>
```

**参数**:
```json
{
  "index_pattern": "*",
  "health": "green|yellow|red (optional)",
  "sort_by": "docs.count|store.size (optional)"
}
```

**返回示例**:
```json
[
  {
    "index": "yq_account_related",
    "health": "green",
    "status": "open",
    "pri": 5,
    "rep": 1,
    "docs_count": 1500000,
    "store_size": "2.3gb",
    "pri_store_size": "1.15gb"
  }
]
```

**Agent 使用场景**:
```
User: "哪些索引超过 1GB？"
Agent: 调用 list_indices_detailed(sort_by="store.size")
       -> 过滤 store_size > 1GB
       -> 回复列表
```

---

### 1.2 实现要点

#### 技术栈
- **Rust Elasticsearch Client**: 已有依赖 `elasticsearch = "8.x"`
- **API 端点**:
  - `/_cluster/health` → `get_cluster_health`
  - `/_cat/nodes?v&h=...` → `get_nodes_info`
  - `/_cat/indices?v&h=...` → `list_indices_detailed`

#### 错误处理
```rust
// 统一错误处理模式
match es_client.cluster().health(...).send().await {
    Ok(response) => {
        let health: ClusterHealth = read_json(response).await?;
        Ok(CallToolResult::success(vec![Content::json(health)?]))
    }
    Err(e) => {
        Ok(CallToolResult::error(format!(
            "Failed to get cluster health: {}. Check ES_URL and credentials.",
            e
        )))
    }
}
```

#### 测试用例
```bash
# 测试脚本 tests/test_diagnostics.sh
curl -X POST http://localhost:30090/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "tools/call",
    "params": {
      "name": "get_cluster_health",
      "arguments": {}
    },
    "id": 1
  }'
```

---

## 🎯 Phase 2: 索引管理能力 (Index Management)
**优先级**: 🟠 P1 (High)  
**预计工期**: 3-4 天  
**目标**: 替代 `elasticdump_create/delete_indices.yml`

### 2.1 新增 Tools

#### Tool 4: `create_index`
**功能**: 创建索引（支持 Mapping 和 Settings）  
**替代**: `elasticdump_create_indices.yml`

```rust
#[tool(
    description = "Create a new Elasticsearch index with optional mappings and settings",
    annotations(title = "Create index")
)]
async fn create_index(
    &self,
    req_ctx: RequestContext<RoleServer>,
    Parameters(params): Parameters<CreateIndexParams>,
) -> Result<CallToolResult, rmcp::Error>
```

**参数**:
```json
{
  "index": "my_new_index",
  "mappings": {
    "properties": {
      "timestamp": { "type": "date" },
      "message": { "type": "text" }
    }
  },
  "settings": {
    "number_of_shards": 3,
    "number_of_replicas": 1
  }
}
```

**安全检查**:
```rust
// 1. 检查索引是否已存在
if es_client.indices().exists(...).send().await?.status_code() == 200 {
    return Ok(CallToolResult::error("Index already exists"));
}

// 2. 验证 Mapping 合法性
validate_mapping(&params.mappings)?;

// 3. 执行创建
es_client.indices().create(...).send().await?;
```

**Agent 使用场景**:
```
User: "帮我创建一个日志索引，字段包括时间戳和消息"
Agent: 调用 create_index(
         index="log_2026",
         mappings={...}
       )
       -> 返回成功
       -> 回复: "索引 log_2026 已创建"
```

---

#### Tool 5: `delete_index`
**功能**: 删除索引（带安全确认）  
**替代**: `elasticdump_delete_indices.yml`

```rust
#[tool(
    description = "Delete an Elasticsearch index (use with caution!)",
    annotations(title = "Delete index")
)]
async fn delete_index(
    &self,
    req_ctx: RequestContext<RoleServer>,
    Parameters(params): Parameters<DeleteIndexParams>,
) -> Result<CallToolResult, rmcp::Error>
```

**参数**:
```json
{
  "index": "old_logs_2023",
  "confirm": true
}
```

**安全检查**:
```rust
// 1. 必须显式确认
if !params.confirm {
    return Ok(CallToolResult::error(
        "Deletion requires explicit confirmation (set confirm=true)"
    ));
}

// 2. 禁止删除系统索引
if params.index.starts_with(".") {
    return Ok(CallToolResult::error(
        "Cannot delete system indices (starting with '.')"
    ));
}

// 3. 检查索引是否存在
if es_client.indices().exists(...).send().await?.status_code() != 200 {
    return Ok(CallToolResult::error("Index does not exist"));
}

// 4. 执行删除
es_client.indices().delete(...).send().await?;
```

**Agent 使用场景**:
```
User: "删除 old_logs_2023 索引"
Agent: 调用 list_indices_detailed(index_pattern="old_logs_2023")
       -> 确认索引存在
       -> 询问用户: "确认删除 old_logs_2023 (包含 1000 条文档)？"
User: "确认"
Agent: 调用 delete_index(index="old_logs_2023", confirm=true)
       -> 返回成功
```

---

#### Tool 6: `get_index_settings`
**功能**: 获取索引配置  
**用途**: 辅助索引管理和故障排查

```rust
#[tool(
    description = "Get settings for a specific Elasticsearch index",
    annotations(title = "Get index settings", read_only_hint = true)
)]
async fn get_index_settings(
    &self,
    req_ctx: RequestContext<RoleServer>,
    Parameters(params): Parameters<GetIndexSettingsParams>,
) -> Result<CallToolResult, rmcp::Error>
```

**返回示例**:
```json
{
  "yq_account_related": {
    "settings": {
      "index": {
        "number_of_shards": "5",
        "number_of_replicas": "1",
        "refresh_interval": "1s"
      }
    }
  }
}
```

---

### 2.2 实现要点

#### API 端点映射
- `PUT /<index>` → `create_index`
- `DELETE /<index>` → `delete_index`
- `GET /<index>/_settings` → `get_index_settings`

#### 权限控制
```rust
// 在 ElasticsearchMcpConfig 中添加配置
#[derive(Debug, Serialize, Deserialize)]
pub struct ElasticsearchMcpConfig {
    // ... 现有字段 ...
    
    /// 允许删除的索引模式 (默认禁止所有删除操作)
    #[serde(default)]
    pub allow_delete_patterns: Vec<String>,
    
    /// 禁止删除的索引模式 (优先级高于 allow_delete_patterns)
    #[serde(default)]
    pub deny_delete_patterns: Vec<String>,
}
```

**配置示例** (`elastic-mcp.json5`):
```json5
{
  "elasticsearch": {
    "url": "${ES_URL}",
    "api_key": "${ES_API_KEY}",
    
    // 只允许删除 temp_* 和 old_* 开头的索引
    "allow_delete_patterns": ["temp_*", "old_*"],
    
    // 禁止删除任何以 prod_ 开头的索引
    "deny_delete_patterns": ["prod_*", ".kibana*"]
  }
}
```

---

## 🎯 Phase 3: 数据验证能力 (Data Validation)
**优先级**: 🟡 P2 (Medium)  
**预计工期**: 2-3 天  
**目标**: 辅助 `elasticdump_import_data.yml` 验证导入结果

### 3.1 新增 Tools

#### Tool 7: `count_documents`
**功能**: 统计文档数量  
**用途**: 验证数据导入是否成功

```rust
#[tool(
    description = "Count documents in an Elasticsearch index with optional query filter",
    annotations(title = "Count documents", read_only_hint = true)
)]
async fn count_documents(
    &self,
    req_ctx: RequestContext<RoleServer>,
    Parameters(params): Parameters<CountDocumentsParams>,
) -> Result<CallToolResult, rmcp::Error>
```

**参数**:
```json
{
  "index": "yq_account_related",
  "query": {
    "range": {
      "timestamp": {
        "gte": "2026-01-01"
      }
    }
  }
}
```

**返回示例**:
```json
{
  "count": 1500000,
  "index": "yq_account_related"
}
```

**Agent 使用场景**:
```
[Ansible 执行完 elasticdump_import_data.yml]

Agent: 调用 count_documents(index="yq_account_related")
       -> 返回 count: 1500000
       -> 对比预期值
       -> 回复: "数据导入成功，共导入 1,500,000 条记录"
```

---

#### Tool 8: `get_sample_documents`
**功能**: 获取样本数据  
**用途**: 快速验证数据结构和内容

```rust
#[tool(
    description = "Get sample documents from an Elasticsearch index",
    annotations(title = "Get sample documents", read_only_hint = true)
)]
async fn get_sample_documents(
    &self,
    req_ctx: RequestContext<RoleServer>,
    Parameters(params): Parameters<GetSampleDocumentsParams>,
) -> Result<CallToolResult, rmcp::Error>
```

**参数**:
```json
{
  "index": "yq_account_related",
  "size": 5,
  "sort": [{ "timestamp": "desc" }]
}
```

**返回示例**:
```json
{
  "total": 1500000,
  "samples": [
    {
      "_id": "doc1",
      "_source": {
        "timestamp": "2026-01-20T10:00:00Z",
        "user_id": "12345",
        "action": "login"
      }
    }
  ]
}
```

**Agent 使用场景**:
```
User: "数据导入后，最新的几条记录是什么？"
Agent: 调用 get_sample_documents(
         index="yq_account_related",
         size=3,
         sort=[{"timestamp": "desc"}]
       )
       -> 返回最新 3 条
       -> 格式化展示给用户
```

---

### 3.2 实现要点

#### API 端点映射
- `GET /<index>/_count` → `count_documents`
- `GET /<index>/_search?size=N` → `get_sample_documents`

#### 性能优化
```rust
// count_documents: 使用 _count API 而非 search
es_client.count(CountParts::Index(&[&index]))
    .body(query)
    .send()
    .await?;

// get_sample_documents: 限制返回字段
es_client.search(SearchParts::Index(&[&index]))
    .size(params.size.min(100))  // 最多返回 100 条
    ._source(&params.fields.unwrap_or_default())  // 只返回指定字段
    .send()
    .await?;
```

---

## 🎯 Phase 4: 集成与测试 (Integration & Testing)
**优先级**: 🟢 P3 (Normal)  
**预计工期**: 2-3 天

### 4.1 Docker Compose 配置

**文件**: `mcp/mcp-server-elasticsearch/docker-compose.yml`

```yaml
services:
  elasticsearch-mcp-server:
    image: docker.elastic.co/mcp/elasticsearch:latest
    container_name: elasticsearch-mcp-server
    ports:
      - "30090:8080"
    environment:
      # Elasticsearch 连接配置
      - ES_URL=http://172.30.137.172:9200
      - ES_API_KEY=${ES_API_KEY}
      # 可选配置
      - ES_SSL_SKIP_VERIFY=false
    volumes:
      # 挂载自定义配置文件
      - ./elastic-mcp-aiops.json5:/config/elastic-mcp.json5:ro
    command: ["http", "--config", "/config/elastic-mcp.json5"]
    restart: unless-stopped
    networks:
      - aiops-network

networks:
  aiops-network:
    external: true
```

**配置文件**: `elastic-mcp-aiops.json5`

```json5
{
  "elasticsearch": {
    "url": "${ES_URL}",
    "api_key": "${ES_API_KEY}",
    
    // 索引删除权限控制
    "allow_delete_patterns": [
      "temp_*",
      "old_*",
      "test_*"
    ],
    "deny_delete_patterns": [
      "prod_*",
      ".kibana*",
      ".security*"
    ],
    
    // 工具配置
    "tools": {
      "exclude": []  // 不排除任何工具
    }
  }
}
```

---

### 4.2 Dify 工作流集成

#### 场景 1: 集群健康检查
```yaml
节点: "ES 状态检查"
类型: LLM 节点
工具: elasticsearch-mcp-server.get_cluster_health

Prompt: |
  检查 Elasticsearch 集群健康状态。
  如果状态为 red，调用 get_nodes_info 进一步诊断。
  如果状态为 yellow，检查 unassigned_shards 数量。
```

#### 场景 2: 索引清理
```yaml
节点: "索引清理决策"
类型: LLM 节点
工具: 
  - elasticsearch-mcp-server.list_indices_detailed
  - elasticsearch-mcp-server.delete_index

Prompt: |
  1. 列出所有 temp_* 和 old_* 开头的索引
  2. 过滤出 7 天前创建的索引
  3. 询问用户确认后删除
```

#### 场景 3: 数据导入验证
```yaml
节点 1: "执行数据导入"
类型: 工具节点
工具: ansible-mcp-server.run_playbook
参数:
  playbook: "elasticdump_import_data.yml"
  inventory: "inventory.ini"

节点 2: "验证导入结果"
类型: LLM 节点
工具:
  - elasticsearch-mcp-server.count_documents
  - elasticsearch-mcp-server.get_sample_documents

Prompt: |
  1. 统计索引文档数量
  2. 获取最新 3 条数据样本
  3. 验证数据结构是否正确
  4. 生成验证报告
```

---

### 4.3 测试用例

#### 单元测试 (`tests/test_tools.rs`)

```rust
#[tokio::test]
async fn test_get_cluster_health() {
    let mcp = setup_test_mcp().await;
    let result = mcp.get_cluster_health(...).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().status, "green");
}

#[tokio::test]
async fn test_delete_index_safety() {
    let mcp = setup_test_mcp().await;
    
    // 测试 1: 没有 confirm 参数应该失败
    let result = mcp.delete_index(DeleteIndexParams {
        index: "test_index".to_string(),
        confirm: false,
    }).await;
    assert!(result.is_err());
    
    // 测试 2: 删除系统索引应该失败
    let result = mcp.delete_index(DeleteIndexParams {
        index: ".kibana".to_string(),
        confirm: true,
    }).await;
    assert!(result.is_err());
}
```

#### 集成测试 (`tests/integration_test.sh`)

```bash
#!/bin/bash
# 测试 MCP Server 端到端功能

# 1. 启动 MCP Server
docker-compose up -d

# 2. 等待服务就绪
curl --retry 10 --retry-delay 2 http://localhost:30090/ping

# 3. 测试集群健康检查
curl -X POST http://localhost:30090/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "tools/call",
    "params": {
      "name": "get_cluster_health",
      "arguments": {}
    },
    "id": 1
  }' | jq '.result.content[1].json.status'

# 4. 测试创建索引
curl -X POST http://localhost:30090/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "tools/call",
    "params": {
      "name": "create_index",
      "arguments": {
        "index": "test_integration",
        "settings": {
          "number_of_shards": 1,
          "number_of_replicas": 0
        }
      }
    },
    "id": 2
  }'

# 5. 测试删除索引
curl -X POST http://localhost:30090/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "tools/call",
    "params": {
      "name": "delete_index",
      "arguments": {
        "index": "test_integration",
        "confirm": true
      }
    },
    "id": 3
  }'

# 6. 清理
docker-compose down
```

---

## 📊 迁移对比表 (Migration Comparison)

| Ansible Playbook | 操作类型 | 迁移目标 | 优先级 | 状态 |
|:---|:---|:---|:---|:---|
| **elasticsearch_status.yml** | 集群状态检查 | ✅ `get_cluster_health`<br>✅ `get_nodes_info`<br>✅ `list_indices_detailed` | P0 | Phase 1 |
| **elasticdump_create_indices.yml** | 索引创建 | ✅ `create_index` | P1 | Phase 2 |
| **elasticdump_delete_indices.yml** | 索引删除 | ✅ `delete_index` | P1 | Phase 2 |
| **elasticdump_import_data.yml** | 数据导入 | ⚠️ **混合模式**<br>Ansible 执行导入<br>MCP 验证结果 | P2 | Phase 3 |
| **elasticdump_backup.yml** | 数据备份 | ⚠️ **混合模式**<br>Ansible 执行备份<br>MCP 触发 | P2 | Phase 3 |
| **elasticsearch_deploy.yml** | ES 安装部署 | ❌ **保留在 Ansible** | - | 不迁移 |
| **elasticsearch_start/stop/restart.yml** | 服务启停 | ❌ **保留在 Ansible** | - | 不迁移 |

---

## 🚀 实施路线图 (Implementation Roadmap)

### Week 1: Phase 1 - 只读诊断能力
- **Day 1-2**: 实现 `get_cluster_health` 和 `get_nodes_info`
- **Day 3**: 实现 `list_indices_detailed`
- **Day 4**: 单元测试 + 文档
- **Day 5**: Dify 集成测试

### Week 2: Phase 2 - 索引管理能力
- **Day 1-2**: 实现 `create_index` 和 `delete_index`
- **Day 3**: 实现权限控制和安全检查
- **Day 4**: 实现 `get_index_settings`
- **Day 5**: 单元测试 + 集成测试

### Week 3: Phase 3 - 数据验证能力
- **Day 1-2**: 实现 `count_documents` 和 `get_sample_documents`
- **Day 3**: 性能优化和错误处理
- **Day 4**: 单元测试
- **Day 5**: 端到端集成测试

### Week 4: Phase 4 - 集成与文档
- **Day 1-2**: Docker Compose 配置和部署脚本
- **Day 3**: Dify 工作流集成示例
- **Day 4**: 完善文档和使用指南
- **Day 5**: 生产环境部署和验收

---

## 📝 开发规范 (Development Guidelines)

### 代码风格
```rust
// 1. 所有 Tool 函数必须包含详细的文档注释
/// Tool: Get cluster health
///
/// # Arguments
/// * `wait_for_status` - Optional status to wait for (green, yellow, red)
/// * `timeout` - Optional timeout duration (default: 30s)
///
/// # Returns
/// Cluster health information including status, node count, and shard statistics
#[tool(
    description = "Get Elasticsearch cluster health status",
    annotations(title = "Get cluster health", read_only_hint = true)
)]
async fn get_cluster_health(...) -> Result<CallToolResult, rmcp::Error>

// 2. 统一错误处理模式
match es_client.operation().send().await {
    Ok(response) => {
        let data: DataType = read_json(response).await?;
        Ok(CallToolResult::success(vec![Content::json(data)?]))
    }
    Err(e) => {
        tracing::error!("Operation failed: {:?}", e);
        Ok(CallToolResult::error(format!(
            "Failed to perform operation: {}. Check connection and credentials.",
            e
        )))
    }
}

// 3. 参数验证
fn validate_params(params: &Params) -> Result<(), String> {
    if params.index.is_empty() {
        return Err("Index name cannot be empty".to_string());
    }
    if params.index.starts_with(".") && !params.allow_system_indices {
        return Err("Cannot operate on system indices".to_string());
    }
    Ok(())
}
```

### 测试覆盖率要求
- **单元测试**: 覆盖率 ≥ 80%
- **集成测试**: 覆盖所有核心场景
- **错误场景测试**: 覆盖所有可能的失败路径

### 文档要求
- 每个 Tool 必须有中英文双语说明
- 提供完整的参数示例和返回值示例
- 包含 Agent 使用场景示例

---

## 🔒 安全考虑 (Security Considerations)

### 1. 索引删除保护
```rust
// 多层防护机制
fn can_delete_index(index: &str, config: &Config) -> Result<(), String> {
    // 1. 检查 deny_patterns (最高优先级)
    for pattern in &config.deny_delete_patterns {
        if matches_pattern(index, pattern) {
            return Err(format!("Index {} is protected by deny pattern", index));
        }
    }
    
    // 2. 检查 allow_patterns
    let mut allowed = false;
    for pattern in &config.allow_delete_patterns {
        if matches_pattern(index, pattern) {
            allowed = true;
            break;
        }
    }
    
    if !allowed {
        return Err(format!("Index {} is not in allow list", index));
    }
    
    // 3. 系统索引保护
    if index.starts_with(".") {
        return Err("Cannot delete system indices".to_string());
    }
    
    Ok(())
}
```

### 2. API 认证
- 支持 API Key 和 Basic Auth
- 支持从 HTTP Header 传递认证信息
- 支持 SSL/TLS 证书验证

### 3. 操作审计
```rust
// 记录所有写操作
tracing::info!(
    target: "audit",
    user = ?req_ctx.user,
    action = "delete_index",
    index = %params.index,
    "Index deletion requested"
);
```

---

## 📈 性能优化 (Performance Optimization)

### 1. 连接池复用
```rust
// 使用 Elasticsearch Client 的内置连接池
let client = Elasticsearch::new(
    elasticsearch::http::transport::TransportBuilder::new(url)
        .connection_pool(ConnectionPool::new(10))  // 最多 10 个连接
        .build()?
);
```

### 2. 查询优化
```rust
// list_indices_detailed: 只返回必要字段
es_client.cat().indices(...)
    .h(&["index", "health", "status", "docs.count", "store.size"])
    .format("json")
    .send()
    .await?;

// get_sample_documents: 限制返回大小
es_client.search(...)
    .size(params.size.min(100))
    ._source(&["field1", "field2"])  // 只返回指定字段
    .send()
    .await?;
```

### 3. 缓存机制
```rust
// 对于频繁查询的集群元数据，使用短期缓存
use moka::future::Cache;

struct EsBaseTools {
    es_client: EsClientProvider,
    cluster_health_cache: Cache<(), ClusterHealth>,
}

impl EsBaseTools {
    async fn get_cluster_health_cached(&self) -> Result<ClusterHealth, Error> {
        self.cluster_health_cache
            .try_get_with((), async {
                // 缓存 30 秒
                self.get_cluster_health_internal().await
            })
            .await
    }
}
```

---

## 🎓 使用示例 (Usage Examples)

### 示例 1: 集群健康巡检
```python
# Dify 工作流 Python 代码节点
def health_check():
    # 1. 检查集群健康
    health = mcp_call("elasticsearch-mcp-server", "get_cluster_health", {})
    
    if health["status"] == "red":
        # 2. 获取节点信息
        nodes = mcp_call("elasticsearch-mcp-server", "get_nodes_info", {})
        
        # 3. 检查分片状态
        indices = mcp_call("elasticsearch-mcp-server", "list_indices_detailed", {
            "health": "red"
        })
        
        return {
            "status": "critical",
            "message": f"集群状态异常，{len(indices)} 个索引处于 red 状态",
            "details": {
                "health": health,
                "nodes": nodes,
                "red_indices": indices
            }
        }
    
    return {"status": "ok", "message": "集群健康"}
```

### 示例 2: 自动化索引清理
```python
# Dify 工作流 Python 代码节点
def cleanup_old_indices():
    import datetime
    
    # 1. 列出所有 temp_ 开头的索引
    indices = mcp_call("elasticsearch-mcp-server", "list_indices_detailed", {
        "index_pattern": "temp_*"
    })
    
    # 2. 过滤 7 天前的索引
    cutoff_date = datetime.datetime.now() - datetime.timedelta(days=7)
    old_indices = [
        idx for idx in indices
        if parse_date(idx["index"]) < cutoff_date
    ]
    
    # 3. 删除旧索引
    deleted = []
    for idx in old_indices:
        result = mcp_call("elasticsearch-mcp-server", "delete_index", {
            "index": idx["index"],
            "confirm": True
        })
        if result["success"]:
            deleted.append(idx["index"])
    
    return {
        "deleted_count": len(deleted),
        "deleted_indices": deleted
    }
```

### 示例 3: 数据导入验证
```python
# Dify 工作流 Python 代码节点
def verify_data_import(index_name, expected_count):
    # 1. 统计文档数量
    count_result = mcp_call("elasticsearch-mcp-server", "count_documents", {
        "index": index_name
    })
    
    actual_count = count_result["count"]
    
    # 2. 获取样本数据
    samples = mcp_call("elasticsearch-mcp-server", "get_sample_documents", {
        "index": index_name,
        "size": 5,
        "sort": [{"timestamp": "desc"}]
    })
    
    # 3. 验证结果
    if actual_count < expected_count * 0.95:
        return {
            "status": "warning",
            "message": f"导入数据不足，预期 {expected_count}，实际 {actual_count}",
            "samples": samples
        }
    
    return {
        "status": "success",
        "message": f"数据导入成功，共 {actual_count} 条记录",
        "samples": samples
    }
```

---

## 🔄 版本兼容性 (Version Compatibility)

| ES MCP Server | Elasticsearch | Rust | Docker |
|:---|:---|:---|:---|
| v0.4.x | 8.x, 9.x | 1.70+ | 20.10+ |
| v0.5.x (计划) | 8.x, 9.x, 10.x | 1.75+ | 20.10+ |

---

## 📚 参考资料 (References)

1. **Elasticsearch Official Docs**:
   - [Cluster Health API](https://www.elastic.co/guide/en/elasticsearch/reference/current/cluster-health.html)
   - [Index APIs](https://www.elastic.co/guide/en/elasticsearch/reference/current/indices.html)
   - [Cat APIs](https://www.elastic.co/guide/en/elasticsearch/reference/current/cat.html)

2. **MCP Protocol**:
   - [Model Context Protocol Specification](https://modelcontextprotocol.io/docs)
   - [MCP Rust SDK (rmcp)](https://github.com/modelcontextprotocol/rust-sdk)

3. **Ansible Playbooks** (迁移参考):
   - `mcp/ansible-mcp-server/playbooks/elasticsearch_*.yml`
   - `mcp/ansible-mcp-server/playbooks/elasticdump_*.yml`

---

## ✅ Acceptance Criteria | 验收标准

### Phase 1 完成标准
- [ ] `get_cluster_health` 返回正确的集群状态
- [ ] `get_nodes_info` 返回所有节点的详细信息
- [ ] `list_indices_detailed` 支持按健康状态和大小排序
- [ ] 所有 Tool 通过单元测试
- [ ] 在 Dify 中成功调用并获得正确结果

### Phase 2 完成标准
- [ ] `create_index` 支持自定义 Mapping 和 Settings
- [ ] `delete_index` 实现多层安全检查
- [ ] 权限控制配置生效（allow/deny patterns）
- [ ] 无法删除系统索引和受保护索引
- [ ] 所有 Tool 通过单元测试和集成测试

### Phase 3 完成标准
- [ ] `count_documents` 返回准确的文档数量
- [ ] `get_sample_documents` 返回指定数量的样本数据
- [ ] 性能测试通过（查询响应时间 < 1s）
- [ ] 在 Dify 中成功验证 Ansible 导入的数据

### Phase 4 完成标准
- [ ] Docker Compose 配置正确，服务正常启动
- [ ] 所有集成测试通过
- [ ] Dify 工作流示例运行成功
- [ ] 文档完整，包含所有 Tool 的使用说明
- [ ] 生产环境部署成功，稳定运行 7 天

---

## 📞 联系方式 (Contact)

**项目负责人**: AIOps Team  
**文档维护**: Cursor AI Assistant  
**更新日期**: 2026-01-20

---

**Next Steps | 下一步行动**:
1. ✅ 评审本文档，确认技术方案
2. 🔨 开始 Phase 1 开发（只读诊断能力）
3. 🧪 编写单元测试和集成测试
4. 📦 构建 Docker 镜像并部署到测试环境
5. 🔗 在 Dify 中集成并验证功能
