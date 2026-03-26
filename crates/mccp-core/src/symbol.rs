use super::*;

/// A symbol in the codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub qualified_name: String,
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub context_snippet: String,
    pub containing_scope: String,
    pub language: Language,
    pub ref_kind: RefKind,
}

impl Symbol {
    /// Create a new symbol
    pub fn new(
        name: String,
        kind: SymbolKind,
        qualified_name: String,
        file_path: String,
        line: usize,
        column: usize,
        context_snippet: String,
        containing_scope: String,
        language: Language,
        ref_kind: RefKind,
    ) -> Self {
        Self {
            name,
            kind,
            qualified_name,
            file_path,
            line,
            column,
            context_snippet,
            containing_scope,
            language,
            ref_kind,
        }
    }

    /// Get the symbol's full signature
    pub fn signature(&self) -> String {
        format!("{}::{} at {}:{}", self.containing_scope, self.name, self.file_path, self.line)
    }
}

/// Symbol extractor for parsing source files
pub struct SymbolExtractor;

impl SymbolExtractor {
    /// Extract symbols from a source file
    pub fn extract_symbols(file: &SourceFile) -> Vec<Symbol> {
        //todo how to we know file.language? We can use file extension or content-based detection
        match file.language {
            Language::Rust => Self::extract_rust_symbols(file),
            Language::TypeScript | Language::JavaScript => Self::extract_ts_js_symbols(file),
            Language::Python => Self::extract_python_symbols(file),
            Language::Java => Self::extract_java_symbols(file),
            Language::Go => Self::extract_go_symbols(file),
            Language::C | Language::Cpp => Self::extract_c_symbols(file),
            Language::CSharp => Self::extract_csharp_symbols(file),
            Language::Ruby => Self::extract_ruby_symbols(file),
            Language::PHP => Self::extract_php_symbols(file),
            Language::Kotlin => Self::extract_kotlin_symbols(file),
        }
    }

    /// Extract Rust symbols
    fn extract_rust_symbols(file: &SourceFile) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines = file.lines();
        
        for (line_num, line) in lines.iter().enumerate() {
            let line_num = line_num + 1;
            
            // Extract function definitions
            if let Some(func_match) = Self::extract_pattern(line, r"fn\s+(\w+)") {
                symbols.push(Symbol::new(
                    func_match.clone(),
                    SymbolKind::Function,
                    func_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract struct definitions
            if let Some(struct_match) = Self::extract_pattern(line, r"struct\s+(\w+)") {
                symbols.push(Symbol::new(
                    struct_match.clone(),
                    SymbolKind::Struct,
                    struct_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract trait definitions
            if let Some(trait_match) = Self::extract_pattern(line, r"trait\s+(\w+)") {
                symbols.push(Symbol::new(
                    trait_match.clone(),
                    SymbolKind::Trait,
                    trait_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract enum definitions
            if let Some(enum_match) = Self::extract_pattern(line, r"enum\s+(\w+)") {
                symbols.push(Symbol::new(
                    enum_match.clone(),
                    SymbolKind::Enum,
                    enum_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract const definitions
            if let Some(const_match) = Self::extract_pattern(line, r"const\s+(\w+)") {
                symbols.push(Symbol::new(
                    const_match.clone(),
                    SymbolKind::Const,
                    const_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
        }
        
        symbols
    }

    /// Extract TypeScript/JavaScript symbols
    fn extract_ts_js_symbols(file: &SourceFile) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines = file.lines();
        
        for (line_num, line) in lines.iter().enumerate() {
            let line_num = line_num + 1;
            
            // Extract function definitions
            if let Some(func_match) = Self::extract_pattern(line, r"function\s+(\w+)") {
                symbols.push(Symbol::new(
                    func_match.clone(),
                    SymbolKind::Function,
                    func_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract class definitions
            if let Some(class_match) = Self::extract_pattern(line, r"class\s+(\w+)") {
                symbols.push(Symbol::new(
                    class_match.clone(),
                    SymbolKind::Class,
                    class_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract interface definitions
            if let Some(interface_match) = Self::extract_pattern(line, r"interface\s+(\w+)") {
                symbols.push(Symbol::new(
                    interface_match.clone(),
                    SymbolKind::Interface,
                    interface_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract const/let/var declarations
            if let Some(var_match) = Self::extract_pattern(line, r"(?:const|let|var)\s+(\w+)") {
                symbols.push(Symbol::new(
                    var_match.clone(),
                    SymbolKind::Variable,
                    var_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
        }
        
        symbols
    }

    /// Extract Python symbols
    fn extract_python_symbols(file: &SourceFile) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines = file.lines();
        
        for (line_num, line) in lines.iter().enumerate() {
            let line_num = line_num + 1;
            
            // Extract function definitions
            if let Some(func_match) = Self::extract_pattern(line, r"def\s+(\w+)") {
                symbols.push(Symbol::new(
                    func_match.clone(),
                    SymbolKind::Function,
                    func_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract class definitions
            if let Some(class_match) = Self::extract_pattern(line, r"class\s+(\w+)") {
                symbols.push(Symbol::new(
                    class_match.clone(),
                    SymbolKind::Class,
                    class_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract variable assignments
            if let Some(var_match) = Self::extract_pattern(line, r"^(\w+)\s*=") {
                symbols.push(Symbol::new(
                    var_match.clone(),
                    SymbolKind::Variable,
                    var_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
        }
        
        symbols
    }

    /// Extract Java symbols
    fn extract_java_symbols(file: &SourceFile) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines = file.lines();
        
        for (line_num, line) in lines.iter().enumerate() {
            let line_num = line_num + 1;
            
            // Extract class definitions
            if let Some(class_match) = Self::extract_pattern(line, r"class\s+(\w+)") {
                symbols.push(Symbol::new(
                    class_match.clone(),
                    SymbolKind::Class,
                    class_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract interface definitions
            if let Some(interface_match) = Self::extract_pattern(line, r"interface\s+(\w+)") {
                symbols.push(Symbol::new(
                    interface_match.clone(),
                    SymbolKind::Interface,
                    interface_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract method definitions
            if let Some(method_match) = Self::extract_pattern(line, r"\w+\s+(\w+)\s*\(") {
                symbols.push(Symbol::new(
                    method_match.clone(),
                    SymbolKind::Method,
                    method_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "class".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
        }
        
        symbols
    }

    /// Extract Go symbols
    fn extract_go_symbols(file: &SourceFile) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines = file.lines();
        
        for (line_num, line) in lines.iter().enumerate() {
            let line_num = line_num + 1;
            
            // Extract function definitions
            if let Some(func_match) = Self::extract_pattern(line, r"func\s+(\w+)") {
                symbols.push(Symbol::new(
                    func_match.clone(),
                    SymbolKind::Function,
                    func_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract struct definitions
            if let Some(struct_match) = Self::extract_pattern(line, r"type\s+(\w+)\s+struct") {
                symbols.push(Symbol::new(
                    struct_match.clone(),
                    SymbolKind::Struct,
                    struct_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract interface definitions
            if let Some(interface_match) = Self::extract_pattern(line, r"type\s+(\w+)\s+interface") {
                symbols.push(Symbol::new(
                    interface_match.clone(),
                    SymbolKind::Interface,
                    interface_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
        }
        
        symbols
    }

    /// Extract C/C++ symbols
    fn extract_c_symbols(file: &SourceFile) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines = file.lines();
        
        for (line_num, line) in lines.iter().enumerate() {
            let line_num = line_num + 1;
            
            // Extract function definitions
            if let Some(func_match) = Self::extract_pattern(line, r"\w+\s+(\w+)\s*\(") {
                symbols.push(Symbol::new(
                    func_match.clone(),
                    SymbolKind::Function,
                    func_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract struct definitions
            if let Some(struct_match) = Self::extract_pattern(line, r"struct\s+(\w+)") {
                symbols.push(Symbol::new(
                    struct_match.clone(),
                    SymbolKind::Struct,
                    struct_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract typedef definitions
            if let Some(typedef_match) = Self::extract_pattern(line, r"typedef\s+\w+\s+(\w+)") {
                symbols.push(Symbol::new(
                    typedef_match.clone(),
                    SymbolKind::Type,
                    typedef_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
        }
        
        symbols
    }

    /// Extract C# symbols
    fn extract_csharp_symbols(file: &SourceFile) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines = file.lines();
        
        for (line_num, line) in lines.iter().enumerate() {
            let line_num = line_num + 1;
            
            // Extract class definitions
            if let Some(class_match) = Self::extract_pattern(line, r"class\s+(\w+)") {
                symbols.push(Symbol::new(
                    class_match.clone(),
                    SymbolKind::Class,
                    class_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract method definitions
            if let Some(method_match) = Self::extract_pattern(line, r"\w+\s+(\w+)\s*\(") {
                symbols.push(Symbol::new(
                    method_match.clone(),
                    SymbolKind::Method,
                    method_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "class".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract property definitions
            if let Some(prop_match) = Self::extract_pattern(line, r"\w+\s+(\w+)\s*\{") {
                symbols.push(Symbol::new(
                    prop_match.clone(),
                    SymbolKind::Variable,
                    prop_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "class".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
        }
        
        symbols
    }

    /// Extract Ruby symbols
    fn extract_ruby_symbols(file: &SourceFile) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines = file.lines();
        
        for (line_num, line) in lines.iter().enumerate() {
            let line_num = line_num + 1;
            
            // Extract class definitions
            if let Some(class_match) = Self::extract_pattern(line, r"class\s+(\w+)") {
                symbols.push(Symbol::new(
                    class_match.clone(),
                    SymbolKind::Class,
                    class_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract method definitions
            if let Some(method_match) = Self::extract_pattern(line, r"def\s+(\w+)") {
                symbols.push(Symbol::new(
                    method_match.clone(),
                    SymbolKind::Method,
                    method_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "class".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
        }
        
        symbols
    }

    /// Extract PHP symbols
    fn extract_php_symbols(file: &SourceFile) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines = file.lines();
        
        for (line_num, line) in lines.iter().enumerate() {
            let line_num = line_num + 1;
            
            // Extract class definitions
            if let Some(class_match) = Self::extract_pattern(line, r"class\s+(\w+)") {
                symbols.push(Symbol::new(
                    class_match.clone(),
                    SymbolKind::Class,
                    class_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract function definitions
            if let Some(func_match) = Self::extract_pattern(line, r"function\s+(\w+)") {
                symbols.push(Symbol::new(
                    func_match.clone(),
                    SymbolKind::Function,
                    func_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
        }
        
        symbols
    }

    /// Extract Kotlin symbols
    fn extract_kotlin_symbols(file: &SourceFile) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines = file.lines();
        
        for (line_num, line) in lines.iter().enumerate() {
            let line_num = line_num + 1;
            
            // Extract class definitions
            if let Some(class_match) = Self::extract_pattern(line, r"class\s+(\w+)") {
                symbols.push(Symbol::new(
                    class_match.clone(),
                    SymbolKind::Class,
                    class_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "module".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract function definitions
            if let Some(func_match) = Self::extract_pattern(line, r"fun\s+(\w+)") {
                symbols.push(Symbol::new(
                    func_match.clone(),
                    SymbolKind::Function,
                    func_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "class".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
            
            // Extract property definitions
            if let Some(prop_match) = Self::extract_pattern(line, r"(?:val|var)\s+(\w+)") {
                symbols.push(Symbol::new(
                    prop_match.clone(),
                    SymbolKind::Variable,
                    prop_match.clone(),
                    file.path.to_string_lossy().to_string(),
                    line_num,
                    0,
                    line.clone(),
                    "class".to_string(),
                    file.language,
                    RefKind::Definition,
                ));
            }
        }
        
        symbols
    }

    /// Extract pattern from line using regex
    fn extract_pattern(line: &str, pattern: &str) -> Option<String> {
        let re = regex::Regex::new(pattern).ok()?;
        re.captures(line)?.get(1).map(|m| m.as_str().to_string())
    }
}

/// Symbol store for managing symbols across projects
#[derive(Debug)]
pub struct SymbolStore {
    symbols: dashmap::DashMap<String, Vec<Symbol>>,
}

impl SymbolStore {
    /// Create a new symbol store
    pub fn new() -> Self {
        Self {
            symbols: dashmap::DashMap::new(),
        }
    }

    /// Add symbols for a file
    pub fn add_symbols(&self, file_path: &str, symbols: Vec<Symbol>) {
        self.symbols.insert(file_path.to_string(), symbols);
    }

    /// Get symbols for a file
    pub fn get_symbols(&self, file_path: &str) -> Option<Vec<Symbol>> {
        self.symbols.get(file_path).map(|s| s.clone())
    }

    /// Find symbols by name
    pub fn find_symbols(&self, name: &str) -> Vec<Symbol> {
        let mut results = Vec::new();
        for entry in self.symbols.iter() {
            for symbol in entry.value() {
                if symbol.name == name {
                    results.push(symbol.clone());
                }
            }
        }
        results
    }

    /// Find symbols by kind
    pub fn find_symbols_by_kind(&self, kind: SymbolKind) -> Vec<Symbol> {
        let mut results = Vec::new();
        for entry in self.symbols.iter() {
            for symbol in entry.value() {
                if symbol.kind == kind {
                    results.push(symbol.clone());
                }
            }
        }
        results
    }

    /// Find symbols by file pattern
    pub fn find_symbols_by_pattern(&self, pattern: &str) -> Vec<Symbol> {
        let glob = match glob::Pattern::new(pattern) {
            Ok(g) => g,
            Err(_) => return vec![],
        };
        let mut results = Vec::new();
        for entry in self.symbols.iter() {
            if glob.matches(entry.key()) {
                for symbol in entry.value() {
                    results.push(symbol.clone());
                }
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_symbol_creation() {
        let symbol = Symbol::new(
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
        );
        
        assert_eq!(symbol.name, "main");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.signature(), "module::main at src/main.rs:1");
    }

    #[test]
    fn test_symbol_extractor() {
        let temp_file = tempfile::Builder::new().suffix(".rs").tempfile().unwrap();
        let content = "fn main() {\n    println!(\"hello\");\n}\n\nstruct User {\n    name: String,\n}";
        std::fs::write(temp_file.path(), content).unwrap();
        
        let file = SourceFile::from_path(temp_file.path()).unwrap();
        let symbols = SymbolExtractor::extract_symbols(&file);
        
        assert!(!symbols.is_empty());
        
        let main_func = symbols.iter().find(|s| s.name == "main").unwrap();
        assert_eq!(main_func.kind, SymbolKind::Function);
        
        let user_struct = symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(user_struct.kind, SymbolKind::Struct);
    }
}