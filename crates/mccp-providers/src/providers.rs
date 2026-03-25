use super::*;
use mccp_core::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::bm25::BM25Encoder;

/// Return known embedding dimensions for well-known model names.
/// Returns `None` for unknown models — callers should fall back to `probe_dimensions()`.
pub fn known_dimensions(model: &str) -> Option<usize> {
    match model {
        m if m.contains("nomic-embed-text")       => Some(768),
        m if m.contains("mxbai-embed-large")      => Some(1024),
        m if m.contains("text-embedding-3-small") => Some(1536),
        m if m.contains("text-embedding-3-large") => Some(3072),
        m if m.contains("all-minilm")             => Some(384),
        m if m.contains("bge-large")              => Some(1024),
        m if m.contains("bge-small")              => Some(512),
        m if m.contains("e5-large")               => Some(1024),
        m if m.contains("e5-small")               => Some(384),
        _                                         => None,
    }
}

/// OpenAI provider implementation
#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    endpoint: String,
    model: String,
    max_tokens: usize,
    temperature: f32,
    timeout: Duration,
}

impl OpenAiProvider {
    /// Create a new OpenAI provider
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            model,
            max_tokens: 1000,
            temperature: 0.7,
            timeout: Duration::from_secs(60),
        }
    }

    /// Set the endpoint URL
    pub fn with_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = endpoint;
        self
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
impl LlmProvider for OpenAiProvider {
    async fn complete(&self, prompt: &str, schema: Option<&JsonSchema>) -> Result<String> {
        let request = OpenAiRequest {
            model: self.model.clone(),
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            response_format: schema.map(|s| OpenAiResponseFormat {
                r#type: "json_schema".to_string(),
                json_schema: OpenAiJsonSchema {
                    name: "response_schema".to_string(),
                    schema: s.clone(),
                    strict: true,
                },
            }),
            stream: None,
        };

        let response = self.client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .timeout(self.timeout)
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::ProviderError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderError(format!("OpenAI API error: {}", error_text)));
        }

        let response_data: OpenAiResponse = response
            .json()
            .await
            .map_err(|e| Error::ProviderError(e.to_string()))?;

        response_data.choices
            .first()
            .and_then(|choice| choice.message.content.as_ref())
            .cloned()
            .ok_or_else(|| Error::ProviderError("No response content".to_string()))
    }

    async fn stream(&self, prompt: &str) -> Result<tokio::sync::mpsc::Receiver<String>> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        let request = OpenAiRequest {
            model: self.model.clone(),
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            stream: Some(true),
            response_format: None,
        };

        tokio::spawn(async move {
            let client = Client::new();
            
            match client
                .post(&"https://api.openai.com/v1/chat/completions".to_string())
                .header("Authorization", format!("Bearer {}", "dummy"))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
            {
                Ok(_) => {
                    // For now, just send a dummy response
                    let _ = tx.send("Streaming response...".to_string()).await;
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
            .get(&format!("{}/models", self.endpoint))
            .header("Authorization", format!("Bearer {}", self.api_key))
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

    fn provider_fingerprint(&self) -> String {
        format!("openai:{}", self.model)
    }
}

/// Anthropic provider implementation
#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    endpoint: String,
    model: String,
    max_tokens: usize,
    temperature: f32,
    timeout: Duration,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            endpoint: "https://api.anthropic.com/v1/messages".to_string(),
            model,
            max_tokens: 1000,
            temperature: 0.7,
            timeout: Duration::from_secs(60),
        }
    }

    /// Set the endpoint URL
    pub fn with_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = endpoint;
        self
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
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, prompt: &str, schema: Option<&JsonSchema>) -> Result<String> {
        let request = AnthropicRequest {
            model: self.model.clone(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            max_tokens: self.max_tokens,
            temperature: self.temperature,
        };

        let response = self.client
            .post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .timeout(self.timeout)
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::ProviderError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderError(format!("Anthropic API error: {}", error_text)));
        }

        let response_data: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| Error::ProviderError(e.to_string()))?;

        response_data.content
            .first()
            .and_then(|content| content.text.as_ref())
            .cloned()
            .ok_or_else(|| Error::ProviderError("No response content".to_string()))
    }

    async fn stream(&self, prompt: &str) -> Result<tokio::sync::mpsc::Receiver<String>> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        // For now, just send a dummy response
        let _ = tx.send("Streaming response from Anthropic...".to_string()).await;
        
        Ok(rx)
    }

    async fn health(&self) -> ProviderHealth {
        match self.client
            .get(&format!("{}/models", self.endpoint))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
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

    fn provider_fingerprint(&self) -> String {
        format!("anthropic:{}", self.model)
    }
}

/// Local provider implementation (for testing)
#[derive(Debug, Clone)]
pub struct LocalProvider {
    model: String,
    max_tokens: usize,
    temperature: f32,
}

impl LocalProvider {
    /// Create a new local provider
    pub fn new(model: String) -> Self {
        Self {
            model,
            max_tokens: 1000,
            temperature: 0.7,
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
}

#[async_trait::async_trait]
impl LlmProvider for LocalProvider {
    async fn complete(&self, prompt: &str, _schema: Option<&JsonSchema>) -> Result<String> {
        // For testing, just return a mock response
        Ok(format!("Local response to: {}", prompt))
    }

    async fn stream(&self, prompt: &str) -> Result<tokio::sync::mpsc::Receiver<String>> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        // For testing, just send a mock response
        let _ = tx.send(format!("Local streaming response to: {}", prompt)).await;
        
        Ok(rx)
    }

    async fn health(&self) -> ProviderHealth {
        ProviderHealth {
            status: ProviderStatusType::Healthy,
            last_check: chrono::Utc::now(),
            error_message: None,
        }
    }

    fn provider_fingerprint(&self) -> String {
        format!("local:{}", self.model)
    }
}

// OpenAI API types
#[derive(Debug, Serialize, Deserialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    max_tokens: usize,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<OpenAiResponseFormat>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiResponseFormat {
    r#type: String,
    json_schema: OpenAiJsonSchema,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiJsonSchema {
    name: String,
    schema: serde_json::Value,
    strict: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

// Anthropic API types
#[derive(Debug, Serialize, Deserialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: usize,
    temperature: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_creation() {
        let provider = OpenAiProvider::new("test_key".to_string(), "gpt-4".to_string());
        
        assert_eq!(provider.name(), "OpenAI");
        assert_eq!(provider.version(), "1.0.0");
        assert!(provider.models().contains(&"gpt-4".to_string()));
    }

    #[test]
    fn test_anthropic_provider_creation() {
        let provider = AnthropicProvider::new("test_key".to_string(), "claude-3-opus-20240229".to_string());
        
        assert_eq!(provider.name(), "Anthropic");
        assert_eq!(provider.version(), "1.0.0");
        assert!(provider.models().contains(&"claude-3-opus-20240229".to_string()));
    }

    #[test]
    fn test_local_provider_creation() {
        let provider = LocalProvider::new("mock-model".to_string());
        
        assert_eq!(provider.name(), "Local");
        assert_eq!(provider.version(), "1.0.0");
        assert!(provider.models().contains(&"mock-model".to_string()));
    }

    #[tokio::test]
    async fn test_openai_provider_health() {
        let provider = OpenAiProvider::new("test_key".to_string(), "gpt-4".to_string());
        let health = provider.health().await;
        
        // Should be unhealthy since we're using a dummy API key
        assert_eq!(health.status, ProviderStatusType::Unhealthy);
        assert!(health.error_message.is_some());
    }

    #[tokio::test]
    async fn test_anthropic_provider_health() {
        let provider = AnthropicProvider::new("test_key".to_string(), "claude-3-opus-20240229".to_string());
        let health = provider.health().await;
        
        // Should be unhealthy since we're using a dummy API key
        assert_eq!(health.status, ProviderStatusType::Unhealthy);
        assert!(health.error_message.is_some());
    }

    #[tokio::test]
    async fn test_local_provider_health() {
        let provider = LocalProvider::new("mock-model".to_string());
        let health = provider.health().await;
        
        // Should be healthy since it's local
        assert_eq!(health.status, ProviderStatusType::Healthy);
        assert!(health.error_message.is_none());
    }

    #[tokio::test]
    async fn test_openai_provider_complete() {
        let provider = OpenAiProvider::new("test_key".to_string(), "gpt-4".to_string());
        
        // This will fail since we're using a dummy API key, but we can test the error handling
        let result = provider.complete("test prompt", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_local_provider_complete() {
        let provider = LocalProvider::new("mock-model".to_string());
        
        let result = provider.complete("test prompt", None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Local response to: test prompt"));
    }
}