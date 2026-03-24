use super::*;
use mccp_core::*;
use std::path::PathBuf;
use std::fs;
use colored::*;

/// Utility functions for CLI operations
pub mod file_utils {
    use super::*;
    
    /// Check if a path is a valid project directory
    pub fn is_valid_project_path(path: &PathBuf) -> bool {
        path.exists() && path.is_dir()
    }
    
    /// Get all source files in a directory
    pub fn get_source_files(path: &PathBuf, language: Option<Language>) -> Vec<PathBuf> {
        let mut files = Vec::new();
        
        if !is_valid_project_path(path) {
            return files;
        }
        
        // Common source file extensions
        let extensions = match language {
            Some(Language::Rust) => vec!["rs"],
            Some(Language::TypeScript) => vec!["ts", "tsx"],
            Some(Language::JavaScript) => vec!["js", "jsx"],
            Some(Language::Python) => vec!["py"],
            Some(Language::Go) => vec!["go"],
            Some(Language::Java) => vec!["java"],
            Some(Language::C) => vec!["c", "h"],
            Some(Language::Cpp) => vec!["cpp", "cxx", "cc", "hpp", "hxx", "h++"],
            Some(Language::CSharp) => vec!["cs"],
            Some(Language::Ruby) => vec!["rb"],
            Some(Language::PHP) => vec!["php"],
            Some(Language::Swift) => vec!["swift"],
            Some(Language::Kotlin) => vec!["kt", "kts"],
            Some(Language::Rust) => vec!["rs"],
            None => vec![
                "rs", "ts", "tsx", "js", "jsx", "py", "go", "java", 
                "c", "h", "cpp", "cxx", "cc", "hpp", "hxx", "h++", 
                "cs", "rb", "php", "swift", "kt", "kts"
            ],
        };
        
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let path = entry.path();
                
                if path.is_dir() {
                    // Skip common directories
                    if let Some(name) = path.file_name() {
                        let name = name.to_string_lossy().to_lowercase();
                        if name == "node_modules" || name == ".git" || name == "target" || name == "build" {
                            continue;
                        }
                    }
                    
                    // Recursively get files from subdirectories
                    files.extend(get_source_files(&path, language));
                } else if let Some(ext) = path.extension() {
                    if extensions.contains(&ext.to_string_lossy().as_ref()) {
                        files.push(path);
                    }
                }
            }
        }
        
        files
    }
    
    /// Get file size in human-readable format
    pub fn format_file_size(size: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        
        if size == 0 {
            return "0 B".to_string();
        }
        
        let mut size = size as f64;
        let mut unit_index = 0;
        
        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }
        
        format!("{:.1} {}", size, UNITS[unit_index])
    }
    
    /// Check if a file should be ignored based on .mccpignore patterns
    pub fn should_ignore_file(path: &PathBuf, ignore_patterns: &[String]) -> bool {
        let path_str = path.to_string_lossy().to_string();
        
        for pattern in ignore_patterns {
            if path_str.contains(pattern) {
                return true;
            }
        }
        
        false
    }
}

/// Utility functions for output formatting
pub mod output_utils {
    use super::*;
    
    /// Print a success message
    pub fn print_success(message: &str) {
        println!("{} {}", "✓".green().bold(), message.green());
    }
    
    /// Print an error message
    pub fn print_error(message: &str) {
        println!("{} {}", "✗".red().bold(), message.red());
    }
    
    /// Print a warning message
    pub fn print_warning(message: &str) {
        println!("{} {}", "⚠".yellow().bold(), message.yellow());
    }
    
    /// Print an info message
    pub fn print_info(message: &str) {
        println!("{} {}", "ℹ".blue().bold(), message.blue());
    }
    
    /// Print a section header
    pub fn print_header(title: &str) {
        println!("\n{}", title.bold().underline());
    }
    
    /// Print a table row
    pub fn print_table_row(columns: &[&str]) {
        let mut row = String::new();
        
        for (i, column) in columns.iter().enumerate() {
            if i > 0 {
                row.push_str("  ");
            }
            row.push_str(column);
        }
        
        println!("{}", row);
    }
    
    /// Print a progress bar
    pub fn print_progress(current: usize, total: usize, message: &str) {
        let percentage = if total == 0 { 100 } else { (current * 100) / total };
        let bar_width = 30;
        let filled = (percentage * bar_width) / 100;
        
        let mut bar = String::new();
        bar.push('[');
        for i in 0..bar_width {
            if i < filled {
                bar.push_str("█".green());
            } else {
                bar.push_str("░".dimmed());
            }
        }
        bar.push(']');
        
        print!("\r{} {}% {} {}", bar, percentage, message, if current == total { "\n" } else { "" });
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
    }
}

/// Utility functions for validation
pub mod validation_utils {
    use super::*;
    
    /// Validate a project name
    pub fn validate_project_name(name: &str) -> Result<(), String> {
        if name.is_empty() {
            return Err("Project name cannot be empty".to_string());
        }
        
        if name.len() > 100 {
            return Err("Project name cannot be longer than 100 characters".to_string());
        }
        
        if name.chars().any(|c| !c.is_alphanumeric() && c != '_' && c != '-') {
            return Err("Project name can only contain alphanumeric characters, underscores, and hyphens".to_string());
        }
        
        Ok(())
    }
    
    /// Validate a port number
    pub fn validate_port(port: u16) -> Result<(), String> {
        if port == 0 {
            return Err("Port cannot be 0".to_string());
        }
        
        if port > 65535 {
            return Err("Port must be between 1 and 65535".to_string());
        }
        
        Ok(())
    }
    
    /// Validate a host address
    pub fn validate_host(host: &str) -> Result<(), String> {
        if host.is_empty() {
            return Err("Host cannot be empty".to_string());
        }
        
        // Basic validation - could be enhanced with proper IP/domain validation
        if host.len() > 255 {
            return Err("Host cannot be longer than 255 characters".to_string());
        }
        
        Ok(())
    }
    
    /// Validate a path
    pub fn validate_path(path: &PathBuf) -> Result<(), String> {
        if path.as_os_str().is_empty() {
            return Err("Path cannot be empty".to_string());
        }
        
        Ok(())
    }
}

/// Utility functions for configuration
pub mod config_utils {
    use super::*;
    
    /// Get the default configuration path
    pub fn get_default_config_path() -> PathBuf {
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home_dir).join(".mccp").join("config.toml")
    }
    
    /// Get the default project configuration path
    pub fn get_default_project_config_path(project_path: &PathBuf) -> PathBuf {
        project_path.join(".mccp.toml")
    }
    
    /// Create default configuration directory
    pub fn create_config_dir() -> Result<(), String> {
        let config_dir = get_default_config_path().parent().unwrap();
        
        if !config_dir.exists() {
            fs::create_dir_all(config_dir)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }
        
        Ok(())
    }
    
    /// Load default configuration
    pub fn load_default_config() -> Result<CliConfig, String> {
        let config_path = get_default_config_path();
        
        if config_path.exists() {
            CliConfig::load(&config_path)
                .map_err(|e| format!("Failed to load config: {}", e))
        } else {
            Ok(CliConfig::default())
        }
    }
    
    /// Save default configuration
    pub fn save_default_config(config: &CliConfig) -> Result<(), String> {
        create_config_dir()?;
        
        let config_path = get_default_config_path();
        config.save(&config_path)
            .map_err(|e| format!("Failed to save config: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_valid_project_path() {
        let temp_dir = TempDir::new().unwrap();
        let valid_path = temp_dir.path().to_path_buf();
        let invalid_path = PathBuf::from("/nonexistent/path");
        
        assert!(file_utils::is_valid_project_path(&valid_path));
        assert!(!file_utils::is_valid_project_path(&invalid_path));
    }

    #[test]
    fn test_get_source_files() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path();
        
        // Create some test files
        fs::write(project_path.join("main.rs"), "fn main() {}").unwrap();
        fs::write(project_path.join("utils.js"), "function test() {}").unwrap();
        fs::write(project_path.join("README.md"), "# Test").unwrap();
        
        let rust_files = file_utils::get_source_files(project_path, Some(Language::Rust));
        assert_eq!(rust_files.len(), 1);
        assert!(rust_files[0].ends_with("main.rs"));
        
        let js_files = file_utils::get_source_files(project_path, Some(Language::JavaScript));
        assert_eq!(js_files.len(), 1);
        assert!(js_files[0].ends_with("utils.js"));
        
        let all_files = file_utils::get_source_files(project_path, None);
        assert_eq!(all_files.len(), 2);
    }

    #[test]
    fn test_format_file_size() {
        assert_eq!(file_utils::format_file_size(0), "0 B");
        assert_eq!(file_utils::format_file_size(1024), "1.0 KB");
        assert_eq!(file_utils::format_file_size(1048576), "1.0 MB");
        assert_eq!(file_utils::format_file_size(1073741824), "1.0 GB");
    }

    #[test]
    fn test_validate_project_name() {
        assert!(validation_utils::validate_project_name("test").is_ok());
        assert!(validation_utils::validate_project_name("test-project").is_ok());
        assert!(validation_utils::validate_project_name("test_project").is_ok());
        
        assert!(validation_utils::validate_project_name("").is_err());
        assert!(validation_utils::validate_project_name(&"a".repeat(101)).is_err());
        assert!(validation_utils::validate_project_name("test project").is_err());
    }

    #[test]
    fn test_validate_port() {
        assert!(validation_utils::validate_port(8080).is_ok());
        assert!(validation_utils::validate_port(3000).is_ok());
        
        assert!(validation_utils::validate_port(0).is_err());
        assert!(validation_utils::validate_port(65536).is_err());
    }

    #[test]
    fn test_validate_host() {
        assert!(validation_utils::validate_host("localhost").is_ok());
        assert!(validation_utils::validate_host("127.0.0.1").is_ok());
        assert!(validation_utils::validate_host("0.0.0.0").is_ok());
        
        assert!(validation_utils::validate_host("").is_err());
        assert!(validation_utils::validate_host(&"a".repeat(256)).is_err());
    }

    #[test]
    fn test_config_paths() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path();
        
        let config_path = config_utils::get_default_project_config_path(project_path);
        assert!(config_path.ends_with(".mccp.toml"));
        
        let default_config_path = config_utils::get_default_config_path();
        assert!(default_config_path.ends_with("config.toml"));
    }
}