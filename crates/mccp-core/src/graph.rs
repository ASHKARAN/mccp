use super::*;

/// A node in the call graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub project_id: String,
}

impl GraphNode {
    /// Create a new graph node
    pub fn new(
        name: String,
        kind: SymbolKind,
        file_path: String,
        line: usize,
        column: usize,
        project_id: String,
    ) -> Self {
        let id = format!("{}:{}:{}", project_id, file_path, name);
        Self {
            id,
            name,
            kind,
            file_path,
            line,
            column,
            project_id,
        }
    }

    /// Get the node's qualified name
    pub fn qualified_name(&self) -> String {
        format!("{}::{}", self.file_path, self.name)
    }
}

/// An edge in the call graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    pub project_id: String,
}

impl GraphEdge {
    /// Create a new graph edge
    pub fn new(from: String, to: String, kind: EdgeKind, project_id: String) -> Self {
        Self {
            from,
            to,
            kind,
            project_id,
        }
    }
}

/// Graph store for managing call graphs
#[derive(Debug)]
pub struct GraphStore {
    nodes: dashmap::DashMap<String, GraphNode>,
    edges: dashmap::DashMap<String, Vec<GraphEdge>>,
}

impl GraphStore {
    /// Create a new graph store
    pub fn new() -> Self {
        Self {
            nodes: dashmap::DashMap::new(),
            edges: dashmap::DashMap::new(),
        }
    }

    /// Add a node to the graph
    pub fn add_node(&self, node: GraphNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    /// Add an edge to the graph
    pub fn add_edge(&self, from: &str, to: &str, kind: EdgeKind, project_id: &str) {
        let edge = GraphEdge::new(from.to_string(), to.to_string(), kind, project_id.to_string());
        
        // Add edge from -> to
        self.edges.entry(from.to_string())
            .or_insert_with(Vec::new)
            .push(edge.clone());
    }

    /// Get a node by ID
    pub fn get_node(&self, id: &str) -> Option<GraphNode> {
        self.nodes.get(id).map(|n| n.clone())
    }

    /// Get all edges from a node
    pub fn get_edges(&self, node_id: &str) -> Vec<GraphEdge> {
        self.edges.get(node_id)
            .map(|edges| edges.clone())
            .unwrap_or_default()
    }

    /// Check if an edge exists
    pub fn has_edge(&self, from: &str, to: &str) -> bool {
        self.edges.get(from)
            .map(|edges| edges.iter().any(|e| e.to == to))
            .unwrap_or(false)
    }

    /// Get all nodes
    pub fn all_nodes(&self) -> Vec<GraphNode> {
        self.nodes.iter().map(|n| n.clone()).collect()
    }

    /// Get all edges
    pub fn all_edges(&self) -> Vec<GraphEdge> {
        self.edges.iter()
            .flat_map(|edges| edges.clone())
            .collect()
    }

    /// Traverse the graph from a starting node
    pub fn traverse(&self, start_node: &str, config: TraversalConfig) -> Vec<String> {
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        let mut result = Vec::new();

        if config.include_self {
            if let Some(node) = self.nodes.get(start_node) {
                result.push(node.id.clone());
                visited.insert(node.id.clone());
            }
        }

        queue.push_back(start_node.to_string());

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current.clone()) {
                continue;
            }

            if result.len() >= config.max_depth {
                break;
            }

            if let Some(edges) = self.edges.get(&current) {
                for edge in edges {
                    if config.edge_kinds.contains(&edge.kind) {
                        if visited.insert(edge.to.clone()) {
                            result.push(edge.to.clone());
                            queue.push_back(edge.to.clone());
                        }
                    }
                }
            }
        }

        result
    }

    /// Get entry points (nodes with no incoming edges)
    pub fn entry_points(&self) -> Vec<String> {
        let all_nodes: std::collections::HashSet<_> = self.nodes.iter().map(|n| n.id.clone()).collect();
        let target_nodes: std::collections::HashSet<_> = self.edges.iter()
            .flat_map(|edges| edges.iter().map(|e| e.to.clone()))
            .collect();

        all_nodes.difference(&target_nodes)
            .cloned()
            .collect()
    }

    /// Get leaf nodes (nodes with no outgoing edges)
    pub fn leaf_nodes(&self) -> Vec<String> {
        let source_nodes: std::collections::HashSet<_> = self.edges.iter().map(|e| e.from.clone()).collect();
        let all_nodes: std::collections::HashSet<_> = self.nodes.iter().map(|n| n.id.clone()).collect();

        all_nodes.difference(&source_nodes)
            .cloned()
            .collect()
    }

    /// Get nodes with high betweenness centrality (bottlenecks)
    pub fn bottlenecks(&self, limit: usize) -> Vec<String> {
        // Simple approximation: nodes with many outgoing edges
        let mut node_degrees: Vec<(String, usize)> = self.edges.iter()
            .map(|(node_id, edges)| (node_id.clone(), edges.len()))
            .collect();

        node_degrees.sort_by(|a, b| b.1.cmp(&a.1));
        node_degrees.into_iter()
            .take(limit)
            .map(|(id, _)| id)
            .collect()
    }

    /// Get neighbors of a node
    pub fn neighbours(&self, node_id: &str) -> Vec<String> {
        self.edges.get(node_id)
            .map(|edges| edges.iter().map(|e| e.to.clone()).collect())
            .unwrap_or_default()
    }
}

/// Multi-project graph store
#[derive(Debug)]
pub struct MultiProjectGraphStore {
    stores: dashmap::DashMap<String, GraphStore>,
}

impl MultiProjectGraphStore {
    /// Create a new multi-project graph store
    pub fn new() -> Self {
        Self {
            stores: dashmap::DashMap::new(),
        }
    }

    /// Get or create a graph store for a project
    pub fn get_or_create(&self, project_id: &str) -> GraphStore {
        self.stores.entry(project_id.to_string())
            .or_insert_with(GraphStore::new)
            .clone()
    }

    /// Add an edge to a specific project
    pub fn add_edge(&self, project_id: &str, from: &str, to: &str, kind: EdgeKind) {
        let store = self.get_or_create(project_id);
        store.add_edge(from, to, kind, project_id);
    }

    /// Check if an edge exists in a specific project
    pub fn has_edge(&self, project_id: &str, from: &str, to: &str) -> bool {
        if let Some(store) = self.stores.get(project_id) {
            store.has_edge(from, to)
        } else {
            false
        }
    }

    /// Get all edges for a specific project
    pub fn all_edges(&self, project_id: &str) -> Vec<GraphEdge> {
        if let Some(store) = self.stores.get(project_id) {
            store.all_edges()
        } else {
            Vec::new()
        }
    }

    /// Traverse a specific project's graph
    pub fn traverse(&self, project_id: &str, start_node: &str, config: TraversalConfig) -> Vec<String> {
        if let Some(store) = self.stores.get(project_id) {
            store.traverse(start_node, config)
        } else {
            Vec::new()
        }
    }
}

/// Graph builder for constructing call graphs from source files
pub struct GraphBuilder;

impl GraphBuilder {
    /// Build a call graph from source files
    pub fn build_graph(files: &[SourceFile]) -> GraphStore {
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
            Self::add_edges_from_symbols(&store, &symbols);
        }

        store
    }

    /// Add edges based on symbol references
    fn add_edges_from_symbols(store: &GraphStore, symbols: &[Symbol]) {
        for symbol in symbols {
            // Look for function calls in the context snippet
            if let Some(calls) = Self::extract_function_calls(&symbol.context_snippet) {
                for call in calls {
                    let from_id = format!("{}:{}:{}", symbol.file_path, symbol.line, symbol.name);
                    let to_id = format!("{}:{}:{}", symbol.file_path, symbol.line, call);
                    
                    store.add_edge(&from_id, &to_id, EdgeKind::Call, "unknown");
                }
            }

            // Look for imports
            if let Some(imports) = Self::extract_imports(&symbol.context_snippet) {
                for import in imports {
                    let from_id = format!("{}:{}:{}", symbol.file_path, symbol.line, symbol.name);
                    let to_id = format!("{}:{}:{}", import, 0, symbol.name);
                    
                    store.add_edge(&from_id, &to_id, EdgeKind::Import, "unknown");
                }
            }
        }
    }

    /// Extract function calls from code
    fn extract_function_calls(code: &str) -> Option<Vec<String>> {
        let re = regex::Regex::new(r"\b(\w+)\s*\(").ok()?;
        let calls: Vec<String> = re.captures_iter(code)
            .map(|cap| cap[1].to_string())
            .collect();
        
        if calls.is_empty() {
            None
        } else {
            Some(calls)
        }
    }

    /// Extract imports from code
    fn extract_imports(code: &str) -> Option<Vec<String>> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_graph_node_creation() {
        let node = GraphNode::new(
            "main".to_string(),
            SymbolKind::Function,
            "src/main.rs".to_string(),
            1,
            0,
            "proj_123".to_string(),
        );

        assert_eq!(node.name, "main");
        assert_eq!(node.kind, SymbolKind::Function);
        assert_eq!(node.qualified_name(), "src/main.rs::main");
    }

    #[test]
    fn test_graph_store() {
        let store = GraphStore::new();
        
        let node1 = GraphNode::new("main".to_string(), SymbolKind::Function, "src/main.rs".to_string(), 1, 0, "proj_123".to_string());
        let node2 = GraphNode::new("helper".to_string(), SymbolKind::Function, "src/helper.rs".to_string(), 1, 0, "proj_123".to_string());
        
        store.add_node(node1.clone());
        store.add_node(node2.clone());
        
        store.add_edge(&node1.id, &node2.id, EdgeKind::Call, "proj_123");
        
        assert!(store.has_edge(&node1.id, &node2.id));
        assert_eq!(store.get_edges(&node1.id).len(), 1);
    }

    #[test]
    fn test_graph_traversal() {
        let store = GraphStore::new();
        
        let node1 = GraphNode::new("A".to_string(), SymbolKind::Function, "src/a.rs".to_string(), 1, 0, "proj_123".to_string());
        let node2 = GraphNode::new("B".to_string(), SymbolKind::Function, "src/b.rs".to_string(), 1, 0, "proj_123".to_string());
        let node3 = GraphNode::new("C".to_string(), SymbolKind::Function, "src/c.rs".to_string(), 1, 0, "proj_123".to_string());
        
        store.add_node(node1.clone());
        store.add_node(node2.clone());
        store.add_node(node3.clone());
        
        store.add_edge(&node1.id, &node2.id, EdgeKind::Call, "proj_123");
        store.add_edge(&node2.id, &node3.id, EdgeKind::Call, "proj_123");
        
        let config = TraversalConfig {
            max_depth: 3,
            include_self: true,
            edge_kinds: vec![EdgeKind::Call],
        };
        
        let result = store.traverse(&node1.id, config);
        
        assert!(result.contains(&node1.id));
        assert!(result.contains(&node2.id));
        assert!(result.contains(&node3.id));
    }

    #[test]
    fn test_multi_project_graph_store() {
        let store = MultiProjectGraphStore::new();
        
        store.add_edge("proj_a", "A", "B", EdgeKind::Call);
        store.add_edge("proj_b", "X", "Y", EdgeKind::Call);
        
        assert!(store.has_edge("proj_a", "A", "B"));
        assert!(!store.has_edge("proj_b", "A", "B"));
        
        let edges_a = store.all_edges("proj_a");
        let edges_b = store.all_edges("proj_b");
        
        assert_eq!(edges_a.len(), 1);
        assert_eq!(edges_b.len(), 1);
    }
}