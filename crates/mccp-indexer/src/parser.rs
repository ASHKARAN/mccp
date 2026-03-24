use super::*;
use mccp_core::*;

/// Parser for extracting symbols from source files
#[derive(Debug, Clone)]
pub struct Parser;

impl Parser {
    /// Create a new parser
    pub fn new() -> Self {
        Self
    }

    /// Parse a source file and extract symbols
    pub fn parse(&self, content: &str, language: Language) -> Result<Vec<Symbol>> {
        let source_file = SourceFile {
            path: PathBuf::from("temp"),
            language,
            content: content.to_string(),
            hash: format!("{:x}", sha2::Sha256::digest(content.as_bytes())),
            size: content.len(),
            modified: chrono::Utc::now(),
        };
        
        Ok(SymbolExtractor::extract_symbols(&source_file))
    }

    /// Parse multiple files
    pub fn parse_batch(&self, files: &[SourceFile]) -> Result<Vec<Symbol>> {
        let mut all_symbols = Vec::new();
        
        for file in files {
            let symbols = SymbolExtractor::extract_symbols(file);
            all_symbols.extend(symbols);
        }
        
        Ok(all_symbols)
    }

    /// Get parsing statistics
    pub fn stats(&self, symbols: &[Symbol]) -> ParseStats {
        let by_kind = symbols.iter()
            .fold(std::collections::HashMap::new(), |mut acc, symbol| {
                *acc.entry(symbol.kind).or_insert(0) += 1;
                acc
            });
        
        ParseStats {
            total_symbols: symbols.len(),
            by_kind,
        }
    }
}

/// Parsing statistics
#[derive(Debug, Clone)]
pub struct ParseStats {
    pub total_symbols: usize,
    pub by_kind: std::collections::HashMap<SymbolKind, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parser_creation() {
        let parser = Parser::new();
        
        assert!(parser.parse("fn main() {}", Language::Rust).is_ok());
    }

    #[test]
    fn test_parse_rust() {
        let parser = Parser::new();
        let content = "fn main() {\n    println!(\"hello\");\n}\n\nstruct User {\n    name: String,\n}";
        
        let symbols = parser.parse(content, Language::Rust).unwrap();
        
        assert!(!symbols.is_empty());
        
        let main_func = symbols.iter().find(|s| s.name == "main").unwrap();
        assert_eq!(main_func.kind, SymbolKind::Function);
        
        let user_struct = symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(user_struct.kind, SymbolKind::Struct);
    }

    #[test]
    fn test_parse_typescript() {
        let parser = Parser::new();
        let content = "function main() {\n    console.log('hello');\n}\n\nclass User {\n    name: string;\n}";
        
        let symbols = parser.parse(content, Language::TypeScript).unwrap();
        
        assert!(!symbols.is_empty());
        
        let main_func = symbols.iter().find(|s| s.name == "main").unwrap();
        assert_eq!(main_func.kind, SymbolKind::Function);
        
        let user_class = symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(user_class.kind, SymbolKind::Class);
    }

    #[test]
    fn test_parse_python() {
        let parser = Parser::new();
        let content = "def main():\n    print('hello')\n\nclass User:\n    def __init__(self):\n        self.name = ''";
        
        let symbols = parser.parse(content, Language::Python).unwrap();
        
        assert!(!symbols.is_empty());
        
        let main_func = symbols.iter().find(|s| s.name == "main").unwrap();
        assert_eq!(main_func.kind, SymbolKind::Function);
        
        let user_class = symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(user_class.kind, SymbolKind::Class);
    }

    #[test]
    fn test_parse_batch() {
        let parser = Parser::new();
        let files = vec![]; // Empty for testing
        
        let symbols = parser.parse_batch(&files).unwrap();
        
        assert_eq!(symbols.len(), 0);
    }

    #[test]
    fn test_parse_stats() {
        let parser = Parser::new();
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
                "User".to_string(),
                SymbolKind::Struct,
                "User".to_string(),
                "src/user.rs".to_string(),
                1,
                0,
                "struct User {}".to_string(),
                "module".to_string(),
                Language::Rust,
                RefKind::Definition,
            ),
        ];
        
        let stats = parser.stats(&symbols);
        
        assert_eq!(stats.total_symbols, 2);
        assert_eq!(stats.by_kind[&SymbolKind::Function], 1);
        assert_eq!(stats.by_kind[&SymbolKind::Struct], 1);
    }
}