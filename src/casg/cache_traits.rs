/// Traits for caching systems
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Base trait for cache operations
pub trait Cache<K, V>: Send + Sync
where
    K: Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// Get a value from cache
    fn get(&self, key: &K) -> Option<V>;

    /// Put a value in cache
    fn put(&self, key: K, value: V) -> Result<()>;

    /// Remove a value from cache
    fn remove(&self, key: &K) -> Result<()>;

    /// Clear the entire cache
    fn clear(&self) -> Result<()>;

    /// Check if key exists in cache
    fn contains(&self, key: &K) -> bool;

    /// Get cache statistics
    fn stats(&self) -> CacheStats;
}

/// Trait for eviction policies
pub trait EvictionPolicy<K>: Send + Sync {
    /// Determine which key to evict
    fn select_victim(&self, keys: &[K]) -> Option<K>;

    /// Update access information
    fn on_access(&mut self, key: &K);

    /// Update on insertion
    fn on_insert(&mut self, key: &K);

    /// Update on removal
    fn on_remove(&mut self, key: &K);
}

/// Trait for cache persistence
pub trait PersistentCache<K, V>: Cache<K, V>
where
    K: Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// Save cache to persistent storage
    fn persist(&self) -> Result<()>;

    /// Load cache from persistent storage
    fn restore(&mut self) -> Result<()>;

    /// Get path to persistent storage
    fn storage_path(&self) -> &std::path::Path;
}

/// Trait for TTL (time-to-live) based caching
pub trait TTLCache<K, V>: Cache<K, V>
where
    K: Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// Put a value with TTL
    fn put_with_ttl(&self, key: K, value: V, ttl: Duration) -> Result<()>;

    /// Get remaining TTL for a key
    fn ttl(&self, key: &K) -> Option<Duration>;

    /// Refresh TTL for a key
    fn refresh_ttl(&self, key: &K, ttl: Duration) -> Result<()>;

    /// Remove expired entries
    fn cleanup_expired(&self) -> Result<usize>;
}

/// Trait for cache warming/preloading
pub trait WarmableCache<K, V>: Cache<K, V>
where
    K: Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// Preload cache with data
    fn warm(&mut self, data: Vec<(K, V)>) -> Result<()>;

    /// Preload cache from a source
    fn warm_from_source<F>(&mut self, source: F) -> Result<()>
    where
        F: Fn() -> Result<Vec<(K, V)>>;

    /// Get warmup progress
    fn warmup_progress(&self) -> f64;
}

/// Trait for distributed caching
pub trait DistributedCache<K, V>: Cache<K, V>
where
    K: Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// Get value from local or remote cache
    fn get_distributed(&self, key: &K) -> Result<Option<V>>;

    /// Put value to local and remote cache
    fn put_distributed(&self, key: K, value: V) -> Result<()>;

    /// Invalidate key across all nodes
    fn invalidate_distributed(&self, key: &K) -> Result<()>;

    /// Get list of cache nodes
    fn nodes(&self) -> Vec<String>;

    /// Get node responsible for key
    fn node_for_key(&self, key: &K) -> String;
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub entries: usize,
    pub total_size: usize,
    pub max_size: usize,
    pub hit_count: u64,
    pub miss_count: u64,
    pub eviction_count: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hit_count + self.miss_count;
        if total == 0 {
            0.0
        } else {
            self.hit_count as f64 / total as f64
        }
    }

    pub fn utilization(&self) -> f64 {
        if self.max_size == 0 {
            0.0
        } else {
            (self.total_size as f64) / (self.max_size as f64)
        }
    }

    pub fn average_entry_size(&self) -> usize {
        if self.entries == 0 {
            0
        } else {
            self.total_size / self.entries
        }
    }
}

/// LRU eviction policy
pub struct LRUPolicy<K> {
    access_order: Vec<K>,
}

impl<K: Clone + Eq> LRUPolicy<K> {
    pub fn new() -> Self {
        Self {
            access_order: Vec::new(),
        }
    }
}

impl<K: Clone + Eq + Send + Sync> EvictionPolicy<K> for LRUPolicy<K> {
    fn select_victim(&self, _keys: &[K]) -> Option<K> {
        self.access_order.first().cloned()
    }

    fn on_access(&mut self, key: &K) {
        self.access_order.retain(|k| k != key);
        self.access_order.push(key.clone());
    }

    fn on_insert(&mut self, key: &K) {
        self.access_order.push(key.clone());
    }

    fn on_remove(&mut self, key: &K) {
        self.access_order.retain(|k| k != key);
    }
}

/// LFU (Least Frequently Used) eviction policy
pub struct LFUPolicy<K> {
    frequency_map: std::collections::HashMap<K, usize>,
}

impl<K: Clone + Eq + std::hash::Hash> LFUPolicy<K> {
    pub fn new() -> Self {
        Self {
            frequency_map: std::collections::HashMap::new(),
        }
    }
}

impl<K: Clone + Eq + std::hash::Hash + Send + Sync> EvictionPolicy<K> for LFUPolicy<K> {
    fn select_victim(&self, keys: &[K]) -> Option<K> {
        keys.iter()
            .min_by_key(|k| self.frequency_map.get(k).unwrap_or(&0))
            .cloned()
    }

    fn on_access(&mut self, key: &K) {
        *self.frequency_map.entry(key.clone()).or_insert(0) += 1;
    }

    fn on_insert(&mut self, key: &K) {
        self.frequency_map.insert(key.clone(), 1);
    }

    fn on_remove(&mut self, key: &K) {
        self.frequency_map.remove(key);
    }
}

/// FIFO eviction policy
pub struct FIFOPolicy<K> {
    insertion_order: Vec<K>,
}

impl<K: Clone> FIFOPolicy<K> {
    pub fn new() -> Self {
        Self {
            insertion_order: Vec::new(),
        }
    }
}

impl<K: Clone + Eq + Send + Sync> EvictionPolicy<K> for FIFOPolicy<K> {
    fn select_victim(&self, _keys: &[K]) -> Option<K> {
        self.insertion_order.first().cloned()
    }

    fn on_access(&mut self, _key: &K) {
        // FIFO doesn't care about access
    }

    fn on_insert(&mut self, key: &K) {
        self.insertion_order.push(key.clone());
    }

    fn on_remove(&mut self, key: &K) {
        self.insertion_order.retain(|k| k != key);
    }
}

/// Random eviction policy
pub struct RandomPolicy;

impl<K: Clone + Send + Sync> EvictionPolicy<K> for RandomPolicy {
    fn select_victim(&self, keys: &[K]) -> Option<K> {
        use rand::prelude::*;
        let mut rng = thread_rng();
        keys.choose(&mut rng).cloned()
    }

    fn on_access(&mut self, _key: &K) {}
    fn on_insert(&mut self, _key: &K) {}
    fn on_remove(&mut self, _key: &K) {}
}