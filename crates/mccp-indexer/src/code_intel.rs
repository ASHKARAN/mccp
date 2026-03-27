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
        analysis.language = Some(lang);
        self.extract_symbols(root, content, file_path, lang, &mut analysis, None);
        // Collect references: scan all identifiers and match against known symbol names
        self.collect_references(root, content, file_path, &mut analysis);
        Some(analysis)
    }

    fn extract_symbols(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        lang: mccp_core::Language,
        analysis: &mut FileAnalysis,
        parent_sym_id: Option<&str>,
    ) {
        match lang {
            mccp_core::Language::Rust => self.extract_rust_symbols(node, source, file_path, analysis),
            mccp_core::Language::TypeScript | mccp_core::Language::JavaScript => {
                self.extract_js_symbols(node, source, file_path, analysis, parent_sym_id)
            }
            mccp_core::Language::Python => {
                self.extract_python_symbols(node, source, file_path, analysis, parent_sym_id)
            }
            mccp_core::Language::Java => {
                self.extract_java_symbols(node, source, file_path, analysis, parent_sym_id)
            }
            mccp_core::Language::Go => {
                self.extract_go_symbols(node, source, file_path, analysis)
            }
            mccp_core::Language::Kotlin => {
                self.extract_kotlin_symbols(node, source, file_path, analysis, parent_sym_id)
            }
            mccp_core::Language::CSharp => {
                self.extract_csharp_symbols(node, source, file_path, analysis, parent_sym_id)
            }
            _ => {
                // Walk children for unsupported languages
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        self.extract_symbols(child, source, file_path, lang, analysis, parent_sym_id);
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
                    sym.annotations = self.extract_rust_attributes(&node, source);
                    sym = sym.with_language("rust");
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
                    sym.annotations = self.extract_rust_attributes(&node, source);
                    sym = sym.with_language("rust");
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
        _parent_sym_id: Option<&str>,
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
                self.extract_js_symbols(child, source, file_path, analysis, _parent_sym_id);
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
        _parent_sym_id: Option<&str>,
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
                self.extract_python_symbols(child, source, file_path, analysis, _parent_sym_id);
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

    /// Extract Rust #[...] attributes as annotations
    fn extract_rust_attributes(&self, node: &Node, source: &str) -> Vec<Annotation> {
        let mut annotations = Vec::new();
        let mut prev = node.prev_sibling();
        while let Some(sib) = prev {
            if sib.kind() == "attribute_item" {
                let text = node_text(sib, source);
                // Parse #[derive(Debug, Clone, Serialize)] etc
                let inner = text.trim_start_matches("#[").trim_end_matches(']');
                if inner.starts_with("derive(") {
                    let derives = inner
                        .trim_start_matches("derive(")
                        .trim_end_matches(')')
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>();
                    annotations.push(Annotation {
                        name: "derive".to_string(),
                        arguments: derives,
                        line: sib.start_position().row as u32 + 1,
                    });
                } else {
                    let name = inner.split('(').next().unwrap_or(inner).to_string();
                    let args = if inner.contains('(') {
                        inner.split_once('(')
                            .and_then(|(_, rest)| rest.strip_suffix(')'))
                            .unwrap_or("")
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else {
                        Vec::new()
                    };
                    annotations.push(Annotation {
                        name,
                        arguments: args,
                        line: sib.start_position().row as u32 + 1,
                    });
                }
            } else if sib.kind() == "line_comment" || sib.kind() == "block_comment" {
                // skip comments
            } else {
                break;
            }
            prev = sib.prev_sibling();
        }
        annotations.reverse();
        annotations
    }

    // ── Reference collection ──────────────────────────────────────────

    /// Scan all identifiers in the file and record references to known symbols
    fn collect_references(
        &self,
        root: Node,
        source: &str,
        file_path: &str,
        analysis: &mut FileAnalysis,
    ) {
        // Build set of known symbol names for quick lookup
        let known: HashMap<String, String> = analysis
            .symbols
            .iter()
            .map(|s| (s.name.clone(), s.id.clone()))
            .collect();

        self.walk_identifiers(root, source, file_path, &known, analysis);
    }

    fn walk_identifiers(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        known: &HashMap<String, String>,
        analysis: &mut FileAnalysis,
    ) {
        let kind = node.kind();
        // Identifier nodes that might be references
        if kind == "identifier" || kind == "type_identifier" || kind == "field_identifier" {
            let name = node_text(node, source);
            if let Some(sym_id) = known.get(&name) {
                // Don't record definition site as a reference
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32;
                let is_def = analysis.symbols.iter().any(|s| {
                    s.id == *sym_id && s.file == file_path && s.start_line == line && s.start_column == col
                });
                if !is_def {
                    // Find corresponding symbol and add reference
                    if let Some(sym) = analysis.symbols.iter_mut().find(|s| s.id == *sym_id) {
                        sym.references.push(SymbolRef {
                            file: file_path.to_string(),
                            line,
                            column: col,
                            end_line: node.end_position().row as u32 + 1,
                            end_column: node.end_position().column as u32,
                            context: get_line_context(source, line as usize),
                            ref_kind: Some(infer_ref_kind(node).to_string()),
                        });
                    }
                }
            }
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.walk_identifiers(child, source, file_path, known, analysis);
            }
        }
    }

    // ── Java extractor ────────────────────────────────────────────────

    fn extract_java_symbols(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        analysis: &mut FileAnalysis,
        parent_sym_id: Option<&str>,
    ) {
        let kind = node.kind();
        match kind {
            "class_declaration" | "interface_declaration" | "enum_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let sk = match kind {
                        "interface_declaration" => SymbolKind::Interface,
                        "enum_declaration" => SymbolKind::Enum,
                        _ => SymbolKind::Class,
                    };
                    let mut sym = SymbolDef::new(
                        name.clone(),
                        sk,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym = sym
                        .with_columns(node.start_position().column as u32, node.end_position().column as u32)
                        .with_language("java");
                    sym.visibility = self.java_visibility(&node, source);
                    sym.doc_comment = self.preceding_javadoc(&node, source);
                    sym.annotations = self.extract_annotation_nodes(&node, source);
                    if let Some(pid) = parent_sym_id {
                        sym = sym.with_parent(pid.to_string());
                    }
                    let sym_id = sym.id.clone();
                    // Detect Lombok codegen
                    self.detect_lombok_codegen(&sym, file_path, analysis);
                    analysis.symbols.push(sym);

                    // Recurse into body for methods, inner classes
                    if let Some(body) = node.child_by_field_name("body") {
                        for i in 0..body.child_count() {
                            if let Some(child) = body.child(i) {
                                self.extract_java_symbols(child, source, file_path, analysis, Some(&sym_id));
                            }
                        }
                    }
                    return; // don't recurse again below
                }
            }
            "method_declaration" | "constructor_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let sk = if kind == "constructor_declaration" { SymbolKind::Function } else { SymbolKind::Method };
                    let mut sym = SymbolDef::new(
                        name.clone(),
                        sk,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym = sym
                        .with_columns(node.start_position().column as u32, node.end_position().column as u32)
                        .with_language("java");
                    sym.visibility = self.java_visibility(&node, source);
                    sym.doc_comment = self.preceding_javadoc(&node, source);
                    sym.annotations = self.extract_annotation_nodes(&node, source);
                    sym.signature = Some(self.extract_java_signature(&node, source));
                    if let Some(pid) = parent_sym_id {
                        sym = sym.with_parent(pid.to_string());
                        sym.qualified_name = Some(format!("{}.{}", pid.split("::").nth(1).unwrap_or(""), name));
                    }
                    let sym_id = sym.id.clone();
                    analysis.symbols.push(sym);

                    // Extract calls from method body
                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_generic_calls(body, source, file_path, &sym_id, analysis);
                    }
                    return;
                }
            }
            "field_declaration" => {
                // Extract field names
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        if child.kind() == "variable_declarator" {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let name = node_text(name_node, source);
                                let mut sym = SymbolDef::new(
                                    name,
                                    SymbolKind::Variable,
                                    file_path.to_string(),
                                    node.start_position().row as u32 + 1,
                                    node.end_position().row as u32 + 1,
                                );
                                sym = sym.with_language("java");
                                sym.visibility = self.java_visibility(&node, source);
                                sym.annotations = self.extract_annotation_nodes(&node, source);
                                if let Some(pid) = parent_sym_id {
                                    sym = sym.with_parent(pid.to_string());
                                }
                                analysis.symbols.push(sym);
                            }
                        }
                    }
                }
                return;
            }
            "import_declaration" => {
                let text = node_text(node, source);
                let path = text
                    .trim_start_matches("import ")
                    .trim_start_matches("static ")
                    .trim_end_matches(';')
                    .trim()
                    .to_string();
                analysis.import_edges.push(ImportEdge {
                    from_file: file_path.to_string(),
                    to_file: path,
                    symbol: None,
                });
                return;
            }
            "annotation" => {
                // Annotations are extracted as part of parent nodes
                return;
            }
            _ => {}
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.extract_java_symbols(child, source, file_path, analysis, parent_sym_id);
            }
        }
    }

    // ── Go extractor ──────────────────────────────────────────────────

    fn extract_go_symbols(
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
                    let vis = if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                        Visibility::Public
                    } else {
                        Visibility::Private
                    };
                    let mut sym = SymbolDef::new(
                        name,
                        SymbolKind::Function,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym = sym
                        .with_columns(node.start_position().column as u32, node.end_position().column as u32)
                        .with_language("go");
                    sym.visibility = vis;
                    sym.doc_comment = self.preceding_go_comment(&node, source);
                    let sym_id = sym.id.clone();
                    analysis.symbols.push(sym);

                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_generic_calls(body, source, file_path, &sym_id, analysis);
                    }
                }
            }
            "method_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let vis = if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                        Visibility::Public
                    } else {
                        Visibility::Private
                    };
                    let mut sym = SymbolDef::new(
                        name,
                        SymbolKind::Method,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym = sym
                        .with_columns(node.start_position().column as u32, node.end_position().column as u32)
                        .with_language("go");
                    sym.visibility = vis;
                    sym.doc_comment = self.preceding_go_comment(&node, source);
                    let sym_id = sym.id.clone();
                    analysis.symbols.push(sym);

                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_generic_calls(body, source, file_path, &sym_id, analysis);
                    }
                }
            }
            "type_declaration" => {
                // Go type declarations: type Foo struct { ... } or type Foo interface { ... }
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        if child.kind() == "type_spec" {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let name = node_text(name_node, source);
                                let vis = if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                                    Visibility::Public
                                } else {
                                    Visibility::Private
                                };
                                // Determine if struct or interface
                                let sk = child.child_by_field_name("type")
                                    .map(|t| match t.kind() {
                                        "struct_type" => SymbolKind::Struct,
                                        "interface_type" => SymbolKind::Interface,
                                        _ => SymbolKind::TypeAlias,
                                    })
                                    .unwrap_or(SymbolKind::TypeAlias);
                                let mut sym = SymbolDef::new(
                                    name,
                                    sk,
                                    file_path.to_string(),
                                    child.start_position().row as u32 + 1,
                                    child.end_position().row as u32 + 1,
                                );
                                sym = sym.with_language("go");
                                sym.visibility = vis;
                                sym.doc_comment = self.preceding_go_comment(&node, source);
                                analysis.symbols.push(sym);
                            }
                        }
                    }
                }
            }
            "import_declaration" => {
                // Go imports
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        if child.kind() == "import_spec" || child.kind() == "interpreted_string_literal" {
                            let path = node_text(child, source).trim_matches('"').to_string();
                            if !path.is_empty() {
                                analysis.import_edges.push(ImportEdge {
                                    from_file: file_path.to_string(),
                                    to_file: path,
                                    symbol: None,
                                });
                            }
                        } else if child.kind() == "import_spec_list" {
                            for j in 0..child.child_count() {
                                if let Some(spec) = child.child(j) {
                                    if let Some(path_node) = spec.child_by_field_name("path") {
                                        let path = node_text(path_node, source).trim_matches('"').to_string();
                                        if !path.is_empty() {
                                            analysis.import_edges.push(ImportEdge {
                                                from_file: file_path.to_string(),
                                                to_file: path,
                                                symbol: None,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        // Recurse for non-handled nodes
        if kind != "function_declaration" && kind != "method_declaration" {
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    self.extract_go_symbols(child, source, file_path, analysis);
                }
            }
        }
    }

    // ── Kotlin extractor ──────────────────────────────────────────────

    fn extract_kotlin_symbols(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        analysis: &mut FileAnalysis,
        parent_sym_id: Option<&str>,
    ) {
        let kind = node.kind();
        match kind {
            "class_declaration" | "object_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name")
                    .or_else(|| find_child_by_kind(&node, "type_identifier"))
                    .or_else(|| find_child_by_kind(&node, "simple_identifier"))
                {
                    let name = node_text(name_node, source);
                    let sk = if kind == "object_declaration" { SymbolKind::Class } else { SymbolKind::Class };
                    let mut sym = SymbolDef::new(
                        name.clone(),
                        sk,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym = sym.with_language("kotlin");
                    sym.visibility = self.kotlin_visibility(&node, source);
                    sym.annotations = self.extract_kotlin_annotations(&node, source);
                    if let Some(pid) = parent_sym_id {
                        sym = sym.with_parent(pid.to_string());
                    }
                    // Detect Kotlin data class codegen
                    let full_text = node_text(node, source);
                    if full_text.starts_with("data class") {
                        analysis.codegen_patterns.push(CodegenPattern {
                            file: file_path.to_string(),
                            line: node.start_position().row as u32 + 1,
                            pattern_type: CodegenType::KotlinDataClass,
                            generated_members: vec![
                                "equals".to_string(), "hashCode".to_string(),
                                "toString".to_string(), "copy".to_string(),
                                "componentN".to_string(),
                            ],
                            source_annotation: "data class".to_string(),
                        });
                    }
                    let sym_id = sym.id.clone();
                    analysis.symbols.push(sym);

                    if let Some(body) = node.child_by_field_name("class_body")
                        .or_else(|| find_child_by_kind(&node, "class_body"))
                    {
                        for i in 0..body.child_count() {
                            if let Some(child) = body.child(i) {
                                self.extract_kotlin_symbols(child, source, file_path, analysis, Some(&sym_id));
                            }
                        }
                    }
                    return;
                }
            }
            "function_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name")
                    .or_else(|| find_child_by_kind(&node, "simple_identifier"))
                {
                    let name = node_text(name_node, source);
                    let sk = if parent_sym_id.is_some() { SymbolKind::Method } else { SymbolKind::Function };
                    let mut sym = SymbolDef::new(
                        name,
                        sk,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym = sym.with_language("kotlin");
                    sym.visibility = self.kotlin_visibility(&node, source);
                    sym.annotations = self.extract_kotlin_annotations(&node, source);
                    if let Some(pid) = parent_sym_id {
                        sym = sym.with_parent(pid.to_string());
                    }
                    let sym_id = sym.id.clone();
                    analysis.symbols.push(sym);

                    if let Some(body) = node.child_by_field_name("function_body") {
                        self.extract_generic_calls(body, source, file_path, &sym_id, analysis);
                    }
                    return;
                }
            }
            "property_declaration" => {
                // Extract property name from variable_declaration > simple_identifier, or direct simple_identifier
                let name_str = find_child_by_kind(&node, "simple_identifier")
                    .map(|n| node_text(n, source))
                    .or_else(|| {
                        for i in 0..node.child_count() {
                            if let Some(child) = node.child(i) {
                                if child.kind() == "variable_declaration" {
                                    if let Some(id) = find_child_by_kind(&child, "simple_identifier") {
                                        return Some(node_text(id, source));
                                    }
                                }
                            }
                        }
                        None
                    });
                if let Some(name) = name_str {
                    let mut sym = SymbolDef::new(
                        name,
                        SymbolKind::Variable,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym = sym.with_language("kotlin");
                    if let Some(pid) = parent_sym_id {
                        sym = sym.with_parent(pid.to_string());
                    }
                    analysis.symbols.push(sym);
                }
            }
            "import_header" | "import_list" => {
                let text = node_text(node, source);
                for line in text.lines() {
                    let line = line.trim();
                    if line.starts_with("import ") {
                        let path = line.trim_start_matches("import ").trim().to_string();
                        analysis.import_edges.push(ImportEdge {
                            from_file: file_path.to_string(),
                            to_file: path,
                            symbol: None,
                        });
                    }
                }
            }
            _ => {}
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.extract_kotlin_symbols(child, source, file_path, analysis, parent_sym_id);
            }
        }
    }

    // ── C# extractor ─────────────────────────────────────────────────

    fn extract_csharp_symbols(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        analysis: &mut FileAnalysis,
        parent_sym_id: Option<&str>,
    ) {
        let kind = node.kind();
        match kind {
            "class_declaration" | "interface_declaration" | "struct_declaration"
            | "enum_declaration" | "record_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let sk = match kind {
                        "interface_declaration" => SymbolKind::Interface,
                        "struct_declaration" => SymbolKind::Struct,
                        "enum_declaration" => SymbolKind::Enum,
                        "record_declaration" => {
                            analysis.codegen_patterns.push(CodegenPattern {
                                file: file_path.to_string(),
                                line: node.start_position().row as u32 + 1,
                                pattern_type: CodegenType::CSharpRecord,
                                generated_members: vec![
                                    "Equals".to_string(), "GetHashCode".to_string(),
                                    "ToString".to_string(), "Deconstruct".to_string(),
                                ],
                                source_annotation: "record".to_string(),
                            });
                            SymbolKind::Class
                        }
                        _ => SymbolKind::Class,
                    };
                    let mut sym = SymbolDef::new(
                        name.clone(),
                        sk,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym = sym.with_language("csharp");
                    sym.visibility = self.csharp_visibility(&node, source);
                    sym.annotations = self.extract_csharp_attributes(&node, source);
                    if let Some(pid) = parent_sym_id {
                        sym = sym.with_parent(pid.to_string());
                    }
                    let sym_id = sym.id.clone();
                    analysis.symbols.push(sym);

                    if let Some(body) = node.child_by_field_name("body")
                        .or_else(|| find_child_by_kind(&node, "declaration_list"))
                    {
                        for i in 0..body.child_count() {
                            if let Some(child) = body.child(i) {
                                self.extract_csharp_symbols(child, source, file_path, analysis, Some(&sym_id));
                            }
                        }
                    }
                    return;
                }
            }
            "method_declaration" | "constructor_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let mut sym = SymbolDef::new(
                        name,
                        SymbolKind::Method,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym = sym.with_language("csharp");
                    sym.visibility = self.csharp_visibility(&node, source);
                    sym.annotations = self.extract_csharp_attributes(&node, source);
                    if let Some(pid) = parent_sym_id {
                        sym = sym.with_parent(pid.to_string());
                    }
                    let sym_id = sym.id.clone();
                    analysis.symbols.push(sym);

                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_generic_calls(body, source, file_path, &sym_id, analysis);
                    }
                    return;
                }
            }
            "property_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let mut sym = SymbolDef::new(
                        name,
                        SymbolKind::Variable,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym = sym.with_language("csharp");
                    if let Some(pid) = parent_sym_id {
                        sym = sym.with_parent(pid.to_string());
                    }
                    analysis.symbols.push(sym);
                }
            }
            "namespace_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let mut sym = SymbolDef::new(
                        name,
                        SymbolKind::Module,
                        file_path.to_string(),
                        node.start_position().row as u32 + 1,
                        node.end_position().row as u32 + 1,
                    );
                    sym = sym.with_language("csharp");
                    let sym_id = sym.id.clone();
                    analysis.symbols.push(sym);

                    if let Some(body) = node.child_by_field_name("body")
                        .or_else(|| find_child_by_kind(&node, "declaration_list"))
                    {
                        for i in 0..body.child_count() {
                            if let Some(child) = body.child(i) {
                                self.extract_csharp_symbols(child, source, file_path, analysis, Some(&sym_id));
                            }
                        }
                    }
                    return;
                }
            }
            "using_directive" => {
                let text = node_text(node, source);
                let path = text
                    .trim_start_matches("using ")
                    .trim_start_matches("static ")
                    .trim_end_matches(';')
                    .trim()
                    .to_string();
                analysis.import_edges.push(ImportEdge {
                    from_file: file_path.to_string(),
                    to_file: path,
                    symbol: None,
                });
                return;
            }
            _ => {}
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.extract_csharp_symbols(child, source, file_path, analysis, parent_sym_id);
            }
        }
    }

    // ── Helper methods for language-specific features ─────────────────

    fn extract_generic_calls(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        caller_id: &str,
        analysis: &mut FileAnalysis,
    ) {
        let kind = node.kind();
        if kind == "call_expression" || kind == "method_invocation" || kind == "invocation_expression" {
            // Try function field, then name field, else first child
            let callee_name = node.child_by_field_name("function")
                .or_else(|| node.child_by_field_name("name"))
                .or_else(|| node.child(0))
                .map(|n| node_text(n, source))
                .unwrap_or_default();
            if !callee_name.is_empty() {
                analysis.call_edges.push(CallEdge {
                    caller: caller_id.to_string(),
                    callee: callee_name,
                });
            }
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.extract_generic_calls(child, source, file_path, caller_id, analysis);
            }
        }
    }

    fn java_visibility(&self, node: &Node, source: &str) -> Visibility {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "modifiers" {
                    let text = node_text(child, source);
                    if text.contains("public") { return Visibility::Public; }
                    if text.contains("private") { return Visibility::Private; }
                    if text.contains("protected") { return Visibility::Super; }
                }
            }
        }
        Visibility::Private // package-private
    }

    fn kotlin_visibility(&self, node: &Node, source: &str) -> Visibility {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "modifiers" || child.kind() == "visibility_modifier" {
                    let text = node_text(child, source);
                    if text.contains("public") { return Visibility::Public; }
                    if text.contains("private") { return Visibility::Private; }
                    if text.contains("internal") { return Visibility::Crate; }
                    if text.contains("protected") { return Visibility::Super; }
                }
            }
        }
        Visibility::Public // Kotlin default is public
    }

    fn csharp_visibility(&self, node: &Node, source: &str) -> Visibility {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                let kind = child.kind();
                if kind == "modifier" || kind.contains("modifier") {
                    let text = node_text(child, source);
                    if text.contains("public") { return Visibility::Public; }
                    if text.contains("private") { return Visibility::Private; }
                    if text.contains("internal") { return Visibility::Crate; }
                    if text.contains("protected") { return Visibility::Super; }
                }
            }
        }
        Visibility::Private
    }

    fn extract_java_signature(&self, node: &Node, source: &str) -> String {
        // Get parameters and return type
        let mut sig = String::new();
        if let Some(ret) = node.child_by_field_name("type") {
            sig.push_str(&node_text(ret, source));
            sig.push(' ');
        }
        if let Some(name) = node.child_by_field_name("name") {
            sig.push_str(&node_text(name, source));
        }
        if let Some(params) = node.child_by_field_name("parameters") {
            sig.push_str(&node_text(params, source));
        }
        sig
    }

    fn preceding_javadoc(&self, node: &Node, source: &str) -> Option<String> {
        let mut prev = node.prev_sibling();
        while let Some(sib) = prev {
            let kind = sib.kind();
            if kind == "block_comment" || kind == "line_comment" {
                let text = node_text(sib, source);
                if text.starts_with("/**") || text.starts_with("//") {
                    return Some(text);
                }
            } else if kind == "annotation" || kind == "marker_annotation" {
                prev = sib.prev_sibling();
                continue;
            } else {
                break;
            }
            prev = sib.prev_sibling();
        }
        None
    }

    fn preceding_go_comment(&self, node: &Node, source: &str) -> Option<String> {
        let mut comments = Vec::new();
        let mut prev = node.prev_sibling();
        while let Some(sib) = prev {
            if sib.kind() == "comment" {
                let text = node_text(sib, source);
                comments.push(text.trim_start_matches("//").trim().to_string());
            } else {
                break;
            }
            prev = sib.prev_sibling();
        }
        if comments.is_empty() { None } else {
            comments.reverse();
            Some(comments.join("\n"))
        }
    }

    fn extract_annotation_nodes(&self, node: &Node, source: &str) -> Vec<Annotation> {
        let mut annotations = Vec::new();

        // Strategy 1: Look inside a `modifiers` child (Java tree-sitter grammar
        // nests annotations under `modifiers` within the declaration node).
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "modifiers" {
                    for j in 0..child.child_count() {
                        if let Some(annot) = child.child(j) {
                            let ak = annot.kind();
                            if ak == "annotation" || ak == "marker_annotation" {
                                let text = node_text(annot, source);
                                let name = text.trim_start_matches('@').split('(').next().unwrap_or("").to_string();
                                let args: Vec<String> = if text.contains('(') {
                                    text.split_once('(')
                                        .and_then(|(_, rest)| rest.strip_suffix(')'))
                                        .unwrap_or("")
                                        .split(',')
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect()
                                } else {
                                    Vec::new()
                                };
                                annotations.push(Annotation {
                                    name,
                                    arguments: args,
                                    line: annot.start_position().row as u32 + 1,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Strategy 2: Look at preceding siblings (fallback for grammars where
        // annotations are sibling nodes).
        if annotations.is_empty() {
            let mut prev = node.prev_sibling();
            while let Some(sib) = prev {
                let kind = sib.kind();
                if kind == "annotation" || kind == "marker_annotation" {
                    let text = node_text(sib, source);
                    let name = text.trim_start_matches('@').split('(').next().unwrap_or("").to_string();
                    let args: Vec<String> = if text.contains('(') {
                        text.split_once('(')
                            .and_then(|(_, rest)| rest.strip_suffix(')'))
                            .unwrap_or("")
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else {
                        Vec::new()
                    };
                    annotations.push(Annotation {
                        name,
                        arguments: args,
                        line: sib.start_position().row as u32 + 1,
                    });
                } else if kind == "line_comment" || kind == "block_comment" {
                    // skip comments
                } else {
                    break;
                }
                prev = sib.prev_sibling();
            }
            annotations.reverse();
        }

        annotations
    }

    fn extract_kotlin_annotations(&self, node: &Node, source: &str) -> Vec<Annotation> {
        let mut annotations = Vec::new();
        // Kotlin annotations can be in modifiers child
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "modifiers" {
                    for j in 0..child.child_count() {
                        if let Some(annot) = child.child(j) {
                            if annot.kind() == "annotation" {
                                let text = node_text(annot, source);
                                let name = text.trim_start_matches('@')
                                    .split('(').next().unwrap_or("").to_string();
                                annotations.push(Annotation {
                                    name,
                                    arguments: Vec::new(),
                                    line: annot.start_position().row as u32 + 1,
                                });
                            }
                        }
                    }
                }
            }
        }
        // Also check preceding siblings
        let mut prev = node.prev_sibling();
        while let Some(sib) = prev {
            if sib.kind() == "annotation" {
                let text = node_text(sib, source);
                let name = text.trim_start_matches('@').split('(').next().unwrap_or("").to_string();
                annotations.push(Annotation {
                    name,
                    arguments: Vec::new(),
                    line: sib.start_position().row as u32 + 1,
                });
            } else if sib.kind() == "comment" || sib.kind() == "multiline_comment" {
                // skip
            } else {
                break;
            }
            prev = sib.prev_sibling();
        }
        annotations
    }

    fn extract_csharp_attributes(&self, node: &Node, source: &str) -> Vec<Annotation> {
        let mut annotations = Vec::new();
        // C# attributes in [Attribute] syntax
        let mut prev = node.prev_sibling();
        while let Some(sib) = prev {
            if sib.kind() == "attribute_list" {
                let text = node_text(sib, source);
                let inner = text.trim_start_matches('[').trim_end_matches(']');
                for attr in inner.split(',') {
                    let name = attr.trim().split('(').next().unwrap_or("").to_string();
                    if !name.is_empty() {
                        annotations.push(Annotation {
                            name,
                            arguments: Vec::new(),
                            line: sib.start_position().row as u32 + 1,
                        });
                    }
                }
            } else if sib.kind() == "comment" {
                // skip
            } else {
                break;
            }
            prev = sib.prev_sibling();
        }
        annotations
    }

    /// Detect Lombok codegen patterns on Java classes
    fn detect_lombok_codegen(
        &self,
        sym: &SymbolDef,
        file_path: &str,
        analysis: &mut FileAnalysis,
    ) {
        let lombok_annotations = [
            ("Data", vec!["getters", "setters", "equals", "hashCode", "toString"]),
            ("Builder", vec!["builder()", "build()"]),
            ("Getter", vec!["getters for all fields"]),
            ("Setter", vec!["setters for all fields"]),
            ("NoArgsConstructor", vec!["no-args constructor"]),
            ("AllArgsConstructor", vec!["all-args constructor"]),
            ("RequiredArgsConstructor", vec!["required-args constructor"]),
            ("Value", vec!["getters", "equals", "hashCode", "toString", "final fields"]),
            ("ToString", vec!["toString()"]),
            ("EqualsAndHashCode", vec!["equals()", "hashCode()"]),
            ("Slf4j", vec!["log field"]),
            ("Log", vec!["log field"]),
        ];

        for annot in &sym.annotations {
            for (lombok_name, generated) in &lombok_annotations {
                if annot.name == *lombok_name {
                    analysis.codegen_patterns.push(CodegenPattern {
                        file: file_path.to_string(),
                        line: annot.line,
                        pattern_type: CodegenType::Lombok,
                        generated_members: generated.iter().map(|s| s.to_string()).collect(),
                        source_annotation: format!("@{}", lombok_name),
                    });
                }
            }
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
        let mut language_file_counts: HashMap<String, usize> = HashMap::new();
        let mut language_line_counts: HashMap<String, usize> = HashMap::new();

        for file_path in &files {
            let relative = file_path
                .strip_prefix(project_root)
                .unwrap_or(file_path)
                .to_string_lossy()
                .to_string();

            if let Ok(content) = std::fs::read_to_string(file_path) {
                let ext = file_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if let Some(lang) = mccp_core::Language::from_extension(ext) {
                    let lang_name = lang.to_string();
                    *language_file_counts.entry(lang_name.clone()).or_insert(0) += 1;
                    *language_line_counts.entry(lang_name).or_insert(0) += content.lines().count();

                    if let Some(analysis) = self.analyze_file(&relative, &content, lang) {
                        snapshot.symbols.extend(analysis.symbols);
                        snapshot.call_edges.extend(analysis.call_edges);
                        snapshot.use_edges.extend(analysis.use_edges);
                        snapshot.import_edges.extend(analysis.import_edges);
                        snapshot.codegen_patterns.extend(analysis.codegen_patterns);
                        snapshot.frameworks.extend(analysis.frameworks);
                    }

                    // Detect frameworks from file content
                    detect_frameworks(&relative, &content, &mut snapshot.frameworks);
                }
            }
        }

        // Build project structure
        snapshot.structure = Some(build_project_structure(
            project_root,
            &snapshot.symbols,
            &snapshot.import_edges,
            &language_file_counts,
            &language_line_counts,
        ));

        // Detect execution flows
        snapshot.flows = detect_execution_flows(&snapshot.symbols, &snapshot.call_edges);

        // Detect Rust derive codegen patterns
        detect_rust_derive_codegen(&snapshot.symbols, &mut snapshot.codegen_patterns);

        Ok(snapshot)
    }

    fn supports_language(&self, lang: mccp_core::Language) -> bool {
        // Supports all languages that have tree-sitter grammars
        get_ts_language(lang).is_some()
    }
}

/// Analysis results for a single file
#[derive(Debug, Default)]
struct FileAnalysis {
    symbols: Vec<SymbolDef>,
    call_edges: Vec<CallEdge>,
    use_edges: Vec<UseEdge>,
    import_edges: Vec<ImportEdge>,
    language: Option<mccp_core::Language>,
    annotations: Vec<Annotation>,
    codegen_patterns: Vec<CodegenPattern>,
    frameworks: Vec<FrameworkInfo>,
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
        mccp_core::Language::Java => Some(tree_sitter_java::language()),
        mccp_core::Language::Go => Some(tree_sitter_go::language()),
        mccp_core::Language::C => Some(tree_sitter_c::language()),
        mccp_core::Language::Cpp => Some(tree_sitter_cpp::language()),
        mccp_core::Language::CSharp => Some(tree_sitter_c_sharp::language()),
        mccp_core::Language::Ruby => Some(tree_sitter_ruby::language()),
        mccp_core::Language::PHP => {
            // tree-sitter-php has a version conflict; skip for now
            None
        }
        mccp_core::Language::Kotlin => Some(tree_sitter_kotlin::language()),
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

/// Find a child node by its kind
fn find_child_by_kind<'a>(node: &'a Node<'a>, kind: &str) -> Option<Node<'a>> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == kind {
                return Some(child);
            }
        }
    }
    None
}

/// Get the line content for a given line number (1-indexed)
fn get_line_context(source: &str, line: usize) -> String {
    source.lines().nth(line.saturating_sub(1)).unwrap_or("").trim().to_string()
}

/// Infer reference kind from surrounding AST context
fn infer_ref_kind(node: Node) -> &'static str {
    if let Some(parent) = node.parent() {
        match parent.kind() {
            "call_expression" | "method_invocation" | "invocation_expression" => "call",
            "import_statement" | "use_declaration" | "import_declaration" | "using_directive" => "import",
            "type_annotation" | "type_identifier" | "generic_type" => "type_annotation",
            "assignment_expression" | "assignment_statement" => "assignment",
            "field_expression" | "field_access" | "member_expression" => "field_access",
            _ => "reference",
        }
    } else {
        "reference"
    }
}

// ---------------------------------------------------------------------------
// Framework detection
// ---------------------------------------------------------------------------

/// Detect frameworks from file content and import patterns
fn detect_frameworks(file_path: &str, content: &str, frameworks: &mut Vec<FrameworkInfo>) {
    let patterns: &[(&str, Framework, &[&str])] = &[
        // Java/Kotlin
        ("org.springframework", Framework::SpringBoot, &["@RestController", "@Service", "@Repository", "@Component"]),
        ("org.springframework.web", Framework::SpringMVC, &["@RequestMapping", "@GetMapping", "@PostMapping"]),
        ("io.quarkus", Framework::Quarkus, &["@Path", "@GET", "@POST"]),
        ("io.micronaut", Framework::Micronaut, &["@Controller", "@Get", "@Post"]),
        // JS/TS
        ("express", Framework::Express, &["app.get", "app.post", "app.use", "Router()"]),
        ("@nestjs", Framework::NestJS, &["@Controller", "@Injectable", "@Module", "@Get", "@Post"]),
        ("next", Framework::NextJS, &["getServerSideProps", "getStaticProps", "NextPage"]),
        ("fastify", Framework::Fastify, &["fastify.get", "fastify.post"]),
        // Python
        ("django", Framework::Django, &["views.py", "models.py", "urls.py", "from django"]),
        ("flask", Framework::Flask, &["@app.route", "Flask(__name__)"]),
        ("fastapi", Framework::FastAPI, &["@app.get", "@app.post", "FastAPI()"]),
        // Rust
        ("actix_web", Framework::Actix, &["#[get", "#[post", "HttpServer", "web::get"]),
        ("axum", Framework::Axum, &["Router::new", "axum::routing"]),
        ("rocket", Framework::Rocket, &["#[get", "#[post", "#[launch]"]),
        // Go
        ("github.com/gin-gonic/gin", Framework::Gin, &["gin.Default", "gin.New"]),
        ("github.com/labstack/echo", Framework::Echo, &["echo.New()"]),
        ("github.com/gofiber/fiber", Framework::Fiber, &["fiber.New()"]),
        // C#
        ("Microsoft.AspNetCore", Framework::AspNetCore, &["[ApiController]", "[HttpGet]", "[HttpPost]"]),
        // Kotlin
        ("io.ktor", Framework::Ktor, &["routing {", "get(", "post("]),
    ];

    for (import_pattern, framework, content_patterns) in patterns {
        if content.contains(import_pattern) {
            let detected: Vec<String> = content_patterns.iter()
                .filter(|p| content.contains(**p))
                .map(|p| p.to_string())
                .collect();
            if !detected.is_empty() {
                // Avoid duplicates
                let already = frameworks.iter().any(|f| f.framework == *framework && f.file == file_path);
                if !already {
                    frameworks.push(FrameworkInfo {
                        framework: framework.clone(),
                        version: None,
                        file: file_path.to_string(),
                        detected_patterns: detected,
                    });
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Execution flow detection
// ---------------------------------------------------------------------------

/// Detect execution flows by finding entry points and tracing call chains
fn detect_execution_flows(symbols: &[SymbolDef], call_edges: &[CallEdge]) -> Vec<ExecutionFlow> {
    let mut flows = Vec::new();

    // Build a map from symbol names/ids to symbol defs for quick lookup
    let sym_by_id: HashMap<&str, &SymbolDef> = symbols.iter()
        .map(|s| (s.id.as_str(), s))
        .collect();

    // Find entry points: symbols with HTTP/route/CLI annotations
    let entry_annotations = [
        ("GetMapping", FlowType::HttpEndpoint),
        ("PostMapping", FlowType::HttpEndpoint),
        ("PutMapping", FlowType::HttpEndpoint),
        ("DeleteMapping", FlowType::HttpEndpoint),
        ("RequestMapping", FlowType::HttpEndpoint),
        ("Get", FlowType::HttpEndpoint),
        ("Post", FlowType::HttpEndpoint),
        ("Put", FlowType::HttpEndpoint),
        ("Delete", FlowType::HttpEndpoint),
        ("Path", FlowType::HttpEndpoint),
        ("ApiController", FlowType::HttpEndpoint),
        ("HttpGet", FlowType::HttpEndpoint),
        ("HttpPost", FlowType::HttpEndpoint),
        ("Command", FlowType::CliCommand),
        ("EventListener", FlowType::EventHandler),
        ("Scheduled", FlowType::ScheduledTask),
        ("KafkaListener", FlowType::MessageConsumer),
        ("RabbitListener", FlowType::MessageConsumer),
        ("WebSocketHandler", FlowType::WebSocket),
    ];

    for sym in symbols {
        for (annot_name, flow_type) in &entry_annotations {
            if sym.annotations.iter().any(|a| a.name == *annot_name) {
                let mut steps = Vec::new();
                let layer = infer_architectural_layer(sym);

                steps.push(FlowStep {
                    symbol_id: sym.id.clone(),
                    file: sym.file.clone(),
                    start_line: sym.start_line,
                    end_line: sym.end_line,
                    layer: layer.clone(),
                    description: format!("{} entry: {}", layer.as_str(), sym.name),
                    annotations: sym.annotations.iter().map(|a| a.name.clone()).collect(),
                });

                // Follow call chain
                trace_call_chain(&sym.id, call_edges, &sym_by_id, &mut steps, 0, 10);

                let flow_name = format!("{:?}::{}", flow_type, sym.name);
                flows.push(ExecutionFlow {
                    id: format!("flow::{}", sym.id),
                    name: flow_name,
                    flow_type: flow_type.clone(),
                    steps,
                    entry_file: sym.file.clone(),
                    entry_line: sym.start_line,
                });
            }
        }
    }

    flows
}

/// Trace a call chain from a symbol, building up flow steps
fn trace_call_chain(
    caller_id: &str,
    call_edges: &[CallEdge],
    sym_by_id: &HashMap<&str, &SymbolDef>,
    steps: &mut Vec<FlowStep>,
    depth: usize,
    max_depth: usize,
) {
    if depth >= max_depth { return; }

    let callees: Vec<&str> = call_edges.iter()
        .filter(|e| e.caller == caller_id)
        .map(|e| e.callee.as_str())
        .collect();

    for callee_name in callees {
        // Try to find symbol by id or by name
        let callee_sym = sym_by_id.get(callee_name)
            .or_else(|| sym_by_id.values().find(|s| s.name == callee_name));

        if let Some(sym) = callee_sym {
            // Avoid cycles
            if steps.iter().any(|s| s.symbol_id == sym.id) { continue; }

            let layer = infer_architectural_layer(sym);
            steps.push(FlowStep {
                symbol_id: sym.id.clone(),
                file: sym.file.clone(),
                start_line: sym.start_line,
                end_line: sym.end_line,
                layer,
                description: format!("calls {}", sym.name),
                annotations: sym.annotations.iter().map(|a| a.name.clone()).collect(),
            });

            trace_call_chain(&sym.id, call_edges, sym_by_id, steps, depth + 1, max_depth);
        }
    }
}

/// Infer the architectural layer of a symbol from its name, annotations, and file path
fn infer_architectural_layer(sym: &SymbolDef) -> ArchitecturalLayer {
    let name_lower = sym.name.to_lowercase();
    let file_lower = sym.file.to_lowercase();
    let annots: Vec<String> = sym.annotations.iter().map(|a| a.name.to_lowercase()).collect();

    // Check annotations first
    if annots.iter().any(|a| a.contains("controller") || a == "restcontroller" || a == "apicontroller") {
        return ArchitecturalLayer::Controller;
    }
    if annots.iter().any(|a| a == "service" || a == "injectable") {
        return ArchitecturalLayer::Service;
    }
    if annots.iter().any(|a| a == "repository" || a.contains("repo")) {
        return ArchitecturalLayer::Repository;
    }
    if annots.iter().any(|a| a == "entity" || a == "table" || a == "document") {
        return ArchitecturalLayer::Model;
    }
    if annots.iter().any(|a| a == "middleware" || a == "filter" || a == "interceptor") {
        return ArchitecturalLayer::Middleware;
    }
    if annots.iter().any(|a| a == "configuration" || a == "config") {
        return ArchitecturalLayer::Config;
    }

    // Check file path patterns
    if file_lower.contains("controller") || file_lower.contains("handler") || file_lower.contains("endpoint") {
        if file_lower.contains("handler") { return ArchitecturalLayer::Handler; }
        return ArchitecturalLayer::Controller;
    }
    if file_lower.contains("service") { return ArchitecturalLayer::Service; }
    if file_lower.contains("repository") || file_lower.contains("repo") || file_lower.contains("dao") {
        return ArchitecturalLayer::Repository;
    }
    if file_lower.contains("model") || file_lower.contains("entity") || file_lower.contains("dto") {
        return ArchitecturalLayer::Model;
    }
    if file_lower.contains("middleware") || file_lower.contains("filter") {
        return ArchitecturalLayer::Middleware;
    }
    if file_lower.contains("config") || file_lower.contains("setting") {
        return ArchitecturalLayer::Config;
    }
    if file_lower.contains("route") || file_lower.contains("router") {
        return ArchitecturalLayer::Router;
    }
    if file_lower.contains("interface") { return ArchitecturalLayer::Interface; }
    if file_lower.contains("util") || file_lower.contains("helper") {
        return ArchitecturalLayer::Utility;
    }

    // Check symbol name patterns
    if name_lower.contains("controller") { return ArchitecturalLayer::Controller; }
    if name_lower.contains("service") { return ArchitecturalLayer::Service; }
    if name_lower.contains("repository") || name_lower.contains("repo") || name_lower.contains("dao") {
        return ArchitecturalLayer::Repository;
    }

    ArchitecturalLayer::Unknown
}

// ---------------------------------------------------------------------------
// Project structure analysis
// ---------------------------------------------------------------------------

/// Build a high-level project structure from analyzed symbols
fn build_project_structure(
    project_root: &Path,
    symbols: &[SymbolDef],
    imports: &[ImportEdge],
    lang_file_counts: &HashMap<String, usize>,
    lang_line_counts: &HashMap<String, usize>,
) -> ProjectStructure {
    // Group symbols by top-level directory (module)
    let mut modules_map: HashMap<String, Vec<&SymbolDef>> = HashMap::new();
    for sym in symbols {
        let module_name = sym.file.split('/').next().unwrap_or(&sym.file).to_string();
        modules_map.entry(module_name).or_default().push(sym);
    }

    let mut modules = Vec::new();
    for (name, syms) in &modules_map {
        let languages: Vec<String> = syms.iter()
            .filter_map(|s| s.language.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let file_count = syms.iter()
            .map(|s| s.file.as_str())
            .collect::<std::collections::HashSet<_>>()
            .len();

        // Infer dependencies from imports
        let deps: Vec<String> = imports.iter()
            .filter(|i| i.from_file.starts_with(name.as_str()))
            .map(|i| i.to_file.split('/').next().unwrap_or(&i.to_file).to_string())
            .filter(|d| d != name)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Determine layer for the module
        let layer = if syms.iter().any(|s| infer_architectural_layer(s) == ArchitecturalLayer::Controller) {
            ArchitecturalLayer::Controller
        } else if syms.iter().any(|s| infer_architectural_layer(s) == ArchitecturalLayer::Service) {
            ArchitecturalLayer::Service
        } else if syms.iter().any(|s| infer_architectural_layer(s) == ArchitecturalLayer::Repository) {
            ArchitecturalLayer::Repository
        } else if syms.iter().any(|s| infer_architectural_layer(s) == ArchitecturalLayer::Model) {
            ArchitecturalLayer::Model
        } else {
            ArchitecturalLayer::Unknown
        };

        modules.push(StructureModule {
            name: name.clone(),
            path: name.clone(),
            languages,
            file_count,
            symbol_count: syms.len(),
            dependencies: deps,
            layer,
        });
    }

    // Build language stats
    let mut language_stats = HashMap::new();
    for (lang, &file_count) in lang_file_counts {
        let line_count = lang_line_counts.get(lang).copied().unwrap_or(0);
        let sym_count = symbols.iter().filter(|s| s.language.as_deref() == Some(lang.as_str())).count();
        let fn_count = symbols.iter().filter(|s| {
            s.language.as_deref() == Some(lang.as_str())
                && matches!(s.kind, SymbolKind::Function | SymbolKind::Method)
        }).count();
        let class_count = symbols.iter().filter(|s| {
            s.language.as_deref() == Some(lang.as_str())
                && matches!(s.kind, SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface)
        }).count();

        language_stats.insert(lang.clone(), LanguageStats {
            file_count,
            line_count,
            symbol_count: sym_count,
            function_count: fn_count,
            class_count,
        });
    }

    ProjectStructure {
        modules,
        language_stats,
    }
}

// ---------------------------------------------------------------------------
// Rust derive codegen detection
// ---------------------------------------------------------------------------

fn detect_rust_derive_codegen(symbols: &[SymbolDef], codegen_patterns: &mut Vec<CodegenPattern>) {
    for sym in symbols {
        for annot in &sym.annotations {
            if annot.name == "derive" {
                let generated: Vec<String> = annot.arguments.iter()
                    .flat_map(|arg| {
                        match arg.as_str() {
                            "Debug" => vec!["fmt::Debug impl".to_string()],
                            "Clone" => vec!["clone()".to_string()],
                            "Serialize" => vec!["serde::Serialize impl".to_string()],
                            "Deserialize" => vec!["serde::Deserialize impl".to_string()],
                            "PartialEq" => vec!["eq()".to_string()],
                            "Eq" => vec!["Eq impl".to_string()],
                            "Hash" => vec!["hash()".to_string()],
                            "Default" => vec!["default()".to_string()],
                            _ => vec![format!("{} impl", arg)],
                        }
                    })
                    .collect();

                if !generated.is_empty() {
                    codegen_patterns.push(CodegenPattern {
                        file: sym.file.clone(),
                        line: annot.line,
                        pattern_type: CodegenType::RustDerive,
                        generated_members: generated,
                        source_annotation: format!("#[derive({})]", annot.arguments.join(", ")),
                    });
                }
            }
        }
    }
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
                start_column: 0,
                end_column: 0,
                visibility: Visibility::Public,
                doc_comment: None,
                references: Vec::new(),
                in_cycle: false,
                annotations: Vec::new(),
                qualified_name: None,
                parent_symbol: None,
                language: None,
                signature: None,
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

    // ── Rust analysis tests ──────────────────────────────────────────

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

        let a_calls: Vec<&str> = analysis
            .call_edges
            .iter()
            .filter(|e| e.caller.contains("::a::"))
            .map(|e| e.callee.as_str())
            .collect();
        assert!(a_calls.iter().any(|c| c.contains("b")), "a should call b: {:?}", a_calls);

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

    #[test]
    fn test_rust_derive_annotations() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub name: String,
    pub value: i32,
}

#[derive(PartialEq, Eq, Hash)]
enum Status {
    Active,
    Inactive,
}
"#;
        let analysis = analyzer
            .analyze_file("src/config.rs", code, mccp_core::Language::Rust)
            .unwrap();

        let config = analysis.symbols.iter().find(|s| s.name == "Config").unwrap();
        assert!(!config.annotations.is_empty(), "Config should have derive annotations");
        let derive = config.annotations.iter().find(|a| a.name == "derive").unwrap();
        assert!(derive.arguments.contains(&"Debug".to_string()));
        assert!(derive.arguments.contains(&"Clone".to_string()));
        assert!(derive.arguments.contains(&"Serialize".to_string()));

        let status = analysis.symbols.iter().find(|s| s.name == "Status").unwrap();
        let derive = status.annotations.iter().find(|a| a.name == "derive").unwrap();
        assert!(derive.arguments.contains(&"PartialEq".to_string()));
        assert!(derive.arguments.contains(&"Hash".to_string()));
    }

    #[test]
    fn test_rust_language_tag() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
pub struct Foo {}
"#;
        let analysis = analyzer
            .analyze_file("src/lib.rs", code, mccp_core::Language::Rust)
            .unwrap();

        let sym = analysis.symbols.iter().find(|s| s.name == "Foo").unwrap();
        assert_eq!(sym.language.as_deref(), Some("rust"));
    }

    #[test]
    fn test_rust_module_extraction() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
pub mod utils;
mod internal;
"#;
        let analysis = analyzer
            .analyze_file("src/lib.rs", code, mccp_core::Language::Rust)
            .unwrap();

        let names: Vec<&str> = analysis.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"utils"), "should find utils module: {:?}", names);
        assert!(names.contains(&"internal"), "should find internal module: {:?}", names);
    }

    // ── Reference tracking tests ─────────────────────────────────────

    #[test]
    fn test_reference_collection_rust() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
fn helper() -> i32 {
    42
}

fn main() {
    let x = helper();
    let y = helper();
}
"#;
        let analysis = analyzer
            .analyze_file("src/main.rs", code, mccp_core::Language::Rust)
            .unwrap();

        let helper = analysis.symbols.iter().find(|s| s.name == "helper").unwrap();
        // helper should have references from main's calls
        assert!(
            !helper.references.is_empty(),
            "helper should have references: found {} refs",
            helper.references.len()
        );
        // All references should have line and column info
        for r in &helper.references {
            assert!(r.line > 0, "reference line should be positive");
        }
    }

    // ── Java analysis tests ──────────────────────────────────────────

    #[test]
    fn test_java_class_and_methods() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
package com.example;

import java.util.List;

public class UserService {
    private UserRepository userRepo;

    public User findById(Long id) {
        return userRepo.findById(id);
    }

    public List<User> findAll() {
        return userRepo.findAll();
    }
}
"#;
        let analysis = analyzer
            .analyze_file("src/main/java/UserService.java", code, mccp_core::Language::Java)
            .unwrap();

        let names: Vec<&str> = analysis.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"UserService"), "should find UserService class: {:?}", names);
        assert!(names.contains(&"findById"), "should find findById method: {:?}", names);
        assert!(names.contains(&"findAll"), "should find findAll method: {:?}", names);

        let user_service = analysis.symbols.iter().find(|s| s.name == "UserService").unwrap();
        assert_eq!(user_service.kind, SymbolKind::Class);
        assert_eq!(user_service.visibility, Visibility::Public);
        assert_eq!(user_service.language.as_deref(), Some("java"));

        // Methods should have parent set to the class
        let find_by_id = analysis.symbols.iter().find(|s| s.name == "findById").unwrap();
        assert!(find_by_id.parent_symbol.is_some(), "findById should have parent symbol");
    }

    #[test]
    fn test_java_annotations() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api/users")
public class UserController {

    @GetMapping("/{id}")
    public User getUser(@PathVariable Long id) {
        return null;
    }

    @PostMapping
    public User createUser(@RequestBody User user) {
        return null;
    }
}
"#;
        let analysis = analyzer
            .analyze_file("src/main/java/UserController.java", code, mccp_core::Language::Java)
            .unwrap();

        let controller = analysis.symbols.iter().find(|s| s.name == "UserController").unwrap();
        let annot_names: Vec<&str> = controller.annotations.iter().map(|a| a.name.as_str()).collect();
        assert!(
            annot_names.contains(&"RestController"),
            "should have @RestController: {:?}",
            annot_names
        );
    }

    #[test]
    fn test_java_imports() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
import java.util.List;
import java.util.Map;
import com.example.service.UserService;

public class Main {}
"#;
        let analysis = analyzer
            .analyze_file("Main.java", code, mccp_core::Language::Java)
            .unwrap();

        assert!(
            analysis.import_edges.len() >= 3,
            "should have at least 3 imports: got {}",
            analysis.import_edges.len()
        );
    }

    #[test]
    fn test_java_interface() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
public interface UserRepository {
    User findById(Long id);
    List<User> findAll();
}
"#;
        let analysis = analyzer
            .analyze_file("UserRepository.java", code, mccp_core::Language::Java)
            .unwrap();

        let repo = analysis.symbols.iter().find(|s| s.name == "UserRepository").unwrap();
        assert_eq!(repo.kind, SymbolKind::Interface);
    }

    #[test]
    fn test_java_enum() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
public enum Status {
    ACTIVE,
    INACTIVE,
    DELETED;
}
"#;
        let analysis = analyzer
            .analyze_file("Status.java", code, mccp_core::Language::Java)
            .unwrap();

        let status = analysis.symbols.iter().find(|s| s.name == "Status").unwrap();
        assert_eq!(status.kind, SymbolKind::Enum);
    }

    #[test]
    fn test_java_lombok_detection() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
import lombok.Data;
import lombok.Builder;

@Data
@Builder
public class UserDto {
    private String name;
    private String email;
    private int age;
}
"#;
        let analysis = analyzer
            .analyze_file("UserDto.java", code, mccp_core::Language::Java)
            .unwrap();

        let dto = analysis.symbols.iter().find(|s| s.name == "UserDto").unwrap();
        let annot_names: Vec<&str> = dto.annotations.iter().map(|a| a.name.as_str()).collect();
        assert!(annot_names.contains(&"Data"), "should have @Data: {:?}", annot_names);
        assert!(annot_names.contains(&"Builder"), "should have @Builder: {:?}", annot_names);

        // Codegen patterns should be detected
        assert!(
            !analysis.codegen_patterns.is_empty(),
            "should detect Lombok codegen patterns"
        );
        assert!(
            analysis.codegen_patterns.iter().any(|p| p.pattern_type == CodegenType::Lombok),
            "should detect Lombok type: {:?}",
            analysis.codegen_patterns
        );
    }

    #[test]
    fn test_java_call_extraction() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
public class Service {
    public void process() {
        validate();
        save();
    }

    private void validate() {}
    private void save() {}
}
"#;
        let analysis = analyzer
            .analyze_file("Service.java", code, mccp_core::Language::Java)
            .unwrap();

        let process_calls: Vec<&str> = analysis.call_edges.iter()
            .filter(|e| e.caller.contains("process"))
            .map(|e| e.callee.as_str())
            .collect();
        assert!(!process_calls.is_empty(), "process should have call edges: {:?}", analysis.call_edges);
    }

    // ── Go analysis tests ────────────────────────────────────────────

    #[test]
    fn test_go_functions_and_types() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
package main

import "fmt"

// Handler handles HTTP requests
func Handler(w http.ResponseWriter, r *http.Request) {
    fmt.Fprintf(w, "Hello")
}

func helper() string {
    return "private"
}

type User struct {
    Name  string
    Email string
}

type Logger interface {
    Log(msg string)
}
"#;
        let analysis = analyzer
            .analyze_file("main.go", code, mccp_core::Language::Go)
            .unwrap();

        let names: Vec<&str> = analysis.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Handler"), "should find Handler: {:?}", names);
        assert!(names.contains(&"helper"), "should find helper: {:?}", names);
        assert!(names.contains(&"User"), "should find User: {:?}", names);
        assert!(names.contains(&"Logger"), "should find Logger: {:?}", names);

        // Go visibility: uppercase = public
        let handler = analysis.symbols.iter().find(|s| s.name == "Handler").unwrap();
        assert_eq!(handler.visibility, Visibility::Public);
        assert_eq!(handler.language.as_deref(), Some("go"));

        let helper = analysis.symbols.iter().find(|s| s.name == "helper").unwrap();
        assert_eq!(helper.visibility, Visibility::Private);

        let user = analysis.symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(user.kind, SymbolKind::Struct);

        let logger = analysis.symbols.iter().find(|s| s.name == "Logger").unwrap();
        assert_eq!(logger.kind, SymbolKind::Interface);
    }

    #[test]
    fn test_go_imports() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
package main

import (
    "fmt"
    "net/http"
    "github.com/gin-gonic/gin"
)

func main() {}
"#;
        let analysis = analyzer
            .analyze_file("main.go", code, mccp_core::Language::Go)
            .unwrap();

        assert!(
            analysis.import_edges.len() >= 2,
            "should have multiple imports: got {}: {:?}",
            analysis.import_edges.len(),
            analysis.import_edges
        );
    }

    #[test]
    fn test_go_method_declaration() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
package main

type Server struct {
    port int
}

func (s *Server) Start() error {
    return nil
}

func (s *Server) stop() {
}
"#;
        let analysis = analyzer
            .analyze_file("server.go", code, mccp_core::Language::Go)
            .unwrap();

        let start = analysis.symbols.iter().find(|s| s.name == "Start").unwrap();
        assert_eq!(start.kind, SymbolKind::Method);
        assert_eq!(start.visibility, Visibility::Public);

        let stop = analysis.symbols.iter().find(|s| s.name == "stop").unwrap();
        assert_eq!(stop.kind, SymbolKind::Method);
        assert_eq!(stop.visibility, Visibility::Private);
    }

    // ── TypeScript analysis tests ────────────────────────────────────

    #[test]
    fn test_typescript_class_and_imports() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
import { Injectable } from '@nestjs/common';
import { UserRepository } from './user.repository';

export class UserService {
    constructor(private userRepo: UserRepository) {}

    async findAll(): Promise<User[]> {
        return this.userRepo.find();
    }
}

function helper() {
    return "test";
}
"#;
        let analysis = analyzer
            .analyze_file("user.service.ts", code, mccp_core::Language::TypeScript)
            .unwrap();

        let names: Vec<&str> = analysis.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"UserService"), "should find UserService: {:?}", names);
        assert!(names.contains(&"helper"), "should find helper: {:?}", names);

        assert!(
            !analysis.import_edges.is_empty(),
            "should have import edges"
        );
    }

    // ── Python analysis tests ────────────────────────────────────────

    #[test]
    fn test_python_class_and_functions() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
from flask import Flask
from models import User

app = Flask(__name__)

class UserService:
    def __init__(self):
        self.users = []

    def find_all(self):
        return self.users

async def fetch_data(url: str) -> dict:
    pass

def helper():
    return 42
"#;
        let analysis = analyzer
            .analyze_file("service.py", code, mccp_core::Language::Python)
            .unwrap();

        let names: Vec<&str> = analysis.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"UserService"), "should find UserService: {:?}", names);
        assert!(names.contains(&"fetch_data"), "should find fetch_data: {:?}", names);
        assert!(names.contains(&"helper"), "should find helper: {:?}", names);
    }

    // ── C# analysis tests ────────────────────────────────────────────

    #[test]
    fn test_csharp_class_and_methods() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
using System;
using System.Collections.Generic;

namespace MyApp.Services
{
    public class UserService
    {
        public User FindById(int id)
        {
            return null;
        }

        private void Validate(User user)
        {
        }
    }

    public interface IUserRepository
    {
        User FindById(int id);
    }

    public enum Status
    {
        Active,
        Inactive
    }
}
"#;
        let analysis = analyzer
            .analyze_file("UserService.cs", code, mccp_core::Language::CSharp)
            .unwrap();

        let names: Vec<&str> = analysis.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"UserService"), "should find UserService: {:?}", names);
        assert!(names.contains(&"FindById"), "should find FindById: {:?}", names);
        assert!(names.contains(&"IUserRepository"), "should find IUserRepository: {:?}", names);
        assert!(names.contains(&"Status"), "should find Status: {:?}", names);

        let iface = analysis.symbols.iter().find(|s| s.name == "IUserRepository").unwrap();
        assert_eq!(iface.kind, SymbolKind::Interface);

        let status = analysis.symbols.iter().find(|s| s.name == "Status").unwrap();
        assert_eq!(status.kind, SymbolKind::Enum);

        // Check imports
        assert!(
            analysis.import_edges.len() >= 2,
            "should have using directives: {:?}",
            analysis.import_edges
        );
    }

    #[test]
    fn test_csharp_namespace_hierarchy() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = r#"
namespace MyApp.Controllers
{
    public class HomeController
    {
        public string Index()
        {
            return "Hello";
        }
    }
}
"#;
        let analysis = analyzer
            .analyze_file("HomeController.cs", code, mccp_core::Language::CSharp)
            .unwrap();

        let ns = analysis.symbols.iter().find(|s| s.kind == SymbolKind::Module);
        assert!(ns.is_some(), "should find namespace as module");

        let controller = analysis.symbols.iter().find(|s| s.name == "HomeController").unwrap();
        assert!(controller.parent_symbol.is_some(), "controller should have parent namespace");
    }

    // ── Framework detection tests ────────────────────────────────────

    #[test]
    fn test_spring_boot_detection() {
        let mut frameworks = Vec::new();
        let code = r#"
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.bind.annotation.GetMapping;

@RestController
public class UserController {
    @GetMapping("/users")
    public List<User> getUsers() {
        return null;
    }
}
"#;
        detect_frameworks("UserController.java", code, &mut frameworks);
        assert!(
            frameworks.iter().any(|f| f.framework == Framework::SpringMVC),
            "should detect Spring MVC: {:?}",
            frameworks
        );
    }

    #[test]
    fn test_express_detection() {
        let mut frameworks = Vec::new();
        let code = r#"
const express = require('express');
const app = express();

app.get('/users', (req, res) => {
    res.json([]);
});

app.post('/users', (req, res) => {
    res.json({});
});
"#;
        detect_frameworks("server.js", code, &mut frameworks);
        assert!(
            frameworks.iter().any(|f| f.framework == Framework::Express),
            "should detect Express: {:?}",
            frameworks
        );
    }

    #[test]
    fn test_fastapi_detection() {
        let mut frameworks = Vec::new();
        let code = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/users")
async def get_users():
    return []

@app.post("/users")
async def create_user(user: User):
    return user
"#;
        detect_frameworks("main.py", code, &mut frameworks);
        assert!(
            frameworks.iter().any(|f| f.framework == Framework::FastAPI),
            "should detect FastAPI: {:?}",
            frameworks
        );
    }

    #[test]
    fn test_nestjs_detection() {
        let mut frameworks = Vec::new();
        let code = r#"
import { Controller, Get, Post } from '@nestjs/common';
import { Injectable } from '@nestjs/common';

@Controller('users')
export class UserController {
    @Get()
    findAll() {}
}
"#;
        detect_frameworks("user.controller.ts", code, &mut frameworks);
        assert!(
            frameworks.iter().any(|f| f.framework == Framework::NestJS),
            "should detect NestJS: {:?}",
            frameworks
        );
    }

    #[test]
    fn test_axum_detection() {
        let mut frameworks = Vec::new();
        let code = r#"
use axum::routing::{get, post};
use axum::Router;

let app = Router::new()
    .route("/users", get(list_users))
    .route("/users", post(create_user));
"#;
        detect_frameworks("main.rs", code, &mut frameworks);
        assert!(
            frameworks.iter().any(|f| f.framework == Framework::Axum),
            "should detect Axum: {:?}",
            frameworks
        );
    }

    #[test]
    fn test_gin_detection() {
        let mut frameworks = Vec::new();
        let code = r#"
package main

import "github.com/gin-gonic/gin"

func main() {
    r := gin.Default()
    r.GET("/users", getUsers)
}
"#;
        detect_frameworks("main.go", code, &mut frameworks);
        assert!(
            frameworks.iter().any(|f| f.framework == Framework::Gin),
            "should detect Gin: {:?}",
            frameworks
        );
    }

    #[test]
    fn test_aspnet_detection() {
        let mut frameworks = Vec::new();
        let code = r#"
using Microsoft.AspNetCore.Mvc;

[ApiController]
[Route("api/[controller]")]
public class UsersController : ControllerBase
{
    [HttpGet]
    public IActionResult GetAll()
    {
        return Ok();
    }
}
"#;
        detect_frameworks("UsersController.cs", code, &mut frameworks);
        assert!(
            frameworks.iter().any(|f| f.framework == Framework::AspNetCore),
            "should detect ASP.NET Core: {:?}",
            frameworks
        );
    }

    #[test]
    fn test_no_framework_false_positive() {
        let mut frameworks = Vec::new();
        let code = r#"
fn main() {
    println!("Hello, world!");
}
"#;
        detect_frameworks("main.rs", code, &mut frameworks);
        assert!(frameworks.is_empty(), "should not detect any framework for simple code");
    }

    // ── Execution flow detection tests ───────────────────────────────

    #[test]
    fn test_flow_detection_from_annotations() {
        let mut symbols = vec![
            {
                let mut s = SymbolDef::new(
                    "getUsers".to_string(),
                    SymbolKind::Method,
                    "controller/UserController.java".to_string(),
                    10, 20,
                );
                s.annotations.push(Annotation {
                    name: "GetMapping".to_string(),
                    arguments: vec!["/users".to_string()],
                    line: 9,
                });
                s
            },
            {
                let mut s = SymbolDef::new(
                    "findAll".to_string(),
                    SymbolKind::Method,
                    "service/UserService.java".to_string(),
                    5, 15,
                );
                s
            },
        ];

        let call_edges = vec![
            CallEdge {
                caller: symbols[0].id.clone(),
                callee: symbols[1].id.clone(),
            },
        ];

        let flows = detect_execution_flows(&symbols, &call_edges);
        assert!(
            !flows.is_empty(),
            "should detect at least one HTTP flow"
        );
        assert_eq!(flows[0].flow_type, FlowType::HttpEndpoint);
        assert!(flows[0].steps.len() >= 1, "flow should have at least entry step");
    }

    #[test]
    fn test_flow_detection_no_entry_points() {
        let symbols = vec![
            SymbolDef::new("helper".to_string(), SymbolKind::Function, "util.rs".to_string(), 1, 10),
            SymbolDef::new("internal".to_string(), SymbolKind::Function, "lib.rs".to_string(), 1, 5),
        ];
        let call_edges = vec![
            CallEdge { caller: symbols[0].id.clone(), callee: symbols[1].id.clone() },
        ];
        let flows = detect_execution_flows(&symbols, &call_edges);
        assert!(flows.is_empty(), "should not detect flows without entry point annotations");
    }

    // ── Architectural layer inference tests ───────────────────────────

    #[test]
    fn test_infer_layer_from_annotations() {
        let mut sym = SymbolDef::new("UserController".to_string(), SymbolKind::Class, "ctrl.java".to_string(), 1, 50);
        sym.annotations.push(Annotation { name: "RestController".to_string(), arguments: vec![], line: 1 });
        assert_eq!(infer_architectural_layer(&sym), ArchitecturalLayer::Controller);

        let mut sym = SymbolDef::new("UserService".to_string(), SymbolKind::Class, "svc.java".to_string(), 1, 50);
        sym.annotations.push(Annotation { name: "Service".to_string(), arguments: vec![], line: 1 });
        assert_eq!(infer_architectural_layer(&sym), ArchitecturalLayer::Service);

        let mut sym = SymbolDef::new("UserRepo".to_string(), SymbolKind::Class, "repo.java".to_string(), 1, 50);
        sym.annotations.push(Annotation { name: "Repository".to_string(), arguments: vec![], line: 1 });
        assert_eq!(infer_architectural_layer(&sym), ArchitecturalLayer::Repository);
    }

    #[test]
    fn test_infer_layer_from_file_path() {
        let sym = SymbolDef::new("Foo".to_string(), SymbolKind::Class, "src/controllers/user.ts".to_string(), 1, 50);
        assert_eq!(infer_architectural_layer(&sym), ArchitecturalLayer::Controller);

        let sym = SymbolDef::new("Bar".to_string(), SymbolKind::Class, "src/services/auth.ts".to_string(), 1, 50);
        assert_eq!(infer_architectural_layer(&sym), ArchitecturalLayer::Service);

        let sym = SymbolDef::new("Baz".to_string(), SymbolKind::Class, "src/models/user.py".to_string(), 1, 50);
        assert_eq!(infer_architectural_layer(&sym), ArchitecturalLayer::Model);

        let sym = SymbolDef::new("Qux".to_string(), SymbolKind::Class, "src/repository/data.go".to_string(), 1, 50);
        assert_eq!(infer_architectural_layer(&sym), ArchitecturalLayer::Repository);
    }

    // ── Codegen pattern detection tests ──────────────────────────────

    #[test]
    fn test_rust_derive_codegen_detection() {
        let mut symbols = vec![{
            let mut s = SymbolDef::new("MyStruct".to_string(), SymbolKind::Struct, "lib.rs".to_string(), 3, 6);
            s.annotations.push(Annotation {
                name: "derive".to_string(),
                arguments: vec!["Debug".to_string(), "Clone".to_string(), "Serialize".to_string()],
                line: 2,
            });
            s
        }];

        let mut patterns = Vec::new();
        detect_rust_derive_codegen(&symbols, &mut patterns);

        assert!(!patterns.is_empty(), "should detect derive codegen");
        assert_eq!(patterns[0].pattern_type, CodegenType::RustDerive);
        assert!(patterns[0].generated_members.iter().any(|g| g.contains("Debug")));
        assert!(patterns[0].generated_members.iter().any(|g| g.contains("clone")));
    }

    // ── Project structure tests ──────────────────────────────────────

    #[test]
    fn test_build_project_structure() {
        let symbols = vec![
            {
                let mut s = SymbolDef::new("main".to_string(), SymbolKind::Function, "src/main.rs".to_string(), 1, 10);
                s.language = Some("rust".to_string());
                s
            },
            {
                let mut s = SymbolDef::new("UserService".to_string(), SymbolKind::Class, "src/service.ts".to_string(), 1, 50);
                s.language = Some("typescript".to_string());
                s
            },
        ];
        let imports = vec![
            ImportEdge {
                from_file: "src/main.rs".to_string(),
                to_file: "src/lib.rs".to_string(),
                symbol: None,
            },
        ];
        let lang_files: HashMap<String, usize> = [("rust".to_string(), 5), ("typescript".to_string(), 3)].into();
        let lang_lines: HashMap<String, usize> = [("rust".to_string(), 500), ("typescript".to_string(), 300)].into();

        let structure = build_project_structure(
            std::path::Path::new("/tmp"),
            &symbols,
            &imports,
            &lang_files,
            &lang_lines,
        );

        assert!(!structure.modules.is_empty(), "should have modules");
        assert!(!structure.language_stats.is_empty(), "should have language stats");
        assert!(
            structure.language_stats.contains_key("rust"),
            "should have rust stats"
        );
        assert!(
            structure.language_stats.contains_key("typescript"),
            "should have typescript stats"
        );
    }

    // ── Multi-language project tests ─────────────────────────────────

    #[test]
    fn test_multi_language_same_project() {
        let analyzer = TreeSitterAnalyzer::new();

        // Analyze Rust file
        let rust_code = "pub fn rust_fn() { go_fn(); }";
        let rust = analyzer.analyze_file("src/main.rs", rust_code, mccp_core::Language::Rust).unwrap();
        assert!(!rust.symbols.is_empty());

        // Analyze Java file
        let java_code = "public class JavaClass { public void method() {} }";
        let java = analyzer.analyze_file("src/Main.java", java_code, mccp_core::Language::Java).unwrap();
        assert!(!java.symbols.is_empty());

        // Analyze Go file
        let go_code = "package main\nfunc GoFunc() {}";
        let go = analyzer.analyze_file("main.go", go_code, mccp_core::Language::Go).unwrap();
        assert!(!go.symbols.is_empty());

        // Analyze Python file
        let py_code = "def python_func():\n    pass\nclass PyClass:\n    pass";
        let py = analyzer.analyze_file("main.py", py_code, mccp_core::Language::Python).unwrap();
        assert!(!py.symbols.is_empty());

        // All should produce distinct symbols
        let all_names: Vec<&str> = rust.symbols.iter()
            .chain(java.symbols.iter())
            .chain(go.symbols.iter())
            .chain(py.symbols.iter())
            .map(|s| s.name.as_str())
            .collect();
        assert!(all_names.contains(&"rust_fn"));
        assert!(all_names.contains(&"JavaClass"));
        assert!(all_names.contains(&"GoFunc"));
        assert!(all_names.contains(&"python_func"));
    }

    // ── Column information tests ─────────────────────────────────────

    #[test]
    fn test_java_column_info() {
        let analyzer = TreeSitterAnalyzer::new();
        let code = "public class Foo {\n    public void bar() {}\n}";
        let analysis = analyzer.analyze_file("Foo.java", code, mccp_core::Language::Java).unwrap();

        let foo = analysis.symbols.iter().find(|s| s.name == "Foo").unwrap();
        assert_eq!(foo.start_line, 1);
        // start_column should be set
        assert_eq!(foo.start_column, 0);
    }

    // ── Default skip dirs test ───────────────────────────────────────

    #[test]
    fn test_default_skip_dirs_list() {
        use super::super::pipeline::DEFAULT_SKIP_DIRS;
        assert!(DEFAULT_SKIP_DIRS.contains(&".git"), "should skip .git");
        assert!(DEFAULT_SKIP_DIRS.contains(&"target"), "should skip target");
        assert!(DEFAULT_SKIP_DIRS.contains(&"dist"), "should skip dist");
        assert!(DEFAULT_SKIP_DIRS.contains(&"node_modules"), "should skip node_modules");
        assert!(DEFAULT_SKIP_DIRS.contains(&"build"), "should skip build");
        assert!(DEFAULT_SKIP_DIRS.contains(&".idea"), "should skip .idea");
        assert!(DEFAULT_SKIP_DIRS.contains(&".vscode"), "should skip .vscode");
        assert!(DEFAULT_SKIP_DIRS.contains(&"__pycache__"), "should skip __pycache__");
        assert!(DEFAULT_SKIP_DIRS.contains(&"vendor"), "should skip vendor");
        assert!(DEFAULT_SKIP_DIRS.contains(&"bin"), "should skip bin");
        assert!(DEFAULT_SKIP_DIRS.contains(&"obj"), "should skip obj");
    }

    // ── Snapshot serialization round-trip test ───────────────────────

    #[test]
    fn test_snapshot_serialization_roundtrip() {
        let mut snap = CodeIntelSnapshot::new("test-project".to_string());
        snap.symbols.push({
            let mut s = SymbolDef::new("test_fn".to_string(), SymbolKind::Function, "main.rs".to_string(), 1, 10);
            s.annotations.push(Annotation { name: "test".to_string(), arguments: vec![], line: 1 });
            s.language = Some("rust".to_string());
            s.references.push(SymbolRef {
                file: "lib.rs".to_string(),
                line: 5,
                column: 10,
                end_line: 5,
                end_column: 17,
                context: "test_fn()".to_string(),
                ref_kind: Some("call".to_string()),
            });
            s
        });
        snap.flows.push(ExecutionFlow {
            id: "flow-1".to_string(),
            name: "test flow".to_string(),
            flow_type: FlowType::HttpEndpoint,
            steps: vec![FlowStep {
                symbol_id: "test".to_string(),
                file: "main.rs".to_string(),
                start_line: 1,
                end_line: 10,
                layer: ArchitecturalLayer::Controller,
                description: "entry".to_string(),
                annotations: vec!["GetMapping".to_string()],
            }],
            entry_file: "main.rs".to_string(),
            entry_line: 1,
        });
        snap.codegen_patterns.push(CodegenPattern {
            file: "model.rs".to_string(),
            line: 1,
            pattern_type: CodegenType::RustDerive,
            generated_members: vec!["Debug impl".to_string()],
            source_annotation: "#[derive(Debug)]".to_string(),
        });
        snap.frameworks.push(FrameworkInfo {
            framework: Framework::Axum,
            version: None,
            file: "main.rs".to_string(),
            detected_patterns: vec!["Router::new".to_string()],
        });

        let json = serde_json::to_string(&snap).unwrap();
        let loaded: CodeIntelSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.project_id, snap.project_id);
        assert_eq!(loaded.symbols.len(), 1);
        assert_eq!(loaded.flows.len(), 1);
        assert_eq!(loaded.codegen_patterns.len(), 1);
        assert_eq!(loaded.frameworks.len(), 1);
        assert_eq!(loaded.symbols[0].references.len(), 1);
        assert_eq!(loaded.symbols[0].references[0].column, 10);
    }

    // ── Incremental update test ──────────────────────────────────────

    #[test]
    fn test_incremental_update() {
        let mut snap = CodeIntelSnapshot::new("test".to_string());
        snap.symbols.push(SymbolDef::new("old_fn".to_string(), SymbolKind::Function, "a.rs".to_string(), 1, 5));
        snap.symbols.push(SymbolDef::new("keep_fn".to_string(), SymbolKind::Function, "b.rs".to_string(), 1, 5));

        let mut partial = CodeIntelSnapshot::new("test".to_string());
        partial.symbols.push(SymbolDef::new("new_fn".to_string(), SymbolKind::Function, "a.rs".to_string(), 1, 10));

        snap.incremental_update(&["a.rs".to_string()], partial);

        let names: Vec<&str> = snap.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"new_fn"), "should have new_fn: {:?}", names);
        assert!(names.contains(&"keep_fn"), "should keep keep_fn: {:?}", names);
        assert!(!names.contains(&"old_fn"), "should remove old_fn: {:?}", names);
    }

    // ── Helper function tests ────────────────────────────────────────

    #[test]
    fn test_get_line_context() {
        let source = "line one\nline two\nline three\n";
        assert_eq!(get_line_context(source, 1), "line one");
        assert_eq!(get_line_context(source, 2), "line two");
        assert_eq!(get_line_context(source, 3), "line three");
        assert_eq!(get_line_context(source, 0), "line one");  // saturating_sub
    }

    #[test]
    fn test_language_stats_in_project_structure() {
        let symbols = vec![
            { let mut s = SymbolDef::new("fn1".to_string(), SymbolKind::Function, "a.rs".to_string(), 1, 5); s.language = Some("rust".to_string()); s },
            { let mut s = SymbolDef::new("fn2".to_string(), SymbolKind::Function, "b.rs".to_string(), 1, 5); s.language = Some("rust".to_string()); s },
            { let mut s = SymbolDef::new("Cls".to_string(), SymbolKind::Class, "c.java".to_string(), 1, 50); s.language = Some("java".to_string()); s },
        ];
        let lang_files = [("rust".to_string(), 10usize), ("java".to_string(), 5usize)].into();
        let lang_lines = [("rust".to_string(), 1000usize), ("java".to_string(), 500usize)].into();

        let structure = build_project_structure(
            std::path::Path::new("/tmp"),
            &symbols,
            &[],
            &lang_files,
            &lang_lines,
        );

        let rust_stats = structure.language_stats.get("rust").unwrap();
        assert_eq!(rust_stats.file_count, 10);
        assert_eq!(rust_stats.line_count, 1000);
        assert_eq!(rust_stats.function_count, 2);

        let java_stats = structure.language_stats.get("java").unwrap();
        assert_eq!(java_stats.file_count, 5);
        assert_eq!(java_stats.class_count, 1);
    }
}
