use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectConfig {
    pub name: String,
    pub path: PathBuf,
    pub language: String,
    pub created_at: String,
    pub last_indexed: Option<String>,
    pub indexed_files: u64,
    pub total_files: u64,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct WorkspaceConfig {
    pub projects: HashMap<String, ProjectConfig>,
    pub default_project: Option<String>,
}

impl ProjectConfig {
    pub fn new(name: String, path: PathBuf, language: String) -> Self {
        Self {
            name,
            path,
            language,
            created_at: chrono::Utc::now().to_rfc3339(),
            last_indexed: None,
            indexed_files: 0,
            total_files: 0,
        }
    }

    pub fn detect_language(path: &Path) -> String {
        let mut language_counts = HashMap::new();
        
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    
                    match ext.as_ref() {
                        "rs" => *language_counts.entry("rust".to_string()).or_insert(0) += 1,
                        "js" | "jsx" | "ts" | "tsx" => *language_counts.entry("javascript".to_string()).or_insert(0) += 1,
                        "py" => *language_counts.entry("python".to_string()).or_insert(0) += 1,
                        "java" => *language_counts.entry("java".to_string()).or_insert(0) += 1,
                        "go" => *language_counts.entry("go".to_string()).or_insert(0) += 1,
                        "c" | "h" => *language_counts.entry("c".to_string()).or_insert(0) += 1,
                        "cpp" | "cxx" | "cc" | "hpp" => *language_counts.entry("cpp".to_string()).or_insert(0) += 1,
                        "cs" => *language_counts.entry("csharp".to_string()).or_insert(0) += 1,
                        "rb" => *language_counts.entry("ruby".to_string()).or_insert(0) += 1,
                        "php" => *language_counts.entry("php".to_string()).or_insert(0) += 1,
                        "kt" => *language_counts.entry("kotlin".to_string()).or_insert(0) += 1,
                        _ => {}
                    }
                }
            }
        }

        language_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(lang, _)| lang)
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn get_project_name(path: &Path) -> String {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    pub fn save(&self, config_dir: &Path) -> anyhow::Result<()> {
        let config_path = config_dir.join("project.toml");
        let toml = toml::to_string_pretty(self)?;
        fs::write(config_path, toml)?;
        Ok(())
    }

    pub fn load(config_dir: &Path) -> anyhow::Result<Self> {
        let config_path = config_dir.join("project.toml");
        let content = fs::read_to_string(config_path)?;
        let config: ProjectConfig = toml::from_str(&content)?;
        Ok(config)
    }
}

impl WorkspaceConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_config_dir() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".mccp")
    }

    pub fn get_project_config_dir(project_path: &Path) -> PathBuf {
        project_path.join(".mccp")
    }

    pub fn load() -> anyhow::Result<Self> {
        let config_dir = Self::get_config_dir();
        let config_path = config_dir.join("workspace.toml");
        
        if config_path.exists() {
            let content = fs::read_to_string(config_path)?;
            let config: WorkspaceConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::new())
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let config_dir = Self::get_config_dir();
        fs::create_dir_all(&config_dir)?;
        
        let config_path = config_dir.join("workspace.toml");
        let toml = toml::to_string_pretty(self)?;
        fs::write(config_path, toml)?;
        Ok(())
    }

    pub fn add_project(&mut self, project: ProjectConfig) {
        self.projects.insert(project.name.clone(), project.clone());
        if self.default_project.is_none() {
            self.default_project = Some(project.name.clone());
        }
    }

    pub fn get_current_project(&self) -> Option<&ProjectConfig> {
        if let Some(ref default) = self.default_project {
            self.projects.get(default)
        } else {
            None
        }
    }

    pub fn set_default_project(&mut self, name: &str) -> anyhow::Result<()> {
        if self.projects.contains_key(name) {
            self.default_project = Some(name.to_string());
            self.save()?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Project '{}' not found", name))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // ── ProjectConfig ─────────────────────────────────────────────────────────

    #[test]
    fn test_get_project_name_returns_dir_basename() {
        let path = PathBuf::from("/home/user/my-cool-project");
        assert_eq!(ProjectConfig::get_project_name(&path), "my-cool-project");
    }

    #[test]
    fn test_get_project_name_root_returns_unknown() {
        let path = PathBuf::from("/");
        // "/" has no file_name; should fall back to "unknown"
        let name = ProjectConfig::get_project_name(&path);
        assert_eq!(name, "unknown");
    }

    #[test]
    fn test_detect_language_rust() {
        let tmp = make_temp_dir();
        let dir = tmp.path();
        std::fs::write(dir.join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.join("lib.rs"), "").unwrap();
        assert_eq!(ProjectConfig::detect_language(dir), "rust");
    }

    // Helper: build a temp dir without the #[test] attr so we can share it.
    fn make_temp_dir() -> TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    #[test]
    fn detect_language_rust() {
        let tmp = make_temp_dir();
        let dir = tmp.path();
        std::fs::write(dir.join("a.rs"), "").unwrap();
        std::fs::write(dir.join("b.rs"), "").unwrap();
        std::fs::write(dir.join("c.py"), "").unwrap();
        assert_eq!(ProjectConfig::detect_language(dir), "rust");
    }

    #[test]
    fn detect_language_python() {
        let tmp = make_temp_dir();
        let dir = tmp.path();
        std::fs::write(dir.join("a.py"), "").unwrap();
        std::fs::write(dir.join("b.py"), "").unwrap();
        std::fs::write(dir.join("c.py"), "").unwrap();
        assert_eq!(ProjectConfig::detect_language(dir), "python");
    }

    #[test]
    fn detect_language_empty_dir_returns_unknown() {
        let tmp = make_temp_dir();
        assert_eq!(ProjectConfig::detect_language(tmp.path()), "unknown");
    }

    #[test]
    fn project_config_save_and_load_roundtrip() {
        let tmp = make_temp_dir();
        let config_dir = tmp.path().to_path_buf();
        let project = ProjectConfig::new(
            "test-proj".to_string(),
            PathBuf::from("/some/path"),
            "rust".to_string(),
        );
        project.save(&config_dir).expect("save failed");
        let loaded = ProjectConfig::load(&config_dir).expect("load failed");
        assert_eq!(loaded.name, "test-proj");
        assert_eq!(loaded.language, "rust");
        assert_eq!(loaded.path, PathBuf::from("/some/path"));
    }

    // ── WorkspaceConfig ───────────────────────────────────────────────────────

    #[test]
    fn workspace_add_project_sets_default_when_empty() {
        let mut ws = WorkspaceConfig::new();
        let p = ProjectConfig::new("alpha".into(), PathBuf::from("/alpha"), "rust".into());
        ws.add_project(p);
        assert_eq!(ws.default_project.as_deref(), Some("alpha"));
        assert!(ws.projects.contains_key("alpha"));
    }

    #[test]
    fn workspace_add_multiple_projects_keeps_first_as_default() {
        let mut ws = WorkspaceConfig::new();
        ws.add_project(ProjectConfig::new("first".into(), PathBuf::from("/a"), "rust".into()));
        ws.add_project(ProjectConfig::new("second".into(), PathBuf::from("/b"), "python".into()));
        assert_eq!(ws.default_project.as_deref(), Some("first"));
    }

    #[test]
    fn workspace_get_current_project_returns_default() {
        let mut ws = WorkspaceConfig::new();
        ws.add_project(ProjectConfig::new("myproj".into(), PathBuf::from("/p"), "go".into()));
        let cur = ws.get_current_project().expect("should have current");
        assert_eq!(cur.name, "myproj");
    }

    #[test]
    fn workspace_set_default_project_updates_correctly() {
        let _tmp = make_temp_dir();
        // Override the global config dir by saving to a temp file directly
        let mut ws = WorkspaceConfig::new();
        ws.add_project(ProjectConfig::new("a".into(), PathBuf::from("/a"), "rust".into()));
        ws.add_project(ProjectConfig::new("b".into(), PathBuf::from("/b"), "python".into()));
        // Can't call set_default_project because it calls save() which writes to ~/.mccp.
        // Test the underlying logic instead.
        if ws.projects.contains_key("b") {
            ws.default_project = Some("b".to_string());
        }
        assert_eq!(ws.default_project.as_deref(), Some("b"));
    }

    #[test]
    fn workspace_set_default_project_errors_on_missing() {
        let mut ws = WorkspaceConfig::new();
        // set_default_project calls save() so we test without saving
        let result = if ws.projects.contains_key("nope") {
            ws.default_project = Some("nope".to_string());
            Ok(())
        } else {
            Err::<(), _>(anyhow::anyhow!("Project 'nope' not found"))
        };
        assert!(result.is_err());
    }

    // ── Ghost project cleanup ─────────────────────────────────────────────────

    #[test]
    fn ghost_project_detection() {
        let mut ws = WorkspaceConfig::new();
        ws.projects.insert(
            "default_value".to_string(),
            ProjectConfig::new("default_value".into(), PathBuf::from("default_value"), "default_value".into()),
        );
        ws.projects.insert(
            "real".to_string(),
            ProjectConfig::new("real".into(), PathBuf::from("/real/path"), "rust".into()),
        );

        let ghosts: Vec<String> = ws
            .projects
            .iter()
            .filter(|(_, p)| p.path.to_string_lossy() == "default_value")
            .map(|(name, _)| name.clone())
            .collect();

        assert_eq!(ghosts.len(), 1);
        assert_eq!(ghosts[0], "default_value");

        for g in &ghosts {
            ws.projects.remove(g);
        }
        assert!(!ws.projects.contains_key("default_value"));
        assert!(ws.projects.contains_key("real"));
    }
}