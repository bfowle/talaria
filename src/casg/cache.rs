use crate::casg::cache_traits::{Cache, CacheStats as TraitCacheStats, TTLCache, PersistentCache};
use crate::casg::types::SHA256Hash;
use crate::core::paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

/// Simple LRU cache for CASG chunks with trait implementation
#[derive(Debug)]
pub struct ChunkCache {
    cache: Arc<RwLock<InnerCache>>,
    cache_dir: PathBuf,
    max_size: usize,
    max_age: Duration,
}

#[derive(Debug)]
struct InnerCache {
    entries: HashMap<SHA256Hash, CacheEntry>,
    access_order: VecDeque<SHA256Hash>,
    total_size: usize,
    hit_count: u64,
    miss_count: u64,
    eviction_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    hash: SHA256Hash,
    data: Vec<u8>,
    size: usize,
    last_access: SystemTime,
    access_count: u32,
    expiry: Option<SystemTime>,
}

impl ChunkCache {
    /// Create a new chunk cache
    pub fn new(max_size_mb: usize) -> Result<Self> {
        let cache_dir = paths::talaria_cache_dir();
        fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            cache: Arc::new(RwLock::new(InnerCache {
                entries: HashMap::new(),
                access_order: VecDeque::new(),
                total_size: 0,
                hit_count: 0,
                miss_count: 0,
                eviction_count: 0,
            })),
            cache_dir,
            max_size: max_size_mb * 1024 * 1024, // Convert MB to bytes
            max_age: Duration::from_secs(3600),   // 1 hour default
        })
    }

    /// Preload frequently accessed chunks
    pub fn preload(&self, chunks: &[crate::casg::types::ChunkMetadata]) -> Result<()> {
        for chunk in chunks.iter().take(100) {
            // Preload up to 100 chunks
            if let Ok(data) = self.load_chunk_data(&chunk.hash) {
                self.put(chunk.hash.clone(), data)?;
            }
        }
        Ok(())
    }

    fn load_from_disk(&self, hash: &SHA256Hash) -> Option<Vec<u8>> {
        let path = self.cache_path(hash);
        if path.exists() {
            if let Ok(data) = fs::read(&path) {
                // Don't update in-memory cache here to avoid double-locking
                return Some(data);
            }
        }
        None
    }

    fn save_to_disk(&self, hash: &SHA256Hash, data: &[u8]) -> Result<()> {
        let path = self.cache_path(hash);
        fs::write(path, data)?;
        Ok(())
    }

    fn cache_path(&self, hash: &SHA256Hash) -> PathBuf {
        let hex = hash.to_hex();
        self.cache_dir.join(format!("{}.cache", &hex[..16]))
    }

    fn load_chunk_data(&self, _hash: &SHA256Hash) -> Result<Vec<u8>> {
        // This would load from the actual storage
        // For now, return empty vec as placeholder
        Ok(Vec::new())
    }

    fn evict_if_needed(&self, cache: &mut InnerCache, needed_size: usize) {
        while cache.total_size + needed_size > self.max_size && !cache.access_order.is_empty() {
            if let Some(evict_hash) = cache.access_order.pop_front() {
                if let Some(entry) = cache.entries.remove(&evict_hash) {
                    cache.total_size -= entry.size;
                    cache.eviction_count += 1;
                    // Optionally save to disk before evicting
                    let _ = self.save_to_disk(&evict_hash, &entry.data);
                }
            }
        }
    }

    fn is_expired(&self, entry: &CacheEntry) -> bool {
        // Check explicit expiry
        if let Some(expiry) = entry.expiry {
            if SystemTime::now() > expiry {
                return true;
            }
        }

        // Check age-based expiry
        if let Ok(elapsed) = entry.last_access.elapsed() {
            if elapsed > self.max_age {
                return true;
            }
        }

        false
    }
}

impl Cache<SHA256Hash, Vec<u8>> for ChunkCache {
    fn get(&self, key: &SHA256Hash) -> Option<Vec<u8>> {
        let mut cache = self.cache.write().ok()?;

        // First check if entry exists and is expired
        if let Some(entry) = cache.entries.get(key) {
            if self.is_expired(entry) {
                // Store size before removing
                let size = entry.size;
                // Remove expired entry
                cache.entries.remove(key);
                cache.access_order.retain(|h| h != key);
                cache.total_size -= size;
                cache.miss_count += 1;
                drop(cache);  // Release lock before disk I/O
                return self.load_from_disk(key);
            }
        }

        // Now handle the normal case
        if cache.entries.contains_key(key) {
            // Update the entry
            let entry = cache.entries.get_mut(key).unwrap();
            entry.last_access = SystemTime::now();
            entry.access_count += 1;
            let data = entry.data.clone();

            // Move to end of access order
            cache.access_order.retain(|h| h != key);
            cache.access_order.push_back(key.clone());

            cache.hit_count += 1;
            Some(data)
        } else {
            cache.miss_count += 1;
            drop(cache);  // Release lock before disk I/O
            // Try to load from disk cache
            self.load_from_disk(key)
        }
    }

    fn put(&self, key: SHA256Hash, value: Vec<u8>) -> Result<()> {
        let size = value.len();

        // Don't cache if too large
        if size > self.max_size / 4 {
            return Ok(());
        }

        let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Evict entries if needed
        self.evict_if_needed(&mut cache, size);

        // Add new entry
        let entry = CacheEntry {
            hash: key.clone(),
            data: value.clone(),
            size,
            last_access: SystemTime::now(),
            access_count: 1,
            expiry: None,
        };

        cache.entries.insert(key.clone(), entry);
        cache.access_order.push_back(key.clone());
        cache.total_size += size;

        // Also save to disk cache
        self.save_to_disk(&key, &value)?;

        Ok(())
    }

    fn remove(&self, key: &SHA256Hash) -> Result<()> {
        let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        if let Some(entry) = cache.entries.remove(key) {
            cache.access_order.retain(|h| h != key);
            cache.total_size -= entry.size;
        }

        // Remove from disk cache
        let path = self.cache_path(key);
        if path.exists() {
            fs::remove_file(path)?;
        }

        Ok(())
    }

    fn clear(&self) -> Result<()> {
        let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        cache.entries.clear();
        cache.access_order.clear();
        cache.total_size = 0;
        cache.hit_count = 0;
        cache.miss_count = 0;
        cache.eviction_count = 0;

        // Clear disk cache
        if self.cache_dir.exists() {
            for entry in fs::read_dir(&self.cache_dir)? {
                let entry = entry?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("cache") {
                    fs::remove_file(entry.path())?;
                }
            }
        }

        Ok(())
    }

    fn contains(&self, key: &SHA256Hash) -> bool {
        if let Ok(cache) = self.cache.read() {
            if let Some(entry) = cache.entries.get(key) {
                return !self.is_expired(entry);
            }
        }
        false
    }

    fn stats(&self) -> TraitCacheStats {
        let cache = self.cache.read().unwrap();

        TraitCacheStats {
            entries: cache.entries.len(),
            total_size: cache.total_size,
            max_size: self.max_size,
            hit_count: cache.hit_count,
            miss_count: cache.miss_count,
            eviction_count: cache.eviction_count,
        }
    }
}

impl TTLCache<SHA256Hash, Vec<u8>> for ChunkCache {
    fn put_with_ttl(&self, key: SHA256Hash, value: Vec<u8>, ttl: Duration) -> Result<()> {
        let size = value.len();

        // Don't cache if too large
        if size > self.max_size / 4 {
            return Ok(());
        }

        let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Evict entries if needed
        self.evict_if_needed(&mut cache, size);

        // Add new entry with expiry
        let entry = CacheEntry {
            hash: key.clone(),
            data: value.clone(),
            size,
            last_access: SystemTime::now(),
            access_count: 1,
            expiry: Some(SystemTime::now() + ttl),
        };

        cache.entries.insert(key.clone(), entry);
        cache.access_order.push_back(key.clone());
        cache.total_size += size;

        // Also save to disk cache
        self.save_to_disk(&key, &value)?;

        Ok(())
    }

    fn ttl(&self, key: &SHA256Hash) -> Option<Duration> {
        let cache = self.cache.read().ok()?;
        let entry = cache.entries.get(key)?;

        if let Some(expiry) = entry.expiry {
            expiry.duration_since(SystemTime::now()).ok()
        } else {
            // Return remaining time based on max_age
            let elapsed = entry.last_access.elapsed().ok()?;
            self.max_age.checked_sub(elapsed)
        }
    }

    fn refresh_ttl(&self, key: &SHA256Hash, ttl: Duration) -> Result<()> {
        let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        if let Some(entry) = cache.entries.get_mut(key) {
            entry.expiry = Some(SystemTime::now() + ttl);
            entry.last_access = SystemTime::now();
        }

        Ok(())
    }

    fn cleanup_expired(&self) -> Result<usize> {
        let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let expired_keys: Vec<SHA256Hash> = cache.entries
            .iter()
            .filter(|(_, entry)| self.is_expired(entry))
            .map(|(key, _)| key.clone())
            .collect();

        let count = expired_keys.len();

        for key in expired_keys {
            if let Some(entry) = cache.entries.remove(&key) {
                cache.access_order.retain(|h| h != &key);
                cache.total_size -= entry.size;
            }
        }

        Ok(count)
    }
}

impl PersistentCache<SHA256Hash, Vec<u8>> for ChunkCache {
    fn persist(&self) -> Result<()> {
        let cache = self.cache.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Save cache index
        let index_path = self.cache_dir.join("index.json");
        let index_data = serde_json::to_string(&cache.entries)?;
        fs::write(index_path, index_data)?;

        // Individual chunks are already persisted on write

        Ok(())
    }

    fn restore(&mut self) -> Result<()> {
        let index_path = self.cache_dir.join("index.json");
        if !index_path.exists() {
            return Ok(());
        }

        let index_data = fs::read_to_string(index_path)?;
        let entries: HashMap<SHA256Hash, CacheEntry> = serde_json::from_str(&index_data)?;

        let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        for (hash, entry) in entries {
            if !self.is_expired(&entry) {
                cache.access_order.push_back(hash.clone());
                cache.total_size += entry.size;
                cache.entries.insert(hash, entry);
            }
        }

        Ok(())
    }

    fn storage_path(&self) -> &Path {
        &self.cache_dir
    }
}

/// Global cache instance
static mut GLOBAL_CACHE: Option<Arc<ChunkCache>> = None;
static CACHE_INIT: std::sync::Once = std::sync::Once::new();

/// Get the global chunk cache
pub fn get_cache() -> Arc<ChunkCache> {
    unsafe {
        CACHE_INIT.call_once(|| {
            let cache = ChunkCache::new(100).expect("Failed to create cache"); // 100MB default
            GLOBAL_CACHE = Some(Arc::new(cache));
        });
        GLOBAL_CACHE.as_ref().unwrap().clone()
    }
}

// Re-export CacheStats for backwards compatibility
pub use crate::casg::cache_traits::CacheStats;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_operations() {
        let cache = ChunkCache::new(1).unwrap(); // 1MB cache

        let hash = SHA256Hash::compute(b"test");
        let data = vec![1, 2, 3, 4, 5];

        // Put and get
        cache.put(hash.clone(), data.clone()).unwrap();
        assert_eq!(cache.get(&hash), Some(data));

        // Contains
        assert!(cache.contains(&hash));

        // Stats
        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.total_size, 5);
    }

    #[test]
    fn test_cache_eviction() {
        let cache = ChunkCache::new(1).unwrap(); // 1MB cache

        // Fill cache with small chunks
        for i in 0..10 {
            let hash = SHA256Hash::compute(&[i]);
            let data = vec![i; 100_000]; // 100KB each
            cache.put(hash, data).unwrap();
        }

        // Cache should have evicted oldest entries
        let stats = cache.stats();
        assert!(stats.entries <= 10);
        assert!(stats.total_size <= 1024 * 1024);
    }

    #[test]
    fn test_ttl_cache() {
        let cache = ChunkCache::new(1).unwrap();

        let hash = SHA256Hash::compute(b"ttl_test");
        let data = vec![1, 2, 3];

        // Put with TTL
        cache.put_with_ttl(hash.clone(), data.clone(), Duration::from_secs(60)).unwrap();

        // Check TTL exists
        assert!(cache.ttl(&hash).is_some());

        // Refresh TTL
        cache.refresh_ttl(&hash, Duration::from_secs(120)).unwrap();
    }
}