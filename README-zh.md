# Elasticsearch MCP Server

> [!CAUTION]
> 此 MCP Server 已被官方标记为弃用 (Deprecated)，未来仅接收关键安全更新。
> 官方推荐使用 [Elastic Agent Builder](https://ela.st/agent-builder-docs) 的 [MCP endpoint](https://ela.st/agent-builder-mcp)，适用于 Elastic 9.2.0+ 和 Elasticsearch Serverless 项目。

> [!NOTE]
> **🚀 AIOps 增强版本**: 我们正在基于此项目开发 AIOps 专用增强版本，将 Ansible Playbook 中的 ES 操作迁移到 MCP，提升智能体的灵活性与决策能力。
> 
> - 📖 [完整开发计划](./docs/AIOPS_ENHANCEMENT_PLAN.md) - 详细的技术方案与实施路线图
> - ⚡ [快速开始指南](./docs/QUICK_START_ZH.md) - 5 分钟快速部署与使用

通过 Model Context Protocol (MCP) 从任意 MCP 客户端直接连接到你的 Elasticsearch 数据。

本服务器允许 AI Agent 通过自然语言对话的方式与 Elasticsearch 索引进行交互。

## Available Tools (可用工具)

* `list_indices`: 列出所有可用的 Elasticsearch 索引
* `get_mappings`: 获取指定索引的字段映射 (Field Mappings)
* `search`: 执行 Elasticsearch 查询 (支持 Query DSL)
* `esql`: 执行 ES|QL 查询
* `get_shards`: 获取所有或指定索引的分片 (Shard) 信息

## Prerequisites (前置要求)

* 一个运行中的 Elasticsearch 实例
* Elasticsearch 认证凭据 (API Key 或 Username/Password)
* 一个 MCP 客户端 (如 [Claude Desktop](https://claude.ai/download), [Goose](https://block.github.io/goose/))

**支持的 Elasticsearch 版本**

适用于 Elasticsearch `8.x` 和 `9.x` 版本。

## Installation & Setup (安装与配置)

> [!NOTE]
> 0.3.1 及更早版本通过 `npm` 安装，现已弃用。以下说明仅适用于 0.4.0 及更高版本。

本 MCP Server 以 Docker 镜像形式提供：`docker.elastic.co/mcp/elasticsearch`
支持 MCP 的 stdio、SSE 和 streamable-HTTP 协议。

**查看使用帮助**：
```bash
docker run docker.elastic.co/mcp/elasticsearch
```

**输出**：
```
Usage: elasticsearch-mcp-server <COMMAND>

Commands:
  stdio  Start a stdio server
  http   Start a streamable-HTTP server with optional SSE support
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## 使用方式

### 方式 1: stdio 协议

**环境变量配置**：
* `ES_URL`: Elasticsearch 集群的 URL 地址
* 认证方式（二选一）:
  * API Key: `ES_API_KEY`
  * Basic Auth: `ES_USERNAME` 和 `ES_PASSWORD`
* 可选: `ES_SSL_SKIP_VERIFY=true` 跳过 SSL/TLS 证书验证

**启动命令**：
```bash
docker run -i --rm -e ES_URL -e ES_API_KEY docker.elastic.co/mcp/elasticsearch stdio
```

**Claude Desktop 配置示例**：
```json
{
 "mcpServers": {
   "elasticsearch-mcp-server": {
    "command": "docker",
    "args": [
     "run", "-i", "--rm",
     "-e", "ES_URL", "-e", "ES_API_KEY",
     "docker.elastic.co/mcp/elasticsearch",
     "stdio"
    ],
    "env": {
      "ES_URL": "<your-elasticsearch-cluster-url>",
      "ES_API_KEY": "<your-elasticsearch-API-key>"
    }
   }
 }
}
```

### 方式 2: streamable-HTTP 协议 (推荐)

> 注意: streamable-HTTP 是推荐协议，SSE 已被标记为弃用。

**启动命令**：
```bash
docker run --rm -e ES_URL -e ES_API_KEY -p 8080:8080 docker.elastic.co/mcp/elasticsearch http
```

**如果无法传递启动参数**，可以通过 `CLI_ARGS` 环境变量传递：
```bash
docker run --rm -e ES_URL -e ES_API_KEY -e CLI_ARGS=http -p 8080:8080 ...
```

**端点地址**：
- MCP 端点: `http://<host>:8080/mcp`
- 健康检查: `http://<host>:8080/ping`

### 方式 3: Docker Compose 启动 (生产推荐)

创建 `docker-compose.yml` 文件：

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
      # 认证方式 1: API Key (推荐)
      - ES_API_KEY=your_api_key_here
      # 认证方式 2: Basic Auth (如果不用 API Key)
      # - ES_USERNAME=elastic
      # - ES_PASSWORD=your_password
      # 可选: 跳过 SSL 验证 (仅开发环境)
      # - ES_SSL_SKIP_VERIFY=true
    restart: unless-stopped
    command: ["http"]
    networks:
      - mcp-network

networks:
  mcp-network:
    driver: bridge
```

**启动服务**：
```bash
docker-compose up -d
```

**查看日志**：
```bash
docker-compose logs -f elasticsearch-mcp-server
```

**停止服务**：
```bash
docker-compose down
```
**端点地址**：
- MCP 端点: `http://<host>:30090/mcp`
- 健康检查: `http://<host>:30090/ping`

**Claude Desktop 配置 (需要 mcp-proxy 桥接)**：

1. 安装 `mcp-proxy`：
   ```bash
   uv tool install mcp-proxy
   ```

2. 配置 Claude Desktop：
   ```json
   {
     "mcpServers": {
       "elasticsearch-mcp-server": {
         "command": "/<home-directory>/.local/bin/mcp-proxy",
         "args": [
           "--transport=streamablehttp",
           "--header", "Authorization", "ApiKey <your-elasticsearch-API-key>",
           "http://<mcp-server-host>:<mcp-server-port>/mcp"
         ]
       }
     }
   }
   ```
   

## Docker 镜像构建 (Build)

### 使用 Makefile (推荐)

**本地单架构构建**：
```bash
make docker-image
```

**多架构构建 (amd64 + arm64)**：
```bash
make docker-multiarch-image
```

### 直接使用 Docker 命令

**单架构构建**：
```bash
docker build -t es-mcp:latest .
```

**多架构构建**：
```bash
docker buildx build --platform linux/amd64,linux/arm64 --tag es-mcp:latest .
```

## 推送到私有仓库

**示例：推送到自定义仓库**
```bash
# 构建镜像
docker build -t docker.kxdigit.com/stellar/elasticsearch-mcp-server:latest .

# 推送镜像
docker push docker.kxdigit.com/stellar/elasticsearch-mcp-server:latest
```

## 技术架构

- **语言**: Rust
- **构建方式**: 两阶段 Docker 构建 (Multi-stage Build)
  - Stage 1: 使用 `rust:1.89` 编译二进制文件
  - Stage 2: 使用轻量级 `wolfi-base` 运行时镜像
- **端口**: 8080 (HTTP 模式)
- **协议支持**: stdio, streamable-HTTP, SSE (已弃用)
