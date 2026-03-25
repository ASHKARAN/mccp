use super::*;
use mccp_core::*;
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::RwLock;
use std::time::{Duration, Instant};

/// A record of a recent query (used for V3-8 cache warming)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueryRecord {
    pub project: String,
    pub query: String,
    pub timestamp: u64,
}

/// Load the most recent queries for cache warming (V3-8).
/// Returns an empty list if no history is persisted yet.
pub async fn load_recent_queries(project: &str, limit: usize) -> anyhow::Result<Vec<QueryRecord>> {
    // Stub: a real impl would read from SQLite/sled; return empty for now
    let _ = (project, limit);
    Ok(vec![])
}

/// Storage backend for managing all data
#[derive(Debug, Clone)]
pub struct StorageBackend {
    /// Projects storage
    projects: Arc<DashMap<String, Project>>,
    
    /// Symbols storage (project_id -> symbols)
    symbols: Arc<DashMap<String, Vec<Symbol>>>,
    
    /// Chunks storage (project_id -> chunks)
    chunks: Arc<DashMap<String, Vec<Chunk>>>,
    
    /// Summaries storage (project_id -> summaries)
    summaries: Arc<DashMap<String, Vec<Summary>>>,
    
    /// Graphs storage (project_id -> graph)
    graphs: Arc<DashMap<String, GraphStore>>,
    
    /// Cache for frequently accessed data
    cache: Arc<RwLock<Cache>>,
    
    /// Persistence layer
    persistence: Arc<RwLock<Persistence>>,
}

impl StorageBackend {
    /// Create a new storage backend
    pub fn new() -> Self {
        Self {
            projects: Arc::new(DashMap::new()),
            symbols: Arc::new(DashMap::new()),
            chunks: Arc::new(DashMap::new()),
            summaries: Arc::new(DashMap::new()),
            graphs: Arc::new(DashMap::new()),
            cache: Arc::new(RwLock::new(Cache::new())),
            persistence: Arc::new(RwLock::new(Persistence::new())),
        }
    }

    /// Get a project
    pub async fn get_project(&self, project_id: &str) -> Result<Project> {
        if let Some(project) = self.projects.get(project_id) {
            Ok(project.clone())
        } else {
            Err(Error::ProjectNotFound(project_id.to_string()))
        }
    }

    /// Add or update a project
    pub async fn set_project(&self, project: Project) -> Result<()> {
        self.projects.insert(project.id.as_str().to_string(), project);
        Ok(())
    }

    /// List all projects
    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let projects: Vec<Project> = self.projects.iter().map(|entry| entry.value().clone()).collect();
        Ok(projects)
    }

    /// Get symbols for a project
    pub async fn get_symbols(&self, project_id: &str) -> Result<Vec<Symbol>> {
        if let Some(symbols) = self.symbols.get(project_id) {
            Ok(symbols.clone())
        } else {
            Err(Error::ProjectNotFound(project_id.to_string()))
        }
    }

    /// Set symbols for a project
    pub async fn set_symbols(&self, project_id: &str, symbols: Vec<Symbol>) -> Result<()> {
        self.symbols.insert(project_id.to_string(), symbols);
        Ok(())
    }

    /// Get chunks for a project
    pub async fn get_chunks(&self, project_id: &str) -> Result<Vec<Chunk>> {
        if let Some(chunks) = self.chunks.get(project_id) {
            Ok(chunks.clone())
        } else {
            Err(Error::ProjectNotFound(project_id.to_string()))
        }
    }

    /// Set chunks for a project
    pub async fn set_chunks(&self, project_id: &str, chunks: Vec<Chunk>) -> Result<()> {
        self.chunks.insert(project_id.to_string(), chunks);
        Ok(())
    }

    /// Get summaries for a project
    pub async fn get_summaries(&self, project_id: &str) -> Result<Vec<Summary>> {
        if let Some(summaries) = self.summaries.get(project_id) {
            Ok(summaries.clone())
        } else {
            Err(Error::ProjectNotFound(project_id.to_string()))
        }
    }

    /// Set summaries for a project
    pub async fn set_summaries(&self, project_id: &str, summaries: Vec<Summary>) -> Result<()> {
        self.summaries.insert(project_id.to_string(), summaries);
        Ok(())
    }

    /// Get call graph for a project
    pub async fn get_graph(&self, project_id: &str) -> Result<GraphStore> {
        if let Some(graph) = self.graphs.get(project_id) {
            Ok(graph.clone())
        } else {
            Err(Error::ProjectNotFound(project_id.to_string()))
        }
    }

    /// Set call graph for a project
    pub async fn set_graph(&self, project_id: &str, graph: GraphStore) -> Result<()> {
        self.graphs.insert(project_id.to_string(), graph);
        Ok(())
    }

    /// Search for symbols in a project
    pub async fn search_symbols(&self, project_id: &str, query: &str) -> Result<Vec<Symbol>> {
        let symbols = self.get_symbols(project_id).await?;
        let query_lower = query.to_lowercase();
        
        let results: Vec<Symbol> = symbols.into_iter()
            .filter(|symbol| {
                symbol.name.to_lowercase().contains(&query_lower) ||
                symbol.context_snippet.to_lowercase().contains(&query_lower)
            })
            .collect();
        
        Ok(results)
    }

    /// Search for chunks in a project
    pub async fn search_chunks(&self, project_id: &str, query: &str) -> Result<Vec<Chunk>> {
        let chunks = self.get_chunks(project_id).await?;
        let query_lower = query.to_lowercase();
        
        let results: Vec<Chunk> = chunks.into_iter()
            .filter(|chunk| {
                chunk.content.to_lowercase().contains(&query_lower)
            })
            .collect();
        
        Ok(results)
    }

    /// Get context for a specific location
    pub async fn get_context(&self, project_id: &str, file_path: &str, line: usize, column: usize) -> Result<Context> {
        let symbols = self.get_symbols(project_id).await?;
        let chunks = self.get_chunks(project_id).await?;
        
        let relevant_symbols: Vec<Symbol> = symbols.into_iter()
            .filter(|s| s.file_path == file_path && s.line == line)
            .collect();
        
        let relevant_chunks: Vec<Chunk> = chunks.into_iter()
            .filter(|c| c.file_path == file_path && c.start_line <= line && c.end_line >= line)
            .collect();
        
        Ok(Context {
            project_id: project_id.to_string(),
            file_path: file_path.to_string(),
            line,
            column,
            symbols: relevant_symbols,
            chunks: relevant_chunks,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Get related symbols
    pub async fn get_related_symbols(&self, project_id: &str, symbol_name: &str) -> Result<Vec<Symbol>> {
        let symbols = self.get_symbols(project_id).await?;
        
        let results: Vec<Symbol> = symbols.into_iter()
            .filter(|symbol| {
                symbol.name != symbol_name &&
                (symbol.name.contains(symbol_name) || symbol_name.contains(&symbol.name))
            })
            .collect();
        
        Ok(results)
    }

    /// Get callers of a symbol
    pub async fn get_callers(&self, project_id: &str, symbol_name: &str) -> Result<Vec<Symbol>> {
        let graph = self.get_graph(project_id).await?;
        let nodes = graph.all_nodes();
        
        let caller_nodes: Vec<String> = nodes.iter()
            .filter(|node| {
                let edges = graph.get_edges_from(&node.id);
                edges.iter().any(|edge| edge.to == symbol_name)
            })
            .map(|node| node.id.clone())
            .collect();
        
        let symbols = self.get_symbols(project_id).await?;
        let callers: Vec<Symbol> = symbols.into_iter()
            .filter(|symbol| caller_nodes.contains(&symbol.name))
            .collect();
        
        Ok(callers)
    }

    /// Get callees of a symbol
    pub async fn get_callees(&self, project_id: &str, symbol_name: &str) -> Result<Vec<Symbol>> {
        let graph = self.get_graph(project_id).await?;
        let nodes = graph.all_nodes();
        
        let callee_nodes: Vec<String> = nodes.iter()
            .filter(|node| {
                let edges = graph.get_edges_from(symbol_name);
                edges.iter().any(|edge| edge.to == node.id)
            })
            .map(|node| node.id.clone())
            .collect();
        
        let symbols = self.get_symbols(project_id).await?;
        let callees: Vec<Symbol> = symbols.into_iter()
            .filter(|symbol| callee_nodes.contains(&symbol.name))
            .collect();
        
        Ok(callees)
    }

    /// Get file dependencies
    pub async fn get_file_dependencies(&self, project_id: &str, file_path: &str) -> Result<Vec<String>> {
        let symbols = self.get_symbols(project_id).await?;
        
        let dependencies: Vec<String> = symbols.iter()
            .filter(|symbol| symbol.file_path == file_path)
            .flat_map(|symbol| {
                // Extract imports from context snippet
                let imports = self.extract_imports(&symbol.context_snippet);
                imports.into_iter()
                    .filter(|import| !import.is_empty() && !import.starts_with("std::"))
                    .collect::<Vec<_>>()
            })
            .collect();
        
        Ok(dependencies)
    }

    /// Get project statistics
    pub async fn get_stats(&self, project_id: &str) -> Result<ProjectStats> {
        let symbols = self.get_symbols(project_id).await?;
        let chunks = self.get_chunks(project_id).await?;
        let summaries = self.get_summaries(project_id).await?;
        
        let by_kind = symbols.iter()
            .fold(std::collections::HashMap::new(), |mut acc, symbol| {
                *acc.entry(symbol.kind).or_insert(0) += 1;
                acc
            });
        
        let total_tokens: usize = chunks.iter().map(|c| c.token_count).sum();
        let avg_tokens = if chunks.is_empty() { 0 } else { total_tokens / chunks.len() };
        
        Ok(ProjectStats {
            project_id: project_id.to_string(),
            total_symbols: symbols.len(),
            total_chunks: chunks.len(),
            total_summaries: summaries.len(),
            total_tokens,
            avg_tokens,
            by_kind,
            last_updated: chrono::Utc::now(),
        })
    }

    /// Get storage statistics
    pub async fn get_stats(&self) -> Result<StorageStats> {
        let total_projects = self.projects.len();
        let total_symbols: usize = self.symbols.iter().map(|entry| entry.value().len()).sum();
        let total_chunks: usize = self.chunks.iter().map(|entry| entry.value().len()).sum();
        let total_summaries: usize = self.summaries.iter().map(|entry| entry.value().len()).sum();
        
        Ok(StorageStats {
            total_projects,
            total_symbols,
            total_chunks,
            total_summaries,
        })
    }

    /// Clear all data for a project
    pub async fn clear_project(&self, project_id: &str) -> Result<()> {
        self.projects.remove(project_id);
        self.symbols.remove(project_id);
        self.chunks.remove(project_id);
        self.summaries.remove(project_id);
        self.graphs.remove(project_id);
        Ok(())
    }

    /// Backup storage to a file
    pub async fn backup(&self, path: &str) -> Result<()> {
        let mut persistence = self.persistence.write().await;
        persistence.backup(self, path).await
    }

    /// Restore storage from a backup file
    pub async fn restore(&self, path: &str) -> Result<()> {
        let mut persistence = self.persistence.write().await;
        persistence.restore(self, path).await
    }

    /// Extract imports from code
    fn extract_imports(&self, code: &str) -> Vec<String> {
        let patterns = [
            r"use\s+([\w:]+)",
            r#"import\s+[\w\*]+\s+from\s+['"]([^'"]+)['"]"#,
            r#"from\s+['"]([^'"]+)['"]\s+import"#,
            r#"#include\s+[<"]([^>"]+)[>"]"#,
        ];

        let mut imports = Vec::new();
        
        for pattern in &patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                for cap in re.captures_iter(code) {
                    if let Some(import) = cap.get(1) {
                        imports.push(import.as_str().to_string());
                    }
                }
            }
        }

        imports
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_storage_backend_creation() {
        let storage = StorageBackend::new();
        
        assert_eq!(storage.projects.len(), 0);
        assert_eq!(storage.symbols.len(), 0);
        assert_eq!(storage.chunks.len(), 0);
        assert_eq!(storage.summaries.len(), 0);
        assert_eq!(storage.graphs.len(), 0);
    }

    #[tokio::test]
    async fn test_project_operations() {
        let storage = StorageBackend::new();
        
        let project = Project::new("test".to_string(), &std::path::PathBuf::from("/tmp"));
        storage.set_project(project.clone()).await.unwrap();
        
        let retrieved = storage.get_project("test").await.unwrap();
        assert_eq!(retrieved.id, project.id);
        
        let projects = storage.list_projects().await.unwrap();
        assert_eq!(projects.len(), 1);
    }

    #[tokio::test]
    async fn test_symbol_operations() {
        let storage = StorageBackend::new();
        
        let symbols = vec![
            Symbol::new(
                "main".to_string(),
                SymbolKind::Function,
                "main".to_string(),
                "src/main.rs".to_string(),
                1,
                0,
                "fn main() {}".to_string(),
                "module".to_string(),
                Language::Rust,
                RefKind::Definition,
            ),
        ];
        
        storage.set_symbols("test", symbols.clone()).await.unwrap();
        
        let retrieved = storage.get_symbols("test").await.unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].name, "main");
    }

    #[tokio::test]
    async fn test_search_operations() {
        let storage = StorageBackend::new();
        
        let symbols = vec![
            Symbol::new(
                "main".to_string(),
                SymbolKind::Function,
                "main".to_string(),
                "src/main.rs".to_string(),
                1,
                0,
                "fn main() {}".to_string(),
                "module".to_string(),
                Language::Rust,
                RefKind::Definition,
            ),
            Symbol::new(
                "helper".to_string(),
                SymbolKind::Function,
                "helper".to_string(),
                "src/helper.rs".to_string(),
                1,
                0,
                "fn helper() {}".to_string(),
                "module".to_string(),
                Language::Rust,
                RefKind::Definition,
            ),
        ];
        
        storage.set_symbols("test", symbols).await.unwrap();
        
        let results = storage.search_symbols("test", "main").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "main");
    }

    #[tokio::test]
    async fn test_stats() {
        let storage = StorageBackend::new();
        
        let project = Project::new("test".to_string(), &std::path::PathBuf::from("/tmp"));
        storage.set_project(project).await.unwrap();
        
        let symbols = vec![
            Symbol::new(
                "main".to_string(),
                SymbolKind::Function,
                "main".to_string(),
                "src/main.rs".to_string(),
                1,
                0,
                "fn main() {}".to_string(),
                "module".to_string(),
                Language::Rust,
                RefKind::Definition,
            ),
        ];
        storage.set_symbols("test", symbols).await.unwrap();
        
        let stats = storage.get_stats("test").await.unwrap();
        assert_eq!(stats.project_id, "test");
        assert_eq!(stats.total_symbols, 1);
        assert_eq!(stats.total_chunks, 0);
        assert_eq!(stats.total_summaries, 0);
        
        let storage_stats = storage.get_stats().await.unwrap();
        assert_eq!(storage_stats.total_projects, 1);
        assert_eq!(storage_stats.total_symbols, 1);
        assert_eq!(storage_stats.total_chunks, 0);
        assert_eq!(storage_stats.total_summaries, 0);
    }
}