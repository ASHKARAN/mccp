use mccp_indexer::*;
use serde::{Deserialize, Serialize};

/// MCP resource: index status snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingResource {
    pub status: IndexingStatus,
}

impl IndexingResource {
    pub fn from_pipeline(pipeline: &IndexingPipeline) -> Self {
        Self { status: pipeline.status() }
    }
}

/// MCP resource: server health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResource {
    pub ok: bool,
    pub version: String,
}

impl HealthResource {
    pub fn ok() -> Self {
        Self { ok: true, version: env!("CARGO_PKG_VERSION").to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_resource() {
        let r = HealthResource::ok();
        assert!(r.ok);
    }
}
