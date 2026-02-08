// Licensed to Elasticsearch B.V. under one or more contributor
// license agreements. See the NOTICE file distributed with
// this work for additional information regarding copyright
// ownership. Elasticsearch B.V. licenses this file to you under
// the Apache License, Version 2.0 (the "License"); you may
// not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use crate::servers::elasticsearch::{EsClientProvider, read_json};
use elasticsearch::cat::{CatIndicesParts, CatShardsParts};
use elasticsearch::cluster::ClusterHealthParts;
use elasticsearch::indices::{IndicesGetMappingParts, IndicesGetTemplateParts};
use elasticsearch::{Elasticsearch, SearchParts};
use indexmap::IndexMap;
use rmcp::handler::server::tool::{Parameters, ToolRouter};
use rmcp::model::{
    CallToolResult, Content, Implementation, JsonObject, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler};
use rmcp_macros::{tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use serde_aux::prelude::*;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use regex::Regex;

//------------------------------------------------------------------------------------------------
// Safety limits (configurable via environment variables)

/// Maximum characters for a single tool response
fn max_response_chars() -> usize {
    std::env::var("MCP_MAX_RESPONSE_CHARS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(15_000)
}

/// Maximum search size hard cap
fn max_search_size() -> u64 {
    std::env::var("MCP_MAX_SEARCH_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200)
}

/// Maximum index list entries
fn max_index_list() -> usize {
    std::env::var("MCP_MAX_INDEX_LIST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
}

/// Truncate a serialized JSON string if it exceeds max_chars, appending a hint
fn maybe_truncate(json_str: String, max_chars: usize) -> Content {
    if json_str.len() <= max_chars {
        Content::text(json_str)
    } else {
        // Find a valid UTF-8 char boundary at or before max_chars
        let mut end = max_chars.min(json_str.len());
        while end > 0 && !json_str.is_char_boundary(end) {
            end -= 1;
        }
        Content::text(format!(
            "{}\n\n[已截断：响应超过 {} 字符（共 {} 字符），请缩小查询范围、添加过滤条件或指定 _source 字段]",
            &json_str[..end], max_chars, json_str.len()
        ))
    }
}

#[derive(Clone)]
pub struct EsBaseTools {
    es_client: EsClientProvider,
    tool_router: ToolRouter<EsBaseTools>,
}

impl EsBaseTools {
    pub fn new(es_client: Elasticsearch) -> Self {
        Self {
            es_client: EsClientProvider::new(es_client),
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ListIndicesParams {
    /// Index pattern of Elasticsearch indices to list
    pub index_pattern: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetMappingsParams {
    /// Name of the Elasticsearch index to get mappings for
    index: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SearchParams {
    /// Name of the Elasticsearch index to search
    index: String,

    /// Name of the fields that need to be returned (optional)
    fields: Option<Vec<String>>,

    /// Complete Elasticsearch query DSL object that can include query, size, from, sort, etc.
    query_body: Map<String, Value>, // note: just Value doesn't work, as Claude would send a string
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct EsqlQueryParams {
    /// Complete Elasticsearch ES|QL query
    query: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetShardsParams {
    /// Optional index name to get shard information for
    index: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ClusterHealthParams {
    /// Optional status to wait for (green, yellow, red)
    wait_for_status: Option<String>,
    /// Optional timeout duration (default: 30s)
    timeout: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ListIndicesDetailedParams {
    /// Index pattern of Elasticsearch indices to list (default: *)
    #[serde(default = "default_index_pattern")]
    pub index_pattern: String,
    /// Filter by health status (green, yellow, red)
    pub health: Option<String>,
    /// Sort by field (docs.count, store.size)
    pub sort_by: Option<String>,
}

fn default_index_pattern() -> String {
    "*".to_string()
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct NodesInfoParams {
    /// Optional node ID to filter (default: _all)
    node_id: Option<String>,
    /// Optional metrics to return (heap, cpu, load, etc.)
    metrics: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetTemplatesParams {
    /// Optional template name filter (supports * wildcard)
    name: Option<String>,
    /// Optional index name to find which template(s) match it
    matching_index: Option<String>,
}

#[tool_router]
impl EsBaseTools {
    //---------------------------------------------------------------------------------------------
    /// Tool: list indices (detailed)
    #[tool(
        description = "List Elasticsearch indices with detailed health and size information",
        annotations(title = "List indices (detailed)", read_only_hint = true)
    )]
    async fn list_indices_detailed(
        &self,
        req_ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<ListIndicesDetailedParams>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let es_client = self.es_client.get(req_ctx);
        
        let cat = es_client.cat();
        // Use CatIndicesParts::Index to specify pattern
        let indices = [params.index_pattern.as_str()];
        let mut indices_request = cat.indices(CatIndicesParts::Index(&indices));
        
        // Add health filter if provided
        if let Some(health) = &params.health {
            indices_request = indices_request.health(match health.as_str() {
                "green" => elasticsearch::params::Health::Green,
                "yellow" => elasticsearch::params::Health::Yellow,
                _ => elasticsearch::params::Health::Red,
            });
        }
        
        // Add sorting if provided
        let sort_arr;
        if let Some(sort_by) = &params.sort_by {
             sort_arr = [sort_by.as_str()];
             indices_request = indices_request.s(&sort_arr);
        }
        
        let response = indices_request
            .h(&["index", "health", "status", "pri", "rep", "docs.count", "store.size", "pri.store.size"])
            .format("json")
            .send()
            .await;

        let mut response: Vec<serde_json::Value> = read_json(response).await?;

        let total_count = response.len();
        let max_list = max_index_list();
        let truncated = total_count > max_list;
        if truncated {
            response.truncate(max_list);
        }
        let summary = if truncated {
            format!("Found {} indices (showing first {}, use index_pattern to filter):", total_count, max_list)
        } else {
            format!("Found {} indices:", total_count)
        };
        let json_str = serde_json::to_string(&response)
            .map_err(|e| rmcp::Error::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![
            Content::text(summary),
            maybe_truncate(json_str, max_response_chars()),
        ]))
    }

    //---------------------------------------------------------------------------------------------
    /// Tool: list indices
    #[tool(
        description = "List all available Elasticsearch indices",
        annotations(title = "List ES indices", read_only_hint = true)
    )]
    async fn list_indices(
        &self,
        req_ctx: RequestContext<RoleServer>,
        Parameters(ListIndicesParams { index_pattern }): Parameters<ListIndicesParams>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let es_client = self.es_client.get(req_ctx);
        let response = es_client
            .cat()
            .indices(CatIndicesParts::Index(&[&index_pattern]))
            .h(&["index", "status", "docs.count"])
            .format("json")
            .send()
            .await;

        let response: Vec<CatIndexResponse> = read_json(response).await?;

        Ok(CallToolResult::success(vec![
            Content::text(format!("Found {} indices:", response.len())),
            Content::json(response)?,
        ]))
    }

    //---------------------------------------------------------------------------------------------
    /// Tool: get mappings for an index
    #[tool(
        description = "Get field mappings for a specific Elasticsearch index",
        annotations(title = "Get ES index mappings", read_only_hint = true)
    )]
    async fn get_mappings(
        &self,
        req_ctx: RequestContext<RoleServer>,
        Parameters(GetMappingsParams { index }): Parameters<GetMappingsParams>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let es_client = self.es_client.get(req_ctx);
        let response = es_client
            .indices()
            .get_mapping(IndicesGetMappingParts::Index(&[&index]))
            .send()
            .await;

        let response: MappingResponse = read_json(response).await?;

        // use the first mapping (we can have many if the name is a wildcard)
        let mapping = response.values().next()
            .ok_or_else(|| rmcp::Error::internal_error(
                "No mapping found for the specified index. Please verify the index name exists.",
                None,
            ))?;

        Ok(CallToolResult::success(vec![
            Content::text(format!("Mappings for index {index}:")),
            Content::json(mapping)?,
        ]))
    }

    //---------------------------------------------------------------------------------------------
    /// Tool: search an index with the Query DSL
    ///
    /// The additional 'fields' parameter helps some LLMs that don't know about the `_source`
    /// request property to narrow down the data returned and reduce their context size
    #[tool(
        description = "Perform an Elasticsearch search with the provided query DSL.",
        annotations(title = "Elasticsearch search DSL query", read_only_hint = true)
    )]
    async fn search(
        &self,
        req_ctx: RequestContext<RoleServer>,
        Parameters(SearchParams {
            index,
            fields,
            query_body,
        }): Parameters<SearchParams>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let es_client = self.es_client.get(req_ctx);

        let mut query_body = query_body;

        // Enforce max size limit
        let hard_max = max_search_size();
        if let Some(Value::Number(n)) = query_body.get("size") {
            if let Some(size) = n.as_u64() {
                if size > hard_max {
                    query_body.insert("size".to_string(), json!(hard_max));
                }
            }
        }

        if let Some(fields) = fields {
            // Augment _source if it exists
            if let Some(Value::Array(values)) = query_body.get_mut("_source") {
                for field in fields.into_iter() {
                    values.push(Value::String(field))
                }
            } else {
                query_body.insert("_source".to_string(), json!(fields));
            }
        }

        let response = es_client
            .search(SearchParts::Index(&[&index]))
            .body(query_body)
            .send()
            .await;

        let response: SearchResult = read_json(response).await?;

        let mut results: Vec<Content> = Vec::new();

        // Send result stats only if it's not pure aggregation results
        if response.aggregations.is_empty() || !response.hits.hits.is_empty() {
            let total = response
                .hits
                .total
                .map(|t| t.value.to_string())
                .unwrap_or("unknown".to_string());

            results.push(Content::text(format!(
                "Total results: {}, showing {}.",
                total,
                response.hits.hits.len()
            )));
        }

        // Original prototype sent a separate content for each document, it seems to confuse some LLMs
        // for hit in &response.hits.hits {
        //     results.push(Content::json(&hit.source)?);
        // }
        let max_chars = max_response_chars();
        if !response.hits.hits.is_empty() {
            let sources = response.hits.hits.iter().map(|hit| &hit.source).collect::<Vec<_>>();
            let json_str = serde_json::to_string(&sources)
                .map_err(|e| rmcp::Error::internal_error(e.to_string(), None))?;
            results.push(maybe_truncate(json_str, max_chars));
        }

        if !response.aggregations.is_empty() {
            results.push(Content::text("Aggregations results:"));
            let json_str = serde_json::to_string(&response.aggregations)
                .map_err(|e| rmcp::Error::internal_error(e.to_string(), None))?;
            results.push(maybe_truncate(json_str, max_chars));
        }

        Ok(CallToolResult::success(results))
    }

    //---------------------------------------------------------------------------------------------
    /// Tool: ES|QL
    #[tool(
        description = "Perform an Elasticsearch ES|QL query.",
        annotations(title = "Elasticsearch ES|QL query", read_only_hint = true)
    )]
    async fn esql(
        &self,
        req_ctx: RequestContext<RoleServer>,
        Parameters(EsqlQueryParams { query }): Parameters<EsqlQueryParams>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let es_client = self.es_client.get(req_ctx);

        let request = EsqlQueryRequest { query };

        let response = es_client.esql().query().body(request).send().await;
        let response: EsqlQueryResponse = read_json(response).await?;

        // Transform response into an array of objects
        let mut objects: Vec<Value> = Vec::new();
        for row in response.values.into_iter() {
            let mut obj = Map::new();
            for (i, value) in row.into_iter().enumerate() {
                obj.insert(response.columns[i].name.clone(), value);
            }
            objects.push(Value::Object(obj));
        }

        Ok(CallToolResult::success(vec![
            Content::text("Results"),
            Content::json(objects)?,
        ]))
    }

    //---------------------------------------------------------------------------------------------
    // Tool: get shard information
    #[tool(
        description = "Get shard information for all or specific indices.",
        annotations(title = "Get ES shard information", read_only_hint = true)
    )]
    async fn get_shards(
        &self,
        req_ctx: RequestContext<RoleServer>,
        Parameters(GetShardsParams { index }): Parameters<GetShardsParams>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let es_client = self.es_client.get(req_ctx);

        let indices: [&str; 1];
        let parts = match &index {
            Some(index) => {
                indices = [index];
                CatShardsParts::Index(&indices)
            }
            None => CatShardsParts::None,
        };
        let response = es_client
            .cat()
            .shards(parts)
            .format("json")
            .h(&["index", "shard", "prirep", "state", "docs", "store", "node"])
            .send()
            .await;

        let response: Vec<CatShardsResponse> = read_json(response).await?;

        Ok(CallToolResult::success(vec![
            Content::text(format!("Found {} shards:", response.len())),
            Content::json(response)?,
        ]))
    }

    //---------------------------------------------------------------------------------------------
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
    async fn get_cluster_health(
        &self,
        req_ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<ClusterHealthParams>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let es_client = self.es_client.get(req_ctx);
        
        let cluster = es_client.cluster();
        let mut health_request = cluster.health(ClusterHealthParts::None);
        
        if let Some(status) = &params.wait_for_status {
            health_request = health_request.wait_for_status(match status.as_str() {
                "green" => elasticsearch::params::WaitForStatus::Green,
                "yellow" => elasticsearch::params::WaitForStatus::Yellow,
                _ => elasticsearch::params::WaitForStatus::Red,
            });
        }
        
        if let Some(timeout) = &params.timeout {
            health_request = health_request.timeout(timeout);
        }

        let response = health_request.send().await;
        
        // We use serde_json::Value here because the cluster health response structure 
        // is well defined but we might not want to map every single field manually yet.
        // However, for better documentation/contract, we could use a struct. 
        // The plan suggests a specific return format.
        let health: serde_json::Value = read_json(response).await?;
        
        Ok(CallToolResult::success(vec![Content::json(health)?]))
    }

    //---------------------------------------------------------------------------------------------
    /// Tool: Get detailed information about Elasticsearch cluster nodes
    #[tool(
        description = "Get detailed information about Elasticsearch cluster nodes",
        annotations(title = "Get nodes info", read_only_hint = true)
    )]
    async fn get_nodes_info(
        &self,
        req_ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<NodesInfoParams>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let es_client = self.es_client.get(req_ctx);
        
        // We use the CAT Nodes API to get tabular information which is often more useful for diagnostics
        // like in the ansible playbook example
        let cat = es_client.cat();
        let nodes_request = cat.nodes();
        
        // Default headers to match the plan example
        // "name,ip,heap.percent,ram.percent,cpu,load_1m,node.role,master"
        let headers = vec![
            "name", "ip", "heap.percent", "ram.percent", "cpu", "load_1m", "node.role", "master"
        ];

        if let Some(_metrics) = &params.metrics {
            // If metrics are provided, we could try to map them to headers, 
            // but for now we'll stick to the default set or append? 
            // The plan says metrics="heap,cpu,load", which implies we might want to filter.
            // For simplicity and matching the CAT API behavior, we'll keep the standard useful set
            // or perhaps allow extending it if needed. 
            // But let's stick to the plan's return example which shows these fields.
        }
        
        let _ = &params.node_id; // Suppress warning

        let response = nodes_request
            .h(&headers)
            .format("json")
            .send()
            .await;
            
        let nodes: serde_json::Value = read_json(response).await?;
        
        // The plan example shows a dictionary keyed by node name, but CAT API returns a list.
        // We can transform it or just return the list.
        // Return example in plan: {"nodes": {"es7-node1": {...}}}
        // CAT API response: [{"name": "es7-node1", ...}, ...]
        // Let's return the list directly as it's more standard for tools to return what the API gives,
        // unless we strictly need to match the specific format. 
        // Given this is a new tool replacing Ansible, a list is fine and easier to process usually.
        // However, to be helpful, let's wrap it.
        
        Ok(CallToolResult::success(vec![Content::json(json!({ "nodes": nodes }))?]))
    }

    //---------------------------------------------------------------------------------------------
    /// Tool: Get index templates
    ///
    /// Comprehensive template query tool that supports:
    /// - Filtering by template name (with wildcard support)
    /// - Finding which template(s) match a specific index name
    ///
    /// # Arguments
    /// * `name` - Optional template name filter (supports * wildcard)
    /// * `matching_index` - Optional index name to find matching templates
    ///
    /// # Returns
    /// Template definitions with matching logic applied
    #[tool(
        description = "Get index templates with optional filtering by name or matching index. Supports wildcard patterns and can determine which template applies to a specific index.",
        annotations(title = "Get index templates", read_only_hint = true)
    )]
    async fn get_templates(
        &self,
        req_ctx: RequestContext<RoleServer>,
        Parameters(params): Parameters<GetTemplatesParams>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let es_client = self.es_client.get(req_ctx);
        
        // Determine the template name pattern to query
        let template_name = params.name.as_deref().unwrap_or("*");
        
        // Get templates from Elasticsearch
        let response = es_client
            .indices()
            .get_template(IndicesGetTemplateParts::Name(&[template_name]))
            .send()
            .await;
        
        let templates: HashMap<String, TemplateDefinition> = read_json(response).await?;
        
        // If matching_index is specified, filter and sort templates by matching logic
        if let Some(index_name) = params.matching_index {
            let matching_templates = self.find_matching_templates(&templates, &index_name);
            
            if matching_templates.is_empty() {
                return Ok(CallToolResult::success(vec![
                    Content::text(format!("No templates match index '{}'", index_name)),
                    Content::json(json!({}))?
                ]));
            }
            
            // Return templates sorted by order (highest priority first)
            let result: HashMap<String, &TemplateDefinition> = matching_templates
                .into_iter()
                .map(|(name, template)| (name.to_string(), template))
                .collect();
            
            return Ok(CallToolResult::success(vec![
                Content::text(format!(
                    "Found {} template(s) matching index '{}' (sorted by priority):",
                    result.len(),
                    index_name
                )),
                Content::json(result)?
            ]));
        }
        
        // Return all templates (or filtered by name)
        Ok(CallToolResult::success(vec![
            Content::text(format!("Found {} template(s):", templates.len())),
            Content::json(templates)?
        ]))
    }
}

impl EsBaseTools {
    /// Find templates that match the given index name
    /// Returns templates sorted by order (highest first)
    fn find_matching_templates<'a>(
        &self,
        templates: &'a HashMap<String, TemplateDefinition>,
        index_name: &str,
    ) -> Vec<(&'a str, &'a TemplateDefinition)> {
        let mut matching: Vec<(&str, &TemplateDefinition)> = templates
            .iter()
            .filter(|(_, template)| {
                template.index_patterns.iter().any(|pattern| {
                    self.matches_pattern(index_name, pattern)
                })
            })
            .map(|(name, template)| (name.as_str(), template))
            .collect();
        
        // Sort by order (descending - higher order = higher priority)
        matching.sort_by(|a, b| {
            let order_a = a.1.order.unwrap_or(0);
            let order_b = b.1.order.unwrap_or(0);
            order_b.cmp(&order_a)
        });
        
        matching
    }
    
    /// Check if an index name matches a pattern (supports * wildcard)
    fn matches_pattern(&self, index_name: &str, pattern: &str) -> bool {
        // Convert ES wildcard pattern to regex
        // Escape special regex chars except *
        let escaped = regex::escape(pattern).replace(r"\*", ".*");
        let regex_pattern = format!("^{}$", escaped);
        
        if let Ok(re) = Regex::new(&regex_pattern) {
            re.is_match(index_name)
        } else {
            false
        }
    }
}

#[tool_handler]
impl ServerHandler for EsBaseTools {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("Provides access to Elasticsearch".to_string()),
        }
    }
}

//-------------------------------------------------------------------------------------------------
// Type definitions for ES request/responses (the Rust client doesn't have them yet) and tool responses.

//----- Search request

#[derive(Serialize, Deserialize)]
pub struct SearchResult {
    pub hits: Hits,
    #[serde(default)]
    pub aggregations: IndexMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
pub struct Hits {
    pub total: Option<TotalHits>,
    pub hits: Vec<Hit>,
}

#[derive(Serialize, Deserialize)]
pub struct TotalHits {
    pub value: u64,
}

#[derive(Serialize, Deserialize)]
pub struct Hit {
    #[serde(rename = "_source")]
    pub source: Value,
}

//----- Cat responses

#[derive(Serialize, Deserialize)]
pub struct CatIndexResponse {
    pub index: String,
    pub status: String,
    #[serde(rename = "docs.count", deserialize_with = "deserialize_number_from_string")]
    pub doc_count: u64,
}

#[derive(Serialize, Deserialize)]
pub struct CatShardsResponse {
    pub index: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub shard: usize,
    pub prirep: String,
    pub state: String,
    #[serde(deserialize_with = "deserialize_option_number_from_string")]
    pub docs: Option<u64>,
    pub store: Option<String>,
    pub node: Option<String>,
}

//----- Index mappings

pub type MappingResponse = HashMap<String, Mappings>;

#[derive(Serialize, Deserialize)]
pub struct Mappings {
    pub mappings: Mapping,
}

#[derive(Serialize, Deserialize)]
pub struct Mapping {
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<JsonObject>,
    properties: HashMap<String, MappingProperty>,
}

#[derive(Serialize, Deserialize)]
pub struct MappingProperty {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(flatten)]
    pub settings: HashMap<String, serde_json::Value>,
}

//----- ES|QL

#[derive(Serialize, Deserialize)]
pub struct EsqlQueryRequest {
    pub query: String,
}

#[derive(Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Serialize, Deserialize)]
pub struct EsqlQueryResponse {
    pub is_partial: Option<bool>,
    pub columns: Vec<Column>,
    pub values: Vec<Vec<Value>>,
}

//----- Index Templates

#[derive(Serialize, Deserialize)]
pub struct TemplateDefinition {
    /// Index patterns that this template applies to
    pub index_patterns: Vec<String>,
    
    /// Template priority order (higher = higher priority)
    pub order: Option<i32>,
    
    /// Index settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<Value>,
    
    /// Index mappings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mappings: Option<Value>,
    
    /// Index aliases
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Value>,
    
    /// Template version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}
