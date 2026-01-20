#[cfg(test)]
mod tests {
    // These are integration tests that require a running Elasticsearch instance
    // Run with: cargo test --test test_tools_unit -- --nocapture
    
    use elasticsearch::Elasticsearch;
    use elasticsearch::http::transport::TransportBuilder;
    use elasticsearch::http::Url;
    
    #[tokio::test]
    #[ignore] // Only run when ES is available
    async fn test_cluster_health_direct() {
        let url = Url::parse("http://172.30.137.172:9200").unwrap();
        let pool = elasticsearch::http::transport::SingleNodeConnectionPool::new(url);
        let transport = TransportBuilder::new(pool).build().unwrap();
        let client = Elasticsearch::new(transport);
        
        let response = client
            .cluster()
            .health(elasticsearch::cluster::ClusterHealthParts::None)
            .send()
            .await;
            
        assert!(response.is_ok(), "Cluster health request failed");
        
        let response = response.unwrap();
        assert_eq!(response.status_code(), 200, "Expected 200 status code");
        
        let json: serde_json::Value = response.json().await.unwrap();
        println!("Cluster health: {}", serde_json::to_string_pretty(&json).unwrap());
        
        assert!(json.get("status").is_some(), "Response should contain status field");
    }
    
    #[tokio::test]
    #[ignore]
    async fn test_cat_nodes_direct() {
        let url = Url::parse("http://172.30.137.172:9200").unwrap();
        let pool = elasticsearch::http::transport::SingleNodeConnectionPool::new(url);
        let transport = TransportBuilder::new(pool).build().unwrap();
        let client = Elasticsearch::new(transport);
        
        let response = client
            .cat()
            .nodes()
            .h(&["name", "ip", "heap.percent", "ram.percent", "cpu", "load_1m", "node.role", "master"])
            .format("json")
            .send()
            .await;
            
        assert!(response.is_ok(), "Cat nodes request failed");
        
        let response = response.unwrap();
        let json: serde_json::Value = response.json().await.unwrap();
        println!("Nodes info: {}", serde_json::to_string_pretty(&json).unwrap());
        
        assert!(json.is_array(), "Response should be an array");
    }
    
    #[tokio::test]
    #[ignore]
    async fn test_cat_indices_detailed_direct() {
        let url = Url::parse("http://172.30.137.172:9200").unwrap();
        let pool = elasticsearch::http::transport::SingleNodeConnectionPool::new(url);
        let transport = TransportBuilder::new(pool).build().unwrap();
        let client = Elasticsearch::new(transport);
        
        let indices = ["*"];
        let response = client
            .cat()
            .indices(elasticsearch::cat::CatIndicesParts::Index(&indices))
            .h(&["index", "health", "status", "pri", "rep", "docs.count", "store.size", "pri.store.size"])
            .format("json")
            .send()
            .await;
            
        assert!(response.is_ok(), "Cat indices request failed");
        
        let response = response.unwrap();
        let json: serde_json::Value = response.json().await.unwrap();
        println!("Indices info: {}", serde_json::to_string_pretty(&json).unwrap());
        
        assert!(json.is_array(), "Response should be an array");
    }
}
