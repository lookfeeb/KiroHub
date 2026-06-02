#![allow(dead_code)]

use lru::LruCache;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

/// Token cache entry with TTL support
#[derive(Debug, Clone)]
pub struct TokenCacheEntry {
    pub tokens: usize,
    pub expires_at: Instant,
}

impl TokenCacheEntry {
    /// Create a new cache entry with a TTL of 5 minutes
    pub fn new(tokens: usize) -> Self {
        Self {
            tokens,
            expires_at: Instant::now() + Duration::from_secs(300), // 5 minutes
        }
    }

    /// Check if the entry has expired
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

/// LRU cache for token counts with TTL support
pub struct TokenCache {
    cache: LruCache<String, TokenCacheEntry>,
}

impl TokenCache {
    /// Create a new token cache with a capacity of 1000 entries
    pub fn new() -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(1000).unwrap_or(NonZeroUsize::MIN)),
        }
    }

    /// Get a token count from the cache, returning None if expired or not found
    pub fn get(&mut self, key: &str) -> Option<usize> {
        if let Some(entry) = self.cache.get(key) {
            if entry.is_expired() {
                // Remove expired entry
                self.cache.pop(key);
                None
            } else {
                Some(entry.tokens)
            }
        } else {
            None
        }
    }

    /// Insert a token count into the cache with a 5-minute TTL
    pub fn insert(&mut self, key: String, tokens: usize) {
        self.cache.put(key, TokenCacheEntry::new(tokens));
    }

    /// Clear all entries from the cache
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get the number of entries in the cache
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for TokenCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_token_cache_basic() {
        let mut cache = TokenCache::new();
        
        // Insert and retrieve
        cache.insert("key1".to_string(), 100);
        assert_eq!(cache.get("key1"), Some(100));
        
        // Non-existent key
        assert_eq!(cache.get("key2"), None);
    }

    #[test]
    fn test_token_cache_ttl() {
        let mut cache = TokenCache::new();
        
        // Insert with custom TTL for testing
        let key = "test_key".to_string();
        let entry = TokenCacheEntry {
            tokens: 100,
            expires_at: Instant::now() + Duration::from_millis(100),
        };
        cache.cache.put(key.clone(), entry);
        
        // Should be available immediately
        assert_eq!(cache.get(&key), Some(100));
        
        // Wait for expiration
        sleep(Duration::from_millis(150));
        
        // Should be expired and removed
        assert_eq!(cache.get(&key), None);
    }

    #[test]
    fn test_token_cache_lru() {
        let mut cache = TokenCache::new();
        
        // Fill cache beyond capacity (1000 entries)
        for i in 0..1001 {
            cache.insert(format!("key{}", i), i);
        }
        
        // First entry should be evicted
        assert_eq!(cache.get("key0"), None);
        
        // Last entry should still be there
        assert_eq!(cache.get("key1000"), Some(1000));
    }
}
