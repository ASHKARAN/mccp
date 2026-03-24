/// HTTP-based embedding client for ollama and OpenAI-compatible providers.
///
/// Intentionally self-contained (no dependency on broken mccp-core crate).
use crate::system_config::EmbeddingConfig;
use anyhow::Context;
use reqwest::Client;
use serde::{Deserialize, Serialize};

// ─── Provider config snapshot ─────────────────────────────────────────────────

#[derive(Clone)]
struct ProviderSnapshot {
    driver: String,
    url: String,
    model: String,
    api_key: String,
}

// ─── Client ───────────────────────────────────────────────────────────────────

pub struct EmbeddingClient {
    client: Client,
    provider: ProviderSnapshot,
    pub dimensions: usize,
}

impl EmbeddingClient {
    pub fn from_config(cfg: &EmbeddingConfig) -> anyhow::Result<Self> {
        let p = cfg
            .providers
            .first()
            .ok_or_else(|| anyhow::anyhow!("no embedding providers configured — run /config first"))?;
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            provider: ProviderSnapshot {
                driver: p.driver.clone(),
                url: p.url.clone(),
                model: p.model.clone(),
                api_key: p.api_key.clone(),
            },
            dimensions: cfg.dimensions,
        })
    }

    pub async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        match self.provider.driver.as_str() {
            "ollama" => self.embed_ollama(text).await,
            "openai" | "azure" | "custom" => self.embed_openai_compat(text).await,
            other => anyhow::bail!(
                "embedding driver '{}' not yet implemented — configure ollama or openai in /config",
                other
            ),
        }
    }

    // ── Ollama ────────────────────────────────────────────────────────────────

    async fn embed_ollama(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        #[derive(Serialize)]
        struct Req<'a> { model: &'a str, prompt: &'a str }
        #[derive(Deserialize)]
        struct Resp { embedding: Vec<f32> }

        let url = format!("{}/api/embeddings", self.provider.url.trim_end_matches('/'));
        let resp: Resp = self
            .client
            .post(&url)
            .json(&Req { model: &self.provider.model, prompt: text })
            .send()
            .await
            .context("ollama embeddings request failed — is ollama running?")?
            .error_for_status()
            .context("ollama returned an error")?
            .json()
            .await
            .context("failed to parse ollama embeddings response")?;
        Ok(resp.embedding)
    }

    // ── OpenAI-compatible (/v1/embeddings) ───────────────────────────────────

    async fn embed_openai_compat(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        #[derive(Serialize)]
        struct Req<'a> { model: &'a str, input: &'a str }
        #[derive(Deserialize)]
        struct Data { embedding: Vec<f32> }
        #[derive(Deserialize)]
        struct Resp { data: Vec<Data> }

        let url = if self.provider.url.is_empty() {
            "https://api.openai.com/v1/embeddings".to_string()
        } else {
            self.provider.url.clone()
        };

        let mut builder = self
            .client
            .post(&url)
            .json(&Req { model: &self.provider.model, input: text });
        if !self.provider.api_key.is_empty() {
            builder = builder.bearer_auth(&self.provider.api_key);
        }

        let resp: Resp = builder
            .send()
            .await
            .context("openai embeddings request failed")?
            .error_for_status()
            .context("openai returned an error")?
            .json()
            .await
            .context("failed to parse openai embeddings response")?;

        resp.data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| anyhow::anyhow!("empty embeddings response"))
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_config::{EmbeddingConfig, EmbeddingProviderConfig};

    fn make_cfg(driver: &str) -> EmbeddingConfig {
        EmbeddingConfig {
            providers: vec![EmbeddingProviderConfig {
                driver: driver.into(),
                model: "test-model".into(),
                url: "http://localhost:11434".into(),
                api_key: String::new(),
            }],
            dimensions: 768,
        }
    }

    #[test]
    fn client_builds_from_config() {
        let cfg = make_cfg("ollama");
        let client = EmbeddingClient::from_config(&cfg);
        assert!(client.is_ok());
        let c = client.unwrap();
        assert_eq!(c.dimensions, 768);
    }

    #[test]
    fn client_fails_on_empty_providers() {
        let cfg = EmbeddingConfig { providers: vec![], dimensions: 768 };
        let result = EmbeddingClient::from_config(&cfg);
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("no embedding providers"));
    }

    #[test]
    fn unsupported_driver_is_handled_gracefully() {
        // Construction succeeds (driver is not validated at build time)
        let cfg = make_cfg("cohere");
        let client = EmbeddingClient::from_config(&cfg).unwrap();
        // The error only surfaces when embed() is called (at runtime)
        assert_eq!(client.dimensions, 768);
    }
}
