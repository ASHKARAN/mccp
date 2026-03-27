use mccp_core::*;
use mccp_indexer::*;
use serde::{Deserialize, Serialize};

/// Tool input for getting indexing status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetIndexStatusInput {}

/// Tool input for searching symbols
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSymbolsInput {
    pub project_id: String,
    pub query: String,
}

/// Tool input for listing providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListProvidersInput {}

/// Tool input for clearing a project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearProjectInput {
    pub project_id: String,
}

/// Indexer tool wrapping the pipeline
#[derive(Debug, Clone)]
pub struct IndexerTool {
    pub pipeline: std::sync::Arc<IndexingPipeline>,
}

impl IndexerTool {
    pub fn new(pipeline: std::sync::Arc<IndexingPipeline>) -> Self {
        Self { pipeline }
    }

    /// Get current indexing status
    pub fn status(&self) -> IndexingStatus {
        self.pipeline.status()
    }
}

// ── V4-3 MCP Tool inputs ────────────────────────────────────────────

/// Tool input for get_callers — who calls this function?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCallersInput {
    pub symbol: String,
}

/// Tool input for get_callees — what does this function call?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCalleesInput {
    pub symbol: String,
}

/// Tool input for check_cycles — circular dependency check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckCyclesInput {}

/// Tool input for find_usages backed by CodeIntelSnapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindUsagesSnapshotInput {
    pub symbol: String,
}

/// Tool output for MCP tool results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput<T: Serialize> {
    pub result: T,
}

/// V4-3: Code intel tool that wraps a shared snapshot
#[derive(Clone)]
pub struct CodeIntelTool {
    pub snapshot: std::sync::Arc<tokio::sync::RwLock<Option<CodeIntelSnapshot>>>,
}

impl CodeIntelTool {
    pub fn new(snapshot: std::sync::Arc<tokio::sync::RwLock<Option<CodeIntelSnapshot>>>) -> Self {
        Self { snapshot }
    }

    /// Get all callers of a symbol
    pub async fn get_callers(&self, input: GetCallersInput) -> anyhow::Result<Vec<String>> {
        let guard = self.snapshot.read().await;
        let snap = guard.as_ref().ok_or_else(|| anyhow::anyhow!("no snapshot available"))?;
        Ok(snap.callers_of(&input.symbol).into_iter().map(String::from).collect())
    }

    /// Get all callees of a symbol
    pub async fn get_callees(&self, input: GetCalleesInput) -> anyhow::Result<Vec<String>> {
        let guard = self.snapshot.read().await;
        let snap = guard.as_ref().ok_or_else(|| anyhow::anyhow!("no snapshot available"))?;
        Ok(snap.callees_of(&input.symbol).into_iter().map(String::from).collect())
    }

    /// Check for circular dependencies
    pub async fn check_cycles(&self) -> anyhow::Result<CycleReport> {
        let guard = self.snapshot.read().await;
        let snap = guard.as_ref().ok_or_else(|| anyhow::anyhow!("no snapshot available"))?;
        Ok(CycleDetector::detect_all(snap))
    }

    /// Enhanced find_usages backed by snapshot reference data
    pub async fn find_usages(&self, input: FindUsagesSnapshotInput) -> anyhow::Result<Vec<SymbolRef>> {
        let guard = self.snapshot.read().await;
        let snap = guard.as_ref().ok_or_else(|| anyhow::anyhow!("no snapshot available"))?;
        Ok(snap.usages_of(&input.symbol).into_iter().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_input_serialization() {
        let input = SearchSymbolsInput {
            project_id: "test".to_string(),
            query: "fn main".to_string(),
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("test"));
    }
}
