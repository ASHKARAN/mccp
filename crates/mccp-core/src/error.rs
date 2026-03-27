use thiserror::Error;

/// Result type for mccp operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for mccp
#[derive(Error, Debug)]
pub enum Error {
    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("File read error: {path} - {error}")]
    FileReadError {
        path: String,
        error: String,
    },

    #[error("Unsupported file: {0}")]
    UnsupportedFile(String),

    #[error("Project not found: {0}")]
    ProjectNotFound(String),

    #[error("Project already exists: {0}")]
    ProjectAlreadyExists(String),

    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Index error: {0}")]
    IndexError(String),

    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Docker error: {0}")]
    DockerError(String),

    #[error("Daemon error: {0}")]
    DaemonError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    TomlError(#[from] toml::de::Error),

    #[error("Regex error: {0}")]
    RegexError(#[from] regex::Error),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Timeout error: {0}")]
    TimeoutError(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

impl Error {
    /// Check if the error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            Error::FileNotFound(_) => true,
            Error::FileReadError { .. } => true,
            Error::UnsupportedFile(_) => true,
            Error::ProjectNotFound(_) => true,
            Error::SymbolNotFound(_) => true,
            Error::ConfigError(_) => false,
            Error::ProviderError(_) => true,
            Error::StorageError(_) => true,
            Error::IndexError(_) => true,
            Error::QueryError(_) => true,
            Error::DockerError(_) => true,
            Error::DaemonError(_) => true,
            Error::IoError(_) => true,
            Error::JsonError(_) => true,
            Error::TomlError(_) => true,
            Error::RegexError(_) => true,
            Error::HttpError(_) => true,
            Error::ParseError(_) => true,
            Error::ValidationError(_) => false,
            Error::PermissionDenied(_) => false,
            Error::NetworkError(_) => true,
            Error::TimeoutError(_) => true,
            Error::NotImplemented(_) => true,
            Error::ProjectAlreadyExists(_) => false,
        }
    }

    /// Get the error code for CLI output
    pub fn error_code(&self) -> i32 {
        match self {
            Error::FileNotFound(_) => 1,
            Error::FileReadError { .. } => 2,
            Error::UnsupportedFile(_) => 3,
            Error::ProjectNotFound(_) => 4,
            Error::ProjectAlreadyExists(_) => 5,
            Error::SymbolNotFound(_) => 6,
            Error::ConfigError(_) => 7,
            Error::ProviderError(_) => 8,
            Error::StorageError(_) => 9,
            Error::IndexError(_) => 10,
            Error::QueryError(_) => 11,
            Error::DockerError(_) => 12,
            Error::DaemonError(_) => 13,
            Error::IoError(_) => 14,
            Error::JsonError(_) => 15,
            Error::TomlError(_) => 16,
            Error::RegexError(_) => 17,
            Error::HttpError(_) => 18,
            Error::ParseError(_) => 19,
            Error::ValidationError(_) => 20,
            Error::PermissionDenied(_) => 21,
            Error::NetworkError(_) => 22,
            Error::TimeoutError(_) => 23,
            Error::NotImplemented(_) => 24,
        }
    }
}

/// Error context for better error reporting
#[derive(Debug)]
pub struct ErrorContext {
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub function: Option<String>,
    pub module: Option<String>,
}

impl ErrorContext {
    /// Create a new error context
    pub fn new() -> Self {
        Self {
            file: None,
            line: None,
            column: None,
            function: None,
            module: None,
        }
    }

    /// Set the file path
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Set the line number
    pub fn with_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }

    /// Set the column number
    pub fn with_column(mut self, column: usize) -> Self {
        self.column = Some(column);
        self
    }

    /// Set the function name
    pub fn with_function(mut self, function: impl Into<String>) -> Self {
        self.function = Some(function.into());
        self
    }

    /// Set the module name
    pub fn with_module(mut self, module: impl Into<String>) -> Self {
        self.module = Some(module.into());
        self
    }
}

/// Error wrapper that includes context
#[derive(Debug)]
pub struct ContextualError {
    pub error: Error,
    pub context: ErrorContext,
}

impl std::fmt::Display for ContextualError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)?;
        
        if let Some(file) = &self.context.file {
            write!(f, "\n  File: {}", file)?;
        }
        
        if let Some(line) = &self.context.line {
            write!(f, "\n  Line: {}", line)?;
        }
        
        if let Some(column) = &self.context.column {
            write!(f, "\n  Column: {}", column)?;
        }
        
        if let Some(function) = &self.context.function {
            write!(f, "\n  Function: {}", function)?;
        }
        
        if let Some(module) = &self.context.module {
            write!(f, "\n  Module: {}", module)?;
        }
        
        Ok(())
    }
}

impl std::error::Error for ContextualError {}

/// Macro for creating contextual errors
#[macro_export]
macro_rules! contextual_error {
    ($error:expr) => {
        $crate::error::ContextualError {
            error: $error,
            context: $crate::error::ErrorContext::new(),
        }
    };
    
    ($error:expr, file: $file:expr) => {
        $crate::error::ContextualError {
            error: $error,
            context: $crate::error::ErrorContext::new().with_file($file),
        }
    };
    
    ($error:expr, line: $line:expr) => {
        $crate::error::ContextualError {
            error: $error,
            context: $crate::error::ErrorContext::new().with_line($line),
        }
    };
    
    ($error:expr, function: $function:expr) => {
        $crate::error::ContextualError {
            error: $error,
            context: $crate::error::ErrorContext::new().with_function($function),
        }
    };
    
    ($error:expr, file: $file:expr, line: $line:expr) => {
        $crate::error::ContextualError {
            error: $error,
            context: $crate::error::ErrorContext::new()
                .with_file($file)
                .with_line($line),
        }
    };
    
    ($error:expr, file: $file:expr, line: $line:expr, function: $function:expr) => {
        $crate::error::ContextualError {
            error: $error,
            context: $crate::error::ErrorContext::new()
                .with_file($file)
                .with_line($line)
                .with_function($function),
        }
    };
}

/// Result type alias for contextual errors
pub type ContextualResult<T> = std::result::Result<T, ContextualError>;

/// Error reporter for consistent error output
pub struct ErrorReporter;

impl ErrorReporter {
    /// Report an error to stderr
    pub fn report_error(error: &Error) {
        eprintln!("Error: {}", error);
    }

    /// Report a contextual error to stderr
    pub fn report_contextual_error(error: &ContextualError) {
        eprintln!("Error: {}", error);
    }

    /// Report an error with suggestions
    pub fn report_error_with_suggestions(error: &Error, suggestions: &[&str]) {
        Self::report_error(error);
        eprintln!("\nSuggestions:");
        for suggestion in suggestions {
            eprintln!("  - {}", suggestion);
        }
    }

    /// Report an error and exit
    pub fn report_and_exit(error: &Error) -> ! {
        Self::report_error(error);
        std::process::exit(error.error_code());
    }

    /// Report a contextual error and exit
    pub fn report_contextual_and_exit(error: &ContextualError) -> ! {
        Self::report_contextual_error(error);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_is_recoverable() {
        assert!(Error::FileNotFound("test".to_string()).is_recoverable());
        assert!(Error::FileReadError { path: "test".to_string(), error: "test".to_string() }.is_recoverable());
        assert!(Error::UnsupportedFile("test".to_string()).is_recoverable());
        assert!(Error::ProjectNotFound("test".to_string()).is_recoverable());
        assert!(Error::SymbolNotFound("test".to_string()).is_recoverable());
        assert!(!Error::ConfigError("test".to_string()).is_recoverable());
        assert!(!Error::ValidationError("test".to_string()).is_recoverable());
        assert!(!Error::PermissionDenied("test".to_string()).is_recoverable());
    }

    #[test]
    fn test_error_code() {
        assert_eq!(Error::FileNotFound("test".to_string()).error_code(), 1);
        assert_eq!(Error::FileReadError { path: "test".to_string(), error: "test".to_string() }.error_code(), 2);
        assert_eq!(Error::UnsupportedFile("test".to_string()).error_code(), 3);
        assert_eq!(Error::ProjectNotFound("test".to_string()).error_code(), 4);
        assert_eq!(Error::ProjectAlreadyExists("test".to_string()).error_code(), 5);
        assert_eq!(Error::SymbolNotFound("test".to_string()).error_code(), 6);
        assert_eq!(Error::ConfigError("test".to_string()).error_code(), 7);
        assert_eq!(Error::ProviderError("test".to_string()).error_code(), 8);
        assert_eq!(Error::StorageError("test".to_string()).error_code(), 9);
        assert_eq!(Error::IndexError("test".to_string()).error_code(), 10);
        assert_eq!(Error::QueryError("test".to_string()).error_code(), 11);
        assert_eq!(Error::DockerError("test".to_string()).error_code(), 12);
        assert_eq!(Error::DaemonError("test".to_string()).error_code(), 13);
        assert_eq!(Error::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "test")).error_code(), 14);
        assert_eq!(Error::JsonError(serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err()).error_code(), 15);
        assert_eq!(Error::TomlError(toml::from_str::<toml::Value>("invalid = [").unwrap_err()).error_code(), 16);
        assert_eq!(Error::RegexError(regex::Error::Syntax("test".to_string())).error_code(), 17);
        // HttpError code 18 — skip reqwest::Error construction (not constructible in unit tests)
        assert_eq!(Error::ParseError("test".to_string()).error_code(), 19);
        assert_eq!(Error::ValidationError("test".to_string()).error_code(), 20);
        assert_eq!(Error::PermissionDenied("test".to_string()).error_code(), 21);
        assert_eq!(Error::NetworkError("test".to_string()).error_code(), 22);
        assert_eq!(Error::TimeoutError("test".to_string()).error_code(), 23);
        assert_eq!(Error::NotImplemented("test".to_string()).error_code(), 24);
    }

    #[test]
    fn test_error_context() {
        let context = ErrorContext::new()
            .with_file("test.rs")
            .with_line(42)
            .with_column(10)
            .with_function("main")
            .with_module("test_module");

        assert_eq!(context.file, Some("test.rs".to_string()));
        assert_eq!(context.line, Some(42));
        assert_eq!(context.column, Some(10));
        assert_eq!(context.function, Some("main".to_string()));
        assert_eq!(context.module, Some("test_module".to_string()));
    }

    #[test]
    fn test_contextual_error_display() {
        let error = Error::FileNotFound("test.rs".to_string());
        let context = ErrorContext::new()
            .with_file("test.rs")
            .with_line(42)
            .with_function("main");
        
        let contextual_error = ContextualError { error, context };
        let display = format!("{}", contextual_error);
        
        assert!(display.contains("File not found: test.rs"));
        assert!(display.contains("File: test.rs"));
        assert!(display.contains("Line: 42"));
        assert!(display.contains("Function: main"));
    }
}