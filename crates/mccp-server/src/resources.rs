use super::*;
use mccp_core::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MCP resource for project information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectResource {
    pub project: Project,
    pub symbols: Vec<Symbol>,
    pub chunks: Vec<Chunk>,
    pub summaries: Vec<Summary>,
    pub graph: GraphStore,
}

/// MCP resource for symbol information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolResource {
    pub symbol: Symbol,
    pub context: Context,
    pub related_symbols: Vec<Symbol>,
    pub callers: Vec<Symbol>,
    pub callees: Vec<Symbol>,
}

/// MCP resource for chunk information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkResource {
    pub chunk: Chunk,
    pub summary: Option<Summary>,
    pub context: Context,
}

/// MCP resource for search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResource {
    pub query: String,
    pub symbols: Vec<Symbol>,
    pub chunks: Vec<Chunk>,
    pub total_results: usize,
}

/// MCP resource for project statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResource {
    pub project_id: String,
    pub stats: ProjectStats,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// MCP resource for server status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResource {
    pub server_status: ServerStatus,
    pub provider_status: ProviderStatus,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// MCP resource for provider information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResource {
    pub providers: Vec<ProviderInfo>,
    pub total_providers: usize,
    pub healthy_providers: usize,
    pub total_models: usize,
}

/// MCP resource for indexing status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingResource {
    pub status: IndexingStatus,
    pub queue_depth: usize,
    pub processed_files: usize,
    pub total_files: usize,
}

/// MCP resource for storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageResource {
    pub stats: StorageStats,
    pub disk_usage: u64,
    pub memory_usage: u64,
}

/// Resource manager for MCP server
#[derive(Debug, Clone)]
pub struct ResourceManager {
    server: Arc<MccpServer>,
}

impl ResourceManager {
    /// Create a new resource manager
    pub fn new(server: Arc<MccpServer>) -> Self {
        Self { server }
    }

    /// Get project resource
    pub async fn get_project_resource(&self, project_id: &str) -> Result<ProjectResource> {
        let project = self.server.get_project(project_id).await?;
        let symbols = self.server.get_symbols(project_id).await?;
        let chunks = self.server.get_chunks(project_id).await?;
        let summaries = self.server.get_summaries(project_id).await?;
        let graph = self.server.get_graph(project_id).await?;

        Ok(ProjectResource {
            project,
            symbols,
            chunks,
            summaries,
            graph,
        })
    }

    /// Get symbol resource
    pub async fn get_symbol_resource(&self, project_id: &str, symbol_name: &str) -> Result<SymbolResource> {
        let symbols = self.server.get_symbols(project_id).await?;
        let symbol = symbols.iter().find(|s| s.name == symbol_name)
            .ok_or_else(|| Error::SymbolNotFound(symbol_name.to_string()))?;
        
        let context = self.server.get_context(project_id, &symbol.file_path, symbol.line, symbol.column).await?;
        let related_symbols = self.server.get_related_symbols(project_id, symbol_name).await?;
        let callers = self.server.get_callers(project_id, symbol_name).await?;
        let callees = self.server.get_callees(project_id, symbol_name).await?;

        Ok(SymbolResource {
            symbol: symbol.clone(),
            context,
            related_symbols,
            callers,
            callees,
        })
    }

    /// Get chunk resource
    pub async fn get_chunk_resource(&self, project_id: &str, chunk_id: &str) -> Result<ChunkResource> {
        let chunks = self.server.get_chunks(project_id).await?;
        let chunk = chunks.iter().find(|c| c.id == chunk_id)
            .ok_or_else(|| Error::ChunkNotFound(chunk_id.to_string()))?;
        
        let summaries = self.server.get_summaries(project_id).await?;
        let summary = summaries.iter().find(|s| s.chunk_id == chunk_id).cloned();
        
        let context = self.server.get_context(project_id, &chunk.file_path, chunk.start_line, chunk.start_column).await?;

        Ok(ChunkResource {
            chunk: chunk.clone(),
            summary,
            context,
        })
    }

    /// Get search resource
    pub async fn get_search_resource(&self, project_id: &str, query: &str) -> Result<SearchResource> {
        let symbols = self.server.search_symbols(project_id, query).await?;
        let chunks = self.server.search_chunks(project_id, query).await?;
        let total_results = symbols.len() + chunks.len();

        Ok(SearchResource {
            query: query.to_string(),
            symbols,
            chunks,
            total_results,
        })
    }

    /// Get stats resource
    pub async fn get_stats_resource(&self, project_id: &str) -> Result<StatsResource> {
        let stats = self.server.get_stats(project_id).await?;
        
        Ok(StatsResource {
            project_id: project_id.to_string(),
            stats,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Get status resource
    pub async fn get_status_resource(&self) -> Result<StatusResource> {
        let server_status = self.server.status().await;
        let provider_status = {
            let provider_manager = self.server.provider_manager.read().await;
            provider_manager.status()
        };

        Ok(StatusResource {
            server_status,
            provider_status,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Get provider resource
    pub async fn get_provider_resource(&self) -> Result<ProviderResource> {
        let providers = self.server.get_providers().await;
        let total_providers = providers.len();
        let healthy_providers = providers.iter().filter(|p| p.health.is_healthy()).count();
        let total_models = providers.iter().map(|p| p.models.len()).sum();

        Ok(ProviderResource {
            providers,
            total_providers,
            healthy_providers,
            total_models,
        })
    }

    /// Get indexing resource
    pub async fn get_indexing_resource(&self) -> Result<IndexingResource> {
        let status = {
            let indexer = self.server.indexer.read().await;
            indexer.status()
        };
        
        // TODO: Get actual queue depth and processed files
        let queue_depth = 0;
        let processed_files = status.indexed_files;
        let total_files = status.file_count;

        Ok(IndexingResource {
            status,
            queue_depth,
            processed_files,
            total_files,
        })
    }

    /// Get storage resource
    pub async fn get_storage_resource(&self) -> Result<StorageResource> {
        let stats = {
            let storage = self.server.storage.read().await;
            storage.get_stats().await?
        };
        
        // TODO: Get actual disk and memory usage
        let disk_usage = 0;
        let memory_usage = 0;

        Ok(StorageResource {
            stats,
            disk_usage,
            memory_usage,
        })
    }
}

/// Resource URI patterns
pub mod uri_patterns {
    pub const PROJECT: &str = "mccp://project/{project_id}";
    pub const SYMBOL: &str = "mccp://symbol/{project_id}/{symbol_name}";
    pub const CHUNK: &str = "mccp://chunk/{project_id}/{chunk_id}";
    pub const SEARCH: &str = "mccp://search/{project_id}/{query}";
    pub const STATS: &str = "mccp://stats/{project_id}";
    pub const STATUS: &str = "mccp://status";
    pub const PROVIDER: &str = "mccp://provider";
    pub const INDEXING: &str = "mccp://indexing";
    pub const STORAGE: &str = "mccp://storage";
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_resource_manager() {
        let config = ServerConfig::default();
        let server = Arc::new(MccpServer::new(config).await.unwrap());
        let resource_manager = ResourceManager::new(server);

        // Test status resource
        let status_resource = resource_manager.get_status_resource().await.unwrap();
        assert!(!status_resource.server_status.running);
        assert_eq!(status_resource.server_status.indexer_status.project_id, "default");
    }

    #[tokio::test]
    async fn test_project_resource() {
        let config = ServerConfig::default();
        let server = Arc::new(MccpServer::new(config).await.unwrap());
        let resource_manager = ResourceManager::new(server);

        // Test project resource (should fail since project doesn't exist)
        let result = resource_manager.get_project_resource("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_provider_resource() {
        let config = ServerConfig::default();
        let server = Arc::new(MccpServer::new(config).await.unwrap());
        let resource_manager = ResourceManager::new(server);

        // Test provider resource
        let provider_resource = resource_manager.get_provider_resource().await.unwrap();
        assert_eq!(provider_resource.total_providers, 0);
        assert_eq!(provider_resource.healthy_providers, 0);
        assert_eq!(provider_resource.total_models, 0);
    }

    #[tokio::test]
    async fn test_indexing_resource() {
        let config = ServerConfig::default();
        let server = Arc::new(MccpServer::new(config).await.unwrap());
        let resource_manager = ResourceManager::new(server);

        // Test indexing resource
        let indexing_resource = resource_manager.get_indexing_resource().await.unwrap();
        assert_eq!(indexing_resource.status.project_id, "default");
        assert_eq!(indexing_resource.processed_files, 0);
        assert_eq!(indexing_resource.total_files, 0);
    }

    #[tokio::test]
    async fn test_storage_resource() {
        let config = ServerConfig::default();
        let server = Arc::new(MccpServer::new(config).await.unwrap());
        let resource_manager = ResourceManager::new(server);

        // Test storage resource
        let storage_resource = resource_manager.get_storage_resource().await.unwrap();
        assert_eq!(storage_resource.stats.total_projects, 0);
        assert_eq!(storage_resource.stats.total_symbols, 0);
        assert_eq!(storage_resource.stats.total_chunks, 0);
        assert_eq!(storage_resource.stats.total_summaries, 0);
    }
}