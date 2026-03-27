use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt as _;

/// Cost tracking configuration
#[derive(Debug, Clone)]
pub struct CostGuardConfig {
    pub enabled: bool,
    pub max_index_cost_usd: f64,
    pub warn_at_usd: f64,
    pub track_provider_tokens: bool,
}

impl Default for CostGuardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_index_cost_usd: 5.00,
            warn_at_usd: 1.00,
            track_provider_tokens: true,
        }
    }
}

/// Cost tracker for monitoring provider usage
#[derive(Clone)]
pub struct CostTracker {
    tokens_used: Arc<AtomicU64>,
    spend_usd: Arc<std::sync::Mutex<f64>>,
    config: CostGuardConfig,
}

impl CostTracker {
    /// Create a new cost tracker
    pub fn new(config: CostGuardConfig) -> Self {
        Self {
            tokens_used: Arc::new(AtomicU64::new(0)),
            spend_usd: Arc::new(std::sync::Mutex::new(0.0)),
            config,
        }
    }

    /// Record usage and check limits
    pub fn record(&self, provider: &str, model: &str, tokens: u64) -> anyhow::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        self.tokens_used.fetch_add(tokens, Ordering::Relaxed);
        
        if let Some(price) = price_per_million(provider, model) {
            let cost = price * tokens as f64 / 1_000_000.0;
            let mut spend = self.spend_usd.lock().unwrap();
            *spend += cost;
            
            if *spend >= self.config.max_index_cost_usd {
                return Err(anyhow::anyhow!(
                    "cost guard: spend limit ${:.2} reached (used ${:.4}); \
                     indexing stopped. Raise [cost_guard] max_index_cost_usd to continue.",
                    self.config.max_index_cost_usd, *spend
                ));
            }
            
            if *spend >= self.config.warn_at_usd {
                tracing::warn!(
                    spend_usd = *spend, 
                    max_usd = self.config.max_index_cost_usd,
                    "cost guard warning: approaching spend limit"
                );
            }
        }
        
        Ok(())
    }

    /// Get total spend
    pub fn total_spend(&self) -> f64 {
        *self.spend_usd.lock().unwrap()
    }

    /// Get total tokens used
    pub fn total_tokens(&self) -> u64 {
        self.tokens_used.load(Ordering::Relaxed)
    }

    /// Reset spend and tokens
    pub fn reset(&self) {
        *self.spend_usd.lock().unwrap() = 0.0;
        self.tokens_used.store(0, Ordering::Relaxed);
    }
}

/// Get price per million tokens for a provider/model combination
pub fn price_per_million(provider: &str, model: &str) -> Option<f64> {
    match (provider, model) {
        ("openai", m) if m.contains("gpt-4o-mini") => Some(0.15),
        ("openai", m) if m.contains("gpt-4o") => Some(5.00),
        ("anthropic", m) if m.contains("claude-3-5-sonnet") => Some(3.00),
        ("anthropic", m) if m.contains("claude-3-5-haiku") => Some(0.25),
        ("groq", _) => Some(0.05),
        ("ollama", _) | ("vllm", _) => Some(0.00), // Local providers
        _ => None,
    }
}

/// Cost audit log entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CostAuditEntry {
    pub timestamp: i64,
    pub provider: String,
    pub model: String,
    pub tokens: u64,
    pub cost_usd: f64,
}

/// Cost audit logger
pub struct CostAuditLogger {
    log_path: std::path::PathBuf,
}

impl CostAuditLogger {
    /// Create a new audit logger
    pub fn new(log_path: std::path::PathBuf) -> Self {
        Self { log_path }
    }

    /// Log a cost entry
    pub async fn log_entry(&self, entry: &CostAuditEntry) -> anyhow::Result<()> {
        let json = serde_json::to_string(entry)?;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .await?;
        
        file.write_all(format!("{}\n", json).as_bytes()).await?;
        Ok(())
    }

    /// Get audit entries for a project
    pub async fn get_entries(&self) -> anyhow::Result<Vec<CostAuditEntry>> {
        if !self.log_path.exists() {
            return Ok(vec![]);
        }

        let content = tokio::fs::read_to_string(&self.log_path).await?;
        let mut entries = Vec::new();
        
        for line in content.lines() {
            if let Ok(entry) = serde_json::from_str::<CostAuditEntry>(line) {
                entries.push(entry);
            }
        }
        
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_per_million() {
        assert_eq!(price_per_million("openai", "gpt-4o"), Some(5.00));
        assert_eq!(price_per_million("openai", "gpt-4o-mini"), Some(0.15));
        assert_eq!(price_per_million("anthropic", "claude-3-5-sonnet"), Some(3.00));
        assert_eq!(price_per_million("ollama", "codellama:13b"), Some(0.00));
        assert_eq!(price_per_million("unknown", "model"), None);
    }

    #[test]
    fn test_cost_tracker_creation() {
        let config = CostGuardConfig::default();
        let tracker = CostTracker::new(config);
        
        assert_eq!(tracker.total_spend(), 0.0);
        assert_eq!(tracker.total_tokens(), 0);
    }

    #[test]
    fn test_cost_tracker_record() {
        let config = CostGuardConfig {
            enabled: true,
            max_index_cost_usd: 1.00,
            warn_at_usd: 0.50,
            track_provider_tokens: true,
        };
        let tracker = CostTracker::new(config);
        
        // Record some tokens
        assert!(tracker.record("openai", "gpt-4o-mini", 1_000_000).is_ok());
        assert!(tracker.total_tokens() > 0);
        assert!(tracker.total_spend() > 0.0);
    }

    #[test]
    fn test_cost_tracker_limit() {
        let config = CostGuardConfig {
            enabled: true,
            max_index_cost_usd: 0.10,
            warn_at_usd: 0.05,
            track_provider_tokens: true,
        };
        let tracker = CostTracker::new(config);
        
        // Should hit the limit
        let result = tracker.record("openai", "gpt-4o", 1_000_000);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("spend limit"));
    }
}