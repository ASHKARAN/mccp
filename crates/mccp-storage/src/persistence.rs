use super::*;
use mccp_core::*;
use std::collections::HashMap;
use std::path::Path;
use std::fs;
use serde::{Deserialize, Serialize};

/// Persistence layer for saving and loading data
#[derive(Debug, Clone)]
pub struct Persistence;

impl Persistence {
    /// Create a new persistence layer
    pub fn new() -> Self {
        Self
    }

    /// Backup storage to a file
    pub async fn backup(&self, storage: &StorageBackend, path: &str) -> Result<()> {
        let backup_data = BackupData {
            projects: self.export_projects(storage).await?,
            symbols: self.export_symbols(storage).await?,
            chunks: self.export_chunks(storage).await?,
            summaries: self.export_summaries(storage).await?,
        };

        let json = serde_json::to_string_pretty(&backup_data)
            .map_err(|e| Error::ParseError(e.to_string()))?;

        fs::write(path, json)
            .map_err(|e| Error::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

        Ok(())
    }

    /// Restore storage from a backup file
    pub async fn restore(&self, storage: &StorageBackend, path: &str) -> Result<()> {
        let content = fs::read_to_string(path)
            .map_err(|e| Error::IoError(e))?;

        let backup_data: BackupData = serde_json::from_str(&content)
            .map_err(|e| Error::ParseError(e.to_string()))?;

        self.import_projects(storage, backup_data.projects).await?;
        self.import_symbols(storage, backup_data.symbols).await?;
        self.import_chunks(storage, backup_data.chunks).await?;
        self.import_summaries(storage, backup_data.summaries).await?;

        Ok(())
    }

    /// Export projects to backup format
    async fn export_projects(&self, storage: &StorageBackend) -> Result<Vec<Project>> {
        storage.list_projects().await
    }

    /// Export symbols to backup format
    async fn export_symbols(&self, storage: &StorageBackend) -> Result<HashMap<String, Vec<Symbol>>> {
        let projects = storage.list_projects().await?;
        let mut symbols_map = HashMap::new();

        for project in projects {
            if let Ok(symbols) = storage.get_symbols(project.id.as_str()).await {
                symbols_map.insert(project.id.as_str().to_string(), symbols);
            }
        }

        Ok(symbols_map)
    }

    /// Export chunks to backup format
    async fn export_chunks(&self, storage: &StorageBackend) -> Result<HashMap<String, Vec<Chunk>>> {
        let projects = storage.list_projects().await?;
        let mut chunks_map = HashMap::new();

        for project in projects {
            if let Ok(chunks) = storage.get_chunks(project.id.as_str()).await {
                chunks_map.insert(project.id.as_str().to_string(), chunks);
            }
        }

        Ok(chunks_map)
    }

    /// Export summaries to backup format
    async fn export_summaries(&self, storage: &StorageBackend) -> Result<HashMap<String, Vec<Summary>>> {
        let projects = storage.list_projects().await?;
        let mut summaries_map = HashMap::new();

        for project in projects {
            if let Ok(summaries) = storage.get_summaries(project.id.as_str()).await {
                summaries_map.insert(project.id.as_str().to_string(), summaries);
            }
        }

        Ok(summaries_map)
    }

    /// Import projects from backup format
    async fn import_projects(&self, storage: &StorageBackend, projects: Vec<Project>) -> Result<()> {
        for project in projects {
            storage.set_project(project).await?;
        }
        Ok(())
    }

    /// Import symbols from backup format
    async fn import_symbols(&self, storage: &StorageBackend, symbols_map: HashMap<String, Vec<Symbol>>) -> Result<()> {
        for (project_id, symbols) in symbols_map {
            storage.set_symbols(&project_id, symbols).await?;
        }
        Ok(())
    }

    /// Import chunks from backup format
    async fn import_chunks(&self, storage: &StorageBackend, chunks_map: HashMap<String, Vec<Chunk>>) -> Result<()> {
        for (project_id, chunks) in chunks_map {
            storage.set_chunks(&project_id, chunks).await?;
        }
        Ok(())
    }

    /// Import summaries from backup format
    async fn import_summaries(&self, storage: &StorageBackend, summaries_map: HashMap<String, Vec<Summary>>) -> Result<()> {
        for (project_id, summaries) in summaries_map {
            storage.set_summaries(&project_id, summaries).await?;
        }
        Ok(())
    }

    /// Check if a backup file exists
    pub fn backup_exists(path: &str) -> bool {
        Path::new(path).exists()
    }

    /// Get backup file size
    pub fn backup_size(path: &str) -> Result<u64> {
        let metadata = fs::metadata(path)
            .map_err(|e| Error::IoError(e))?;
        
        Ok(metadata.len())
    }
}

/// Backup data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupData {
    projects: Vec<Project>,
    symbols: HashMap<String, Vec<Symbol>>,
    chunks: HashMap<String, Vec<Chunk>>,
    summaries: HashMap<String, Vec<Summary>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_persistence_creation() {
        let persistence = Persistence::new();
        assert!(Persistence::backup_exists("nonexistent.json") == false);
    }

    #[tokio::test]
    async fn test_backup_restore() {
        let storage = StorageBackend::new();
        let persistence = Persistence::new();
        
        // Create test data
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
        
        // Create temporary file for backup
        let temp_file = NamedTempFile::new().unwrap();
        let backup_path = temp_file.path().to_str().unwrap();
        
        // Backup
        persistence.backup(&storage, backup_path).await.unwrap();
        
        // Verify backup file exists
        assert!(Persistence::backup_exists(backup_path));
        
        // Create new storage and restore
        let new_storage = StorageBackend::new();
        persistence.restore(&new_storage, backup_path).await.unwrap();
        
        // Verify data was restored
        let restored_project = new_storage.get_project("test").await.unwrap();
        assert_eq!(restored_project.name, "test");
        
        let restored_symbols = new_storage.get_symbols("test").await.unwrap();
        assert_eq!(restored_symbols.len(), 1);
        assert_eq!(restored_symbols[0].name, "main");
    }

    #[tokio::test]
    async fn test_backup_size() {
        let storage = StorageBackend::new();
        let persistence = Persistence::new();
        
        // Create test data
        let project = Project::new("test".to_string(), &std::path::PathBuf::from("/tmp"));
        storage.set_project(project).await.unwrap();
        
        // Create temporary file for backup
        let temp_file = NamedTempFile::new().unwrap();
        let backup_path = temp_file.path().to_str().unwrap();
        
        // Backup
        persistence.backup(&storage, backup_path).await.unwrap();
        
        // Check backup size
        let size = Persistence::backup_size(backup_path).unwrap();
        assert!(size > 0);
    }

    #[tokio::test]
    async fn test_export_import_projects() {
        let storage = StorageBackend::new();
        let persistence = Persistence::new();
        
        // Create test data
        let project = Project::new("test".to_string(), &std::path::PathBuf::from("/tmp"));
        storage.set_project(project.clone()).await.unwrap();
        
        // Export projects
        let exported_projects = persistence.export_projects(&storage).await.unwrap();
        assert_eq!(exported_projects.len(), 1);
        assert_eq!(exported_projects[0].name, "test");
        
        // Create new storage and import
        let new_storage = StorageBackend::new();
        persistence.import_projects(&new_storage, exported_projects).await.unwrap();
        
        // Verify import
        let imported_project = new_storage.get_project("test").await.unwrap();
        assert_eq!(imported_project.name, "test");
    }
}