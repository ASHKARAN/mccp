use super::*;
use std::path::PathBuf;
use std::collections::HashMap;

/// Project metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub root_path: PathBuf,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_indexed: Option<chrono::DateTime<chrono::Utc>>,
    pub file_count: usize,
    pub chunk_count: usize,
    pub embedding_model: Option<String>,
    pub chat_model: Option<String>,
    pub status: ProjectStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProjectStatus {
    Indexed,
    Indexing,
    Error(String),
    NotIndexed,
}

impl Project {
    /// Create a new project
    pub fn new<P: AsRef<std::path::Path>>(name: String, root_path: P) -> Self {
        Self {
            id: ProjectId::from_path(&root_path),
            name,
            root_path: root_path.as_ref().to_path_buf(),
            created_at: chrono::Utc::now(),
            last_indexed: None,
            file_count: 0,
            chunk_count: 0,
            embedding_model: None,
            chat_model: None,
            status: ProjectStatus::NotIndexed,
        }
    }

    /// Get all source files in the project
    pub fn source_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.root_path.exists() {
            for entry in walkdir::WalkDir::new(&self.root_path) {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if Language::from_extension(&ext.to_string_lossy()).is_some() {
                            files.push(path.to_path_buf());
                        }
                    }
                }
            }
        }
        files
    }

    /// Get files matching a glob pattern
    pub fn files_matching(&self, pattern: &str) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.root_path.exists() {
            let glob = glob::glob(&format!("{}/**/{}", self.root_path.display(), pattern)).unwrap();
            for entry in glob {
                if let Ok(path) = entry {
                    files.push(path);
                }
            }
        }
        files
    }

    /// Get relative path from project root
    pub fn relative_path(&self, path: &std::path::Path) -> Option<String> {
        path.strip_prefix(&self.root_path)
            .ok()
            .map(|p| p.to_string_lossy().to_string())
    }

    /// Get absolute path from relative path
    pub fn absolute_path(&self, relative_path: &str) -> PathBuf {
        self.root_path.join(relative_path)
    }
}

/// Project manager for handling multiple projects
#[derive(Debug)]
pub struct ProjectManager {
    projects: HashMap<String, Project>,
    default_project: Option<String>,
}

impl ProjectManager {
    pub fn new() -> Self {
        Self {
            projects: HashMap::new(),
            default_project: None,
        }
    }

    /// Add a project
    pub fn add_project<P: AsRef<std::path::Path>>(&mut self, name: String, path: P) -> Result<()> {
        if self.projects.contains_key(&name) {
            return Err(Error::ProjectAlreadyExists(name));
        }
        
        let project = Project::new(name.clone(), path);
        self.projects.insert(name.clone(), project);
        
        if self.default_project.is_none() {
            self.default_project = Some(name);
        }
        
        Ok(())
    }

    /// Remove a project
    pub fn remove_project(&mut self, name: &str) -> Result<()> {
        if !self.projects.contains_key(name) {
            return Err(Error::ProjectNotFound(name.to_string()));
        }
        
        self.projects.remove(name);
        
        if self.default_project.as_deref() == Some(name) {
            self.default_project = self.projects.keys().next().cloned();
        }
        
        Ok(())
    }

    /// Get a project by name
    pub fn get_project(&self, name: &str) -> Option<&Project> {
        self.projects.get(name)
    }

    /// Get a mutable reference to a project
    pub fn get_project_mut(&mut self, name: &str) -> Option<&mut Project> {
        self.projects.get_mut(name)
    }

    /// List all projects
    pub fn list_projects(&self) -> Vec<&Project> {
        self.projects.values().collect()
    }

    /// Set the default project
    pub fn set_default_project(&mut self, name: &str) -> Result<()> {
        if !self.projects.contains_key(name) {
            return Err(Error::ProjectNotFound(name.to_string()));
        }
        self.default_project = Some(name.to_string());
        Ok(())
    }

    /// Get the default project
    pub fn default_project(&self) -> Option<&Project> {
        self.default_project.as_ref().and_then(|name| self.projects.get(name))
    }

    /// Get the default project name
    pub fn default_project_name(&self) -> Option<&str> {
        self.default_project.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_project_id_deterministic() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();
        
        let id_a = ProjectId::from_path(path);
        let id_b = ProjectId::from_path(path);
        
        assert_eq!(id_a, id_b);
    }

    #[test]
    fn test_project_id_different_paths() {
        let temp_dir_a = TempDir::new().unwrap();
        let temp_dir_b = TempDir::new().unwrap();
        
        let id_a = ProjectId::from_path(temp_dir_a.path());
        let id_b = ProjectId::from_path(temp_dir_b.path());
        
        assert_ne!(id_a, id_b);
    }

    #[test]
    fn test_project_manager() {
        let mut manager = ProjectManager::new();
        
        let temp_dir = TempDir::new().unwrap();
        manager.add_project("test".to_string(), temp_dir.path()).unwrap();
        
        assert_eq!(manager.list_projects().len(), 1);
        assert_eq!(manager.default_project_name(), Some("test"));
        
        let project = manager.get_project("test").unwrap();
        assert_eq!(project.name, "test");
    }
}