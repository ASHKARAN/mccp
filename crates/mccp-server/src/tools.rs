use super::*;
use mccp_core::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MCP tool for code context operations
#[derive(Debug, Clone)]
pub struct CodeContextTool {
    server: Arc<MccpServer>,
}

impl CodeContextTool {
    /// Create a new code context tool
    pub fn new(server: Arc<MccpServer>) -> Self {
        Self { server }
    }

    /// Get project information
    pub async fn get_project(&self, project_id: &str) -> Result<Project> {
        self.server.get_project(project_id).await
    }

    /// List all projects
    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        self.server.list_projects().await
    }

    /// Get symbols for a project
    pub async fn get_symbols(&self, project_id: &str) -> Result<Vec<Symbol>> {
        self.server.get_symbols(project_id).await
    }

    /// Get chunks for a project
    pub async fn get_chunks(&self, project_id: &str) -> Result<Vec<Chunk>> {
        self.server.get_chunks(project_id).await
    }

    /// Get summaries for a project
    pub async fn get_summaries(&self, project_id: &str) -> Result<Vec<Summary>> {
        self.server.get_summaries(project_id).await
    }

    /// Get call graph for a project
    pub async fn get_graph(&self, project_id: &str) -> Result<GraphStore> {
        self.server.get_graph(project_id).await
    }

    /// Search for symbols
    pub async fn search_symbols(&self, project_id: &str, query: &str) -> Result<Vec<Symbol>> {
        self.server.search_symbols(project_id, query).await
    }

    /// Search for chunks
    pub async fn search_chunks(&self, project_id: &str, query: &str) -> Result<Vec<Chunk>> {
        self.server.search_chunks(project_id, query).await
    }

    /// Get context for a specific location
    pub async fn get_context(&self, project_id: &str, file_path: &str, line: usize, column: usize) -> Result<Context> {
        self.server.get_context(project_id, file_path, line, column).await
    }

    /// Get related symbols
    pub async fn get_related_symbols(&self, project_id: &str, symbol_name: &str) -> Result<Vec<Symbol>> {
        self.server.get_related_symbols(project_id, symbol_name).await
    }

    /// Get callers of a symbol
    pub async fn get_callers(&self, project_id: &str, symbol_name: &str) -> Result<Vec<Symbol>> {
        self.server.get_callers(project_id, symbol_name).await
    }

    /// Get callees of a symbol
    pub async fn get_callees(&self, project_id: &str, symbol_name: &str) -> Result<Vec<Symbol>> {
        self.server.get_callees(project_id, symbol_name).await
    }

    /// Get file dependencies
    pub async fn get_file_dependencies(&self, project_id: &str, file_path: &str) -> Result<Vec<String>> {
        self.server.get_file_dependencies(project_id, file_path).await
    }

    /// Get project statistics
    pub async fn get_stats(&self, project_id: &str) -> Result<ProjectStats> {
        self.server.get_stats(project_id).await
    }
}

/// MCP tool for LLM provider operations
#[derive(Debug, Clone)]
pub struct ProviderTool {
    server: Arc<MccpServer>,
}

impl ProviderTool {
    /// Create a new provider tool
    pub fn new(server: Arc<MccpServer>) -> Self {
        Self { server }
    }

    /// Register a new LLM provider
    pub async fn register_provider(&self, provider: Box<dyn LlmProvider>) -> Result<()> {
        self.server.register_provider(provider).await
    }

    /// Get available providers
    pub async fn list_providers(&self) -> Vec<ProviderInfo> {
        self.server.get_providers().await
    }

    /// Get provider health status
    pub async fn get_provider_health(&self, provider_id: &str) -> Result<ProviderHealth> {
        let providers = self.server.get_providers().await;
        let provider_info = providers.iter().find(|p| p.id == provider_id);
        
        match provider_info {
            Some(info) => Ok(info.health.clone()),
            None => Err(Error::ProviderError(format!("Provider {} not found", provider_id))),
        }
    }
}

/// MCP tool for indexing operations
#[derive(Debug, Clone)]
pub struct IndexingTool {
    server: Arc<MccpServer>,
}

impl IndexingTool {
    /// Create a new indexing tool
    pub fn new(server: Arc<MccpServer>) -> Self {
        Self { server }
    }

    /// Get indexing status
    pub async fn get_status(&self) -> IndexingStatus {
        let indexer = self.server.indexer.read().await;
        indexer.status()
    }

    /// Force re-index all files
    pub async fn force_reindex(&self) -> Result<()> {
        let indexer = self.server.indexer.read().await;
        indexer.force_reindex().await
    }

    /// Add a file to the indexing queue
    pub async fn add_file(&self, project_id: &str, file_path: &str) -> Result<()> {
        // TODO: Implement file addition to indexing queue
        Ok(())
    }

    /// Remove a file from the index
    pub async fn remove_file(&self, project_id: &str, file_path: &str) -> Result<()> {
        // TODO: Implement file removal from index
        Ok(())
    }
}

/// MCP tool for storage operations
#[derive(Debug, Clone)]
pub struct StorageTool {
    server: Arc<MccpServer>,
}

impl StorageTool {
    /// Create a new storage tool
    pub fn new(server: Arc<MccpServer>) -> Self {
        Self { server }
    }

    /// Get storage statistics
    pub async fn get_stats(&self) -> Result<StorageStats> {
        let storage = self.server.storage.read().await;
        storage.get_stats().await
    }

    /// Clear all data for a project
    pub async fn clear_project(&self, project_id: &str) -> Result<()> {
        let mut storage = self.server.storage.write().await;
        storage.clear_project(project_id).await
    }

    /// Backup storage
    pub async fn backup(&self, path: &str) -> Result<()> {
        let storage = self.server.storage.read().await;
        storage.backup(path).await
    }

    /// Restore storage from backup
    pub async fn restore(&self, path: &str) -> Result<()> {
        let mut storage = self.server.storage.write().await;
        storage.restore(path).await
    }
}

/// Tool input for getting project information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProjectInput {
    pub project_id: String,
}

/// Tool input for listing projects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListProjectsInput {}

/// Tool input for getting symbols
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetSymbolsInput {
    pub project_id: String,
}

/// Tool input for searching symbols
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSymbolsInput {
    pub project_id: String,
    pub query: String,
}

/// Tool input for getting context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetContextInput {
    pub project_id: String,
    pub file_path: String,
    pub line: usize,
    pub column: usize,
}

/// Tool input for registering a provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterProviderInput {
    pub provider_type: String,
    pub config: HashMap<String, String>,
}

/// Tool input for listing providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListProvidersInput {}

/// Tool input for getting indexing status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetIndexingStatusInput {}

/// Tool input for forcing reindex
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceReindexInput {
    pub project_id: String,
}

/// Tool input for getting storage stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetStorageStatsInput {}

/// Tool input for clearing a project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearProjectInput {
    pub project_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_code_context_tool() {
        let config = ServerConfig::default();
        let server = Arc::new(MccpServer::new(config).await.unwrap());
        let tool = CodeContextTool::new(server);
        
        // Test list projects
        let projects = tool.list_projects().await.unwrap();
        assert_eq!(projects.len(), 0);
    }

    #[tokio::test]
    async fn test_provider_tool() {
        let config = ServerConfig::default();
        let server = Arc::new(MccpServer::new(config).await.unwrap());
        let tool = ProviderTool::new(server);
        
        // Test list providers
        let providers = tool.list_providers().await;
        assert_eq!(providers.len(), 0);
    }

    #[tokio::test]
    async fn test_indexing_tool() {
        let config = ServerConfig::default();
        let server = Arc::new(MccpServer::new(config).await.unwrap());
        let tool = IndexingTool::new(server);
        
        // Test get status
        let status = tool.get_status().await;
        assert_eq!(status.project_id, "default");
    }

    #[tokio::test]
    async fn test_storage_tool() {
        let config = ServerConfig::default();
        let server = Arc::new(MccpServer::new(config).await.unwrap());
        let tool = StorageTool::new(server);
        
        // Test get stats
        let stats = tool.get_stats().await.unwrap();
        assert_eq!(stats.total_projects, 0);
        assert_eq!(stats.total_symbols, 0);
        assert_eq!(stats.total_chunks, 0);
        assert_eq!(stats.total_summaries, 0);
    }
}