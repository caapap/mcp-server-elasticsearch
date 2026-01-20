#!/bin/bash
# Phase 1 Integration Test Script
# Tests: get_cluster_health, get_nodes_info, list_indices_detailed

set -e

# Configuration
MCP_URL="http://localhost:30090"
ES_URL="${ES_URL:-http://172.30.137.172:9200}"

echo "=========================================="
echo "Phase 1 Tools Integration Test"
echo "=========================================="
echo ""

# Function to call MCP tool
call_tool() {
    local tool_name=$1
    local arguments=$2
    
    echo "Testing tool: $tool_name"
    echo "Arguments: $arguments"
    
    curl -s -X POST "${MCP_URL}" \
      -H "Content-Type: application/json" \
      -d "{
        \"jsonrpc\": \"2.0\",
        \"method\": \"tools/call\",
        \"params\": {
          \"name\": \"$tool_name\",
          \"arguments\": $arguments
        },
        \"id\": 1
      }" | jq '.'
    
    echo ""
    echo "=========================================="
    echo ""
}

echo "Test 1: get_cluster_health (no parameters)"
call_tool "get_cluster_health" "{}"

echo "Test 2: get_cluster_health (with timeout)"
call_tool "get_cluster_health" '{"timeout": "5s"}'

echo "Test 3: get_nodes_info (default)"
call_tool "get_nodes_info" "{}"

echo "Test 4: list_indices_detailed (all indices)"
call_tool "list_indices_detailed" '{"index_pattern": "*"}'

echo "Test 5: list_indices_detailed (with health filter)"
call_tool "list_indices_detailed" '{"index_pattern": "*", "health": "green"}'

echo "Test 6: list_indices_detailed (with sorting)"
call_tool "list_indices_detailed" '{"index_pattern": "*", "sort_by": "store.size"}'

echo "=========================================="
echo "All tests completed!"
echo "=========================================="
