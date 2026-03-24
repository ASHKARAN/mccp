use super::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use dashmap::DashMap;

/// Cache entry with expiration
#[derive(Debug, Clone)]
struct CacheEntry<T> {
    value: T,
    expires_at: Option<Instant>,
}

/// In-memory cache with TTL support
#[derive(Debug, Clone)]
pub struct Cache {
    entries: DashMap<String, CacheEntry<String>>,
    default_ttl: Duration,
}

impl Cache {
    /// Create a new cache with default TTL
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            default_ttl: Duration::from_secs(3600), // 1 hour
        }
    }

    /// Create a new cache with custom TTL
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            default_ttl: ttl,
        }
    }

    /// Get a value from cache
    pub fn get(&self, key: &str) -> Option<String> {
        if let Some(entry) = self.entries.get(key) {
            if let Some(expires_at) = entry.expires_at {
                if Instant::now() > expires_at {
                    self.entries.remove(key);
                    return None;
                }
            }
            Some(entry.value.clone())
        } else {
            None
        }
    }

    /// Set a value in cache with default TTL
    pub fn set(&self, key: String, value: String) {
        self.set_with_ttl(key, value, self.default_ttl);
    }

    /// Set a value in cache with custom TTL
    pub fn set_with_ttl(&self, key: String, value: String, ttl: Duration) {
        let expires_at = if ttl.is_zero() {
            None
        } else {
            Some(Instant::now() + ttl)
        };
        
        self.entries.insert(key, CacheEntry {
            value,
            expires_at,
        });
    }

    /// Remove a value from cache
    pub fn remove(&self, key: &str) -> Option<String> {
        self.entries.remove(key).map(|(_, entry)| entry.value)
    }

    /// Clear all entries from cache
    pub fn clear(&self) {
        self.entries.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let total_entries = self.entries.len();
        let expired_entries = self.entries.iter()
            .filter(|entry| {
                entry.expires_at.map_or(false, |expires_at| Instant::now() > expires_at)
            })
            .count();
        
        CacheStats {
            total_entries,
            expired_entries,
            active_entries: total_entries - expired_entries,
        }
    }

    /// Cleanup expired entries
    pub fn cleanup(&self) {
        let now = Instant::now();
        self.entries.retain(|_, entry| {
            entry.expires_at.map_or(true, |expires_at| now <= expires_at)
        });
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub active_entries: usize,
}

/// Cache key builder
pub struct CacheKeyBuilder;

impl CacheKeyBuilder {
    /// Build a cache key for project data
    pub fn project(project_id: &str) -> String {
        format!("project:{}", project_id)
    }

    /// Build a cache key for symbols
    pub fn symbols(project_id: &str) -> String {
        format!("symbols:{}", project_id)
    }

    /// Build a cache key for chunks
    pub fn chunks(project_id: &str) -> String {
        format!("chunks:{}", project_id)
    }

    /// Build a cache key for summaries
    pub fn summaries(project_id: &str) -> String {
        format!("summaries:{}", project_id)
    }

    /// Build a cache key for graph
    pub fn graph(project_id: &str) -> String {
        format!("graph:{}", project_id)
    }

    /// Build a cache key for search results
    pub fn search(project_id: &str, query: &str) -> String {
        format!("search:{}:{}", project_id, query)
    }

    /// Build a cache key for context
    pub fn context(project_id: &str, file_path: &str, line: usize, column: usize) -> String {
        format!("context:{}:{}:{}:{}", project_id, file_path, line, column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_cache_creation() {
        let cache = Cache::new();
        
        assert_eq!(cache.stats().total_entries, 0);
        assert_eq!(cache.stats().active_entries, 0);
        assert_eq!(cache.stats().expired_entries, 0);
    }

    #[test]
    fn test_cache_operations() {
        let cache = Cache::new();
        
        cache.set("key1".to_string(), "value1".to_string());
        
        assert_eq!(cache.get("key1"), Some("value1".to_string()));
        assert_eq!(cache.get("key2"), None);
        
        cache.remove("key1");
        assert_eq!(cache.get("key1"), None);
    }

    #[test]
    fn test_cache_with_ttl() {
        let cache = Cache::with_ttl(Duration::from_millis(100));
        
        cache.set_with_ttl("key1".to_string(), "value1".to_string(), Duration::from_millis(100));
        
        assert_eq!(cache.get("key1"), Some("value1".to_string()));
        
        // Wait for expiration
        std::thread::sleep(Duration::from_millis(150));
        
        assert_eq!(cache.get("key1"), None);
    }

    #[test]
    fn test_cache_stats() {
        let cache = Cache::new();
        
        cache.set("key1".to_string(), "value1".to_string());
        cache.set("key2".to_string(), "value2".to_string());
        
        let stats = cache.stats();
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.active_entries, 2);
        assert_eq!(stats.expired_entries, 0);
    }

    #[test]
    fn test_cache_cleanup() {
        let cache = Cache::new();
        
        cache.set_with_ttl("key1".to_string(), "value1".to_string(), Duration::from_millis(100));
        cache.set("key2".to_string(), "value2".to_string());
        
        // Wait for first entry to expire
        std::thread::sleep(Duration::from_millis(150));
        
        cache.cleanup();
        
        let stats = cache.stats();
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.active_entries, 1);
        assert_eq!(stats.expired_entries, 0);
    }

    #[test]
    fn test_cache_key_builder() {
        assert_eq!(CacheKeyBuilder::project("test"), "project:test");
        assert_eq!(CacheKeyBuilder::symbols("test"), "symbols:test");
        assert_eq!(CacheKeyBuilder::chunks("test"), "chunks:test");
        assert_eq!(CacheKeyBuilder::summaries("test"), "summaries:test");
        assert_eq!(CacheKeyBuilder::graph("test"), "graph:test");
        assert_eq!(CacheKeyBuilder::search("test", "query"), "search:test:query");
        assert_eq!(CacheKeyBuilder::context("test", "file.rs", 10, 5), "context:test:file.rs:10:5");
    }
}