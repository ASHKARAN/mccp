use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::Language;

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "build",
    "dist",
    "vendor",
    ".venv",
    "__pycache__",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub project: ProjectMeta,
    #[serde(default)]
    pub modules: Vec<ModuleDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDefinition {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ProjectManifest {
    pub fn load(project_root: &Path) -> anyhow::Result<Self> {
        let path = project_root.join(".mccp").join("project.toml");
        let content = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save(&self, project_root: &Path) -> anyhow::Result<()> {
        let dir = project_root.join(".mccp");
        std::fs::create_dir_all(&dir)?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(dir.join("project.toml"), content)?;
        Ok(())
    }

    /// Try to load from disk, or auto-detect and create a default
    pub fn load_or_detect(project_root: &Path, project_name: &str) -> Self {
        if let Ok(m) = Self::load(project_root) {
            return m;
        }
        let languages = detect_languages(project_root);
        let modules = detect_modules(project_root);
        ProjectManifest {
            project: ProjectMeta {
                name: project_name.to_string(),
                description: None,
                languages,
            },
            modules,
        }
    }
}

/// Walk the directory recursively, count files per extension, return all
/// detected languages sorted by file count descending.
pub fn detect_languages(root: &Path) -> Vec<String> {
    let mut counts: HashMap<Language, usize> = HashMap::new();
    walk_for_languages(root, &mut counts, 0);

    let mut langs: Vec<(Language, usize)> = counts.into_iter().collect();
    langs.sort_by(|a, b| b.1.cmp(&a.1));
    langs.into_iter().map(|(l, _)| l.to_string()).collect()
}

fn walk_for_languages(dir: &Path, counts: &mut HashMap<Language, usize>, depth: usize) {
    if depth > 20 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') && depth > 0 {
                continue;
            }
            if SKIP_DIRS.contains(&name) {
                continue;
            }
        }
        if path.is_dir() {
            walk_for_languages(&path, counts, depth + 1);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if let Some(lang) = Language::from_extension(ext) {
                *counts.entry(lang).or_insert(0) += 1;
            }
        }
    }
}

/// Look at immediate subdirectories for marker files and create module definitions.
pub fn detect_modules(root: &Path) -> Vec<ModuleDefinition> {
    let mut modules = Vec::new();
    modules.extend(parse_git_submodules(root));
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return modules,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }

        let markers: &[(&str, &[&str], &str)] = &[
            ("Cargo.toml", &["rust"], "Rust module"),
            ("package.json", &["javascript", "typescript"], "JavaScript/TypeScript module"),
            ("pom.xml", &["java"], "Java module"),
            ("build.gradle", &["java"], "Java module"),
            ("go.mod", &["go"], "Go module"),
            ("requirements.txt", &["python"], "Python module"),
            ("pyproject.toml", &["python"], "Python module"),
            ("setup.py", &["python"], "Python module"),
            ("Gemfile", &["ruby"], "Ruby module"),
            ("composer.json", &["php"], "PHP module"),
        ];

        for (marker, langs, purpose) in markers {
            if path.join(marker).exists() {
                if modules.iter().any(|m| m.path.trim_end_matches('/') == name) {
                    break;
                }
                modules.push(ModuleDefinition {
                    name: name.clone(),
                    path: format!("{}/", name),
                    languages: langs.iter().map(|s| s.to_string()).collect(),
                    purpose: Some(purpose.to_string()),
                    description: None,
                });
                break;
            }
        }
    }

    modules.sort_by(|a, b| a.name.cmp(&b.name));
    modules
}

fn parse_git_submodules(root: &Path) -> Vec<ModuleDefinition> {
    let gitmodules_path = root.join(".gitmodules");
    let Ok(content) = std::fs::read_to_string(gitmodules_path) else {
        return Vec::new();
    };

    let mut modules = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_path: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[submodule ") {
            if let (Some(name), Some(path)) = (current_name.take(), current_path.take()) {
                let languages = detect_languages(root.join(&path).as_path());
                modules.push(ModuleDefinition {
                    name,
                    path: if path.ends_with('/') { path } else { format!("{}/", path) },
                    languages,
                    purpose: Some("Git submodule".to_string()),
                    description: None,
                });
            }

            current_name = trimmed
                .split('"')
                .nth(1)
                .map(|s| s.to_string())
                .or_else(|| trimmed.strip_prefix("[submodule ").map(|s| s.trim_end_matches(']').trim_matches('"').to_string()));
            current_path = None;
        } else if let Some(value) = trimmed.strip_prefix("path =") {
            current_path = Some(value.trim().to_string());
        }
    }

    if let (Some(name), Some(path)) = (current_name, current_path) {
        let languages = detect_languages(root.join(&path).as_path());
        modules.push(ModuleDefinition {
            name,
            path: if path.ends_with('/') { path } else { format!("{}/", path) },
            languages,
            purpose: Some("Git submodule".to_string()),
            description: None,
        });
    }

    modules
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_temp_dir() -> TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    #[test]
    fn detect_languages_finds_multiple() {
        let tmp = make_temp_dir();
        let dir = tmp.path();
        std::fs::write(dir.join("main.rs"), "").unwrap();
        std::fs::write(dir.join("lib.rs"), "").unwrap();
        std::fs::write(dir.join("app.ts"), "").unwrap();
        let langs = detect_languages(dir);
        assert!(langs.contains(&"rust".to_string()));
        assert!(langs.contains(&"typescript".to_string()));
        // rust has more files, should be first
        assert_eq!(langs[0], "rust");
    }

    #[test]
    fn detect_languages_empty_dir() {
        let tmp = make_temp_dir();
        let langs = detect_languages(tmp.path());
        assert!(langs.is_empty());
    }

    #[test]
    fn detect_languages_recursive() {
        let tmp = make_temp_dir();
        let dir = tmp.path();
        let sub = dir.join("src");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("main.py"), "").unwrap();
        let langs = detect_languages(dir);
        assert_eq!(langs, vec!["python"]);
    }

    #[test]
    fn detect_modules_finds_cargo_and_package() {
        let tmp = make_temp_dir();
        let dir = tmp.path();
        let server = dir.join("server");
        let client = dir.join("client");
        std::fs::create_dir(&server).unwrap();
        std::fs::create_dir(&client).unwrap();
        std::fs::write(server.join("Cargo.toml"), "").unwrap();
        std::fs::write(client.join("package.json"), "{}").unwrap();
        let modules = detect_modules(dir);
        assert_eq!(modules.len(), 2);
        let names: Vec<&str> = modules.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"server"));
        assert!(names.contains(&"client"));
    }

    #[test]
    fn detect_modules_reads_gitmodules() {
        let tmp = make_temp_dir();
        let dir = tmp.path();
        let vendor = dir.join("vendor/libfoo");
        std::fs::create_dir_all(&vendor).unwrap();
        std::fs::write(vendor.join("package.json"), "{}").unwrap();
        std::fs::write(
            dir.join(".gitmodules"),
            "[submodule \"libfoo\"]\n\tpath = vendor/libfoo\n\turl = https://example.com/libfoo.git\n",
        ).unwrap();

        let modules = detect_modules(dir);
        assert!(modules.iter().any(|m| m.name == "libfoo" && m.path == "vendor/libfoo/"));
    }

    #[test]
    fn manifest_save_and_load_roundtrip() {
        let tmp = make_temp_dir();
        let manifest = ProjectManifest {
            project: ProjectMeta {
                name: "test".to_string(),
                description: Some("A test project".to_string()),
                languages: vec!["rust".to_string(), "typescript".to_string()],
            },
            modules: vec![ModuleDefinition {
                name: "api".to_string(),
                path: "api/".to_string(),
                languages: vec!["rust".to_string()],
                purpose: Some("Backend".to_string()),
                description: None,
            }],
        };
        manifest.save(tmp.path()).unwrap();
        let loaded = ProjectManifest::load(tmp.path()).unwrap();
        assert_eq!(loaded.project.name, "test");
        assert_eq!(loaded.project.languages.len(), 2);
        assert_eq!(loaded.modules.len(), 1);
        assert_eq!(loaded.modules[0].name, "api");
    }

    #[test]
    fn load_or_detect_without_manifest() {
        let tmp = make_temp_dir();
        let dir = tmp.path();
        std::fs::write(dir.join("main.rs"), "").unwrap();
        let manifest = ProjectManifest::load_or_detect(dir, "my-project");
        assert_eq!(manifest.project.name, "my-project");
        assert!(manifest.project.languages.contains(&"rust".to_string()));
    }
}
