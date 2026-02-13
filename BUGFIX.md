# Elasticsearch MCP Server Bug Fix 记录

本文档记录了 Elasticsearch MCP Server 项目自 2026 年 1 月以来的关键 Bug 修复，用于长期维护和经验积累。

---

## 2026-02 (当前)

### 1. Dify 客户端 JSON 解析错误 (Extra data / Expecting ',' delimiter)
- **日期**: 2026-02-12
- **问题描述**: Dify 在调用工具时报错 `Extra data: line 1 column 3 (char 2)` 或 `Expecting ',' delimiter`。
- **根因分析**: 
    1. **多 Content 块问题**: 原先工具返回 `vec![Content::text(header), Content::text(json_data)]`，Dify 会尝试将第二个 Content 块独立解析为 JSON，若截断导致 JSON 不完整则报错。
    2. **换行符干扰**: 响应字符串中包含 `\n`，在某些传输层（如 Streamable HTTP）下干扰了 Dify 的 JSON 解析器。
- **修复方案**:
    1. **结构化响应**: 统一返回单个 `Content::json`，格式为 `{ "message": "...", "data": { ... } }`。
    2. **移除换行**: 将响应中的 `\n` 替换为空格，确保 Dify 能够稳定解析。
    3. **安全截断**: 新增 `pack_json_value` 函数，当数据过大时返回 `{ "truncated": true, "preview": "..." }`。
- **关联文件**: `src/servers/elasticsearch/base_tools.rs`, `tests/http_tests.rs`

### 2. LLM 上下文溢出导致对话中断
- **日期**: 2026-02-08
- **问题描述**: 当集群分片较多（如 > 2000 个）时，调用 `get_shards` 会返回超大文本，超出模型上下文限制。
- **修复方案**:
    1. **默认截断调优**: 将单次工具响应上限从 15,000 字符下调至 8,000 字符（约 4K Tokens），确保 4-5 次连续调用不超 32K 限制。
    2. **智能聚合**: 优化 `get_shards` 工具，在未指定索引且分片数 > 200 时，自动返回按节点聚合的摘要（节点名、分片数、主副本分布、文档总数），不再返回原始明细。
- **关联文件**: `src/servers/elasticsearch/base_tools.rs`, `PROMPT/ElasticSearchBot-ES7.x-latest.md`

### 3. Mapping 反序列化崩溃 (unwrap panic)
- **日期**: 2026-02-08
- **问题描述**: 在某些 ES 版本或特定索引下，`get_mappings` 返回的 JSON 结构包含未知字段，导致 Rust `serde` 反序列化失败并触发 `unwrap()` panic。
- **修复方案**:
    1. **容错处理**: 在 `Mapping` 结构体中使用 `#[serde(flatten)]` 接收所有未知字段，防止反序列化失败。
    2. **消除 Panic**: 将代码中的 `.unwrap()` 替换为 `ok_or_else` 错误处理，确保服务在异常情况下不崩溃。
- **关联文件**: `src/servers/elasticsearch/base_tools.rs`

### 4. Docker 编译失败 (Unstable Feature)
- **日期**: 2026-02-08
- **问题描述**: 使用了 Rust unstable 特性 `floor_char_boundary`，导致在标准 release 镜像中编译失败。
- **修复方案**: 手写 UTF-8 字符边界检查逻辑，替代 unstable 函数，确保在 stable Rust 下可编译。
- **关联文件**: `src/servers/elasticsearch/base_tools.rs`

---

## 2026-01

### 5. 知识库检索死循环
- **日期**: 2026-01-20
- **问题描述**: Prompt 中关于知识库检索的规则过于严格，导致 Agent 在未命中时反复尝试检索，形成死循环。
- **修复方案**: 放宽检索规则，允许 Agent 在知识库未命中时基于自身 ES 知识库构造查询，但需注明来源。
- **关联文件**: `PROMPT/ElasticSearchBot-ES7.x-latest.md`

---

## 维护指南
1. **提交规范**: 所有的 Bug Fix 提交应使用 `fix:` 前缀。
2. **文档同步**: 每次解决重大 Bug 后，应同步更新本项目下的 `BUGFIX.md`。
3. **测试要求**: 所有的修复必须通过 `cargo test` 及对应的集成测试。
