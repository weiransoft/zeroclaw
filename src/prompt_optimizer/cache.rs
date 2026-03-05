//! Prompt Cache - Caches optimized prompts for reuse

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
struct CacheEntry {
    prompt: Arc<String>,
    created_at: Instant,
    access_count: u64,
}

impl CacheEntry {
    fn new(prompt: Arc<String>) -> Self {
        Self {
            prompt,
            created_at: Instant::now(),
            access_count: 1,
        }
    }
    
    fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: usize,
    pub hit_rate: f64,
}

pub struct PromptCache {
    entries: HashMap<String, CacheEntry>,
    max_entries: usize,
    ttl: Duration,
    hits: u64,
    misses: u64,
}

impl PromptCache {
    const DEFAULT_MAX_ENTRIES: usize = 100;
    const DEFAULT_TTL_SECS: u64 = 3600;
    
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: Self::DEFAULT_MAX_ENTRIES,
            ttl: Duration::from_secs(Self::DEFAULT_TTL_SECS),
            hits: 0,
            misses: 0,
        }
    }
    
    pub fn with_config(max_entries: usize, ttl_secs: u64) -> Self {
        Self {
            entries: HashMap::new(),
            max_entries,
            ttl: Duration::from_secs(ttl_secs),
            hits: 0,
            misses: 0,
        }
    }
    
    pub fn generate_key(
        &self,
        workspace_dir: &std::path::Path,
        model_name: &str,
        user_message: &str,
        task_type: super::TaskType,
    ) -> String {
        let mut hasher = Sha256::new();
        
        hasher.update(workspace_dir.to_string_lossy().as_bytes());
        hasher.update(model_name.as_bytes());
        hasher.update(user_message.as_bytes());
        hasher.update(&[task_type as u8]);
        
        format!("{:x}", hasher.finalize())
    }
    
    pub fn get(&mut self, key: &str) -> Option<Arc<String>> {
        if let Some(entry) = self.entries.get_mut(key) {
            if entry.is_expired(self.ttl) {
                self.entries.remove(key);
                self.misses += 1;
                return None;
            }
            
            entry.access_count += 1;
            self.hits += 1;
            return Some(entry.prompt.clone());
        }
        
        self.misses += 1;
        None
    }
    
    pub fn put(&mut self, key: String, prompt: Arc<String>) {
        if self.entries.len() >= self.max_entries {
            self.evict_lru();
        }
        
        self.entries.insert(key, CacheEntry::new(prompt));
    }
    
    fn evict_lru(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        
        let mut oldest_key = None;
        let mut oldest_access = u64::MAX;
        let mut oldest_time = Instant::now();
        
        for (key, entry) in &self.entries {
            if entry.access_count < oldest_access 
                || (entry.access_count == oldest_access && entry.created_at < oldest_time) {
                oldest_access = entry.access_count;
                oldest_time = entry.created_at;
                oldest_key = Some(key.clone());
            }
        }
        
        if let Some(key) = oldest_key {
            self.entries.remove(&key);
        }
    }
    
    pub fn clear(&mut self) {
        self.entries.clear();
        self.hits = 0;
        self.misses = 0;
    }
    
    pub fn clear_expired(&mut self) -> usize {
        let expired_keys: Vec<_> = self.entries
            .iter()
            .filter(|(_, entry)| entry.is_expired(self.ttl))
            .map(|(key, _)| key.clone())
            .collect();
        
        let count = expired_keys.len();
        for key in expired_keys {
            self.entries.remove(&key);
        }
        
        count
    }
    
    pub fn stats(&self) -> CacheStats {
        let total = self.hits + self.misses;
        let hit_rate = if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        };
        
        CacheStats {
            hits: self.hits,
            misses: self.misses,
            entries: self.entries.len(),
            hit_rate,
        }
    }
    
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for PromptCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    #[test]
    fn test_cache_put_get() {
        let mut cache = PromptCache::new();
        let key = "test_key".to_string();
        let prompt = Arc::new("test prompt".to_string());
        
        cache.put(key.clone(), prompt.clone());
        
        let result = cache.get(&key);
        assert!(result.is_some());
        assert_eq!(*result.unwrap(), "test prompt");
    }
    
    #[test]
    fn test_cache_miss() {
        let mut cache = PromptCache::new();
        
        let result = cache.get("nonexistent");
        assert!(result.is_none());
        
        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 0);
    }
    
    #[test]
    fn test_cache_hit_rate() {
        let mut cache = PromptCache::new();
        
        cache.put("key1".to_string(), Arc::new("prompt1".to_string()));
        
        cache.get("key1");
        cache.get("key1");
        cache.get("key2");
        
        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 0.666).abs() < 0.01);
    }
    
    #[test]
    fn test_cache_eviction() {
        let mut cache = PromptCache::with_config(3, 3600);
        
        cache.put("key1".to_string(), Arc::new("prompt1".to_string()));
        cache.put("key2".to_string(), Arc::new("prompt2".to_string()));
        cache.put("key3".to_string(), Arc::new("prompt3".to_string()));
        
        assert_eq!(cache.len(), 3);
        
        cache.put("key4".to_string(), Arc::new("prompt4".to_string()));
        
        assert_eq!(cache.len(), 3);
    }
    
    #[test]
    fn test_cache_clear() {
        let mut cache = PromptCache::new();
        
        cache.put("key1".to_string(), Arc::new("prompt1".to_string()));
        cache.put("key2".to_string(), Arc::new("prompt2".to_string()));
        
        assert_eq!(cache.len(), 2);
        
        cache.clear();
        
        assert_eq!(cache.len(), 0);
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }
    
    #[test]
    fn test_generate_key() {
        let cache = PromptCache::new();
        let path = PathBuf::from("/test/workspace");
        
        let key1 = cache.generate_key(&path, "gpt-4", "hello", super::super::TaskType::Quick);
        let key2 = cache.generate_key(&path, "gpt-4", "hello", super::super::TaskType::Quick);
        let key3 = cache.generate_key(&path, "gpt-4", "world", super::super::TaskType::Quick);
        
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }
    
    #[test]
    fn test_cache_stats() {
        let mut cache = PromptCache::new();
        
        cache.put("key1".to_string(), Arc::new("prompt1".to_string()));
        
        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.hit_rate, 0.0);
        
        cache.get("key1");
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.hit_rate, 1.0);
    }
}
