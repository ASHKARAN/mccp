use super::*;
use mccp_core::Language;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Server configuration
    pub server: ServerConfig,
    
    /// Indexer configuration
    pub indexer: IndexerConfig,
    
    /// Storage configuration
    pub storage: StorageConfig,
    
    /// Provider configuration
    pub providers: ProviderConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Port to run the server on
    pub port: u16,
    
    /// Host to bind to
    pub host: String,
    
    /// Maximum number of connections
    pub max_connections: usize,
    
    /// Enable caching
    pub enable_caching: bool,
    
    /// Cache TTL
    pub cache_ttl: Duration,
}

/// Indexer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    /// Enable file watching
    pub watch_enabled: bool,
    
    /// Number of parallel workers
    pub parallel_workers: usize,
    
    /// Include patterns
    pub include_patterns: Vec<String>,
    
    /// Exclude patterns
    pub exclude_patterns: Vec<String>,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage backend type
    pub backend: String,
    
    /// Storage path
    pub path: PathBuf,
    
    /// Enable compression
    pub enable_compression: bool,
    
    /// Compression level
    pub compression_level: u32,
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Default provider
    pub default: String,
    
    /// Provider settings
    pub settings: serde_json::Value,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            indexer: IndexerConfig::default(),
            storage: StorageConfig::default(),
            providers: ProviderConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 3000,
            host: "127.0.0.1".to_string(),
            max_connections: 100,
            enable_caching: true,
            cache_ttl: Duration::from_secs(3600),
        }
    }
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            watch_enabled: true,
            parallel_workers: 0, // 0 means use number of CPU cores
            include_patterns: vec!["**/*".to_string()],
            exclude_patterns: vec![
                "**/node_modules/**".to_string(),
                "**/.git/**".to_string(),
                "**/target/**".to_string(),
                "**/build/**".to_string(),
            ],
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: "memory".to_string(),
            path: std::env::temp_dir().join("mccp_storage"),
            enable_compression: false,
            compression_level: 1,
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            default: "local".to_string(),
            settings: serde_json::json!({}),
        }
    }
}

impl CliConfig {
    /// Load configuration from file
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?;
        
        let config: CliConfig = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config file: {}", e))?;
        
        Ok(config)
    }
    
    /// Save configuration to file
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize config: {}", e))?;
        
        std::fs::write(path, content)
            .map_err(|e| anyhow::anyhow!("Failed to write config file: {}", e))?;
        
        Ok(())
    }
    
    /// Merge with another configuration
    pub fn merge(&mut self, other: &Self) {
        if other.server.port != 0 {
            self.server.port = other.server.port;
        }
        if !other.server.host.is_empty() {
            self.server.host = other.server.host.clone();
        }
        if other.server.max_connections != 0 {
            self.server.max_connections = other.server.max_connections;
        }
        
        self.server.enable_caching = other.server.enable_caching;
        self.server.cache_ttl = other.server.cache_ttl;
        
        self.indexer.watch_enabled = other.indexer.watch_enabled;
        if other.indexer.parallel_workers != 0 {
            self.indexer.parallel_workers = other.indexer.parallel_workers;
        }
        
        if !other.indexer.include_patterns.is_empty() {
            self.indexer.include_patterns = other.indexer.include_patterns.clone();
        }
        if !other.indexer.exclude_patterns.is_empty() {
            self.indexer.exclude_patterns = other.indexer.exclude_patterns.clone();
        }
        
        if !other.storage.backend.is_empty() {
            self.storage.backend = other.storage.backend.clone();
        }
        if !other.storage.path.as_os_str().is_empty() {
            self.storage.path = other.storage.path.clone();
        }
        
        self.storage.enable_compression = other.storage.enable_compression;
        if other.storage.compression_level != 0 {
            self.storage.compression_level = other.storage.compression_level;
        }
        
        if !other.providers.default.is_empty() {
            self.providers.default = other.providers.default.clone();
        }
        if !other.providers.settings.is_null() {
            self.providers.settings = other.providers.settings.clone();
        }
    }
}

/// Project configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Project ID
    pub project_id: String,
    
    /// Root path
    pub root_path: PathBuf,
    
    /// Language
    pub language: Option<Language>,
    
    /// Include patterns
    pub include_patterns: Vec<String>,
    
    /// Exclude patterns
    pub exclude_patterns: Vec<String>,
    
    /// Chunk size
    pub chunk_size: usize,
    
    /// Chunk overlap
    pub chunk_overlap: usize,
    
    /// Enable file watching
    pub watch_enabled: bool,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            project_id: "default".to_string(),
            root_path: std::env::current_dir().unwrap_or_default(),
            language: None,
            include_patterns: vec!["**/*".to_string()],
            exclude_patterns: vec![
                "**/node_modules/**".to_string(),
                "**/.git/**".to_string(),
                "**/target/**".to_string(),
                "**/build/**".to_string(),
            ],
            chunk_size: 512,
            chunk_overlap: 64,
            watch_enabled: true,
        }
    }
}

impl ProjectConfig {
    /// Load configuration from file
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read project config file: {}", e))?;
        
        let config: ProjectConfig = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse project config file: {}", e))?;
        
        Ok(config)
    }
    
    /// Save configuration to file
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize project config: {}", e))?;
        
        std::fs::write(path, content)
            .map_err(|e| anyhow::anyhow!("Failed to write project config file: {}", e))?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_cli_config_default() {
        let config = CliConfig::default();
        
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.max_connections, 100);
        assert!(config.server.enable_caching);
        assert_eq!(config.server.cache_ttl, Duration::from_secs(3600));
        
        assert!(config.indexer.watch_enabled);
        assert_eq!(config.indexer.parallel_workers, 0);
        assert_eq!(config.indexer.include_patterns, vec!["**/*"]);
        assert!(config.indexer.exclude_patterns.contains(&"**/node_modules/**".to_string()));
        
        assert_eq!(config.storage.backend, "memory");
        assert_eq!(config.storage.enable_compression, false);
        assert_eq!(config.storage.compression_level, 1);
        
        assert_eq!(config.providers.default, "local");
    }

    #[test]
    fn test_project_config_default() {
        let config = ProjectConfig::default();
        
        assert_eq!(config.project_id, "default");
        assert_eq!(config.chunk_size, 512);
        assert_eq!(config.chunk_overlap, 64);
        assert!(config.watch_enabled);
        assert_eq!(config.include_patterns, vec!["**/*"]);
        assert!(config.exclude_patterns.contains(&"**/node_modules/**".to_string()));
    }

    #[tokio::test]
    async fn test_cli_config_load_save() {
        let config = CliConfig::default();
        
        // Create temporary file
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path();
        
        // Save config
        config.save(file_path).unwrap();
        
        // Load config
        let loaded_config = CliConfig::load(file_path).unwrap();
        
        assert_eq!(config.server.port, loaded_config.server.port);
        assert_eq!(config.server.host, loaded_config.server.host);
        assert_eq!(config.indexer.watch_enabled, loaded_config.indexer.watch_enabled);
    }

    #[tokio::test]
    async fn test_project_config_load_save() {
        let config = ProjectConfig::default();
        
        // Create temporary file
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path();
        
        // Save config
        config.save(file_path).unwrap();
        
        // Load config
        let loaded_config = ProjectConfig::load(file_path).unwrap();
        
        assert_eq!(config.project_id, loaded_config.project_id);
        assert_eq!(config.chunk_size, loaded_config.chunk_size);
        assert_eq!(config.chunk_overlap, loaded_config.chunk_overlap);
        assert_eq!(config.watch_enabled, loaded_config.watch_enabled);
    }

    #[test]
    fn test_cli_config_merge() {
        let mut config1 = CliConfig::default();
        let config2 = CliConfig {
            server: ServerConfig {
                port: 8080,
                host: "0.0.0.0".to_string(),
                max_connections: 200,
                enable_caching: false,
                cache_ttl: Duration::from_secs(1800),
            },
            indexer: IndexerConfig {
                watch_enabled: false,
                parallel_workers: 4,
                include_patterns: vec!["**/*.rs".to_string()],
                exclude_patterns: vec!["**/tests/**".to_string()],
            },
            storage: StorageConfig {
                backend: "disk".to_string(),
                path: PathBuf::from("/tmp/mccp"),
                enable_compression: true,
                compression_level: 6,
            },
            providers: ProviderConfig {
                default: "openai".to_string(),
                settings: serde_json::json!({"api_key": "test"}),
            },
        };
        
        config1.merge(&config2);
        
        assert_eq!(config1.server.port, 8080);
        assert_eq!(config1.server.host, "0.0.0.0");
        assert_eq!(config1.server.max_connections, 200);
        assert!(!config1.server.enable_caching);
        assert_eq!(config1.server.cache_ttl, Duration::from_secs(1800));
        
        assert!(!config1.indexer.watch_enabled);
        assert_eq!(config1.indexer.parallel_workers, 4);
        assert_eq!(config1.indexer.include_patterns, vec!["**/*.rs"]);
        assert_eq!(config1.indexer.exclude_patterns, vec!["**/tests/**"]);
        
        assert_eq!(config1.storage.backend, "disk");
        assert_eq!(config1.storage.path, PathBuf::from("/tmp/mccp"));
        assert!(config1.storage.enable_compression);
        assert_eq!(config1.storage.compression_level, 6);
        
        assert_eq!(config1.providers.default, "openai");
    }
}