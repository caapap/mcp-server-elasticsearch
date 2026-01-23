# Dify Agent 配置建议

## 模型选择
- **推荐**：`GPT-4o` 或 `Claude 3.5 Sonnet`
- **原因**：需要强大的推理能力来处理复杂的 DSL 生成和错误诊断

## 工具配置

### 1. 工具超时设置（重要！）
在 Dify 的工具配置中，为每个 MCP 工具设置合理的超时时间：

| 工具名称 | 建议超时时间 | 说明 |
|---------|-------------|------|
| `get_cluster_health` | 10 秒 | 健康检查应该很快，超过 10 秒说明有问题 |
| `get_nodes_info` | 15 秒 | 节点信息可能较多 |
| `list_indices` | 10 秒 | 索引列表通常很快 |
| `list_indices_detailed` | 20 秒 | 详细信息包含大小计算，可能较慢 |
| `search` | 30 秒 | 复杂查询可能需要更长时间 |
| `esql` | 30 秒 | ES\|QL 聚合可能较慢 |
| `get_mappings` | 15 秒 | Mapping 信息通常不大 |
| `get_shards` | 15 秒 | Shard 信息中等大小 |

**重点**：如果超时，Dify 应该返回明确的 `timeout` 错误给 Agent，而不是无限等待。

### 2. 错误处理策略（非常重要！）

**在 Dify 的 Agent 设置中必须启用以下配置**，否则工具失败后 Agent 会直接终止对话：

#### ✅ 必须开启的选项：
1. **「工具调用失败时继续对话」**
   - 位置：Agent 配置 → 工具设置 → 错误处理
   - 作用：当工具超时或返回错误时，Dify 不会直接终止对话，而是将错误信息传递给 Agent
   - **如果不开启**：Agent 会在工具失败后沉默终止（就像你遇到的情况）

2. **「将错误信息返回给 Agent」**
   - 作用：让 Agent 看到具体的错误信息（如 `Connection refused`, `Timeout`）
   - **如果不开启**：Agent 只知道"工具调用失败"，但不知道为什么

3. **「限制单次对话的工具调用次数」**
   - 建议值：10 次
   - 作用：防止 Agent 在工具失败后反复重试导致死循环

#### 📊 Dify 配置截图示意
```
[ ] 工具调用失败时终止对话  ← 不要勾选！
[✓] 工具调用失败时继续对话  ← 必须勾选！
[✓] 将错误详情返回给模型    ← 必须勾选！

工具调用超时设置：
  get_cluster_health: [ 10 ] 秒
  search:            [ 30 ] 秒
  ...
```

### 3. 开场白配置

```
你好！我是 Elasticsearch 运维助手。

我可以帮你：
✅ 监控集群健康状态
✅ 查询和分析索引数据
✅ 排查分片和节点问题
✅ 使用 ES|QL 或 DSL 高效检索

请告诉我你的需求，或者直接问"集群状态怎么样"来开始健康检查。
```

## 常见问题处理

### Q1: Agent 一直说"正在调用工具"但没有结果
**原因**：工具调用超时，但 Dify 没有返回错误。

**解决方案**：
1. 在 Dify 工具配置中设置明确的超时时间（见上表）。
2. 检查 MCP Server 是否正常运行：`docker ps | grep elasticsearch-mcp-server`
3. 检查 ES 连通性：`curl http://<ES_URL>:9200/_cluster/health`

### Q2: Agent 说"无法连接"但实际上 ES 是正常的
**可能原因**：
1. MCP Server 的 `ES_URL` 配置错误（检查 `docker-compose.yml`）
2. MCP Server 的 `ES_API_KEY` 或 `ES_USERNAME/PASSWORD` 配置错误
3. Docker 网络问题（检查 `docker network inspect mcp-network`）

**解决方案**：
```bash
# 进入 MCP Server 容器测试连通性
docker exec -it elasticsearch-mcp-server sh
curl -H "Authorization: ApiKey $ES_API_KEY" $ES_URL/_cluster/health
```

### Q3: Agent 返回的数据格式混乱
**原因**：Prompt 中的格式化指令不够明确。

**解决方案**：在 Prompt 的 `Constraints` 部分明确要求：
- 表格数据用 Markdown Table
- JSON 数据需要格式化（使用代码块）
- 长文本需要截断并说明

## 性能优化建议

1. **限制返回数据量**：
   - `search` 工具默认只返回 10 条
   - `list_indices_detailed` 只查询必要的字段

2. **避免全表扫描**：
   - 查询时总是带上时间范围（如 `@timestamp > NOW() - 1d`）
   - 使用索引模式而不是 `*`（如 `logs-*` 而不是 `*`）

3. **工具调用优化**：
   - 先调用轻量级工具（如 `list_indices`），再调用重量级工具（如 `search`）
   - 避免在循环中调用工具

## 监控与日志

建议在生产环境中启用：
1. **Dify 对话日志**：记录所有工具调用及其响应时间
2. **MCP Server 日志**：`docker logs -f elasticsearch-mcp-server`
3. **ES 慢查询日志**：监控哪些查询导致了性能问题

## 安全建议

1. **API Key 管理**：
   - 使用只读 API Key（不需要写权限）
   - 限制 API Key 的索引访问范围
   - 定期轮换 API Key

2. **网络隔离**：
   - MCP Server 应该与 ES 在同一个内网
   - 不要将 ES 直接暴露到公网

3. **审计**：
   - 记录所有通过 Agent 执行的查询
   - 定期审查异常查询模式
