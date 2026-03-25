use super::*;
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
