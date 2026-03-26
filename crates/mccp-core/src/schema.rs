use crate::{Language, SymbolKind};
use serde::{Deserialize, Serialize};
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
        // Build a lookup of which symbol ids belong to changed files
        let changed_symbol_ids: std::collections::HashSet<String> = self.symbols.iter()
            .filter(|s| changed_files.contains(&s.file))
            .map(|s| s.id.clone())
            .collect();

        // Remove old data for changed files
        self.symbols.retain(|s| !changed_files.contains(&s.file));
        self.call_edges.retain(|e| !changed_symbol_ids.contains(&e.caller));
        self.import_edges.retain(|e| !changed_files.contains(&e.from_file));
        self.use_edges.retain(|e| !changed_symbol_ids.contains(&e.user));

        // Merge in new data
        self.symbols.extend(new_partial.symbols);
        self.call_edges.extend(new_partial.call_edges);
        self.use_edges.extend(new_partial.use_edges);
        self.import_edges.extend(new_partial.import_edges);
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
    pub visibility: Visibility,
    pub doc_comment: Option<String>,
    pub references: Vec<SymbolRef>,
    pub in_cycle: bool,
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
            visibility: Visibility::Private,
            doc_comment: None,
            references: Vec::new(),
            in_cycle: false,
        }
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
    pub context: String,
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
            context: "used_fn()".to_string(),
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
