use super::*;
use mccp_core::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::providers::known_dimensions;

/// Ollama embedding provider — implements EmbeddingProvider with auto-dimension detection.
#[derive(Debug, Clone)]
pub struct OllamaEmbeddingProvider {
    client: Client,
    endpoint: String,
    model: String,
    dims: usize,
    timeout: Duration,
}

impl OllamaEmbeddingProvider {
    /// Create a new embedding provider.
    /// `dims = 0` triggers auto-detection via a probe call; non-zero values are used as-is.
    pub fn new(endpoint: String, model: String, dims: usize) -> Self {
        let resolved_dims = if dims > 0 {
            dims
        } else {
            known_dimensions(&model).unwrap_or(0) // 0 → will be probed on first use
        };
        Self {
            client: Client::new(),
            endpoint,
            model,
            dims: resolved_dims,
            timeout: Duration::from_secs(60),
        }
    }

    /// Create with explicit dimension auto-detection via probe (async).
    pub async fn with_auto_detect(endpoint: String, model: String) -> anyhow::Result<Self> {
        let mut provider = Self::new(endpoint, model.clone(), 0);
        if provider.dims == 0 {
            let probed = provider.probe_dimensions().await
                .map_err(|e| anyhow::anyhow!("failed to probe embedding dimensions for {}: {}", model, e))?;
            tracing::info!("auto-detected embedding dimensions for '{}': {}", model, probed);
            provider.dims = probed;
        }
        Ok(provider)
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for OllamaEmbeddingProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed_one(text).await?);
        }
        Ok(results)
    }

    async fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        #[derive(Serialize)]
        struct EmbedRequest<'a> { model: &'a str, prompt: &'a str }
        #[derive(Deserialize)]
        struct EmbedResponse { embedding: Vec<f32> }

        let response = self.client
            .post(format!("{}/api/embeddings", self.endpoint))
            .timeout(self.timeout)
            .json(&EmbedRequest { model: &self.model, prompt: text })
            .send()
            .await
            .map_err(|e| Error::ProviderError(e.to_string()))?;

        if !response.status().is_success() {
            let err = response.text().await.unwrap_or_default();
            return Err(Error::ProviderError(format!("Ollama embed error: {}", err)));
        }

        let data: EmbedResponse = response.json().await
            .map_err(|e| Error::ProviderError(e.to_string()))?;
        Ok(data.embedding)
    }

    fn dimensions(&self) -> usize { self.dims }

    fn provider_fingerprint(&self) -> String {
        format!("ollama-embed:{}:{}", self.endpoint, self.model)
    }

    async fn health(&self) -> ProviderHealth {
        match self.client.get(format!("{}/api/tags", self.endpoint))
            .timeout(Duration::from_secs(5)).send().await
        {
            Ok(r) if r.status().is_success() => ProviderHealth {
                status: ProviderStatusType::Healthy,
                last_check: chrono::Utc::now(),
                error_message: None,
            },
            Ok(r) => ProviderHealth {
                status: ProviderStatusType::Unhealthy,
                last_check: chrono::Utc::now(),
                error_message: Some(format!("HTTP {}", r.status())),
            },
            Err(e) => ProviderHealth {
                status: ProviderStatusType::Unhealthy,
                last_check: chrono::Utc::now(),
                error_message: Some(e.to_string()),
            },
        }
    }
}

/// Ollama provider implementation
#[derive(Debug, Clone)]
pub struct OllamaProvider {
    client: Client,
    endpoint: String,
    model: String,
    max_tokens: usize,
    temperature: f32,
    timeout: Duration,
}

impl OllamaProvider {
    /// Create a new Ollama provider
    pub fn new(endpoint: String, model: String) -> Self {
        Self {
            client: Client::new(),
            endpoint,
            model,
            max_tokens: 1000,
            temperature: 0.7,
            timeout: Duration::from_secs(120), // Ollama can be slower for model downloads
        }
    }

    /// Set the maximum tokens
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Set the temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Set the timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[async_trait::async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(&self, prompt: &str, _schema: Option<&JsonSchema>) -> Result<String> {
        let request = OllamaRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            stream: false,
            options: OllamaOptions {
                temperature: self.temperature,
                num_predict: self.max_tokens as i32,
            },
        };

        let response = self.client
            .post(&format!("{}/api/generate", self.endpoint))
            .header("Content-Type", "application/json")
            .timeout(self.timeout)
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::ProviderError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderError(format!("Ollama API error: {}", error_text)));
        }

        let response_data: OllamaResponse = response
            .json()
            .await
            .map_err(|e| Error::ProviderError(e.to_string()))?;

        Ok(response_data.response)
    }

    async fn stream(&self, prompt: &str) -> Result<tokio::sync::mpsc::Receiver<String>> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        let request = OllamaRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            stream: true,
            options: OllamaOptions {
                temperature: self.temperature,
                num_predict: self.max_tokens as i32,
            },
        };

        tokio::spawn(async move {
            let client = Client::new();
            
            match client
                .post("http://localhost:11434/api/generate")
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
            {
                Ok(_) => {
                    // For now, just send a dummy response
                    let _ = tx.send("Streaming response from Ollama...".to_string()).await;
                }
                Err(e) => {
                    let _ = tx.send(format!("Error: {}", e)).await;
                }
            }
        });

        Ok(rx)
    }

    async fn health(&self) -> ProviderHealth {
        match self.client
            .get(&format!("{}/api/tags", self.endpoint))
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    ProviderHealth {
                        status: ProviderStatusType::Healthy,
                        last_check: chrono::Utc::now(),
                        error_message: None,
                    }
                } else {
                    ProviderHealth {
                        status: ProviderStatusType::Unhealthy,
                        last_check: chrono::Utc::now(),
                        error_message: Some(format!("HTTP error: {}", response.status())),
                    }
                }
            }
            Err(e) => ProviderHealth {
                status: ProviderStatusType::Unhealthy,
                last_check: chrono::Utc::now(),
                error_message: Some(e.to_string()),
            },
        }
    }

    fn id(&self) -> String {
        format!("ollama-{}", self.model)
    }

    fn name(&self) -> String {
        "Ollama".to_string()
    }

    fn version(&self) -> String {
        "1.0.0".to_string()
    }

    fn models(&self) -> Vec<String> {
        vec![
            "llama2".to_string(),
            "codellama".to_string(),
            "mistral".to_string(),
            "gemma".to_string(),
            "phi".to_string(),
            "llava".to_string(),
        ]
    }

    fn current_model(&self) -> String {
        self.model.clone()
    }

    fn set_model(&mut self, model: String) -> Result<()> {
        self.model = model;
        Ok(())
    }

    async fn download_model(&self, model: &str) -> Result<()> {
        let request = OllamaPullRequest {
            name: model.to_string(),
        };

        let response = self.client
            .post(&format!("{}/api/pull", self.endpoint))
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(300)) // 5 minutes for model download
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::ProviderError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderError(format!("Failed to download model {}: {}", model, error_text)));
        }

        Ok(())
    }
}

// Ollama API types
#[derive(Debug, Serialize, Deserialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaResponse {
    response: String,
    done: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaPullRequest {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_creation() {
        let provider = OllamaProvider::new("http://localhost:11434".to_string(), "llama2".to_string());
        
        assert_eq!(provider.name(), "Ollama");
        assert_eq!(provider.version(), "1.0.0");
        assert!(provider.models().contains(&"llama2".to_string()));
        assert_eq!(provider.current_model(), "llama2");
    }

    #[test]
    fn test_ollama_provider_model_change() {
        let mut provider = OllamaProvider::new("http://localhost:11434".to_string(), "llama2".to_string());
        
        assert!(provider.set_model("mistral".to_string()).is_ok());
        assert_eq!(provider.current_model(), "mistral");
    }

    #[tokio::test]
    async fn test_ollama_provider_health() {
        let provider = OllamaProvider::new("http://localhost:11434".to_string(), "llama2".to_string());
        let health = provider.health().await;
        
        // Should be unhealthy if Ollama is not running
        assert_eq!(health.status, ProviderStatusType::Unhealthy);
        assert!(health.error_message.is_some());
    }

    #[tokio::test]
    async fn test_ollama_provider_complete() {
        let provider = OllamaProvider::new("http://localhost:11434".to_string(), "llama2".to_string());
        
        // This will fail if Ollama is not running, but we can test the error handling
        let result = provider.complete("test prompt", None).await;
        assert!(result.is_err());
    }
}