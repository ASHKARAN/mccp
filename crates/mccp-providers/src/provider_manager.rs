use super::*;
use mccp_core::*;
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::RwLock;
use std::time::{Duration, Instant};

/// Info about a registered provider (V3-2)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub fingerprint: String,
    pub health: ProviderHealth,
}

/// Provider manager for managing LLM providers
#[derive(Debug, Clone)]
pub struct ProviderManager {
    /// Registered providers
    providers: Arc<DashMap<String, Arc<dyn LlmProvider>>>,
    
    /// Provider configurations
    configs: Arc<DashMap<String, ProviderConfig>>,
    
    /// Provider health status
    health_status: Arc<DashMap<String, ProviderHealth>>,
    
    /// Health check interval
    health_check_interval: Duration,
    
    /// Health check task handle
    health_check_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl ProviderManager {
    /// Create a new provider manager
    pub fn new() -> Self {
        Self {
            providers: Arc::new(DashMap::new()),
            configs: Arc::new(DashMap::new()),
            health_status: Arc::new(DashMap::new()),
            health_check_interval: Duration::from_secs(30),
            health_check_task: Arc::new(RwLock::new(None)),
        }
    }

    /// Start the provider manager
    pub async fn start(&self) -> Result<()> {
        // Start health check task
        let providers = self.providers.clone();
        let health_status = self.health_status.clone();
        let interval = self.health_check_interval;
        
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            
            loop {
                interval.tick().await;
                
                // Check health of all providers
                for provider_entry in providers.iter() {
                    let provider_id = provider_entry.key().clone();
                    let provider = provider_entry.value();
                    
                    let health = provider.health().await;
                    health_status.insert(provider_id, health);
                }
            }
        });
        
        *self.health_check_task.write().await = Some(task);
        
        Ok(())
    }

    /// Stop the provider manager
    pub async fn stop(&self) -> Result<()> {
        // Stop health check task
        if let Some(task) = self.health_check_task.write().await.take() {
            task.abort();
        }
        
        Ok(())
    }

    /// Register a new LLM provider
    pub async fn register_provider(&self, provider: Arc<dyn LlmProvider>) -> Result<()> {
        let provider_id = provider.provider_fingerprint();
        let health = provider.health().await;
        
        self.providers.insert(provider_id.clone(), provider);
        self.health_status.insert(provider_id, health);
        
        Ok(())
    }

    /// Unregister a provider
    pub async fn unregister_provider(&self, provider_id: &str) -> Result<()> {
        if self.providers.remove(provider_id).is_some() {
            self.health_status.remove(provider_id);
            self.configs.remove(provider_id);
            Ok(())
        } else {
            Err(Error::ProviderError(format!("Provider {} not found", provider_id)))
        }
    }

    /// Get a provider by ID
    pub async fn get_provider(&self, provider_id: &str) -> Option<Arc<dyn LlmProvider>> {
        self.providers.get(provider_id).map(|p| p.clone())
    }

    /// List all providers
    pub async fn list_providers(&self) -> Vec<ProviderInfo> {
        let mut providers = Vec::new();
        
        for provider_entry in self.providers.iter() {
            let provider_id = provider_entry.key().clone();
            let health = self.health_status.get(&provider_id);
            
            providers.push(ProviderInfo {
                id: provider_id.clone(),
                fingerprint: provider_entry.value().provider_fingerprint(),
                health: health.map(|h| h.clone()).unwrap_or_default(),
            });
        }
        
        providers
    }

    /// Get provider health status
    pub async fn get_provider_health(&self, provider_id: &str) -> Option<ProviderHealth> {
        self.health_status.get(provider_id).map(|h| h.clone())
    }

    /// Get all provider health statuses
    pub async fn get_all_health(&self) -> Vec<(String, ProviderHealth)> {
        let mut healths = Vec::new();
        
        for health_entry in self.health_status.iter() {
            healths.push((health_entry.key().clone(), health_entry.value().clone()));
        }
        
        healths
    }

    /// Get healthy providers
    pub async fn get_healthy_providers(&self) -> Vec<Arc<dyn LlmProvider>> {
        let mut providers = Vec::new();
        
        for provider_entry in self.providers.iter() {
            let provider_id = provider_entry.key().clone();
            let health = self.health_status.get(&provider_id);
            
            if health.map_or(false, |h| h.is_healthy()) {
                providers.push(provider_entry.value().clone());
            }
        }
        
        providers
    }

    /// Get provider status
    pub async fn status(&self) -> ProviderStatus {
        let total_providers = self.providers.len();
        let healthy_providers = self.get_healthy_providers().await.len();
        
        ProviderStatus {
            total_providers,
            healthy_providers,
            total_models: 0,
        }
    }

    /// Set provider configuration
    pub async fn set_config(&self, provider_id: &str, config: ProviderConfig) -> Result<()> {
        self.configs.insert(provider_id.to_string(), config);
        Ok(())
    }

    /// Get provider configuration
    pub async fn get_config(&self, provider_id: &str) -> Option<ProviderConfig> {
        self.configs.get(provider_id).map(|c| c.clone())
    }

    /// Remove provider configuration
    pub async fn remove_config(&self, provider_id: &str) -> Result<()> {
        if self.configs.remove(provider_id).is_some() {
            Ok(())
        } else {
            Err(Error::ProviderError(format!("Config for provider {} not found", provider_id)))
        }
    }

    /// Get all configurations
    pub async fn get_all_configs(&self) -> Vec<(String, ProviderConfig)> {
        let mut configs = Vec::new();
        
        for config_entry in self.configs.iter() {
            configs.push((config_entry.key().clone(), config_entry.value().clone()));
        }
        
        configs
    }
}

/// Provider status
#[derive(Debug, Clone)]
pub struct ProviderStatus {
    pub total_providers: usize,
    pub healthy_providers: usize,
    pub total_models: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_provider_manager_creation() {
        let manager = ProviderManager::new();
        
        assert_eq!(manager.providers.len(), 0);
        assert_eq!(manager.configs.len(), 0);
        assert_eq!(manager.health_status.len(), 0);
    }

    #[tokio::test]
    async fn test_provider_registration() {
        let manager = ProviderManager::new();
        let mock_provider = Arc::new(MockProvider::new());
        manager.register_provider(mock_provider).await.unwrap();
        
        assert_eq!(manager.providers.len(), 1);
        assert_eq!(manager.health_status.len(), 1);
        
        let providers = manager.list_providers().await;
        assert_eq!(providers.len(), 1);
    }

    #[tokio::test]
    async fn test_provider_unregistration() {
        let manager = ProviderManager::new();
        let mock_provider = Arc::new(MockProvider::new());
        let provider_id = mock_provider.provider_fingerprint();
        
        manager.register_provider(mock_provider).await.unwrap();
        assert_eq!(manager.providers.len(), 1);
        
        manager.unregister_provider(&provider_id).await.unwrap();
        assert_eq!(manager.providers.len(), 0);
        assert_eq!(manager.health_status.len(), 0);
    }

    #[tokio::test]
    async fn test_provider_health() {
        let manager = ProviderManager::new();
        let mock_provider = Arc::new(MockProvider::new());
        let provider_id = mock_provider.provider_fingerprint();
        
        manager.register_provider(mock_provider).await.unwrap();
        
        let health = manager.get_provider_health(&provider_id).await;
        assert!(health.is_some());
        assert!(health.unwrap().is_healthy());
        
        let all_health = manager.get_all_health().await;
        assert_eq!(all_health.len(), 1);
        assert!(all_health[0].1.is_healthy);
    }

    #[tokio::test]
    async fn test_provider_status() {
        let manager = ProviderManager::new();
        let mock_provider = Arc::new(MockProvider::new());
        
        manager.register_provider(mock_provider).await.unwrap();
        
        let status = manager.status().await;
        assert_eq!(status.total_providers, 1);
        assert_eq!(status.healthy_providers, 1);
    }

    #[tokio::test]
    async fn test_provider_config() {
        let manager = ProviderManager::new();
        let provider_id = "test_provider";
        
        let config = ProviderConfig {
            api_key: Some("test_key".to_string()),
            endpoint: Some("https://api.example.com".to_string()),
            model: Some("test-model".to_string()),
            max_tokens: Some(1000),
            temperature: Some(0.7),
            timeout: Some(Duration::from_secs(30)),
        };
        
        manager.set_config(provider_id, config.clone()).await.unwrap();
        
        let retrieved_config = manager.get_config(provider_id).await;
        assert!(retrieved_config.is_some());
        assert_eq!(retrieved_config.unwrap().api_key, Some("test_key".to_string()));
        
        let all_configs = manager.get_all_configs().await;
        assert_eq!(all_configs.len(), 1);
        assert_eq!(all_configs[0].0, provider_id);
        
        manager.remove_config(provider_id).await.unwrap();
        let retrieved_config = manager.get_config(provider_id).await;
        assert!(retrieved_config.is_none());
    }
}

/// Mock provider for testing
#[derive(Debug, Clone)]
struct MockProvider {
    id: String,
    name: String,
    version: String,
}

impl MockProvider {
    fn new() -> Self {
        Self {
            id: "mock_provider".to_string(),
            name: "Mock Provider".to_string(),
            version: "1.0.0".to_string(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for MockProvider {
    async fn complete(&self, _prompt: &str, _schema: Option<&JsonSchema>) -> Result<String> {
        Ok("Mock completion".to_string())
    }

    async fn stream(&self, _prompt: &str) -> Result<tokio::sync::mpsc::Receiver<String>> {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tx.send("Mock stream".to_string()).await.unwrap();
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
        format!("mock:{}:{}", self.name, self.version)
    }
}