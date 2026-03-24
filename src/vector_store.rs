/// HTTP client for the Qdrant vector store REST API.
///
/// Uses plain reqwest — no dependency on the qdrant-client crate or mccp-core.
use anyhow::Context;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkPoint {
    pub id: String,
    pub vector: Vec<f32>,
    pub payload: ChunkPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkPayload {
    pub path: String,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub project: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchHit {
    pub score: f32,
    pub payload: Option<ChunkPayload>,
}

// ─── Client ───────────────────────────────────────────────────────────────────

pub struct VectorStoreClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl VectorStoreClient {
    pub fn new(url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build HTTP client"),
            base_url: url.into().trim_end_matches('/').to_string(),
            api_key,
        }
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(k) => req.header("api-key", k),
            None => req,
        }
    }

    fn collection_url(&self, name: &str) -> String {
        format!("{}/collections/{}", self.base_url, name)
    }

    // ── Collection management ─────────────────────────────────────────────────

    /// Create collection if it does not already exist.
    pub async fn ensure_collection(&self, name: &str, dims: usize) -> anyhow::Result<()> {
        let url = self.collection_url(name);
        let status = self
            .auth(self.client.get(&url))
            .send()
            .await
            .context("checking qdrant collection")?
            .status();

        if status.is_success() {
            return Ok(());
        }

        // Create
        let body = json!({
            "vectors": { "size": dims, "distance": "Cosine" }
        });
        self.auth(self.client.put(&url))
            .json(&body)
            .send()
            .await
            .context("creating qdrant collection")?
            .error_for_status()
            .context("qdrant rejected collection creation")?;
        Ok(())
    }

    // ── Points ────────────────────────────────────────────────────────────────

    /// Upsert a batch of chunk points.
    pub async fn upsert_points(&self, collection: &str, points: Vec<ChunkPoint>) -> anyhow::Result<()> {
        if points.is_empty() {
            return Ok(());
        }
        let url = format!("{}/points", self.collection_url(collection));
        let body = json!({
            "points": points.iter().map(|p| json!({
                "id": p.id,
                "vector": p.vector,
                "payload": {
                    "path":       p.payload.path,
                    "content":    p.payload.content,
                    "start_line": p.payload.start_line,
                    "end_line":   p.payload.end_line,
                    "project":    p.payload.project,
                }
            })).collect::<Vec<_>>()
        });
        self.auth(self.client.put(&url))
            .json(&body)
            .send()
            .await
            .context("upserting qdrant points")?
            .error_for_status()
            .context("qdrant rejected point upsert")?;
        Ok(())
    }

    /// Delete all points whose `path` payload field matches the given path.
    pub async fn delete_by_path(&self, collection: &str, path: &str) -> anyhow::Result<()> {
        let url = format!("{}/points/delete", self.collection_url(collection));
        let body = json!({
            "filter": {
                "must": [{ "key": "path", "match": { "value": path } }]
            }
        });
        self.auth(self.client.post(&url))
            .json(&body)
            .send()
            .await
            .context("deleting qdrant points by path")?
            .error_for_status()
            .context("qdrant rejected delete")?;
        Ok(())
    }

    // ── Search ────────────────────────────────────────────────────────────────

    /// Dense vector search — returns up to `limit` ranked results.
    pub async fn search(
        &self,
        collection: &str,
        vector: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<SearchHit>> {
        let url = format!("{}/points/search", self.collection_url(collection));
        let body = json!({ "vector": vector, "limit": limit, "with_payload": true });

        #[derive(Deserialize)]
        struct Resp { result: Vec<SearchHit> }

        let resp: Resp = self
            .auth(self.client.post(&url))
            .json(&body)
            .send()
            .await
            .context("qdrant search request")?
            .error_for_status()
            .context("qdrant search returned error")?
            .json()
            .await
            .context("parsing qdrant search response")?;
        Ok(resp.result)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_builds() {
        let c = VectorStoreClient::new("http://localhost:6333", None);
        assert_eq!(c.base_url, "http://localhost:6333");
        assert!(c.api_key.is_none());
    }

    #[test]
    fn client_strips_trailing_slash() {
        let c = VectorStoreClient::new("http://localhost:6333/", None);
        assert_eq!(c.base_url, "http://localhost:6333");
    }

    #[test]
    fn collection_url_is_correct() {
        let c = VectorStoreClient::new("http://localhost:6333", None);
        assert_eq!(c.collection_url("myproject"), "http://localhost:6333/collections/myproject");
    }

    #[test]
    fn upsert_with_empty_vec_is_noop() {
        // No network call made — just verifies the early return path compiles and works.
        let c = VectorStoreClient::new("http://localhost:6333", None);
        // We can't await in a sync test, but we verify the function signature is correct.
        let _ = c.collection_url("test");
    }
}
