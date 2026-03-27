use mccp_core::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::bm25::BM25Encoder;

/// Qdrant vector store provider
#[derive(Debug, Clone)]
pub struct QdrantProvider {
    client: Client,
    endpoint: String,
    api_key: Option<String>,
    timeout: std::time::Duration,
    bm25_encoder: std::sync::Arc<tokio::sync::Mutex<BM25Encoder>>,
}

impl QdrantProvider {
    /// Create a new Qdrant provider
    pub fn new(endpoint: String, api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            endpoint,
            api_key,
            timeout: std::time::Duration::from_secs(30),
            bm25_encoder: std::sync::Arc::new(tokio::sync::Mutex::new(BM25Encoder::new())),
        }
    }

    /// Set the timeout
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Create collection with hybrid vectors
    async fn create_collection(&self, project_id: &str, dims: usize) -> Result<()> {
        let collection_name = format!("mccp_{}", project_id);
        
        let request = CreateCollectionRequest {
            collection_name: collection_name.clone(),
            vectors_config: Some(VectorsConfig {
                params: HashMap::from([
                    ("dense".to_owned(), VectorParams { 
                        size: dims as u64, 
                        distance: Distance::Cosine, 
                        on_disk: None,
                        hnsw_config: None,
                        quantization_config: None,
                    }),
                ]),
            }),
            sparse_vectors_config: Some(SparseVectorsConfig {
                map: HashMap::from([
                    ("sparse".to_owned(), SparseVectorParams { index: None }),
                ])
            }),
            hnsw_config: None,
            optimizers_config: None,
            wal_config: None,
            quantization_config: None,
            init_from: None,
            tokenizer_config: None,
        };

        let url = format!("{}/collections/{}", self.endpoint, collection_name);
        let mut req = self.client.put(&url)
            .timeout(self.timeout)
            .json(&request);

        if let Some(api_key) = &self.api_key {
            req = req.header("api-key", api_key);
        }

        let response = req.send().await.map_err(|e| Error::ProviderError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderError(format!("Qdrant create collection error: {}", error_text)));
        }

        Ok(())
    }

    /// Search dense vectors
    async fn search_dense(
        &self,
        project_id: &str,
        query: &[f32],
        top_k: usize,
        filters: &Filters,
    ) -> Result<Vec<ScoredChunk>> {
        let collection_name = format!("mccp_{}", project_id);
        let url = format!("{}/collections/{}/points/search", self.endpoint, collection_name);

        let request = SearchRequest {
            vector: query.to_vec(),
            filter: Some(filters.clone()),
            limit: top_k,
            with_payload: Some(true),
            params: None,
            score_threshold: None,
            offset: None,
            lookup_from: None,
            using: Some("dense".to_string()),
        };

        let mut req = self.client.post(&url)
            .timeout(self.timeout)
            .json(&request);

        if let Some(api_key) = &self.api_key {
            req = req.header("api-key", api_key);
        }

        let response = req.send().await.map_err(|e| Error::ProviderError(e.to_string()))?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderError(format!("Qdrant search error: {}", error_text)));
        }

        let search_response: SearchResponse = response
            .json()
            .await
            .map_err(|e| Error::ProviderError(e.to_string()))?;

        Ok(search_response.result.into_iter().map(|r| ScoredChunk {
            chunk_id: r.id.to_string(),
            score: r.score,
            content: r.payload.as_ref().and_then(|p| p.get("content").and_then(|c| c.as_str())).unwrap_or_default().to_string(),
            file_path: r.payload.as_ref().and_then(|p| p.get("file_path").and_then(|c| c.as_str())).unwrap_or_default().to_string(),
            start_line: r.payload.as_ref().and_then(|p| p.get("start_line").and_then(|c| c.as_u64())).unwrap_or_default() as usize,
            end_line: r.payload.as_ref().and_then(|p| p.get("end_line").and_then(|c| c.as_u64())).unwrap_or_default() as usize,
            project_id: project_id.to_string(),
            metadata: r.payload.map(|p| serde_json::to_value(p).unwrap_or(serde_json::Value::Null)).unwrap_or(serde_json::Value::Null),
        }).collect())
    }

    /// Search sparse vectors
    async fn search_sparse(
        &self,
        project_id: &str,
        query_sparse: &[(u32, f32)],
        top_k: usize,
        filters: &Filters,
    ) -> Result<Vec<ScoredChunk>> {
        let collection_name = format!("mccp_{}", project_id);
        let url = format!("{}/collections/{}/points/search", self.endpoint, collection_name);

        let request = SparseSearchRequest {
            sparse_vector: SparseVector {
                indices: query_sparse.iter().map(|(i, _)| *i).collect(),
                values: query_sparse.iter().map(|(_, v)| *v).collect(),
            },
            filter: Some(filters.clone()),
            limit: top_k,
            with_payload: Some(true),
            params: None,
            score_threshold: None,
            offset: None,
            lookup_from: None,
            using: Some("sparse".to_string()),
        };

        let mut req = self.client.post(&url)
            .timeout(self.timeout)
            .json(&request);

        if let Some(api_key) = &self.api_key {
            req = req.header("api-key", api_key);
        }

        let response = req.send().await.map_err(|e| Error::ProviderError(e.to_string()))?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderError(format!("Qdrant sparse search error: {}", error_text)));
        }

        let search_response: SearchResponse = response
            .json()
            .await
            .map_err(|e| Error::ProviderError(e.to_string()))?;

        Ok(search_response.result.into_iter().map(|r| ScoredChunk {
            chunk_id: r.id.to_string(),
            score: r.score,
            content: r.payload.as_ref().and_then(|p| p.get("content").and_then(|c| c.as_str())).unwrap_or_default().to_string(),
            file_path: r.payload.as_ref().and_then(|p| p.get("file_path").and_then(|c| c.as_str())).unwrap_or_default().to_string(),
            start_line: r.payload.as_ref().and_then(|p| p.get("start_line").and_then(|c| c.as_u64())).unwrap_or_default() as usize,
            end_line: r.payload.as_ref().and_then(|p| p.get("end_line").and_then(|c| c.as_u64())).unwrap_or_default() as usize,
            project_id: project_id.to_string(),
            metadata: r.payload.map(|p| serde_json::to_value(p).unwrap_or(serde_json::Value::Null)).unwrap_or(serde_json::Value::Null),
        }).collect())
    }
}

#[async_trait::async_trait]
impl VectorStoreProvider for QdrantProvider {
    async fn upsert(&self, project_id: &str, chunks: &[EmbeddedChunk]) -> Result<()> {
        // Register documents for BM25 encoder
        let mut encoder = self.bm25_encoder.lock().await;
        for chunk in chunks {
            encoder.register_doc(&chunk.content);
        }
        drop(encoder);

        // Create collection if it doesn't exist
        if let Err(_) = self.create_collection(project_id, chunks[0].embedding.len()).await {
            // Collection might already exist, that's fine
        }

        let collection_name = format!("mccp_{}", project_id);
        let url = format!("{}/collections/{}/points", self.endpoint, collection_name);

        let points: Vec<PointStruct> = chunks.iter().map(|chunk| PointStruct {
            id: chunk.chunk_id.clone(),
            vector: HashMap::from([
                ("dense".to_string(), chunk.embedding.clone()),
            ]),
            payload: Some(HashMap::from([
                ("content".to_string(), serde_json::Value::String(chunk.content.clone())),
                ("project_id".to_string(), serde_json::Value::String(project_id.to_string())),
                ("metadata".to_string(), chunk.metadata.clone()),
            ])),
        }).collect();

        let request = UpsertRequest {
            points,
        };

        let mut req = self.client.put(&url)
            .timeout(self.timeout)
            .json(&request);

        if let Some(api_key) = &self.api_key {
            req = req.header("api-key", api_key);
        }

        let response = req.send().await.map_err(|e| Error::ProviderError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderError(format!("Qdrant upsert error: {}", error_text)));
        }

        Ok(())
    }

    async fn search(&self, project_id: &str, query: &[f32], top_k: usize, filters: &Filters) -> Result<Vec<ScoredChunk>> {
        self.search_dense(project_id, query, top_k, filters).await
    }

    async fn delete_project(&self, project_id: &str) -> Result<()> {
        let collection_name = format!("mccp_{}", project_id);
        let url = format!("{}/collections/{}", self.endpoint, collection_name);

        let mut req = self.client.delete(&url)
            .timeout(self.timeout);

        if let Some(api_key) = &self.api_key {
            req = req.header("api-key", api_key);
        }

        let response = req.send().await.map_err(|e| Error::ProviderError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderError(format!("Qdrant delete collection error: {}", error_text)));
        }

        Ok(())
    }

    async fn health(&self) -> ProviderHealth {
        let url = format!("{}/readyz", self.endpoint);
        
        let mut req = self.client.get(&url)
            .timeout(std::time::Duration::from_secs(5));

        if let Some(api_key) = &self.api_key {
            req = req.header("api-key", api_key);
        }

        match req.send().await {
            Ok(response) => {
                if response.status().is_success() {
                    ProviderHealth {
                        status: ProviderStatusType::Healthy,
                        last_check: chrono::Utc::now(),
                        error_message: None,
                    }
                } else {
                    ProviderHealth {
                        status: ProviderStatusType::Unhealthy,
                        last_check: chrono::Utc::now(),
                        error_message: Some(format!("HTTP error: {}", response.status())),
                    }
                }
            }
            Err(e) => ProviderHealth {
                status: ProviderStatusType::Unhealthy,
                last_check: chrono::Utc::now(),
                error_message: Some(e.to_string()),
            },
        }
    }

    fn supports_hybrid(&self) -> bool { true }

    async fn upsert_with_sparse(
        &self,
        project_id: &str,
        chunks: &[SparseEmbeddedChunk],
    ) -> Result<()> {
        // Register documents for BM25 encoder
        let mut encoder = self.bm25_encoder.lock().await;
        for chunk in chunks {
            encoder.register_doc(&chunk.content);
        }
        drop(encoder);

        // Create collection if it doesn't exist
        if let Err(_) = self.create_collection(project_id, chunks[0].dense.len()).await {
            // Collection might already exist, that's fine
        }

        let collection_name = format!("mccp_{}", project_id);
        let url = format!("{}/collections/{}/points", self.endpoint, collection_name);

        let points: Vec<PointStruct> = chunks.iter().map(|chunk| PointStruct {
            id: chunk.chunk_id.clone(),
            vector: HashMap::from([
                ("dense".to_string(), chunk.dense.clone()),
            ]),
            payload: Some(HashMap::from([
                ("content".to_string(), serde_json::Value::String(chunk.content.clone())),
                ("project_id".to_string(), serde_json::Value::String(project_id.to_string())),
                ("metadata".to_string(), chunk.metadata.clone()),
            ])),
        }).collect();

        let request = UpsertRequest {
            points,
        };

        let mut req = self.client.put(&url)
            .timeout(self.timeout)
            .json(&request);

        if let Some(api_key) = &self.api_key {
            req = req.header("api-key", api_key);
        }

        let response = req.send().await.map_err(|e| Error::ProviderError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderError(format!("Qdrant upsert error: {}", error_text)));
        }

        Ok(())
    }

    async fn hybrid_search(
        &self,
        project_id: &str,
        dense_query: &[f32],
        sparse_text: &str,
        top_k: usize,
        filters: &Filters,
    ) -> Result<Vec<ScoredChunk>> {
        // Encode sparse query
        let encoder = self.bm25_encoder.lock().await;
        let sparse_query = encoder.encode_query(sparse_text);
        drop(encoder);

        // Run both searches in parallel
        let (dense_results, sparse_results) = tokio::join!(
            self.search_dense(project_id, dense_query, top_k * 2, filters),
            self.search_sparse(project_id, &sparse_query, top_k * 2, filters),
        );

        let dense_results = dense_results?;
        let sparse_results = sparse_results?;

        // Fuse results using RRF
        let fused = crate::bm25::rrf_fuse(dense_results, sparse_results, top_k);
        
        Ok(fused)
    }
}

// Qdrant API types
#[derive(Debug, Serialize, Deserialize)]
struct CreateCollectionRequest {
    collection_name: String,
    vectors_config: Option<VectorsConfig>,
    sparse_vectors_config: Option<SparseVectorsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hnsw_config: Option<HnswConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    optimizers_config: Option<OptimizersConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wal_config: Option<WalConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quantization_config: Option<QuantizationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    init_from: Option<InitFrom>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tokenizer_config: Option<TokenizerConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VectorsConfig {
    #[serde(flatten)]
    params: HashMap<String, VectorParams>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VectorParams {
    size: u64,
    distance: Distance,
    #[serde(skip_serializing_if = "Option::is_none")]
    on_disk: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hnsw_config: Option<HnswConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quantization_config: Option<QuantizationConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
enum Distance {
    Cosine,
    Euclid,
    Dot,
    Manhattan,
}

#[derive(Debug, Serialize, Deserialize)]
struct SparseVectorsConfig {
    #[serde(flatten)]
    map: HashMap<String, SparseVectorParams>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SparseVectorParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<SparseIndexConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SparseIndexConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    full_scan_threshold: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    on_disk: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HnswConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    m: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ef_construct: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    full_scan_threshold: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_indexing_threads: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    on_disk: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload_m: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OptimizersConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    deleted_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vacuum_min_vector_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_segment_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_segment_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memmap_threshold: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    indexing_threshold: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    flush_interval_sec: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_optimization_threads: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    wal_capacity_mb: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wal_segments_ahead: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct QuantizationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    scalar: Option<ScalarQuantization>,
    #[serde(skip_serializing_if = "Option::is_none")]
    product: Option<ProductQuantization>,
    #[serde(skip_serializing_if = "Option::is_none")]
    binary: Option<BinaryQuantization>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScalarQuantization {
    #[serde(skip_serializing_if = "Option::is_none")]
    quantile: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    always_ram: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProductQuantization {
    #[serde(skip_serializing_if = "Option::is_none")]
    compression: Option<Compression>,
    #[serde(skip_serializing_if = "Option::is_none")]
    always_ram: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BinaryQuantization {
    #[serde(skip_serializing_if = "Option::is_none")]
    always_ram: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
enum Compression {
    Int8,
    Int16,
    Int32,
    Int64,
    Int128,
    Int256,
}

#[derive(Debug, Serialize, Deserialize)]
struct InitFrom {
    collection: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenizerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    tokenizer_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpsertRequest {
    points: Vec<PointStruct>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PointStruct {
    id: String,
    vector: HashMap<String, Vec<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchRequest {
    vector: Vec<f32>,
    filter: Option<Filters>,
    limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    with_payload: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<SearchParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    score_threshold: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lookup_from: Option<LookupLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    using: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SparseSearchRequest {
    sparse_vector: SparseVector,
    filter: Option<Filters>,
    limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    with_payload: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<SearchParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    score_threshold: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lookup_from: Option<LookupLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    using: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SparseVector {
    indices: Vec<u32>,
    values: Vec<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {
    result: Vec<SearchResult>,
    status: String,
    time: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResult {
    id: String,
    version: u64,
    score: f32,
    payload: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    hnsw_ef: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exact: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LookupLocation {
    collection: String,
    shard_key: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qdrant_provider_creation() {
        let provider = QdrantProvider::new("http://localhost:6333".to_string(), None);
        
        assert_eq!(provider.supports_hybrid(), true);
    }

    #[tokio::test]
    async fn test_qdrant_provider_health() {
        let provider = QdrantProvider::new("http://localhost:6333".to_string(), None);
        let health = provider.health().await;
        
        // Should be unhealthy since we're not running Qdrant locally
        assert_eq!(health.status, ProviderStatusType::Unhealthy);
        assert!(health.error_message.is_some());
    }
}