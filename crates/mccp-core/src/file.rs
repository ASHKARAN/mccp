use super::*;
use std::path::PathBuf;
use std::fs;

/// File metadata and content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: PathBuf,
    pub language: Language,
    pub content: String,
    pub hash: String,
    pub size: usize,
    pub modified: chrono::DateTime<chrono::Utc>,
}

impl SourceFile {
    /// Create a new source file from path
    pub fn from_path<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .map_err(|e| Error::FileReadError(path.to_string_lossy().to_string(), e.to_string()))?;
        
        let language = Language::from_extension(
            path.extension()
                .ok_or_else(|| Error::UnsupportedFile(path.to_string_lossy().to_string()))?
                .to_str()
                .ok_or_else(|| Error::UnsupportedFile(path.to_string_lossy().to_string()))?
        ).ok_or_else(|| Error::UnsupportedFile(path.to_string_lossy().to_string()))?;
        
        let hash = format!("{:x}", sha2::Sha256::digest(content.as_bytes()));
        let size = content.len();
        let modified = fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|m| chrono::DateTime::from(m))
            .unwrap_or_else(|_| chrono::Utc::now());
        
        Ok(Self {
            path: path.to_path_buf(),
            language,
            content,
            hash,
            size,
            modified,
        })
    }

    /// Get the relative path from a project root
    pub fn relative_path(&self, project_root: &PathBuf) -> Option<String> {
        self.path.strip_prefix(project_root)
            .ok()
            .map(|p| p.to_string_lossy().to_string())
    }

    /// Check if the file has changed since last indexing
    pub fn has_changed(&self, last_hash: &str) -> bool {
        self.hash != last_hash
    }

    /// Get lines of code
    pub fn lines(&self) -> Vec<String> {
        self.content.lines().map(|s| s.to_string()).collect()
    }

    /// Get line count
    pub fn line_count(&self) -> usize {
        self.lines().len()
    }

    /// Get tokens count (approximate)
    pub fn token_count(&self) -> usize {
        // Simple tokenization - split on whitespace and common delimiters
        self.content.split_whitespace().count()
    }
}

/// File change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileChangeEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Moved { from: PathBuf, to: PathBuf },
}

/// File watcher for monitoring changes
pub struct FileWatcher {
    _watcher: notify::RecommendedWatcher,
    events: tokio::sync::mpsc::UnboundedReceiver<FileChangeEvent>,
}

impl FileWatcher {
    /// Start watching a directory
    pub fn start<P: AsRef<std::path::Path>>(
        path: P,
        debounce_ms: u64,
    ) -> Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        
        let mut watcher = notify::recommended_watcher(move |res| {
            match res {
                Ok(event) => {
                    let change = match event.kind {
                        notify::EventKind::Create(_) => {
                            FileChangeEvent::Created(event.paths[0].clone())
                        }
                        notify::EventKind::Modify(_) => {
                            FileChangeEvent::Modified(event.paths[0].clone())
                        }
                        notify::EventKind::Remove(_) => {
                            FileChangeEvent::Deleted(event.paths[0].clone())
                        }
                        notify::EventKind::Any => {
                            // Handle move events
                            if event.paths.len() == 2 {
                                FileChangeEvent::Moved {
                                    from: event.paths[0].clone(),
                                    to: event.paths[1].clone(),
                                }
                            } else {
                                return;
                            }
                        }
                        _ => return,
                    };
                    
                    let _ = tx.send(change);
                }
                Err(e) => {
                    eprintln!("Watch error: {:?}", e);
                }
            }
        })?;
        
        watcher.watch(path.as_ref(), notify::RecursiveMode::Recursive)?;
        
        Ok(Self {
            _watcher: watcher,
            events: rx,
        })
    }

    /// Get the next file change event
    pub async fn next_event(&mut self) -> Option<FileChangeEvent> {
        self.events.recv().await
    }

    /// Get all pending events
    pub fn drain_events(&mut self) -> Vec<FileChangeEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            events.push(event);
        }
        events
    }
}

/// Secrets scrubber for removing sensitive information
pub struct SecretsScrubber;

impl SecretsScrubber {
    /// Scrub secrets from content
    pub fn scrub(content: &str) -> String {
        let patterns = [
            // API keys
            (r"API[_-]?KEY[_-]?=.*?['\"]([A-Za-z0-9]{20,})['\"]", "[REDACTED_API_KEY]"),
            (r"SECRET[_-]?=.*?['\"]([A-Za-z0-9]{20,})['\"]", "[REDACTED_SECRET]"),
            (r"TOKEN[_-]?=.*?['\"]([A-Za-z0-9]{20,})['\"]", "[REDACTED_TOKEN]"),
            
            // AWS keys
            (r"AKIA[0-9A-Z]{16}", "[REDACTED_AWS_KEY]"),
            
            // GitHub tokens
            (r"ghp_[A-Za-z0-9]{36}", "[REDACTED_GITHUB_TOKEN]"),
            (r"gho_[A-Za-z0-9]{36}", "[REDACTED_GITHUB_TOKEN]"),
            (r"ghu_[A-Za-z0-9]{36}", "[REDACTED_GITHUB_TOKEN]"),
            (r"ghs_[A-Za-z0-9]{36}", "[REDACTED_GITHUB_TOKEN]"),
            (r"ghr_[A-Za-z0-9]{36}", "[REDACTED_GITHUB_TOKEN]"),
            
            // Database URLs
            (r"postgres://[^\\s]+", "[REDACTED_DB_URL]"),
            (r"mysql://[^\\s]+", "[REDACTED_DB_URL]"),
            (r"mongodb://[^\\s]+", "[REDACTED_DB_URL]"),
            
            // JWT tokens
            (r"[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}", "[REDACTED_JWT]"),
        ];
        
        let mut result = content.to_string();
        
        for (pattern, replacement) in &patterns {
            let re = regex::Regex::new(pattern).unwrap();
            result = re.replace_all(&result, *replacement).to_string();
        }
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_secrets_scrubber() {
        let content = r#"
            const API_KEY = "sk-prod-abc123xyz";
            const SECRET = "supersecret123";
            const TOKEN = "ghp_abcdefghijklmnopqrstuvwxyz123456";
            const DB_URL = "postgres://user:pass@localhost:5432/db";
        "#;
        
        let scrubbed = SecretsScrubber::scrub(content);
        
        assert!(!scrubbed.contains("sk-prod-abc123xyz"));
        assert!(!scrubbed.contains("supersecret123"));
        assert!(!scrubbed.contains("ghp_abcdefghijklmnopqrstuvwxyz123456"));
        assert!(!scrubbed.contains("postgres://user:pass@localhost:5432/db"));
        
        assert!(scrubbed.contains("[REDACTED_API_KEY]"));
        assert!(scrubbed.contains("[REDACTED_SECRET]"));
        assert!(scrubbed.contains("[REDACTED_GITHUB_TOKEN]"));
        assert!(scrubbed.contains("[REDACTED_DB_URL]"));
    }

    #[test]
    fn test_source_file_from_path() {
        let temp_file = NamedTempFile::new().unwrap();
        let content = "fn main() { println!(\"hello\"); }";
        std::fs::write(temp_file.path(), content).unwrap();
        
        let file = SourceFile::from_path(temp_file.path()).unwrap();
        
        assert_eq!(file.language, Language::Rust);
        assert_eq!(file.content, content);
        assert_eq!(file.size, content.len());
    }
}