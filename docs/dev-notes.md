## 2. 代码解释：get_cluster_health

这段 Rust 代码定义了一个名为 `get_cluster_health` 的 MCP 工具。即使没学过 Rust，您可以将其理解为一个**类型安全、异步的 API 接口函数**。

下面是逐行通俗解释：

### 2.1 文档注释 (///)
```rust
/// Tool: Get cluster health
/// ... (参数和返回值说明)
```
这些注释会被提取为工具的说明文档，告诉 LLM (如 Claude/Dify) 这个工具是做什么的。

### 2.2 宏注解 (#[tool(...)])
```rust
#[tool(
    description = "Get Elasticsearch cluster health status",
    annotations(title = "Get cluster health", read_only_hint = true)
)]
```
这是 Rust 的"装饰器"。它告诉 MCP 框架："把下面这个函数注册为一个 Tool"。
- `description`: 工具描述
- `annotations`: 额外元数据 (title 是标题，read_only_hint 告诉模型这是一个只读操作，很安全)

### 2.3 函数定义 (async fn)
```rust
async fn get_cluster_health(
    &self,
    req_ctx: RequestContext<RoleServer>,
    Parameters(params): Parameters<ClusterHealthParams>,
) -> Result<CallToolResult, rmcp::Error> {
```
- `async`: 异步函数，类似 JS 的 async
- `&self`: 面向对象写法，表示"这个实例"（类似 Python 的 self 或 JS 的 this）
- `req_ctx`: 请求上下文，包含 API Key 等认证信息
- `Parameters(params)`: 参数解构。它会自动把 JSON 入参转换成我们定义的 Rust 结构体 ClusterHealthParams
- `-> Result<...>`: 返回值类型。Rust 没有异常抛出机制，而是返回 Result (成功或失败)

### 2.4 获取客户端
```rust
let es_client = self.es_client.get(req_ctx);
```
根据请求上下文 (req_ctx) 获取一个鉴权后的 ES 客户端实例。

### 2.5 构建请求 (Builder Pattern)
```rust
let mut health_request = es_client.cluster().health(ClusterHealthParts::None);
```
- 开始构建一个健康检查请求
- `ClusterHealthParts::None` 表示不针对特定索引，查整个集群
- `mut` 表示 `health_request` 这个变量是"可变的" (mutable)，因为我们后面要根据条件修改它

### 2.6 处理可选参数 wait_for_status
```rust
if let Some(status) = &params.wait_for_status {
    health_request = health_request.wait_for_status(match status.as_str() {
        "green" => elasticsearch::params::WaitForStatus::Green,
        "yellow" => elasticsearch::params::WaitForStatus::Yellow,
        _ => elasticsearch::params::WaitForStatus::Red, // 默认 fallback 到 Red
    });
}
```
- `if let Some(...)` 是 Rust 特有的语法，意思是：如果 `params.wait_for_status` 有值 (不是 null)，就执行代码块
- `match` 类似 switch，但更强大。根据字符串匹配对应的枚举值

### 2.7 处理可选参数 timeout
```rust
if let Some(timeout) = &params.timeout {
    health_request = health_request.timeout(timeout);
}
```

### 2.8 发送请求
```rust
let response = health_request.send().await;
```
`.send().await`: 发送 HTTP 请求并等待响应。

### 2.9 解析响应
```rust
let health: serde_json::Value = read_json(response).await?;
```
- `read_json` 是一个辅助函数，负责把 HTTP 响应体解析成 JSON
- `?` 是 Rust 的语法糖：如果出错，直接返回 Error；如果成功，取出里面的值赋给 health
- `serde_json::Value` 表示"任意 JSON 结构"，类似于 Object

### 2.10 返回结果
```rust
Ok(CallToolResult::success(vec![Content::json(health)?]))
```
包装成 MCP 协议要求的 CallToolResult 格式返回。

---

## 3. 总结核心概念

### 3.1 强类型
所有输入参数（`ClusterHealthParams`）都必须先定义结构体，Rust 会自动校验。

### 3.2 安全性
使用 `Result` 和 `?` 显式处理错误，不会因为未捕获异常导致程序崩溃。

### 3.3 链式调用
通过 `.wait_for_status(...).timeout(...)` 像搭积木一样构建请求，这是 Rust 中常见的 Builder 模式。