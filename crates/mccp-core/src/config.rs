use super::*;
use std::path::PathBuf;

fn default_true() -> bool { true }

/// Per-agent access control configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub token: String,
    /// Project allowlist; `["*"]` means all projects
    pub projects: Vec<String>,
    pub can_write: bool,
}

/// Webhook configuration (V3-6)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// HMAC-SHA256 secret for validating push events
    pub secret: Option<String>,
}

/// Configuration for the mccp system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub daemon: DaemonConfig,
    pub indexer: IndexerConfig,
    pub embedding: EmbeddingConfig,
    pub llm: LlmConfig,
    pub vector: VectorConfig,
    pub query: QueryConfig,
    pub storage: StorageConfig,
    pub docker: DockerConfig,
    pub logging: LoggingConfig,
    /// Per-agent access tokens (V3-7). Empty = single-user mode (no auth).
    #[serde(default)]
    pub agents: Vec<AgentConfig>,
    /// Webhook configuration (V3-6)
    #[serde(default)]
    pub webhook: Option<WebhookConfig>,
}

impl Config {
    /// Shortcut to ranker weights (also accessible via `config.query.ranker_weights`)
    pub fn ranker_weights(&self) -> &RankerConfig {
        &self.query.ranker_weights
    }

    /// Find an agent by bearer token
    pub fn find_agent(&self, token: &str) -> Option<&AgentConfig> {
        self.agents.iter().find(|a| a.token == token)
    }

    /// Create default configuration
    pub fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            indexer: IndexerConfig::default(),
            embedding: EmbeddingConfig::default(),
            llm: LlmConfig::default(),
            vector: VectorConfig::default(),
            query: QueryConfig::default(),
            storage: StorageConfig::default(),
            docker: DockerConfig::default(),
            logging: LoggingConfig::default(),
            agents: vec![],
            webhook: None,
        }
    }

    /// Load configuration from file
    pub fn load<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::ConfigError(format!("Failed to read config file: {}", e)))?;
        
        let config: Self = toml::from_str(&content)
            .map_err(|e| Error::ConfigError(format!("Failed to parse config file: {}", e)))?;
        
        Ok(config)
    }

    /// Save configuration to file
    pub fn save<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| Error::ConfigError(format!("Failed to serialize config: {}", e)))?;
        
        std::fs::write(path, content)
            .map_err(|e| Error::ConfigError(format!("Failed to write config file: {}", e)))?;
        
        Ok(())
    }

    /// Get the configuration directory
    pub fn config_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| Error::ConfigError("Could not determine home directory".to_string()))?;
        
        let config_dir = home.join(".mccp");
        std::fs::create_dir_all(&config_dir)
            .map_err(|e| Error::ConfigError(format!("Failed to create config directory: {}", e)))?;
        
        Ok(config_dir)
    }

    /// Get the default config file path
    pub fn default_config_path() -> Result<PathBuf> {
        let config_dir = Self::config_dir()?;
        Ok(config_dir.join("config.toml"))
    }

    /// Load or create default configuration
    pub fn load_or_default() -> Result<Self> {
        let config_path = Self::default_config_path()?;
        
        if config_path.exists() {
            Self::load(&config_path)
        } else {
            let config = Self::default();
            config.save(&config_path)?;
            Ok(config)
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            indexer: IndexerConfig::default(),
            embedding: EmbeddingConfig::default(),
            llm: LlmConfig::default(),
            vector: VectorConfig::default(),
            query: QueryConfig::default(),
            storage: StorageConfig::default(),
            docker: DockerConfig::default(),
            logging: LoggingConfig::default(),
            agents: vec![],
            webhook: None,
        }
    }
}

/// Daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub http_port: u16,
    pub log_level: String,
    pub log_dir: PathBuf,
    pub log_retention: String,
    pub auto_start: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            http_port: 7422,
            log_level: "info".to_string(),
            log_dir: PathBuf::from("~/.mccp/logs"),
            log_retention: "14d".to_string(),
            auto_start: true,
        }
    }
}

/// Indexer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexerConfig {
    pub watch_enabled: bool,
    pub max_chunk_tokens: usize,
    pub chunk_overlap: usize,
    pub batch_size: usize,
    pub parallel_workers: usize,
    pub secrets_scan: bool,
    pub io_buffer_kb: usize,
    pub mmap_threshold_kb: usize,
    /// Extra ignore patterns (gitignore syntax) applied on top of .gitignore rules
    #[serde(default)]
    pub extra_ignore_patterns: Vec<String>,
    /// Skip common build/dependency/system directories by default (e.g. node_modules, target, .git)
    #[serde(default = "default_true")]
    pub skip_default_dirs: bool,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            watch_enabled: true,
            max_chunk_tokens: 512,
            chunk_overlap: 64,
            batch_size: 32,
            parallel_workers: 0, // 0 = auto (num_cpus)
            secrets_scan: true,
            io_buffer_kb: 256,
            mmap_threshold_kb: 512,
            extra_ignore_patterns: vec![],
            skip_default_dirs: true,
        }
    }
}

/// Embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingConfig {
    pub providers: Vec<EmbeddingProviderConfig>,
    pub dimensions: usize,
    pub request_timeout_s: u64,
    pub max_retries: u32,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            providers: vec![
                EmbeddingProviderConfig {
                    driver: "ollama".to_string(),
                    model: "nomic-embed-text".to_string(),
                    url: "http://localhost:11434".to_string(),
                    api_key: String::new(),
                },
            ],
            dimensions: 768,
            request_timeout_s: 30,
            max_retries: 3,
        }
    }
}

/// LLM provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub providers: Vec<LlmProviderConfig>,
    pub max_tokens: usize,
    pub request_timeout_s: u64,
    pub max_retries: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            providers: vec![
                LlmProviderConfig {
                    driver: "ollama".to_string(),
                    model: "codellama:13b".to_string(),
                    url: "http://localhost:11434".to_string(),
                    api_key: String::new(),
                },
            ],
            max_tokens: 2048,
            request_timeout_s: 120,
            max_retries: 2,
        }
    }
}

/// Vector store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VectorConfig {
    pub driver: String,
    pub url: String,
    pub api_key: String,
    pub request_timeout_s: u64,
    pub hnsw_m: usize,
    pub hnsw_ef_construct: usize,
    pub hnsw_ef: usize,
    pub quantization: String,
}

impl Default for VectorConfig {
    fn default() -> Self {
        Self {
            driver: "qdrant".to_string(),
            url: "http://localhost:6333".to_string(),
            api_key: String::new(),
            request_timeout_s: 10,
            hnsw_m: 16,
            hnsw_ef_construct: 128,
            hnsw_ef: 64,
            quantization: "scalar".to_string(),
        }
    }
}

/// Query engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QueryConfig {
    pub default_top_k: usize,
    pub cache_size_entries: usize,
    pub ranker_weights: RankerConfig,
    pub max_graph_depth: usize,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            default_top_k: 10,
            cache_size_entries: 10000,
            ranker_weights: RankerConfig::default(),
            max_graph_depth: 3,
        }
    }
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub data_dir: PathBuf,
    pub metadata_backend: String,
    pub graph_backend: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("~/.mccp/data"),
            metadata_backend: "sled".to_string(),
            graph_backend: "memory+wal".to_string(),
        }
    }
}

/// Docker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DockerConfig {
    pub compose_file: PathBuf,
    pub auto_start: bool,
    pub data_dir: PathBuf,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            compose_file: PathBuf::from("~/.mccp/docker-compose.yml"),
            auto_start: true,
            data_dir: PathBuf::from("~/.mccp/data"),
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
    pub file: Option<PathBuf>,
    pub max_size: String,
    pub max_files: usize,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
            file: None,
            max_size: "100MB".to_string(),
            max_files: 5,
        }
    }
}

/// Embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingProviderConfig {
    pub driver: String,
    pub model: String,
    pub url: String,
    #[serde(default)]
    pub api_key: String,
}

/// LLM provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    pub driver: String,
    pub model: String,
    pub url: String,
    #[serde(default)]
    pub api_key: String,
}

/// Environment variable configuration resolver
pub struct EnvConfigResolver;

impl EnvConfigResolver {
    /// Resolve configuration from environment variables
    pub fn resolve() -> Config {
        let mut config = Config::default();
        
        // Override with environment variables
        if let Ok(port) = std::env::var("MCCP_HTTP_PORT") {
            if let Ok(port) = port.parse::<u16>() {
                config.daemon.http_port = port;
            }
        }
        
        if let Ok(level) = std::env::var("MCCP_LOG_LEVEL") {
            config.daemon.log_level = level;
        }
        
        if let Ok(embedding_provider) = std::env::var("MCCP_EMBEDDING_PROVIDER") {
            if let Some(embedding_config) = config.embedding.providers.get_mut(0) {
                embedding_config.driver = embedding_provider;
            }
        }
        
        if let Ok(embedding_model) = std::env::var("MCCP_EMBEDDING_MODEL") {
            if let Some(embedding_config) = config.embedding.providers.get_mut(0) {
                embedding_config.model = embedding_model;
            }
        }
        
        if let Ok(embedding_url) = std::env::var("MCCP_EMBEDDING_URL") {
            if let Some(embedding_config) = config.embedding.providers.get_mut(0) {
                embedding_config.url = embedding_url;
            }
        }
        
        if let Ok(embedding_api_key) = std::env::var("MCCP_EMBEDDING_API_KEY") {
            if let Some(embedding_config) = config.embedding.providers.get_mut(0) {
                embedding_config.api_key = embedding_api_key;
            }
        }
        
        if let Ok(llm_provider) = std::env::var("MCCP_LLM_PROVIDER") {
            if let Some(llm_config) = config.llm.providers.get_mut(0) {
                llm_config.driver = llm_provider;
            }
        }
        
        if let Ok(llm_model) = std::env::var("MCCP_LLM_MODEL") {
            if let Some(llm_config) = config.llm.providers.get_mut(0) {
                llm_config.model = llm_model;
            }
        }
        
        if let Ok(llm_url) = std::env::var("MCCP_LLM_URL") {
            if let Some(llm_config) = config.llm.providers.get_mut(0) {
                llm_config.url = llm_url;
            }
        }
        
        if let Ok(llm_api_key) = std::env::var("MCCP_LLM_API_KEY") {
            if let Some(llm_config) = config.llm.providers.get_mut(0) {
                llm_config.api_key = llm_api_key;
            }
        }
        
        if let Ok(vector_provider) = std::env::var("MCCP_VECTOR_PROVIDER") {
            config.vector.driver = vector_provider;
        }
        
        if let Ok(vector_url) = std::env::var("MCCP_VECTOR_URL") {
            config.vector.url = vector_url;
        }
        
        if let Ok(vector_api_key) = std::env::var("MCCP_VECTOR_API_KEY") {
            config.vector.api_key = vector_api_key;
        }
        
        config
    }
}

/// Configuration validator
pub struct ConfigValidator;

impl ConfigValidator {
    /// Validate the configuration
    pub fn validate(config: &Config) -> Result<()> {
        // Validate HTTP port
        if config.daemon.http_port == 0 {
            return Err(Error::ConfigError("HTTP port cannot be 0".to_string()));
        }
        
        // Validate log level
        match config.daemon.log_level.as_str() {
            "debug" | "info" | "warn" | "error" => {},
            _ => return Err(Error::ConfigError("Invalid log level".to_string())),
        }
        
        // Validate embedding providers
        if config.embedding.providers.is_empty() {
            return Err(Error::ConfigError("At least one embedding provider must be configured".to_string()));
        }
        
        for provider in &config.embedding.providers {
            if provider.driver.is_empty() {
                return Err(Error::ConfigError("Embedding provider driver cannot be empty".to_string()));
            }
            if provider.model.is_empty() {
                return Err(Error::ConfigError("Embedding provider model cannot be empty".to_string()));
            }
        }
        
        // Validate LLM providers
        if config.llm.providers.is_empty() {
            return Err(Error::ConfigError("At least one LLM provider must be configured".to_string()));
        }
        
        for provider in &config.llm.providers {
            if provider.driver.is_empty() {
                return Err(Error::ConfigError("LLM provider driver cannot be empty".to_string()));
            }
            if provider.model.is_empty() {
                return Err(Error::ConfigError("LLM provider model cannot be empty".to_string()));
            }
        }
        
        // Validate vector store
        if config.vector.driver.is_empty() {
            return Err(Error::ConfigError("Vector store driver cannot be empty".to_string()));
        }
        
        // Validate storage backends
        match config.storage.metadata_backend.as_str() {
            "sled" | "sqlite" | "rocksdb" => {},
            _ => return Err(Error::ConfigError("Invalid metadata backend".to_string())),
        }
        
        match config.storage.graph_backend.as_str() {
            "memory+wal" | "memory" | "sled" | "sqlite" => {},
            _ => return Err(Error::ConfigError("Invalid graph backend".to_string())),
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        
        assert_eq!(config.daemon.http_port, 7422);
        assert_eq!(config.daemon.log_level, "info");
        assert_eq!(config.indexer.max_chunk_tokens, 512);
        assert_eq!(config.indexer.chunk_overlap, 64);
        assert_eq!(config.embedding.dimensions, 768);
        assert_eq!(config.llm.max_tokens, 2048);
        assert_eq!(config.vector.driver, "qdrant");
        assert_eq!(config.query.default_top_k, 10);
        assert_eq!(config.storage.metadata_backend, "sled");
    }

    #[test]
    fn test_config_validation() {
        let config = Config::default();
        assert!(ConfigValidator::validate(&config).is_ok());
    }

    #[test]
    fn test_config_validation_invalid_port() {
        let mut config = Config::default();
        config.daemon.http_port = 0;
        
        assert!(ConfigValidator::validate(&config).is_err());
    }

    #[test]
    fn test_config_validation_invalid_log_level() {
        let mut config = Config::default();
        config.daemon.log_level = "invalid".to_string();
        
        assert!(ConfigValidator::validate(&config).is_err());
    }

    #[test]
    fn test_config_validation_no_embedding_providers() {
        let mut config = Config::default();
        config.embedding.providers.clear();
        
        assert!(ConfigValidator::validate(&config).is_err());
    }

    #[test]
    fn test_config_validation_no_llm_providers() {
        let mut config = Config::default();
        config.llm.providers.clear();
        
        assert!(ConfigValidator::validate(&config).is_err());
    }

    #[test]
    fn test_config_validation_invalid_metadata_backend() {
        let mut config = Config::default();
        config.storage.metadata_backend = "invalid".to_string();
        
        assert!(ConfigValidator::validate(&config).is_err());
    }

    #[test]
    fn test_env_config_resolver() {
        std::env::set_var("MCCP_HTTP_PORT", "8080");
        std::env::set_var("MCCP_LOG_LEVEL", "debug");
        std::env::set_var("MCCP_EMBEDDING_PROVIDER", "openai");
        std::env::set_var("MCCP_EMBEDDING_MODEL", "text-embedding-3-small");
        std::env::set_var("MCCP_LLM_PROVIDER", "anthropic");
        std::env::set_var("MCCP_LLM_MODEL", "claude-3-5-haiku-20241022");
        std::env::set_var("MCCP_VECTOR_PROVIDER", "pgvector");
        
        let config = EnvConfigResolver::resolve();
        
        assert_eq!(config.daemon.http_port, 8080);
        assert_eq!(config.daemon.log_level, "debug");
        
        if let Some(embedding_config) = config.embedding.providers.get(0) {
            assert_eq!(embedding_config.driver, "openai");
            assert_eq!(embedding_config.model, "text-embedding-3-small");
        }
        
        if let Some(llm_config) = config.llm.providers.get(0) {
            assert_eq!(llm_config.driver, "anthropic");
            assert_eq!(llm_config.model, "claude-3-5-haiku-20241022");
        }
        
        assert_eq!(config.vector.driver, "pgvector");
        
        // Clean up
        std::env::remove_var("MCCP_HTTP_PORT");
        std::env::remove_var("MCCP_LOG_LEVEL");
        std::env::remove_var("MCCP_EMBEDDING_PROVIDER");
        std::env::remove_var("MCCP_EMBEDDING_MODEL");
        std::env::remove_var("MCCP_LLM_PROVIDER");
        std::env::remove_var("MCCP_LLM_MODEL");
        std::env::remove_var("MCCP_VECTOR_PROVIDER");
    }
}