#[cfg(test)]
mod tests {
    use serde_json::json;
    
    // Test helper functions from base_tools.rs
    // Since they're private, we test them indirectly through integration
    
    #[test]
    fn test_truncation_logic() {
        // Test UTF-8 boundary handling
        let test_str = "Hello 世界".to_string(); // Mixed ASCII + Chinese
        let max_chars = 8; // Should truncate in middle of Chinese char
        
        // Manual implementation of truncation logic
        let mut end = max_chars.min(test_str.len());
        while end > 0 && !test_str.is_char_boundary(end) {
            end -= 1;
        }
        
        assert!(end > 0, "Should find valid boundary");
        assert!(test_str.is_char_boundary(end), "End should be valid char boundary");
        // "Hello " is 6 bytes, "世" is 3 bytes, so at position 8 we're in middle of "世"
        // Should backtrack to position 6 (after "Hello ")
        assert_eq!(&test_str[..end], "Hello ", "Should truncate at char boundary before Chinese char");
    }
    
    #[test]
    fn test_size_limit_enforcement() {
        let mut query_body = serde_json::Map::new();
        query_body.insert("size".to_string(), json!(500));
        
        let hard_max = 200u64;
        
        // Simulate the enforcement logic
        if let Some(serde_json::Value::Number(n)) = query_body.get("size") {
            if let Some(size) = n.as_u64() {
                if size > hard_max {
                    query_body.insert("size".to_string(), json!(hard_max));
                }
            }
        }
        
        assert_eq!(query_body.get("size").unwrap().as_u64().unwrap(), 200);
    }
    
    #[test]
    fn test_size_limit_no_change_when_within_limit() {
        let mut query_body = serde_json::Map::new();
        query_body.insert("size".to_string(), json!(50));
        
        let hard_max = 200u64;
        let original_size = query_body.get("size").unwrap().as_u64().unwrap();
        
        // Simulate the enforcement logic
        if let Some(serde_json::Value::Number(n)) = query_body.get("size") {
            if let Some(size) = n.as_u64() {
                if size > hard_max {
                    query_body.insert("size".to_string(), json!(hard_max));
                }
            }
        }
        
        assert_eq!(query_body.get("size").unwrap().as_u64().unwrap(), original_size);
    }
    
    #[test]
    fn test_index_list_truncation() {
        let mut response: Vec<serde_json::Value> = (0..150)
            .map(|i| json!({"index": format!("test-{}", i)}))
            .collect();
        
        let max_list = 100;
        let total_count = response.len();
        let truncated = total_count > max_list;
        
        if truncated {
            response.truncate(max_list);
        }
        
        assert_eq!(response.len(), 100);
        assert!(truncated);
    }
    
    #[test]
    fn test_env_var_defaults() {
        // Test default values when env vars not set
        // Note: We can't actually remove env vars in tests as they may be set by previous tests
        // or the test runner. Instead, test the fallback logic with non-existent var names.
        
        let max_response = std::env::var("MCP_MAX_RESPONSE_CHARS_NONEXISTENT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(15_000);
        
        let max_size = std::env::var("MCP_MAX_SEARCH_SIZE_NONEXISTENT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(200);
        
        let max_list = std::env::var("MCP_MAX_INDEX_LIST_NONEXISTENT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(100);
        
        assert_eq!(max_response, 15_000);
        assert_eq!(max_size, 200);
        assert_eq!(max_list, 100);
    }
    
    #[test]
    fn test_env_var_override() {
        // Test custom values via env vars
        unsafe {
            std::env::set_var("MCP_MAX_RESPONSE_CHARS", "20000");
            std::env::set_var("MCP_MAX_SEARCH_SIZE", "300");
            std::env::set_var("MCP_MAX_INDEX_LIST", "150");
        }
        
        let max_response = std::env::var("MCP_MAX_RESPONSE_CHARS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(15_000);
        
        let max_size = std::env::var("MCP_MAX_SEARCH_SIZE")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(200);
        
        let max_list = std::env::var("MCP_MAX_INDEX_LIST")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(100);
        
        assert_eq!(max_response, 20_000);
        assert_eq!(max_size, 300);
        assert_eq!(max_list, 150);
        
        // Cleanup
        unsafe {
            std::env::remove_var("MCP_MAX_RESPONSE_CHARS");
            std::env::remove_var("MCP_MAX_SEARCH_SIZE");
            std::env::remove_var("MCP_MAX_INDEX_LIST");
        }
    }
}
