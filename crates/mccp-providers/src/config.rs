use super::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// API key for the provider
    pub api_key: Option<String>,
    
    /// API endpoint URL
    pub endpoint: Option<String>,
    
    /// Default model to use
    pub model: Option<String>,
    
    /// Maximum tokens for completions
    pub max_tokens: Option<usize>,
    
    /// Temperature for completions
    pub temperature: Option<f32>,
    
    /// Request timeout
    pub timeout: Option<Duration>,
    
    /// Additional provider-specific settings
    pub settings: Option<serde_json::Value>,
}

impl ProviderConfig {
    /// Create a new provider configuration
    pub fn new() -> Self {
        Self {
            api_key: None,
            endpoint: None,
            model: None,
            max_tokens: None,
            temperature: None,
            timeout: None,
            settings: None,
        }
    }

    /// Set the API key
    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    /// Set the endpoint
    pub fn with_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    /// Set the model
    pub fn with_model(mut self, model: String) -> Self {
        self.model = Some(model);
        self
    }

    /// Set the maximum tokens
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set the temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set additional settings
    pub fn with_settings(mut self, settings: serde_json::Value) -> Self {
        self.settings = Some(settings);
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.api_key.is_none() {
            return Err(Error::ProviderError("API key is required".to_string()));
        }
        
        if self.model.is_none() {
            return Err(Error::ProviderError("Model is required".to_string()));
        }
        
        Ok(())
    }
}

/// Provider configuration builder
pub struct ProviderConfigBuilder {
    config: ProviderConfig,
}

impl ProviderConfigBuilder {
    /// Create a new provider configuration builder
    pub fn new() -> Self {
        Self {
            config: ProviderConfig::new(),
        }
    }

    /// Set the API key
    pub fn api_key(mut self, api_key: String) -> Self {
        self.config.api_key = Some(api_key);
        self
    }

    /// Set the endpoint
    pub fn endpoint(mut self, endpoint: String) -> Self {
        self.config.endpoint = Some(endpoint);
        self
    }

    /// Set the model
    pub fn model(mut self, model: String) -> Self {
        self.config.model = Some(model);
        self
    }

    /// Set the maximum tokens
    pub fn max_tokens(mut self, max_tokens: usize) -> Self {
        self.config.max_tokens = Some(max_tokens);
        self
    }

    /// Set the temperature
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.config.temperature = Some(temperature);
        self
    }

    /// Set the timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = Some(timeout);
        self
    }

    /// Set additional settings
    pub fn settings(mut self, settings: serde_json::Value) -> Self {
        self.config.settings = Some(settings);
        self
    }

    /// Build the configuration
    pub fn build(self) -> Result<ProviderConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}

/// Provider configuration manager
#[derive(Debug, Clone)]
pub struct ProviderConfigManager {
    /// Configuration storage
    configs: Arc<RwLock<HashMap<String, ProviderConfig>>>,
}

impl ProviderConfigManager {
    /// Create a new provider configuration manager
    pub fn new() -> Self {
        Self {
            configs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Save a provider configuration
    pub async fn save_config(&self, provider_id: &str, config: ProviderConfig) -> Result<()> {
        let mut configs = self.configs.write().await;
        configs.insert(provider_id.to_string(), config);
        Ok(())
    }

    /// Load a provider configuration
    pub async fn load_config(&self, provider_id: &str) -> Option<ProviderConfig> {
        let configs = self.configs.read().await;
        configs.get(provider_id).cloned()
    }

    /// Remove a provider configuration
    pub async fn remove_config(&self, provider_id: &str) -> Result<()> {
        let mut configs = self.configs.write().await;
        if configs.remove(provider_id).is_some() {
            Ok(())
        } else {
            Err(Error::ProviderError(format!("Configuration for provider {} not found", provider_id)))
        }
    }

    /// List all provider configurations
    pub async fn list_configs(&self) -> Vec<(String, ProviderConfig)> {
        let configs = self.configs.read().await;
        configs.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Clear all configurations
    pub async fn clear(&self) -> Result<()> {
        let mut configs = self.configs.write().await;
        configs.clear();
        Ok(())
    }

    /// Load configurations from a file
    pub async fn load_from_file(&self, path: &str) -> Result<()> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| Error::FileReadError {
                path: path.to_string(),
                error: e.to_string(),
            })?;

        let configs: HashMap<String, ProviderConfig> = serde_json::from_str(&content)
            .map_err(|e| Error::DeserializationError(e.to_string()))?;

        let mut config_map = self.configs.write().await;
        *config_map = configs;

        Ok(())
    }

    /// Save configurations to a file
    pub async fn save_to_file(&self, path: &str) -> Result<()> {
        let configs = {
            let config_map = self.configs.read().await;
            config_map.clone()
        };

        let content = serde_json::to_string_pretty(&configs)
            .map_err(|e| Error::SerializationError(e.to_string()))?;

        tokio::fs::write(path, content)
            .await
            .map_err(|e| Error::FileWriteError {
                path: path.to_string(),
                error: e.to_string(),
            })?;

        Ok(())
    }
}

/// Default configurations for common providers
pub struct DefaultConfigs;

impl DefaultConfigs {
    /// Get default OpenAI configuration
    pub fn openai(api_key: String) -> ProviderConfig {
        ProviderConfig::new()
            .with_api_key(api_key)
            .with_endpoint("https://api.openai.com/v1/chat/completions".to_string())
            .with_model("gpt-4".to_string())
            .with_max_tokens(1000)
            .with_temperature(0.7)
            .with_timeout(Duration::from_secs(60))
    }

    /// Get default Anthropic configuration
    pub fn anthropic(api_key: String) -> ProviderConfig {
        ProviderConfig::new()
            .with_api_key(api_key)
            .with_endpoint("https://api.anthropic.com/v1/messages".to_string())
            .with_model("claude-3-opus-20240229".to_string())
            .with_max_tokens(1000)
            .with_temperature(0.7)
            .with_timeout(Duration::from_secs(60))
    }

    /// Get default local configuration
    pub fn local() -> ProviderConfig {
        ProviderConfig::new()
            .with_model("mock-model".to_string())
            .with_max_tokens(1000)
            .with_temperature(0.7)
            .with_timeout(Duration::from_secs(30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_provider_config_creation() {
        let config = ProviderConfig::new();
        
        assert!(config.api_key.is_none());
        assert!(config.endpoint.is_none());
        assert!(config.model.is_none());
        assert!(config.max_tokens.is_none());
        assert!(config.temperature.is_none());
        assert!(config.timeout.is_none());
        assert!(config.settings.is_none());
    }

    #[test]
    fn test_provider_config_builder() {
        let config = ProviderConfigBuilder::new()
            .api_key("test_key".to_string())
            .endpoint("https://api.example.com".to_string())
            .model("test-model".to_string())
            .max_tokens(1000)
            .temperature(0.7)
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        
        assert_eq!(config.api_key, Some("test_key".to_string()));
        assert_eq!(config.endpoint, Some("https://api.example.com".to_string()));
        assert_eq!(config.model, Some("test-model".to_string()));
        assert_eq!(config.max_tokens, Some(1000));
        assert_eq!(config.temperature, Some(0.7));
        assert_eq!(config.timeout, Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_provider_config_validation() {
        let config = ProviderConfig::new();
        assert!(config.validate().is_err());
        
        let config = ProviderConfig::new()
            .with_api_key("test_key".to_string())
            .with_model("test-model".to_string());
        assert!(config.validate().is_ok());
    }

    #[tokio::test]
    async fn test_provider_config_manager() {
        let manager = ProviderConfigManager::new();
        
        let config = ProviderConfig::new()
            .with_api_key("test_key".to_string())
            .with_model("test-model".to_string());
        
        // Save config
        manager.save_config("test_provider", config.clone()).await.unwrap();
        
        // Load config
        let loaded_config = manager.load_config("test_provider").await;
        assert!(loaded_config.is_some());
        let loaded_config = loaded_config.unwrap();
        assert_eq!(loaded_config.api_key, Some("test_key".to_string()));
        assert_eq!(loaded_config.model, Some("test-model".to_string()));
        
        // List configs
        let configs = manager.list_configs().await;
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].0, "test_provider");
        
        // Remove config
        manager.remove_config("test_provider").await.unwrap();
        let loaded_config = manager.load_config("test_provider").await;
        assert!(loaded_config.is_none());
    }

    #[tokio::test]
    async fn test_provider_config_manager_file_operations() {
        let manager = ProviderConfigManager::new();
        
        let config = ProviderConfig::new()
            .with_api_key("test_key".to_string())
            .with_model("test-model".to_string());
        
        manager.save_config("test_provider", config).await.unwrap();
        
        // Create temporary file
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_str().unwrap();
        
        // Save to file
        manager.save_to_file(file_path).await.unwrap();
        
        // Clear manager
        manager.clear().await.unwrap();
        
        // Load from file
        manager.load_from_file(file_path).await.unwrap();
        
        // Verify config was loaded
        let loaded_config = manager.load_config("test_provider").await;
        assert!(loaded_config.is_some());
    }

    #[test]
    fn test_default_configs() {
        let openai_config = DefaultConfigs::openai("test_key".to_string());
        assert_eq!(openai_config.api_key, Some("test_key".to_string()));
        assert_eq!(openai_config.endpoint, Some("https://api.openai.com/v1/chat/completions".to_string()));
        assert_eq!(openai_config.model, Some("gpt-4".to_string()));
        
        let anthropic_config = DefaultConfigs::anthropic("test_key".to_string());
        assert_eq!(anthropic_config.api_key, Some("test_key".to_string()));
        assert_eq!(anthropic_config.endpoint, Some("https://api.anthropic.com/v1/messages".to_string()));
        assert_eq!(anthropic_config.model, Some("claude-3-opus-20240229".to_string()));
        
        let local_config = DefaultConfigs::local();
        assert_eq!(local_config.model, Some("mock-model".to_string()));
        assert!(local_config.api_key.is_none());
    }
}