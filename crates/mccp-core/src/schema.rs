use crate::{Language, SymbolKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Top-level snapshot of codebase structure, serialized to
/// ~/.mccp/data/mccp/{project_id}/code_intel.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeIntelSnapshot {
    pub project_id: String,
    pub built_at: i64, // unix timestamp
    pub symbols: Vec<SymbolDef>,
    pub call_edges: Vec<CallEdge>,
    pub use_edges: Vec<UseEdge>,
    pub import_edges: Vec<ImportEdge>,
    /// Detected execution flows (API → Controller → Service → Repo → Model)
    #[serde(default)]
    pub flows: Vec<ExecutionFlow>,
    /// Detected framework information per file
    #[serde(default)]
    pub frameworks: Vec<FrameworkInfo>,
    /// High-level project structure
    #[serde(default)]
    pub structure: Option<ProjectStructure>,
    /// Detected codegen patterns (e.g. Lombok, derive macros)
    #[serde(default)]
    pub codegen_patterns: Vec<CodegenPattern>,
}

impl CodeIntelSnapshot {
    pub fn new(project_id: String) -> Self {
        Self {
            project_id,
            built_at: chrono::Utc::now().timestamp(),
            symbols: Vec::new(),
            call_edges: Vec::new(),
            use_edges: Vec::new(),
            import_edges: Vec::new(),
            flows: Vec::new(),
            frameworks: Vec::new(),
            structure: None,
            codegen_patterns: Vec::new(),
        }
    }

    /// Find a symbol by name
    pub fn find_symbol(&self, name: &str) -> Option<&SymbolDef> {
        self.symbols.iter().find(|s| s.name == name)
    }

    /// Find all symbols matching a name (may have duplicates in different files)
    pub fn find_symbols(&self, name: &str) -> Vec<&SymbolDef> {
        self.symbols.iter().filter(|s| s.name == name).collect()
    }

    /// Find symbol by id
    pub fn find_symbol_by_id(&self, id: &str) -> Option<&SymbolDef> {
        self.symbols.iter().find(|s| s.id == id)
    }

    /// Get all callers of a symbol
    pub fn callers_of(&self, symbol_id: &str) -> Vec<&str> {
        self.call_edges
            .iter()
            .filter(|e| e.callee == symbol_id)
            .map(|e| e.caller.as_str())
            .collect()
    }

    /// Get all callees of a symbol
    pub fn callees_of(&self, symbol_id: &str) -> Vec<&str> {
        self.call_edges
            .iter()
            .filter(|e| e.caller == symbol_id)
            .map(|e| e.callee.as_str())
            .collect()
    }

    /// Get all usages of a symbol
    pub fn usages_of(&self, symbol_id: &str) -> Vec<&SymbolRef> {
        self.symbols
            .iter()
            .find(|s| s.id == symbol_id)
            .map(|s| s.references.iter().collect())
            .unwrap_or_default()
    }

    /// Get symbols with zero references (unused)
    pub fn unused_symbols(&self) -> Vec<&SymbolDef> {
        self.symbols.iter().filter(|s| s.references.is_empty()).collect()
    }

    /// Persist snapshot to ~/.mccp/data/{project_id}/code_intel.json
    pub fn save(&self) -> anyhow::Result<std::path::PathBuf> {
        let base = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".mccp")
            .join("data")
            .join(&self.project_id);
        std::fs::create_dir_all(&base)?;
        let path = base.join("code_intel.json");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        tracing::info!("saved code_intel snapshot to {}", path.display());
        Ok(path)
    }

    /// Load snapshot from ~/.mccp/data/{project_id}/code_intel.json
    pub fn load(project_id: &str) -> anyhow::Result<Option<Self>> {
        let path = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".mccp")
            .join("data")
            .join(project_id)
            .join("code_intel.json");
        if !path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(&path)?;
        let snap: Self = serde_json::from_str(&json)?;
        tracing::info!("loaded code_intel snapshot from {}", path.display());
        Ok(Some(snap))
    }

    /// Incrementally update: re-analyze only changed files, keep the rest
    pub fn incremental_update(&mut self, changed_files: &[String], new_partial: CodeIntelSnapshot) {
        let changed_files: std::collections::HashSet<&str> = changed_files.iter()
            .map(String::as_str)
            .collect();

        // Build a lookup of which symbol ids belong to changed files
        let changed_symbol_ids: std::collections::HashSet<String> = self.symbols.iter()
            .filter(|s| changed_files.contains(s.file.as_str()))
            .map(|s| s.id.clone())
            .collect();

        // Remove old data for changed files
        self.symbols.retain(|s| !changed_files.contains(s.file.as_str()));
        self.call_edges.retain(|e| {
            !changed_symbol_ids.contains(&e.caller) && !changed_symbol_ids.contains(&e.callee)
        });
        self.import_edges.retain(|e| {
            !changed_files.contains(e.from_file.as_str()) && !changed_files.contains(e.to_file.as_str())
        });
        self.use_edges.retain(|e| {
            !changed_symbol_ids.contains(&e.user) && !changed_symbol_ids.contains(&e.used)
        });
        self.frameworks.retain(|f| !changed_files.contains(f.file.as_str()));
        self.codegen_patterns.retain(|p| !changed_files.contains(p.file.as_str()));
        self.flows.retain(|flow| !changed_files.contains(flow.entry_file.as_str()));

        // Merge in new data
        self.symbols.extend(new_partial.symbols);
        self.call_edges.extend(new_partial.call_edges);
        self.use_edges.extend(new_partial.use_edges);
        self.import_edges.extend(new_partial.import_edges);
        self.frameworks.extend(new_partial.frameworks);
        self.codegen_patterns.extend(new_partial.codegen_patterns);
        self.flows.extend(new_partial.flows);
        if new_partial.structure.is_some() {
            self.structure = new_partial.structure;
        }
        self.built_at = chrono::Utc::now().timestamp();
    }
}

/// A symbol definition extracted from source code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolDef {
    /// Stable id: "{file}::{name}::{kind}"
    pub id: String,
    pub name: String,
    pub kind: SymbolKind,
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_column: u32,
    pub end_column: u32,
    pub visibility: Visibility,
    pub doc_comment: Option<String>,
    pub references: Vec<SymbolRef>,
    pub in_cycle: bool,
    /// Annotations/decorators/attributes on this symbol
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    /// Qualified name (e.g. "MyClass.myMethod" or "crate::module::func")
    #[serde(default)]
    pub qualified_name: Option<String>,
    /// Parent symbol id (e.g. method's class, inner class's outer class)
    #[serde(default)]
    pub parent_symbol: Option<String>,
    /// Language this symbol was defined in
    #[serde(default)]
    pub language: Option<String>,
    /// Signature (parameters and return type for functions/methods)
    #[serde(default)]
    pub signature: Option<String>,
}

impl SymbolDef {
    pub fn new(name: String, kind: SymbolKind, file: String, start_line: u32, end_line: u32) -> Self {
        let id = format!("{}::{}::{:?}", file, name, kind);
        Self {
            id,
            name,
            kind,
            file,
            start_line,
            end_line,
            start_column: 0,
            end_column: 0,
            visibility: Visibility::Private,
            doc_comment: None,
            references: Vec::new(),
            in_cycle: false,
            annotations: Vec::new(),
            qualified_name: None,
            parent_symbol: None,
            language: None,
            signature: None,
        }
    }

    pub fn with_columns(mut self, start_col: u32, end_col: u32) -> Self {
        self.start_column = start_col;
        self.end_column = end_col;
        self
    }

    pub fn with_language(mut self, lang: &str) -> Self {
        self.language = Some(lang.to_string());
        self
    }

    pub fn with_qualified_name(mut self, qn: String) -> Self {
        self.qualified_name = Some(qn);
        self
    }

    pub fn with_parent(mut self, parent_id: String) -> Self {
        self.parent_symbol = Some(parent_id);
        self
    }

    pub fn with_signature(mut self, sig: String) -> Self {
        self.signature = Some(sig);
        self
    }
}

/// Visibility of a symbol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private,
    Crate,
    Super,
}

/// A reference to a symbol at a specific location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRef {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub context: String,
    /// What kind of reference this is
    #[serde(default)]
    pub ref_kind: Option<String>,
}

/// An annotation/decorator/attribute on a symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub name: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub line: u32,
}

/// Detected codegen pattern (Lombok, derive macros, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodegenPattern {
    pub file: String,
    pub line: u32,
    pub pattern_type: CodegenType,
    pub generated_members: Vec<String>,
    pub source_annotation: String,
}

/// Types of code generation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodegenType {
    /// Java Lombok (@Data, @Builder, @Getter, etc.)
    Lombok,
    /// Rust derive macros (#[derive(...)],  etc.)
    RustDerive,
    /// Python dataclass decorators
    PythonDataclass,
    /// TypeScript decorators
    TypeScriptDecorator,
    /// Kotlin data class
    KotlinDataClass,
    /// C# auto-properties, records
    CSharpRecord,
    /// Other codegen pattern
    Other(String),
}

/// An execution flow through the system (e.g. API call → Controller → Service → Repo)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionFlow {
    pub id: String,
    pub name: String,
    pub flow_type: FlowType,
    pub steps: Vec<FlowStep>,
    pub entry_file: String,
    pub entry_line: u32,
}

/// Type of execution flow
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowType {
    HttpEndpoint,
    CliCommand,
    WebSocket,
    EventHandler,
    ScheduledTask,
    MessageConsumer,
}

/// A single step in an execution flow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowStep {
    pub symbol_id: String,
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
    pub layer: ArchitecturalLayer,
    pub description: String,
    /// Annotations/decorators on this step
    #[serde(default)]
    pub annotations: Vec<String>,
}

/// Architectural layer a symbol belongs to
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchitecturalLayer {
    Controller,
    Service,
    Repository,
    Model,
    Middleware,
    Utility,
    Config,
    Router,
    Handler,
    Interface,
    Unknown,
}

impl ArchitecturalLayer {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Controller => "controller",
            Self::Service => "service",
            Self::Repository => "repository",
            Self::Model => "model",
            Self::Middleware => "middleware",
            Self::Utility => "utility",
            Self::Config => "config",
            Self::Router => "router",
            Self::Handler => "handler",
            Self::Interface => "interface",
            Self::Unknown => "unknown",
        }
    }
}

/// Detected framework information for a file or module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkInfo {
    pub framework: Framework,
    pub version: Option<String>,
    pub file: String,
    pub detected_patterns: Vec<String>,
}

/// Known frameworks
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Framework {
    // Java
    SpringBoot,
    SpringMVC,
    Quarkus,
    Micronaut,
    // JavaScript/TypeScript
    Express,
    NestJS,
    NextJS,
    Fastify,
    // Python
    Django,
    Flask,
    FastAPI,
    // Rust
    Actix,
    Axum,
    Rocket,
    // Go
    Gin,
    Echo,
    Fiber,
    // C#
    AspNetCore,
    // Ruby
    Rails,
    Sinatra,
    // PHP
    Laravel,
    Symfony,
    // Kotlin
    Ktor,
    /// Unknown framework with name hint
    Other(String),
}

impl Framework {
    pub fn as_str(&self) -> &str {
        match self {
            Self::SpringBoot => "Spring Boot",
            Self::SpringMVC => "Spring MVC",
            Self::Quarkus => "Quarkus",
            Self::Micronaut => "Micronaut",
            Self::Express => "Express",
            Self::NestJS => "NestJS",
            Self::NextJS => "Next.js",
            Self::Fastify => "Fastify",
            Self::Django => "Django",
            Self::Flask => "Flask",
            Self::FastAPI => "FastAPI",
            Self::Actix => "Actix",
            Self::Axum => "Axum",
            Self::Rocket => "Rocket",
            Self::Gin => "Gin",
            Self::Echo => "Echo",
            Self::Fiber => "Fiber",
            Self::AspNetCore => "ASP.NET Core",
            Self::Rails => "Rails",
            Self::Sinatra => "Sinatra",
            Self::Laravel => "Laravel",
            Self::Symfony => "Symfony",
            Self::Ktor => "Ktor",
            Self::Other(name) => name.as_str(),
        }
    }
}

/// High-level project structure with modules and their relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStructure {
    pub modules: Vec<StructureModule>,
    pub language_stats: HashMap<String, LanguageStats>,
}

/// A module/package in the project structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureModule {
    pub name: String,
    pub path: String,
    pub languages: Vec<String>,
    pub file_count: usize,
    pub symbol_count: usize,
    pub dependencies: Vec<String>,
    pub layer: ArchitecturalLayer,
}

/// Per-language statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageStats {
    pub file_count: usize,
    pub line_count: usize,
    pub symbol_count: usize,
    pub function_count: usize,
    pub class_count: usize,
}

/// A call relationship between two symbols
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEdge {
    pub caller: String,
    pub callee: String,
}

/// A variable usage relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseEdge {
    pub user: String,
    pub used: String,
    pub kind: UseKind,
}

/// Kind of variable usage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UseKind {
    Read,
    Write,
    Move,
    Borrow,
}

/// A file-level import relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportEdge {
    pub from_file: String,
    pub to_file: String,
    pub symbol: Option<String>,
}

/// Trait for pluggable code analyzers
#[async_trait::async_trait]
pub trait CodeAnalyzer: Send + Sync {
    fn name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    async fn install(&self) -> anyhow::Result<()>;
    async fn analyze(&self, project_root: &Path) -> anyhow::Result<CodeIntelSnapshot>;
    fn supports_language(&self, lang: Language) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_creation() {
        let snap = CodeIntelSnapshot::new("test-project".to_string());
        assert_eq!(snap.project_id, "test-project");
        assert!(snap.symbols.is_empty());
        assert!(snap.call_edges.is_empty());
    }

    #[test]
    fn test_symbol_def_creation() {
        let sym = SymbolDef::new(
            "main".to_string(),
            SymbolKind::Function,
            "src/main.rs".to_string(),
            1,
            10,
        );
        assert_eq!(sym.id, "src/main.rs::main::Function");
        assert_eq!(sym.name, "main");
        assert!(!sym.in_cycle);
    }

    #[test]
    fn test_find_symbol() {
        let mut snap = CodeIntelSnapshot::new("test".to_string());
        snap.symbols.push(SymbolDef::new(
            "foo".to_string(),
            SymbolKind::Function,
            "src/lib.rs".to_string(),
            1,
            5,
        ));
        snap.symbols.push(SymbolDef::new(
            "bar".to_string(),
            SymbolKind::Struct,
            "src/lib.rs".to_string(),
            10,
            20,
        ));

        assert!(snap.find_symbol("foo").is_some());
        assert!(snap.find_symbol("baz").is_none());
        assert_eq!(snap.find_symbols("foo").len(), 1);
    }

    #[test]
    fn test_callers_and_callees() {
        let mut snap = CodeIntelSnapshot::new("test".to_string());
        snap.call_edges.push(CallEdge {
            caller: "a".to_string(),
            callee: "b".to_string(),
        });
        snap.call_edges.push(CallEdge {
            caller: "c".to_string(),
            callee: "b".to_string(),
        });

        let callers = snap.callers_of("b");
        assert_eq!(callers.len(), 2);
        assert!(callers.contains(&"a"));
        assert!(callers.contains(&"c"));

        let callees = snap.callees_of("a");
        assert_eq!(callees, vec!["b"]);
    }

    #[test]
    fn test_unused_symbols() {
        let mut snap = CodeIntelSnapshot::new("test".to_string());
        let mut used_sym = SymbolDef::new(
            "used_fn".to_string(),
            SymbolKind::Function,
            "src/lib.rs".to_string(),
            1,
            5,
        );
        used_sym.references.push(SymbolRef {
            file: "src/main.rs".to_string(),
            line: 10,
            column: 0,
            end_line: 10,
            end_column: 0,
            context: "used_fn()".to_string(),
            ref_kind: None,
        });
        snap.symbols.push(used_sym);

        snap.symbols.push(SymbolDef::new(
            "unused_fn".to_string(),
            SymbolKind::Function,
            "src/lib.rs".to_string(),
            10,
            15,
        ));

        let unused = snap.unused_symbols();
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].name, "unused_fn");
    }
}
