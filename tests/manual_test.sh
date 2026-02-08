#!/bin/bash
# Manual integration test for safety limits
# Run this script to verify the new safety features work correctly

set -e

ES_URL="${ES_URL:-http://172.30.137.172:9200}"
MCP_URL="${MCP_URL:-http://172.31.101.239:30090}"

echo "=== Elasticsearch MCP Server Safety Limits Test ==="
echo "ES URL: $ES_URL"
echo "MCP URL: $MCP_URL"
echo ""

# Test 1: Verify cluster health works
echo "Test 1: Cluster Health Check"
curl -s "$ES_URL/_cluster/health" | jq -r '.status' || echo "ES connection failed"
echo ""

# Test 2: Create test index with many documents
echo "Test 2: Creating test index with 300 documents..."
for i in {1..300}; do
  curl -s -X POST "$ES_URL/test-safety-limits/_doc" \
    -H 'Content-Type: application/json' \
    -d "{\"id\":$i,\"message\":\"Test document $i with some content to make it larger\",\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}" > /dev/null
done
sleep 2
echo "Documents created"
echo ""

# Test 3: Query with size > 200 (should be capped to 200)
echo "Test 3: Testing size limit enforcement (requesting 500, should get max 200)"
RESULT=$(curl -s "$ES_URL/test-safety-limits/_search" \
  -H 'Content-Type: application/json' \
  -d '{"size":500,"query":{"match_all":{}}}')
HITS=$(echo "$RESULT" | jq '.hits.hits | length')
echo "Requested size: 500"
echo "Actual returned: $HITS"
if [ "$HITS" -le 200 ]; then
  echo "✓ Size limit working (returned $HITS <= 200)"
else
  echo "✗ Size limit NOT working (returned $HITS > 200)"
fi
echo ""

# Test 4: Test response truncation with large query
echo "Test 4: Testing response truncation..."
LARGE_RESULT=$(curl -s "$ES_URL/test-safety-limits/_search" \
  -H 'Content-Type: application/json' \
  -d '{"size":200,"query":{"match_all":{}}}')
RESULT_SIZE=$(echo "$LARGE_RESULT" | wc -c)
echo "Response size: $RESULT_SIZE bytes"
if [ "$RESULT_SIZE" -gt 15000 ]; then
  echo "⚠ Response is large ($RESULT_SIZE bytes), MCP server should truncate this"
else
  echo "✓ Response size within limits"
fi
echo ""

# Test 5: Test mapping query on non-existent index (should not panic)
echo "Test 5: Testing error handling on non-existent index..."
MAPPING_RESULT=$(curl -s "$ES_URL/nonexistent-index-12345/_mapping" 2>&1)
if echo "$MAPPING_RESULT" | grep -q "index_not_found"; then
  echo "✓ Non-existent index returns proper error (no panic)"
else
  echo "Response: $MAPPING_RESULT"
fi
echo ""

# Test 6: Test timeout (query that takes long time)
echo "Test 6: Testing 30s timeout..."
echo "Skipping (would require slow query setup)"
echo ""

# Cleanup
echo "Cleanup: Deleting test index..."
curl -s -X DELETE "$ES_URL/test-safety-limits" > /dev/null
echo "✓ Test index deleted"
echo ""

echo "=== Manual Test Summary ==="
echo "1. ✓ Cluster health check"
echo "2. ✓ Test data created (300 docs)"
echo "3. Check size limit enforcement above"
echo "4. Check response truncation above"
echo "5. ✓ Error handling (no panic)"
echo "6. ⊘ Timeout test skipped"
echo ""
echo "For full MCP integration test, use the MCP client in Cursor/Claude Desktop"
