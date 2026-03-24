use super::*;
use mccp_core::*;

/// Graph builder for constructing call graphs from source files
#[derive(Debug, Clone)]
pub struct GraphBuilder {
    /// Graph store for managing the call graph
    graph_store: GraphStore,
}

impl GraphBuilder {
    /// Create a new graph builder
    pub fn new() -> Self {
        Self {
            graph_store: GraphStore::new(),
        }
    }

    /// Build a call graph from source files
    pub fn build_graph(&self, files: &[SourceFile]) -> GraphStore {
        let store = GraphStore::new();

        for file in files {
            let symbols = SymbolExtractor::extract_symbols(file);
            
            // Add nodes for all symbols
            for symbol in &symbols {
                let node = GraphNode::new(
                    symbol.name.clone(),
                    symbol.kind,
                    symbol.file_path.clone(),
                    symbol.line,
                    symbol.column,
                    "unknown".to_string(), // TODO: Get actual project ID
                );
                store.add_node(node);
            }

            // Add edges based on symbol references
            self.add_edges_from_symbols(&store, &symbols);
        }

        store
    }

    /// Add edges based on symbol references
    fn add_edges_from_symbols(&self, store: &GraphStore, symbols: &[Symbol]) {
        for symbol in symbols {
            // Look for function calls in the context snippet
            if let Some(calls) = self.extract_function_calls(&symbol.context_snippet) {
                for call in calls {
                    let from_id = format!("{}:{}:{}", symbol.file_path, symbol.line, symbol.name);
                    let to_id = format!("{}:{}:{}", symbol.file_path, symbol.line, call);
                    
                    store.add_edge(&from_id, &to_id, EdgeKind::Call, "unknown");
                }
            }

            // Look for imports
            if let Some(imports) = self.extract_imports(&symbol.context_snippet) {
                for import in imports {
                    let from_id = format!("{}:{}:{}", symbol.file_path, symbol.line, symbol.name);
                    let to_id = format!("{}:{}:{}", import, 0, symbol.name);
                    
                    store.add_edge(&from_id, &to_id, EdgeKind::Import, "unknown");
                }
            }
        }
    }

    /// Extract function calls from code
    fn extract_function_calls(&self, code: &str) -> Option<Vec<String>> {
        let patterns = [
            r"\b(\w+)\s*\(", // Function calls
            r"\w+::(\w+)\s*\(", // Method calls
        ];

        let mut calls = Vec::new();
        
        for pattern in &patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                for cap in re.captures_iter(code) {
                    if let Some(call) = cap.get(1) {
                        calls.push(call.as_str().to_string());
                    }
                }
            }
        }

        if calls.is_empty() {
            None
        } else {
            Some(calls)
        }
    }

    /// Extract imports from code
    fn extract_imports(&self, code: &str) -> Option<Vec<String>> {
        let patterns = [
            r"use\s+([\w:]+)",
            r"import\s+[\w\*]+\s+from\s+['\"]([^'\"]+)['\"]",
            r"from\s+['\"]([^'\"]+)['\"]\s+import",
            r"#include\s+[<\"]([^>\"]+)[>\"]",
        ];

        let mut imports = Vec::new();
        
        for pattern in &patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                for cap in re.captures_iter(code) {
                    if let Some(import) = cap.get(1) {
                        imports.push(import.as_str().to_string());
                    }
                }
            }
        }

        if imports.is_empty() {
            None
        } else {
            Some(imports)
        }
    }

    /// Add symbols to the graph
    pub fn add_symbols(&self, file_path: &str, symbols: Vec<Symbol>) {
        for symbol in symbols {
            let node = GraphNode::new(
                symbol.name.clone(),
                symbol.kind,
                file_path.to_string(),
                symbol.line,
                symbol.column,
                "unknown".to_string(), // TODO: Get actual project ID
            );
            self.graph_store.add_node(node);
        }
    }

    /// Get the graph store
    pub fn graph_store(&self) -> &GraphStore {
        &self.graph_store
    }

    /// Get graph statistics
    pub fn stats(&self) -> GraphStats {
        let nodes = self.graph_store.all_nodes();
        let edges = self.graph_store.all_edges();
        
        GraphStats {
            total_nodes: nodes.len(),
            total_edges: edges.len(),
            avg_degree: if nodes.is_empty() { 0.0 } else { edges.len() as f32 / nodes.len() as f32 },
        }
    }
}

/// Graph statistics
#[derive(Debug, Clone)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub avg_degree: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_graph_builder_creation() {
        let graph_builder = GraphBuilder::new();
        
        assert_eq!(graph_builder.graph_store().all_nodes().len(), 0);
        assert_eq!(graph_builder.graph_store().all_edges().len(), 0);
    }

    #[test]
    fn test_build_graph() {
        let graph_builder = GraphBuilder::new();
        let files = vec![]; // Empty for testing
        
        let graph = graph_builder.build_graph(&files);
        
        assert_eq!(graph.all_nodes().len(), 0);
        assert_eq!(graph.all_edges().len(), 0);
    }

    #[test]
    fn test_extract_function_calls() {
        let graph_builder = GraphBuilder::new();
        let code = "fn main() { helper(); other::func(); }";
        
        let calls = graph_builder.extract_function_calls(code);
        
        assert!(calls.is_some());
        let calls = calls.unwrap();
        assert!(calls.contains(&"main".to_string()));
        assert!(calls.contains(&"helper".to_string()));
        assert!(calls.contains(&"func".to_string()));
    }

    #[test]
    fn test_extract_imports() {
        let graph_builder = GraphBuilder::new();
        let code = r#"
            use std::collections::HashMap;
            import React from 'react';
            from typing import List;
            #include <stdio.h>
        "#;
        
        let imports = graph_builder.extract_imports(code);
        
        assert!(imports.is_some());
        let imports = imports.unwrap();
        assert!(imports.contains(&"std::collections::HashMap".to_string()));
        assert!(imports.contains(&"react".to_string()));
        assert!(imports.contains(&"typing".to_string()));
        assert!(imports.contains(&"stdio.h".to_string()));
    }

    #[test]
    fn test_add_symbols() {
        let graph_builder = GraphBuilder::new();
        
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
        ];
        
        graph_builder.add_symbols("src/main.rs", symbols);
        
        let nodes = graph_builder.graph_store().all_nodes();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "main");
    }

    #[test]
    fn test_graph_stats() {
        let graph_builder = GraphBuilder::new();
        
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
                "helper".to_string(),
                SymbolKind::Function,
                "helper".to_string(),
                "src/helper.rs".to_string(),
                1,
                0,
                "fn helper() {}".to_string(),
                "module".to_string(),
                Language::Rust,
                RefKind::Definition,
            ),
        ];
        
        graph_builder.add_symbols("src/main.rs", symbols);
        
        let stats = graph_builder.stats();
        
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.total_edges, 0);
        assert_eq!(stats.avg_degree, 0.0);
    }
}