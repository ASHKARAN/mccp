use mccp_core::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Language as TsLanguage, Parser as TsParser, Node};

/// Built-in tree-sitter code intelligence analyzer
pub struct TreeSitterAnalyzer;

impl TreeSitterAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Analyze a single file and extract symbols, calls, imports
    fn analyze_file(
        &self,
        file_path: &str,
        content: &str,
        lang: mccp_core::Language,
    ) -> Option<FileAnalysis> {
        let ts_lang = get_ts_language(lang)?;
        let mut parser = TsParser::new();
        parser.set_language(&ts_lang).ok()?;
        let tree = parser.parse(content, None)?;
        let root = tree.root_node();

        let mut analysis = FileAnalysis::default();
        self.extract_symbols(root, content, file_path, lang, &mut analysis);
        Some(analysis)
    }

    fn extract_symbols(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        lang: mccp_core::Language,
        analysis: &mut FileAnalysis,
    ) {
        let kind = node.kind();

        match lang {
            mccp_core::Language::Rust => self.extract_rust_symbols(node, source, file_path, analysis),
            mccp_core::Language::TypeScript | mccp_core::Language::JavaScript => {
                self.extract_js_symbols(node, source, file_path, analysis)
            }
            mccp_core::Language::Python => {
                self.extract_python_symbols(node, source, file_path, analysis)
            }
            _ => {
                // Walk children for unsupported languages
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        self.extract_symbols(child, source, file_path, lang, analysis);
                    }
                }
            }
        }
    }

    fn extract_rust_symbols(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        analysis: &mut FileAnalysis,
    ) {
        let kind = node.kind();

        match kind {
            "function_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let vis = self.rust_visibility(&node, source);
                    let doc = self.preceding_doc_comment(&node, source);
                    let mut sym = SymbolDef::new(
                        name.clone(),
                        SymbolKind::Function,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym.visibility = vis;
                    sym.doc_comment = doc;
                    let sym_id = sym.id.clone();
                    analysis.symbols.push(sym);

                    // Extract calls from function body
                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_rust_calls(body, source, file_path, &sym_id, analysis);
                    }
                }
            }
            "struct_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let vis = self.rust_visibility(&node, source);
                    let doc = self.preceding_doc_comment(&node, source);
                    let mut sym = SymbolDef::new(
                        name,
                        SymbolKind::Struct,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym.visibility = vis;
                    sym.doc_comment = doc;
                    analysis.symbols.push(sym);
                }
            }
            "enum_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let vis = self.rust_visibility(&node, source);
                    let mut sym = SymbolDef::new(
                        name,
                        SymbolKind::Enum,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym.visibility = vis;
                    analysis.symbols.push(sym);
                }
            }
            "trait_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let vis = self.rust_visibility(&node, source);
                    let mut sym = SymbolDef::new(
                        name,
                        SymbolKind::Trait,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym.visibility = vis;
                    analysis.symbols.push(sym);
                }
            }
            "impl_item" => {
                // Extract methods from impl blocks
                if let Some(body) = node.child_by_field_name("body") {
                    for i in 0..body.child_count() {
                        if let Some(child) = body.child(i) {
                            if child.kind() == "function_item" {
                                if let Some(name_node) = child.child_by_field_name("name") {
                                    let name = node_text(name_node, source);
                                    let vis = self.rust_visibility(&child, source);
                                    let doc = self.preceding_doc_comment(&child, source);
                                    let mut sym = SymbolDef::new(
                                        name,
                                        SymbolKind::Method,
                                        file_path.to_string(),
                                        child.start_position().row as u32 + 1,
                                        child.end_position().row as u32 + 1,
                                    );
                                    sym.visibility = vis;
                                    sym.doc_comment = doc;
                                    let sym_id = sym.id.clone();
                                    analysis.symbols.push(sym);

                                    if let Some(body) = child.child_by_field_name("body") {
                                        self.extract_rust_calls(
                                            body, source, file_path, &sym_id, analysis,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "use_declaration" => {
                // Extract import edges
                let import_text = node_text(node, source);
                // Parse "use crate::foo::Bar" → ImportEdge
                if let Some(path) = import_text.strip_prefix("use ") {
                    let path = path.trim_end_matches(';').trim();
                    analysis.import_edges.push(ImportEdge {
                        from_file: file_path.to_string(),
                        to_file: path.to_string(),
                        symbol: None,
                    });
                }
            }
            "mod_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let mut sym = SymbolDef::new(
                        name,
                        SymbolKind::Module,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym.visibility = self.rust_visibility(&node, source);
                    analysis.symbols.push(sym);
                }
            }
            _ => {}
        }

        // Recurse into children (except for bodies we already handled)
        if kind != "function_item" && kind != "impl_item" {
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    self.extract_rust_symbols(child, source, file_path, analysis);
                }
            }
        }
    }

    fn extract_rust_calls(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        caller_id: &str,
        analysis: &mut FileAnalysis,
    ) {
        if node.kind() == "call_expression" {
            if let Some(func_node) = node.child_by_field_name("function") {
                let callee_name = node_text(func_node, source);
                analysis.call_edges.push(CallEdge {
                    caller: caller_id.to_string(),
                    callee: callee_name,
                });
            }
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.extract_rust_calls(child, source, file_path, caller_id, analysis);
            }
        }
    }

    fn extract_js_symbols(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        analysis: &mut FileAnalysis,
    ) {
        let kind = node.kind();
        match kind {
            "function_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let mut sym = SymbolDef::new(
                        name,
                        SymbolKind::Function,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym.visibility = Visibility::Public;
                    let sym_id = sym.id.clone();
                    analysis.symbols.push(sym);

                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_js_calls(body, source, file_path, &sym_id, analysis);
                    }
                }
            }
            "class_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let sym = SymbolDef::new(
                        name,
                        SymbolKind::Class,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    analysis.symbols.push(sym);
                }
            }
            "import_statement" => {
                let import_text = node_text(node, source);
                // Extract "from 'module'" or "require('module')"
                if let Some(source_node) = node.child_by_field_name("source") {
                    let module = node_text(source_node, source)
                        .trim_matches(|c| c == '\'' || c == '"')
                        .to_string();
                    analysis.import_edges.push(ImportEdge {
                        from_file: file_path.to_string(),
                        to_file: module,
                        symbol: None,
                    });
                }
            }
            _ => {}
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.extract_js_symbols(child, source, file_path, analysis);
            }
        }
    }

    fn extract_js_calls(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        caller_id: &str,
        analysis: &mut FileAnalysis,
    ) {
        if node.kind() == "call_expression" {
            if let Some(func_node) = node.child_by_field_name("function") {
                let callee_name = node_text(func_node, source);
                analysis.call_edges.push(CallEdge {
                    caller: caller_id.to_string(),
                    callee: callee_name,
                });
            }
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.extract_js_calls(child, source, file_path, caller_id, analysis);
            }
        }
    }

    fn extract_python_symbols(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        analysis: &mut FileAnalysis,
    ) {
        let kind = node.kind();
        match kind {
            "function_definition" | "async_function_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let mut sym = SymbolDef::new(
                        name,
                        SymbolKind::Function,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym.visibility = Visibility::Public;
                    let sym_id = sym.id.clone();
                    analysis.symbols.push(sym);

                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_python_calls(body, source, file_path, &sym_id, analysis);
                    }
                }
            }
            "class_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let sym = SymbolDef::new(
                        name,
                        SymbolKind::Class,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    analysis.symbols.push(sym);
                }
            }
            "import_statement" | "import_from_statement" => {
                let import_text = node_text(node, source);
                // Extract module name
                if let Some(module_node) = node.child_by_field_name("module_name") {
                    let module = node_text(module_node, source);
                    analysis.import_edges.push(ImportEdge {
                        from_file: file_path.to_string(),
                        to_file: module,
                        symbol: None,
                    });
                }
            }
            _ => {}
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.extract_python_symbols(child, source, file_path, analysis);
            }
        }
    }

    fn extract_python_calls(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        caller_id: &str,
        analysis: &mut FileAnalysis,
    ) {
        if node.kind() == "call" {
            if let Some(func_node) = node.child_by_field_name("function") {
                let callee_name = node_text(func_node, source);
                analysis.call_edges.push(CallEdge {
                    caller: caller_id.to_string(),
                    callee: callee_name,
                });
            }
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.extract_python_calls(child, source, file_path, caller_id, analysis);
            }
        }
    }

    fn rust_visibility(&self, node: &Node, source: &str) -> Visibility {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "visibility_modifier" {
                    let text = node_text(child, source);
                    return match text.as_str() {
                        "pub" => Visibility::Public,
                        "pub(crate)" => Visibility::Crate,
                        "pub(super)" => Visibility::Super,
                        _ => Visibility::Public,
                    };
                }
            }
        }
        Visibility::Private
    }

    fn preceding_doc_comment(&self, node: &Node, source: &str) -> Option<String> {
        let mut comments = Vec::new();
        let mut prev = node.prev_sibling();
        while let Some(sib) = prev {
            let kind = sib.kind();
            if kind == "line_comment" || kind == "block_comment" {
                let text = node_text(sib, source);
                if text.starts_with("///") || text.starts_with("//!") {
                    comments.push(text.trim_start_matches('/').trim().to_string());
                } else {
                    break;
                }
            } else if kind == "attribute_item" {
                // skip #[...] attributes
            } else {
                break;
            }
            prev = sib.prev_sibling();
        }
        if comments.is_empty() {
            None
        } else {
            comments.reverse();
            Some(comments.join("\n"))
        }
    }
}

#[async_trait::async_trait]
impl CodeAnalyzer for TreeSitterAnalyzer {
    fn name(&self) -> &'static str {
        "tree-sitter"
    }

    fn is_available(&self) -> bool {
        true // always available as it's built-in
    }

    async fn install(&self) -> anyhow::Result<()> {
        Ok(()) // no installation needed
    }

    async fn analyze(&self, project_root: &Path) -> anyhow::Result<CodeIntelSnapshot> {
        let project_id = ProjectId::from_path(project_root).as_str().to_string();
        let mut snapshot = CodeIntelSnapshot::new(project_id);

        let files = collect_source_files(project_root);
        for file_path in files {
            let relative = file_path
                .strip_prefix(project_root)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .to_string();

            if let Ok(content) = std::fs::read_to_string(&file_path) {
                let ext = file_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if let Some(lang) = mccp_core::Language::from_extension(ext) {
                    if let Some(analysis) = self.analyze_file(&relative, &content, lang) {
                        snapshot.symbols.extend(analysis.symbols);
                        snapshot.call_edges.extend(analysis.call_edges);
                        snapshot.use_edges.extend(analysis.use_edges);
                        snapshot.import_edges.extend(analysis.import_edges);
                    }
                }
            }
        }

        Ok(snapshot)
    }

    fn supports_language(&self, lang: mccp_core::Language) -> bool {
        matches!(
            lang,
            mccp_core::Language::Rust
                | mccp_core::Language::TypeScript
                | mccp_core::Language::JavaScript
                | mccp_core::Language::Python
        )
    }
}

/// Analysis results for a single file
#[derive(Debug, Default)]
struct FileAnalysis {
    symbols: Vec<SymbolDef>,
    call_edges: Vec<CallEdge>,
    use_edges: Vec<UseEdge>,
    import_edges: Vec<ImportEdge>,
}

fn node_text(node: Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

fn get_ts_language(lang: mccp_core::Language) -> Option<TsLanguage> {
    match lang {
        mccp_core::Language::Rust => Some(tree_sitter_rust::language()),
        mccp_core::Language::TypeScript | mccp_core::Language::JavaScript => {
            Some(tree_sitter_typescript::language_typescript())
        }
        mccp_core::Language::Python => Some(tree_sitter_python::language()),
        _ => None,
    }
}

fn collect_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if !root.exists() {
        return files;
    }
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.') && name != "node_modules" && name != "target" && name != "vendor"
        })
    {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if mccp_core::Language::from_extension(ext).is_some() {
                        files.push(path.to_path_buf());
                    }
                }
            }
        }
    }
    files
}

fn binary_exists(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// External analyzer adapters
// ---------------------------------------------------------------------------

/// Adapter wrapping the `rust-analyzer` LSP binary.
pub struct RustAnalyzerAdapter;

impl RustAnalyzerAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl CodeAnalyzer for RustAnalyzerAdapter {
    fn name(&self) -> &'static str {
        "rust-analyzer"
    }

    fn is_available(&self) -> bool {
        binary_exists("rust-analyzer")
    }

    async fn install(&self) -> anyhow::Result<()> {
        tracing::info!("Installing rust-analyzer via rustup");
        let status = tokio::process::Command::new("rustup")
            .args(["component", "add", "rust-analyzer"])
            .status()
            .await?;
        anyhow::ensure!(status.success(), "rustup component add rust-analyzer failed");
        Ok(())
    }

    async fn analyze(&self, project_root: &Path) -> anyhow::Result<CodeIntelSnapshot> {
        // Verify rust-analyzer works
        let output = tokio::process::Command::new("rust-analyzer")
            .args(["analysis-stats", "--quiet"])
            .current_dir(project_root)
            .output()
            .await;
        match output {
            Ok(o) if o.status.success() => {
                tracing::info!("rust-analyzer analysis-stats succeeded");
            }
            _ => {
                tracing::warn!("rust-analyzer analysis-stats failed or unavailable");
            }
        }
        // Full LSP integration is complex; fall back to tree-sitter analysis.
        tracing::info!("rust-analyzer adapter: falling back to TreeSitterAnalyzer for full analysis");
        TreeSitterAnalyzer::new().analyze(project_root).await
    }

    fn supports_language(&self, lang: mccp_core::Language) -> bool {
        matches!(lang, mccp_core::Language::Rust)
    }
}

/// Adapter wrapping `universal-ctags`.
pub struct CtagsAdapter;

impl CtagsAdapter {
    pub fn new() -> Self {
        Self
    }

    fn map_ctags_kind(kind: &str) -> Option<SymbolKind> {
        match kind {
            "function" => Some(SymbolKind::Function),
            "class" => Some(SymbolKind::Class),
            "struct" => Some(SymbolKind::Struct),
            "method" => Some(SymbolKind::Method),
            "variable" => Some(SymbolKind::Variable),
            "enum" => Some(SymbolKind::Enum),
            "trait" => Some(SymbolKind::Trait),
            "module" => Some(SymbolKind::Module),
            "interface" => Some(SymbolKind::Interface),
            "constant" => Some(SymbolKind::Const),
            _ => None,
        }
    }
}

#[async_trait::async_trait]
impl CodeAnalyzer for CtagsAdapter {
    fn name(&self) -> &'static str {
        "ctags"
    }

    fn is_available(&self) -> bool {
        binary_exists("ctags")
    }

    async fn install(&self) -> anyhow::Result<()> {
        tracing::info!(
            "Install universal-ctags manually:\n  \
             Debian/Ubuntu: apt install universal-ctags\n  \
             macOS:         brew install universal-ctags"
        );
        Ok(())
    }

    async fn analyze(&self, project_root: &Path) -> anyhow::Result<CodeIntelSnapshot> {
        let output = tokio::process::Command::new("ctags")
            .args([
                "-R",
                "--output-format=json",
                "--fields=+neKS",
                "-f",
                "-",
            ])
            .arg(project_root)
            .output()
            .await?;

        anyhow::ensure!(output.status.success(), "ctags exited with non-zero status");

        let project_id = ProjectId::from_path(project_root).as_str().to_string();
        let mut snapshot = CodeIntelSnapshot::new(project_id);

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let name = match entry.get("name").and_then(|v| v.as_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let kind_str = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let kind = match Self::map_ctags_kind(kind_str) {
                Some(k) => k,
                None => continue,
            };
            let file = entry
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let start_line = entry.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let end_line = entry.get("end").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

            let id = format!("{file}::{name}::{kind:?}");
            snapshot.symbols.push(SymbolDef {
                id,
                name,
                kind,
                file,
                start_line,
                end_line,
                visibility: Visibility::Public,
                doc_comment: None,
                references: Vec::new(),
                in_cycle: false,
            });
        }

        tracing::info!(
            "ctags produced {} symbols (call_edges not available from ctags)",
            snapshot.symbols.len()
        );
        Ok(snapshot)
    }

    fn supports_language(&self, _lang: mccp_core::Language) -> bool {
        true // ctags supports many languages
    }
}

/// Adapter wrapping `ast-grep` (`sg`).
pub struct AstGrepAdapter;

impl AstGrepAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl CodeAnalyzer for AstGrepAdapter {
    fn name(&self) -> &'static str {
        "ast-grep"
    }

    fn is_available(&self) -> bool {
        binary_exists("ast-grep") || binary_exists("sg")
    }

    async fn install(&self) -> anyhow::Result<()> {
        tracing::info!("Installing ast-grep via cargo");
        let status = tokio::process::Command::new("cargo")
            .args(["install", "ast-grep"])
            .status()
            .await?;
        anyhow::ensure!(status.success(), "cargo install ast-grep failed");
        Ok(())
    }

    async fn analyze(&self, project_root: &Path) -> anyhow::Result<CodeIntelSnapshot> {
        let bin = if binary_exists("sg") { "sg" } else { "ast-grep" };

        // Try to find function definitions via pattern matching
        let output = tokio::process::Command::new(bin)
            .args(["run", "--pattern", "fn $NAME($_) { $$$ }", "--json"])
            .arg(project_root)
            .output()
            .await;

        match output {
            Ok(ref o) if o.status.success() => {
                tracing::info!(
                    "ast-grep pattern scan produced {} bytes of output",
                    o.stdout.len()
                );
            }
            _ => {
                tracing::warn!("ast-grep pattern scan failed or unavailable");
            }
        }

        // ast-grep is primarily a pattern matcher and cannot produce a full
        // CodeIntelSnapshot on its own; fall back to tree-sitter analysis.
        tracing::info!("ast-grep adapter: falling back to TreeSitterAnalyzer for full analysis");
        TreeSitterAnalyzer::new().analyze(project_root).await
    }

    fn supports_language(&self, lang: mccp_core::Language) -> bool {
        matches!(
            lang,
            mccp_core::Language::Rust
                | mccp_core::Language::TypeScript
                | mccp_core::Language::JavaScript
                | mccp_core::Language::Python
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_rust_file() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
use std::collections::HashMap;

/// A helper function
pub fn foo() {
    bar();
    let x = baz(42);
}

fn bar() {
    println!("hello");
}

fn baz(n: i32) -> i32 {
    n * 2
}

pub struct MyStruct {
    pub field: String,
}

impl MyStruct {
    pub fn new() -> Self {
        Self { field: String::new() }
    }
}

enum Color {
    Red,
    Green,
    Blue,
}

pub trait Drawable {
    fn draw(&self);
}
"#;

        let analysis = analyzer
            .analyze_file("src/lib.rs", code, mccp_core::Language::Rust)
            .unwrap();

        let symbol_names: Vec<&str> = analysis.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(symbol_names.contains(&"foo"), "should find foo: {:?}", symbol_names);
        assert!(symbol_names.contains(&"bar"), "should find bar: {:?}", symbol_names);
        assert!(symbol_names.contains(&"baz"), "should find baz: {:?}", symbol_names);
        assert!(symbol_names.contains(&"MyStruct"), "should find MyStruct: {:?}", symbol_names);
        assert!(symbol_names.contains(&"new"), "should find new (method): {:?}", symbol_names);
        assert!(symbol_names.contains(&"Color"), "should find Color: {:?}", symbol_names);
        assert!(symbol_names.contains(&"Drawable"), "should find Drawable: {:?}", symbol_names);
    }

    #[test]
    fn test_call_edge_extraction() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
fn a() {
    b();
}

fn b() {
    c();
}

fn c() {}
"#;
        let analysis = analyzer
            .analyze_file("src/lib.rs", code, mccp_core::Language::Rust)
            .unwrap();

        // a calls b
        let a_calls: Vec<&str> = analysis
            .call_edges
            .iter()
            .filter(|e| e.caller.contains("::a::"))
            .map(|e| e.callee.as_str())
            .collect();
        assert!(a_calls.iter().any(|c| c.contains("b")), "a should call b: {:?}", a_calls);

        // b calls c
        let b_calls: Vec<&str> = analysis
            .call_edges
            .iter()
            .filter(|e| e.caller.contains("::b::"))
            .map(|e| e.callee.as_str())
            .collect();
        assert!(b_calls.iter().any(|c| c.contains("c")), "b should call c: {:?}", b_calls);
    }

    #[test]
    fn test_import_edge_extraction() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
use crate::foo::Bar;
use std::collections::HashMap;

fn main() {}
"#;
        let analysis = analyzer
            .analyze_file("src/main.rs", code, mccp_core::Language::Rust)
            .unwrap();

        assert!(
            analysis.import_edges.len() >= 2,
            "should have at least 2 imports: {:?}",
            analysis.import_edges
        );
        let import_targets: Vec<&str> = analysis
            .import_edges
            .iter()
            .map(|e| e.to_file.as_str())
            .collect();
        assert!(
            import_targets.iter().any(|t| t.contains("crate::foo::Bar")),
            "should import crate::foo::Bar: {:?}",
            import_targets
        );
    }

    #[test]
    fn test_visibility_extraction() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
pub fn public_fn() {}
fn private_fn() {}
pub(crate) fn crate_fn() {}
"#;
        let analysis = analyzer
            .analyze_file("src/lib.rs", code, mccp_core::Language::Rust)
            .unwrap();

        let pub_fn = analysis.symbols.iter().find(|s| s.name == "public_fn").unwrap();
        assert_eq!(pub_fn.visibility, Visibility::Public);

        let priv_fn = analysis.symbols.iter().find(|s| s.name == "private_fn").unwrap();
        assert_eq!(priv_fn.visibility, Visibility::Private);

        let crate_fn = analysis.symbols.iter().find(|s| s.name == "crate_fn").unwrap();
        assert_eq!(crate_fn.visibility, Visibility::Crate);
    }

    #[test]
    fn test_doc_comment_extraction() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
/// This is a doc comment
/// with multiple lines
pub fn documented_fn() {}
"#;
        let analysis = analyzer
            .analyze_file("src/lib.rs", code, mccp_core::Language::Rust)
            .unwrap();

        let sym = analysis.symbols.iter().find(|s| s.name == "documented_fn").unwrap();
        assert!(sym.doc_comment.is_some());
        assert!(sym.doc_comment.as_ref().unwrap().contains("doc comment"));
    }
}
