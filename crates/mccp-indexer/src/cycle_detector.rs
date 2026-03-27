use mccp_core::CodeIntelSnapshot;
use serde::Serialize;
use std::collections::HashMap;

const WHITE: u8 = 0;
const GRAY: u8 = 1;
const BLACK: u8 = 2;

/// Report of detected cycles in the codebase
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CycleReport {
    pub call_cycles: Vec<Vec<String>>,
    pub import_cycles: Vec<Vec<String>>,
}

impl CycleReport {
    pub fn is_empty(&self) -> bool {
        self.call_cycles.is_empty() && self.import_cycles.is_empty()
    }

    pub fn total_cycles(&self) -> usize {
        self.call_cycles.len() + self.import_cycles.len()
    }
}

/// Detects circular dependencies in call graphs and import graphs
pub struct CycleDetector;

impl CycleDetector {
    /// Detect cycles in function call edges
    pub fn detect_call_cycles(snapshot: &CodeIntelSnapshot) -> CycleReport {
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &snapshot.call_edges {
            adj.entry(edge.caller.clone())
                .or_default()
                .push(edge.callee.clone());
        }

        // Collect all nodes
        let mut all_nodes: Vec<String> = adj.keys().cloned().collect();
        for targets in adj.values() {
            for t in targets {
                if !all_nodes.contains(t) {
                    all_nodes.push(t.clone());
                }
            }
        }

        let mut colors: HashMap<String, u8> = all_nodes.iter().map(|n| (n.clone(), WHITE)).collect();
        let mut cycles = Vec::new();

        for node in &all_nodes {
            if colors[node] == WHITE {
                let mut path = Vec::new();
                Self::dfs(node, &adj, &mut colors, &mut path, &mut cycles);
            }
        }

        CycleReport {
            call_cycles: cycles,
            import_cycles: Vec::new(),
        }
    }

    /// Detect cycles in file import edges
    pub fn detect_import_cycles(snapshot: &CodeIntelSnapshot) -> CycleReport {
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &snapshot.import_edges {
            adj.entry(edge.from_file.clone())
                .or_default()
                .push(edge.to_file.clone());
        }

        let mut all_nodes: Vec<String> = adj.keys().cloned().collect();
        for targets in adj.values() {
            for t in targets {
                if !all_nodes.contains(t) {
                    all_nodes.push(t.clone());
                }
            }
        }

        let mut colors: HashMap<String, u8> = all_nodes.iter().map(|n| (n.clone(), WHITE)).collect();
        let mut cycles = Vec::new();

        for node in &all_nodes {
            if colors[node] == WHITE {
                let mut path = Vec::new();
                Self::dfs(node, &adj, &mut colors, &mut path, &mut cycles);
            }
        }

        CycleReport {
            call_cycles: Vec::new(),
            import_cycles: cycles,
        }
    }

    /// Detect both call and import cycles in one pass
    pub fn detect_all(snapshot: &CodeIntelSnapshot) -> CycleReport {
        let call_report = Self::detect_call_cycles(snapshot);
        let import_report = Self::detect_import_cycles(snapshot);

        CycleReport {
            call_cycles: call_report.call_cycles,
            import_cycles: import_report.import_cycles,
        }
    }

    /// DFS with color-marking for cycle detection
    fn dfs(
        node: &str,
        edges: &HashMap<String, Vec<String>>,
        colors: &mut HashMap<String, u8>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        colors.insert(node.to_string(), GRAY);
        path.push(node.to_string());

        if let Some(neighbors) = edges.get(node) {
            for neighbor in neighbors {
                let color = *colors.get(neighbor).unwrap_or(&WHITE);
                match color {
                    WHITE => {
                        Self::dfs(neighbor, edges, colors, path, cycles);
                    }
                    GRAY => {
                        // Back-edge detected — extract the cycle
                        if let Some(start_idx) = path.iter().position(|n| n == neighbor) {
                            let mut cycle: Vec<String> = path[start_idx..].to_vec();
                            cycle.push(neighbor.clone()); // close the cycle
                            cycles.push(cycle);
                        }
                    }
                    BLACK => {
                        // Already fully processed, no cycle here
                    }
                    _ => {}
                }
            }
        }

        path.pop();
        colors.insert(node.to_string(), BLACK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mccp_core::CodeIntelSnapshot;

    #[test]
    fn test_detect_call_cycle() {
        let mut snapshot = CodeIntelSnapshot::new("test".to_string());
        // a → b → c → a (cycle)
        snapshot.call_edges.push(CallEdge {
            caller: "a".to_string(),
            callee: "b".to_string(),
        });
        snapshot.call_edges.push(CallEdge {
            caller: "b".to_string(),
            callee: "c".to_string(),
        });
        snapshot.call_edges.push(CallEdge {
            caller: "c".to_string(),
            callee: "a".to_string(),
        });

        let report = CycleDetector::detect_call_cycles(&snapshot);
        assert!(
            !report.call_cycles.is_empty(),
            "should detect cycle: {:?}",
            report
        );

        // The cycle should contain all 3 nodes + closing node
        let cycle = &report.call_cycles[0];
        assert!(cycle.contains(&"a".to_string()));
        assert!(cycle.contains(&"b".to_string()));
        assert!(cycle.contains(&"c".to_string()));
    }

    #[test]
    fn test_no_cycle_acyclic_graph() {
        let mut snapshot = CodeIntelSnapshot::new("test".to_string());
        // a → b → c (no cycle)
        snapshot.call_edges.push(CallEdge {
            caller: "a".to_string(),
            callee: "b".to_string(),
        });
        snapshot.call_edges.push(CallEdge {
            caller: "b".to_string(),
            callee: "c".to_string(),
        });

        let report = CycleDetector::detect_call_cycles(&snapshot);
        assert!(
            report.call_cycles.is_empty(),
            "should not detect cycle in acyclic graph: {:?}",
            report
        );
    }

    #[test]
    fn test_detect_import_cycle() {
        let mut snapshot = CodeIntelSnapshot::new("test".to_string());
        // a.rs imports b.rs, b.rs imports a.rs
        snapshot.import_edges.push(ImportEdge {
            from_file: "a.rs".to_string(),
            to_file: "b.rs".to_string(),
            symbol: None,
        });
        snapshot.import_edges.push(ImportEdge {
            from_file: "b.rs".to_string(),
            to_file: "a.rs".to_string(),
            symbol: None,
        });

        let report = CycleDetector::detect_import_cycles(&snapshot);
        assert!(
            !report.import_cycles.is_empty(),
            "should detect import cycle: {:?}",
            report
        );
    }

    #[test]
    fn test_detect_all() {
        let mut snapshot = CodeIntelSnapshot::new("test".to_string());
        snapshot.call_edges.push(CallEdge {
            caller: "a".to_string(),
            callee: "b".to_string(),
        });
        snapshot.call_edges.push(CallEdge {
            caller: "b".to_string(),
            callee: "a".to_string(),
        });
        snapshot.import_edges.push(ImportEdge {
            from_file: "x.rs".to_string(),
            to_file: "y.rs".to_string(),
            symbol: None,
        });

        let report = CycleDetector::detect_all(&snapshot);
        assert!(!report.call_cycles.is_empty());
        assert!(report.import_cycles.is_empty());
    }

    #[test]
    fn test_large_graph_performance() {
        let mut snapshot = CodeIntelSnapshot::new("test".to_string());

        // Create a large DAG (1000 nodes, no cycles)
        for i in 0..1000 {
            snapshot.call_edges.push(CallEdge {
                caller: format!("node_{}", i),
                callee: format!("node_{}", i + 1),
            });
        }

        let start = std::time::Instant::now();
        let report = CycleDetector::detect_call_cycles(&snapshot);
        let elapsed = start.elapsed();

        assert!(report.call_cycles.is_empty());
        assert!(
            elapsed.as_millis() < 100,
            "large graph took too long: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_self_cycle() {
        let mut snapshot = CodeIntelSnapshot::new("test".to_string());
        // a → a (self-recursion)
        snapshot.call_edges.push(CallEdge {
            caller: "a".to_string(),
            callee: "a".to_string(),
        });

        let report = CycleDetector::detect_call_cycles(&snapshot);
        assert!(
            !report.call_cycles.is_empty(),
            "should detect self-cycle: {:?}",
            report
        );
    }

    #[test]
    fn test_cycle_report_helpers() {
        let report = CycleReport {
            call_cycles: vec![vec!["a".to_string(), "b".to_string(), "a".to_string()]],
            import_cycles: Vec::new(),
        };
        assert!(!report.is_empty());
        assert_eq!(report.total_cycles(), 1);

        let empty = CycleReport {
            call_cycles: Vec::new(),
            import_cycles: Vec::new(),
        };
        assert!(empty.is_empty());
        assert_eq!(empty.total_cycles(), 0);
    }
}
