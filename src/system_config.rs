/// System-wide configuration stored in ~/.mccp/config.toml.
///
/// Covers vector store, embedding providers, LLM providers, and daemon
/// settings. This is intentionally a flat, self-contained struct so the CLI
/// binary can read/write it without depending on the mccp-core crate.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ─── Top-level config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub daemon: DaemonConfig,
    pub vector: VectorConfig,
    pub embedding: EmbeddingConfig,
    pub llm: LlmConfig,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            vector: VectorConfig::default(),
            embedding: EmbeddingConfig::default(),
            llm: LlmConfig::default(),
        }
    }
}

impl SystemConfig {
    pub fn config_path() -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
        let dir = home.join(".mccp");
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join("config.toml"))
    }

    pub fn load_or_default() -> anyhow::Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let cfg: Self = toml::from_str(&content)?;
            Ok(cfg)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path()?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

// ─── Daemon ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub http_port: u16,
    pub log_level: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self { http_port: 7422, log_level: "info".into() }
    }
}

// ─── Vector store ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorConfig {
    pub driver: String,
    pub url: String,
    pub api_key: String,
    pub hnsw_m: usize,
    pub hnsw_ef_construct: usize,
    pub quantization: String,
}

impl Default for VectorConfig {
    fn default() -> Self {
        Self {
            driver: "qdrant".into(),
            url: "http://localhost:6333".into(),
            api_key: String::new(),
            hnsw_m: 16,
            hnsw_ef_construct: 128,
            quantization: "scalar".into(),
        }
    }
}

// ─── Embedding ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub providers: Vec<EmbeddingProviderConfig>,
    pub dimensions: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            providers: vec![EmbeddingProviderConfig {
                driver: "ollama".into(),
                model: "nomic-embed-text".into(),
                url: "http://localhost:11434".into(),
                api_key: String::new(),
            }],
            dimensions: 768,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingProviderConfig {
    pub driver: String,
    pub model: String,
    pub url: String,
    pub api_key: String,
}

// ─── LLM ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub providers: Vec<LlmProviderConfig>,
    pub max_tokens: usize,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            providers: vec![LlmProviderConfig {
                driver: "ollama".into(),
                model: "codellama:13b".into(),
                url: "http://localhost:11434".into(),
                api_key: String::new(),
            }],
            max_tokens: 2048,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    pub driver: String,
    pub model: String,
    pub url: String,
    pub api_key: String,
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_qdrant_vector_store() {
        let cfg = SystemConfig::default();
        assert_eq!(cfg.vector.driver, "qdrant");
        assert_eq!(cfg.vector.url, "http://localhost:6333");
    }

    #[test]
    fn default_config_has_ollama_embedding() {
        let cfg = SystemConfig::default();
        assert_eq!(cfg.embedding.providers.len(), 1);
        assert_eq!(cfg.embedding.providers[0].driver, "ollama");
        assert_eq!(cfg.embedding.providers[0].model, "nomic-embed-text");
    }

    #[test]
    fn default_config_has_ollama_llm() {
        let cfg = SystemConfig::default();
        assert_eq!(cfg.llm.providers.len(), 1);
        assert_eq!(cfg.llm.providers[0].driver, "ollama");
        assert_eq!(cfg.llm.providers[0].model, "codellama:13b");
    }

    #[test]
    fn default_daemon_port_is_7422() {
        let cfg = SystemConfig::default();
        assert_eq!(cfg.daemon.http_port, 7422);
    }

    #[test]
    fn config_roundtrip_via_toml() {
        let mut cfg = SystemConfig::default();
        cfg.vector.driver = "pgvector".into();
        cfg.vector.url = "http://localhost:5432".into();
        cfg.embedding.dimensions = 1024;
        cfg.llm.providers[0].model = "llama3:8b".into();

        let serialized = toml::to_string_pretty(&cfg).expect("serialize");
        let deserialized: SystemConfig = toml::from_str(&serialized).expect("deserialize");

        assert_eq!(deserialized.vector.driver, "pgvector");
        assert_eq!(deserialized.vector.url, "http://localhost:5432");
        assert_eq!(deserialized.embedding.dimensions, 1024);
        assert_eq!(deserialized.llm.providers[0].model, "llama3:8b");
    }

    #[test]
    fn config_roundtrip_preserves_api_keys() {
        let mut cfg = SystemConfig::default();
        cfg.vector.api_key = "secret-qdrant-key".into();
        cfg.embedding.providers[0].api_key = "openai-key".into();

        let serialized = toml::to_string_pretty(&cfg).unwrap();
        let back: SystemConfig = toml::from_str(&serialized).unwrap();

        assert_eq!(back.vector.api_key, "secret-qdrant-key");
        assert_eq!(back.embedding.providers[0].api_key, "openai-key");
    }

    #[test]
    fn adding_llm_provider_prepend() {
        let mut cfg = SystemConfig::default();
        let new_prov = LlmProviderConfig {
            driver: "openai".into(),
            model: "gpt-4o".into(),
            url: "https://api.openai.com/v1/chat/completions".into(),
            api_key: "sk-test".into(),
        };
        cfg.llm.providers.insert(0, new_prov);
        assert_eq!(cfg.llm.providers[0].driver, "openai");
        assert_eq!(cfg.llm.providers[1].driver, "ollama");
    }

    #[test]
    fn adding_embedding_provider_prepend() {
        let mut cfg = SystemConfig::default();
        let new_prov = EmbeddingProviderConfig {
            driver: "openai".into(),
            model: "text-embedding-3-small".into(),
            url: "https://api.openai.com/v1/embeddings".into(),
            api_key: "sk-test".into(),
        };
        cfg.embedding.providers.insert(0, new_prov);
        assert_eq!(cfg.embedding.providers[0].driver, "openai");
        assert_eq!(cfg.embedding.providers.len(), 2);
    }

    #[test]
    fn save_and_load_roundtrip_via_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");

        let mut cfg = SystemConfig::default();
        cfg.vector.driver = "chroma".into();
        cfg.daemon.http_port = 9000;

        let content = toml::to_string_pretty(&cfg).unwrap();
        std::fs::write(&path, &content).unwrap();

        let loaded: SystemConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.vector.driver, "chroma");
        assert_eq!(loaded.daemon.http_port, 9000);
    }
}
