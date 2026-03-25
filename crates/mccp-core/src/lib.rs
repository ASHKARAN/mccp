pub mod project;
pub mod file;
pub mod chunk;
pub mod symbol;
pub mod graph;
pub mod query;
pub mod config;
pub mod error;

pub use project::*;
pub use file::*;
pub use chunk::*;
pub use symbol::*;
pub use graph::*;
pub use query::*;
pub use config::*;
pub use error::*;

use sha2::Digest as _Digest;
use std::path::PathBuf;
use uuid::Uuid;

/// Unique identifier for a project, derived from canonical project root path
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(String);

impl ProjectId {
    /// Create a project ID from a path
    pub fn from_path<P: AsRef<std::path::Path>>(path: P) -> Self {
        let canonical = path.as_ref().canonicalize()
            .unwrap_or_else(|_| path.as_ref().to_path_buf());
        let hash = format!("{:x}", sha2::Sha256::digest(canonical.to_string_lossy().as_bytes()));
        Self(hash[..12].to_string())
    }

    /// Get the string representation
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Language support enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Java,
    Go,
    C,
    Cpp,
    CSharp,
    Ruby,
    PHP,
    Kotlin,
}

impl Language {
    /// Get the file extensions for this language
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["rs"],
            Language::TypeScript => &["ts", "tsx"],
            Language::JavaScript => &["js", "jsx"],
            Language::Python => &["py"],
            Language::Java => &["java"],
            Language::Go => &["go"],
            Language::C => &["c", "h"],
            Language::Cpp => &["cpp", "cxx", "cc", "hpp", "hxx", "hh"],
            Language::CSharp => &["cs"],
            Language::Ruby => &["rb"],
            Language::PHP => &["php"],
            Language::Kotlin => &["kt", "kts"],
        }
    }

    /// Detect language from file extension
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "rs" => Some(Language::Rust),
            "ts" | "tsx" => Some(Language::TypeScript),
            "js" | "jsx" => Some(Language::JavaScript),
            "py" => Some(Language::Python),
            "java" => Some(Language::Java),
            "go" => Some(Language::Go),
            "c" | "h" => Some(Language::C),
            "cpp" | "cxx" | "cc" | "hpp" | "hxx" | "hh" => Some(Language::Cpp),
            "cs" => Some(Language::CSharp),
            "rb" => Some(Language::Ruby),
            "php" => Some(Language::PHP),
            "kt" | "kts" => Some(Language::Kotlin),
            _ => None,
        }
    }
}

/// Symbol kinds for code analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Class,
    Method,
    Variable,
    Interface,
    Type,
    Enum,
    Const,
    Function,
    Struct,
    Trait,
    Module,
}

/// Reference kinds for symbol usage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RefKind {
    Definition,
    Call,
    Import,
    TypeAnnotation,
    Inheritance,
    FieldAccess,
    Assignment,
    Export,
}

/// Edge kinds for graph representation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    Call,
    Import,
    Inheritance,
    FieldAccess,
    TypeAnnotation,
}

/// Scope levels for chunking
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkScope {
    Project,
    Module,
    File,
    Class(String),
    Method(String),
    Function(String),
}

impl ChunkScope {
    /// Get the scope level (used for ranking)
    pub fn level(&self) -> u8 {
        match self {
            ChunkScope::Project => 0,
            ChunkScope::Module => 1,
            ChunkScope::File => 2,
            ChunkScope::Class(_) => 3,
            ChunkScope::Method(_) | ChunkScope::Function(_) => 4,
        }
    }

    /// Get the scope name
    pub fn name(&self) -> String {
        match self {
            ChunkScope::Project => "project".to_string(),
            ChunkScope::Module => "module".to_string(),
            ChunkScope::File => "file".to_string(),
            ChunkScope::Class(name) => format!("class::{}", name),
            ChunkScope::Method(name) => format!("method::{}", name),
            ChunkScope::Function(name) => format!("function::{}", name),
        }
    }
}

/// Provider health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub is_healthy: bool,
    pub error: String,
    pub latency_ms: Option<u64>,
}

impl ProviderHealth {
    pub fn healthy() -> Self {
        Self {
            is_healthy: true,
            error: String::new(),
            latency_ms: None,
        }
    }

    pub fn unhealthy(error: impl Into<String>) -> Self {
        Self {
            is_healthy: false,
            error: error.into(),
            latency_ms: None,
        }
    }
}

/// Configuration for chunking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkConfig {
    pub max_tokens: usize,
    pub overlap_tokens: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            overlap_tokens: 64,
        }
    }
}

/// Configuration for graph traversal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalConfig {
    pub max_depth: usize,
    pub include_self: bool,
    pub edge_kinds: Vec<EdgeKind>,
}

impl Default for TraversalConfig {
    fn default() -> Self {
        Self {
            max_depth: 3,
            include_self: true,
            edge_kinds: vec![EdgeKind::Call, EdgeKind::Import],
        }
    }
}

/// Configuration for ranking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankerConfig {
    pub similarity: f32,
    pub graph: f32,
    pub metadata: f32,
}

impl RankerConfig {
    pub fn new(similarity: f32, graph: f32, metadata: f32) -> std::result::Result<Self, &'static str> {
        let sum = similarity + graph + metadata;
        if (sum - 1.0).abs() > 0.01 {
            return Err("Weights must sum to approximately 1.0");
        }
        Ok(Self {
            similarity,
            graph,
            metadata,
        })
    }
}

impl Default for RankerConfig {
    fn default() -> Self {
        Self {
            similarity: 0.6,
            graph: 0.25,
            metadata: 0.15,
        }
    }
}

/// Feedback signal for result quality
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackSignal {
    Good,
    Bad,
    Irrelevant,
}

/// Query cache key
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QueryCacheKey {
    pub project_id: String,
    pub query_hash: String,
    pub model_version: String,
}

impl QueryCacheKey {
    pub fn new(project_id: &str, query: &str, model_version: &str) -> Self {
        let query_hash = format!("{:x}", sha2::Sha256::digest(query.as_bytes()));
        Self {
            project_id: project_id.to_string(),
            query_hash,
            model_version: model_version.to_string(),
        }
    }
}

/// Embedding provider trait
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    async fn embed_one(&self, text: &str) -> Result<Vec<f32>>;
    fn dimensions(&self) -> usize;
    fn provider_fingerprint(&self) -> String;
    async fn health(&self) -> ProviderHealth;

    /// Probe the embedding dimensions by embedding a test string
    async fn probe_dimensions(&self) -> Result<usize> {
        Ok(self.embed_one("probe").await?.len())
    }
}

/// LLM provider trait
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str, schema: Option<&JsonSchema>) -> Result<String>;
    async fn stream(&self, prompt: &str) -> Result<tokio::sync::mpsc::Receiver<String>>;
    fn provider_fingerprint(&self) -> String;
    async fn health(&self) -> ProviderHealth;
}

/// Vector store provider trait
#[async_trait::async_trait]
pub trait VectorStoreProvider: Send + Sync {
    async fn upsert(&self, project_id: &str, chunks: &[EmbeddedChunk]) -> Result<()>;
    async fn search(&self, project_id: &str, query: &[f32], top_k: usize, filters: &Filters)
        -> Result<Vec<ScoredChunk>>;
    async fn delete_project(&self, project_id: &str) -> Result<()>;
    async fn health(&self) -> ProviderHealth;

    fn supports_hybrid(&self) -> bool { false }
    async fn upsert_with_sparse(
        &self, project_id: &str, chunks: &[SparseEmbeddedChunk]
    ) -> Result<()> {
        Err(anyhow::anyhow!("provider does not support sparse vectors"))
    }
    async fn hybrid_search(
        &self,
        project_id: &str,
        dense_query: &[f32],
        sparse_text: &str,
        top_k: usize,
        filters: &Filters,
    ) -> Result<Vec<ScoredChunk>> {
        // fallback: dense-only
        self.search(project_id, dense_query, top_k, filters).await
    }
}

/// Metrics for observability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    pub query_latency_p50: f64,
    pub query_latency_p95: f64,
    pub query_latency_p99: f64,
    pub query_cache_hit_rate: f64,
    pub indexer_queue_depth: usize,
    pub indexer_lag_seconds: f64,
    pub embedding_provider_latency_p99: f64,
    pub embedding_batch_size_avg: f64,
    pub llm_provider_latency_p99: f64,
    pub vector_store_upsert_latency_p99: f64,
    pub vector_store_search_latency_p99: f64,
    pub provider_error_rate: f64,
    pub graph_traversal_depth_avg: f64,
    pub feedback_good_rate: f64,
    pub active_embedding_provider: String,
    pub active_llm_provider: String,
    pub active_vector_provider: String,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            query_latency_p50: 0.0,
            query_latency_p95: 0.0,
            query_latency_p99: 0.0,
            query_cache_hit_rate: 0.0,
            indexer_queue_depth: 0,
            indexer_lag_seconds: 0.0,
            embedding_provider_latency_p99: 0.0,
            embedding_batch_size_avg: 0.0,
            llm_provider_latency_p99: 0.0,
            vector_store_upsert_latency_p99: 0.0,
            vector_store_search_latency_p99: 0.0,
            provider_error_rate: 0.0,
            graph_traversal_depth_avg: 0.0,
            feedback_good_rate: 0.0,
            active_embedding_provider: "unknown".to_string(),
            active_llm_provider: "unknown".to_string(),
            active_vector_provider: "unknown".to_string(),
        }
    }
}
