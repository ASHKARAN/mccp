use super::*;
use mccp_core::*;
use mccp_indexer::*;
use mccp_storage::*;
use mccp_providers::*;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error, warn};

/// MCP server for code context
pub struct MccpServer {
    /// Storage backend
    storage: Arc<RwLock<StorageBackend>>,
    
    /// Indexing pipeline
    indexer: Arc<RwLock<IndexingPipeline>>,
    
    /// Provider manager
    provider_manager: Arc<RwLock<ProviderManager>>,
    
    /// Server configuration
    config: ServerConfig,
    
    /// Running state
    running: Arc<RwLock<bool>>,
}

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
    pub max_connections: usize,
    pub enable_caching: bool,
    pub cache_ttl: std::time::Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 3000,
            host: "127.0.0.1".to_string(),
            max_connections: 100,
            enable_caching: true,
            cache_ttl: std::time::Duration::from_secs(3600), // 1 hour
        }
    }
}

impl MccpServer {
    /// Create a new MCP server
    pub async fn new(config: ServerConfig) -> Result<Self> {
        let storage = Arc::new(RwLock::new(StorageBackend::new()));
        let provider_manager = Arc::new(RwLock::new(ProviderManager::new()));
        
        Ok(Self {
            storage,
            indexer: Arc::new(RwLock::new(IndexingPipeline::new(
                Project::new("default".to_string(), &std::path::PathBuf::from("/tmp")),
                IndexerConfig::default(),
            ))),
            provider_manager,
            config,
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// Start the server
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(Error::ServerError("Server already running".to_string()));
        }
        
        *running = true;
        drop(running);

        info!("Starting MCP server on {}:{}", self.config.host, self.config.port);
        
        // Start indexing pipeline
        let mut indexer = self.indexer.write().await;
        indexer.start().await?;
        drop(indexer);

        // Start provider manager
        let mut provider_manager = self.provider_manager.write().await;
        provider_manager.start().await?;
        drop(provider_manager);

        // TODO: Start HTTP server
        // For now, just log that we're "running"
        info!("MCP server started successfully");
        
        Ok(())
    }

    /// Stop the server
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Err(Error::ServerError("Server not running".to_string()));
        }
        
        *running = false;
        drop(running);

        info!("Stopping MCP server...");
        
        // Stop indexing pipeline
        let indexer = self.indexer.read().await;
        indexer.stop().await?;
        drop(indexer);

        // Stop provider manager
        let provider_manager = self.provider_manager.read().await;
        provider_manager.stop().await?;
        drop(provider_manager);

        info!("MCP server stopped successfully");
        Ok(())
    }

    /// Get server status
    pub async fn status(&self) -> ServerStatus {
        let running = *self.running.read().await;
        let indexer_status = {
            let indexer = self.indexer.read().await;
            indexer.status()
        };
        let provider_status = {
            let provider_manager = self.provider_manager.read().await;
            provider_manager.status()
        };

        ServerStatus {
            running,
            indexer_status,
            provider_status,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Register a new LLM provider
    pub async fn register_provider(&self, provider: Box<dyn LlmProvider>) -> Result<()> {
        let mut provider_manager = self.provider_manager.write().await;
        provider_manager.register_provider(provider).await
    }

    /// Get available providers
    pub async fn get_providers(&self) -> Vec<ProviderInfo> {
        let provider_manager = self.provider_manager.read().await;
        provider_manager.list_providers().await
    }

    /// Get project information
    pub async fn get_project(&self, project_id: &str) -> Result<Project> {
        let storage = self.storage.read().await;
        storage.get_project(project_id).await
    }

    /// List all projects
    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let storage = self.storage.read().await;
        storage.list_projects().await
    }

    /// Get symbols for a project
    pub async fn get_symbols(&self, project_id: &str) -> Result<Vec<Symbol>> {
        let storage = self.storage.read().await;
        storage.get_symbols(project_id).await
    }

    /// Get chunks for a project
    pub async fn get_chunks(&self, project_id: &str) -> Result<Vec<Chunk>> {
        let storage = self.storage.read().await;
        storage.get_chunks(project_id).await
    }

    /// Get summaries for a project
    pub async fn get_summaries(&self, project_id: &str) -> Result<Vec<Summary>> {
        let storage = self.storage.read().await;
        storage.get_summaries(project_id).await
    }

    /// Get call graph for a project
    pub async fn get_graph(&self, project_id: &str) -> Result<GraphStore> {
        let storage = self.storage.read().await;
        storage.get_graph(project_id).await
    }

    /// Search for symbols
    pub async fn search_symbols(&self, project_id: &str, query: &str) -> Result<Vec<Symbol>> {
        let storage = self.storage.read().await;
        storage.search_symbols(project_id, query).await
    }

    /// Search for chunks
    pub async fn search_chunks(&self, project_id: &str, query: &str) -> Result<Vec<Chunk>> {
        let storage = self.storage.read().await;
        storage.search_chunks(project_id, query).await
    }

    /// Get context for a specific location
    pub async fn get_context(&self, project_id: &str, file_path: &str, line: usize, column: usize) -> Result<Context> {
        let storage = self.storage.read().await;
        storage.get_context(project_id, file_path, line, column).await
    }

    /// Get related symbols
    pub async fn get_related_symbols(&self, project_id: &str, symbol_name: &str) -> Result<Vec<Symbol>> {
        let storage = self.storage.read().await;
        storage.get_related_symbols(project_id, symbol_name).await
    }

    /// Get callers of a symbol
    pub async fn get_callers(&self, project_id: &str, symbol_name: &str) -> Result<Vec<Symbol>> {
        let storage = self.storage.read().await;
        storage.get_callers(project_id, symbol_name).await
    }

    /// Get callees of a symbol
    pub async fn get_callees(&self, project_id: &str, symbol_name: &str) -> Result<Vec<Symbol>> {
        let storage = self.storage.read().await;
        storage.get_callees(project_id, symbol_name).await
    }

    /// Get file dependencies
    pub async fn get_file_dependencies(&self, project_id: &str, file_path: &str) -> Result<Vec<String>> {
        let storage = self.storage.read().await;
        storage.get_file_dependencies(project_id, file_path).await
    }

    /// Get project statistics
    pub async fn get_stats(&self, project_id: &str) -> Result<ProjectStats> {
        let storage = self.storage.read().await;
        storage.get_stats(project_id).await
    }
}

/// Server status
#[derive(Debug, Clone)]
pub struct ServerStatus {
    pub running: bool,
    pub indexer_status: IndexingStatus,
    pub provider_status: ProviderStatus,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Provider status
#[derive(Debug, Clone)]
pub struct ProviderStatus {
    pub total_providers: usize,
    pub healthy_providers: usize,
    pub total_models: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        
        assert_eq!(config.port, 3000);
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.max_connections, 100);
        assert!(config.enable_caching);
        assert_eq!(config.cache_ttl, std::time::Duration::from_secs(3600));
    }

    #[tokio::test]
    async fn test_server_creation() {
        let config = ServerConfig::default();
        let server = MccpServer::new(config).await.unwrap();
        
        assert!(!*server.running.read().await);
    }

    #[tokio::test]
    async fn test_server_start_stop() {
        let config = ServerConfig::default();
        let server = MccpServer::new(config).await.unwrap();
        
        // Start server
        server.start().await.unwrap();
        assert!(*server.running.read().await);
        
        // Stop server
        server.stop().await.unwrap();
        assert!(!*server.running.read().await);
    }

    #[tokio::test]
    async fn test_server_status() {
        let config = ServerConfig::default();
        let server = MccpServer::new(config).await.unwrap();
        
        let status = server.status().await;
        
        assert!(!status.running);
        assert_eq!(status.indexer_status.project_id, "default");
        assert_eq!(status.provider_status.total_providers, 0);
        assert_eq!(status.provider_status.healthy_providers, 0);
        assert_eq!(status.provider_status.total_models, 0);
    }

    #[tokio::test]
    async fn test_server_operations() {
        let config = ServerConfig::default();
        let server = MccpServer::new(config).await.unwrap();
        
        // Test project operations
        let projects = server.list_projects().await.unwrap();
        assert_eq!(projects.len(), 0);
        
        // Test provider operations
        let providers = server.get_providers().await;
        assert_eq!(providers.len(), 0);
    }
}