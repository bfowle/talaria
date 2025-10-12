/// Metadata cache for expensive RocksDB queries
///
/// Caches results of expensive operations like listing databases, versions, and statistics.
/// Cache is invalidated when databases are added, deleted, or updated.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use crate::database::manager::{DatabaseInfo, HeraldStats};
use talaria_core::types::DatabaseVersionInfo;

const CACHE_VERSION: u32 = 1;
const DEFAULT_TTL_SECS: u64 = 300; // 5 minutes

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedDatabaseList {
    databases: Vec<DatabaseInfo>,
    timestamp: SystemTime,
    version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedVersionList {
    versions: Vec<DatabaseVersionInfo>,
    timestamp: SystemTime,
    version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedStats {
    stats: HeraldStats,
    timestamp: SystemTime,
    version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMetadata {
    last_write: SystemTime,
    rocksdb_sst_count: usize,
    rocksdb_total_size: u64,
}

/// In-memory and persistent cache for database metadata
pub struct MetadataCache {
    cache_dir: PathBuf,
    ttl: Duration,

    // In-memory caches (fast access)
    database_list: Arc<RwLock<Option<CachedDatabaseList>>>,
    version_lists: Arc<RwLock<HashMap<String, CachedVersionList>>>,
    stats: Arc<RwLock<Option<CachedStats>>>,

    // Cache metadata for invalidation
    metadata: Arc<RwLock<Option<CacheMetadata>>>,
}

impl MetadataCache {
    /// Create a new metadata cache
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            cache_dir,
            ttl: Duration::from_secs(DEFAULT_TTL_SECS),
            database_list: Arc::new(RwLock::new(None)),
            version_lists: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(None)),
            metadata: Arc::new(RwLock::new(None)),
        })
    }

    /// Set cache TTL
    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.ttl = Duration::from_secs(ttl_secs);
        self
    }

    /// Get cached database list if valid
    pub fn get_database_list(&self) -> Option<Vec<DatabaseInfo>> {
        let cache = self.database_list.read().ok()?;
        let cached = cache.as_ref()?;

        // Check if cache is still valid
        if self.is_expired(cached.timestamp) {
            return None;
        }

        Some(cached.databases.clone())
    }

    /// Cache database list
    pub fn set_database_list(&self, databases: Vec<DatabaseInfo>) -> Result<()> {
        let cached = CachedDatabaseList {
            databases: databases.clone(),
            timestamp: SystemTime::now(),
            version: CACHE_VERSION,
        };

        // Update in-memory cache
        if let Ok(mut cache) = self.database_list.write() {
            *cache = Some(cached.clone());
        }

        // Persist to disk
        let cache_file = self.cache_dir.join("database_list.json");
        let json = serde_json::to_string_pretty(&cached)?;
        std::fs::write(cache_file, json)?;

        Ok(())
    }

    /// Get cached version list for a specific database
    pub fn get_version_list(
        &self,
        source: &str,
        dataset: &str,
    ) -> Option<Vec<DatabaseVersionInfo>> {
        let key = format!("{}:{}", source, dataset);
        let cache = self.version_lists.read().ok()?;
        let cached = cache.get(&key)?;

        // Check if cache is still valid
        if self.is_expired(cached.timestamp) {
            return None;
        }

        Some(cached.versions.clone())
    }

    /// Cache version list for a specific database
    pub fn set_version_list(
        &self,
        source: &str,
        dataset: &str,
        versions: Vec<DatabaseVersionInfo>,
    ) -> Result<()> {
        let key = format!("{}:{}", source, dataset);

        let cached = CachedVersionList {
            versions: versions.clone(),
            timestamp: SystemTime::now(),
            version: CACHE_VERSION,
        };

        // Update in-memory cache
        if let Ok(mut cache) = self.version_lists.write() {
            cache.insert(key.clone(), cached.clone());
        }

        // Persist to disk
        let cache_file = self
            .cache_dir
            .join(format!("versions_{}.json", key.replace('/', "_")));
        let json = serde_json::to_string_pretty(&cached)?;
        std::fs::write(cache_file, json)?;

        Ok(())
    }

    /// Get cached stats if valid
    pub fn get_stats(&self) -> Option<HeraldStats> {
        let cache = self.stats.read().ok()?;
        let cached = cache.as_ref()?;

        // Check if cache is still valid
        if self.is_expired(cached.timestamp) {
            return None;
        }

        Some(cached.stats.clone())
    }

    /// Cache stats
    pub fn set_stats(&self, stats: HeraldStats) -> Result<()> {
        let cached = CachedStats {
            stats: stats.clone(),
            timestamp: SystemTime::now(),
            version: CACHE_VERSION,
        };

        // Update in-memory cache
        if let Ok(mut cache) = self.stats.write() {
            *cache = Some(cached.clone());
        }

        // Persist to disk
        let cache_file = self.cache_dir.join("stats.json");
        let json = serde_json::to_string_pretty(&cached)?;
        std::fs::write(cache_file, json)?;

        Ok(())
    }

    /// Invalidate just the database list cache (lighter than invalidate_all)
    pub fn invalidate_database_list(&self) {
        if let Ok(mut cache) = self.database_list.write() {
            *cache = None;
        }
        // Clear persistent cache file
        let _ = std::fs::remove_file(self.cache_dir.join("database_list.json"));
    }

    /// Invalidate all caches (call when database changes)
    pub fn invalidate_all(&self) {
        if let Ok(mut cache) = self.database_list.write() {
            *cache = None;
        }
        if let Ok(mut cache) = self.version_lists.write() {
            cache.clear();
        }
        if let Ok(mut cache) = self.stats.write() {
            *cache = None;
        }

        // Clear persistent caches
        let _ = std::fs::remove_dir_all(&self.cache_dir);
        let _ = std::fs::create_dir_all(&self.cache_dir);
    }

    /// Invalidate cache for a specific database
    pub fn invalidate_database(&self, source: &str, dataset: &str) {
        let key = format!("{}:{}", source, dataset);

        // Invalidate version list for this database
        if let Ok(mut cache) = self.version_lists.write() {
            cache.remove(&key);
        }

        // Invalidate global caches too (database list and stats affected)
        if let Ok(mut cache) = self.database_list.write() {
            *cache = None;
        }
        if let Ok(mut cache) = self.stats.write() {
            *cache = None;
        }

        // Clear persistent cache files
        let version_cache = self
            .cache_dir
            .join(format!("versions_{}.json", key.replace('/', "_")));
        let _ = std::fs::remove_file(version_cache);
        let _ = std::fs::remove_file(self.cache_dir.join("database_list.json"));
        let _ = std::fs::remove_file(self.cache_dir.join("stats.json"));
    }

    /// Load caches from disk on startup
    pub fn load_from_disk(&self) -> Result<()> {
        // Load database list
        if let Ok(json) = std::fs::read_to_string(self.cache_dir.join("database_list.json")) {
            if let Ok(cached) = serde_json::from_str::<CachedDatabaseList>(&json) {
                if cached.version == CACHE_VERSION && !self.is_expired(cached.timestamp) {
                    if let Ok(mut cache) = self.database_list.write() {
                        *cache = Some(cached);
                    }
                }
            }
        }

        // Load stats
        if let Ok(json) = std::fs::read_to_string(self.cache_dir.join("stats.json")) {
            if let Ok(cached) = serde_json::from_str::<CachedStats>(&json) {
                if cached.version == CACHE_VERSION && !self.is_expired(cached.timestamp) {
                    if let Ok(mut cache) = self.stats.write() {
                        *cache = Some(cached);
                    }
                }
            }
        }

        // Load version lists
        for entry in std::fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(filename) = path.file_name() {
                let filename_str = filename.to_string_lossy();
                if filename_str.starts_with("versions_") && filename_str.ends_with(".json") {
                    if let Ok(json) = std::fs::read_to_string(&path) {
                        if let Ok(cached) = serde_json::from_str::<CachedVersionList>(&json) {
                            if cached.version == CACHE_VERSION && !self.is_expired(cached.timestamp)
                            {
                                // Extract key from filename
                                let key = filename_str
                                    .trim_start_matches("versions_")
                                    .trim_end_matches(".json")
                                    .replace('_', "/");

                                if let Ok(mut cache) = self.version_lists.write() {
                                    cache.insert(key, cached);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if timestamp is expired based on TTL
    fn is_expired(&self, timestamp: SystemTime) -> bool {
        if let Ok(elapsed) = timestamp.elapsed() {
            elapsed > self.ttl
        } else {
            true // If we can't determine elapsed time, consider expired
        }
    }

    /// Check if RocksDB has changed (for smart invalidation)
    pub fn check_rocksdb_changed(&self, rocksdb_path: &Path) -> Result<bool> {
        // Count SST files and total size
        let mut sst_count = 0;
        let mut total_size = 0u64;

        if rocksdb_path.exists() {
            for entry in std::fs::read_dir(rocksdb_path)? {
                let entry = entry?;
                if let Some(ext) = entry.path().extension() {
                    if ext == "sst" {
                        sst_count += 1;
                        if let Ok(meta) = entry.metadata() {
                            total_size += meta.len();
                        }
                    }
                }
            }
        }

        let current_meta = CacheMetadata {
            last_write: SystemTime::now(),
            rocksdb_sst_count: sst_count,
            rocksdb_total_size: total_size,
        };

        // Check against stored metadata
        if let Ok(meta_guard) = self.metadata.read() {
            if let Some(prev_meta) = meta_guard.as_ref() {
                let changed = prev_meta.rocksdb_sst_count != current_meta.rocksdb_sst_count
                    || prev_meta.rocksdb_total_size != current_meta.rocksdb_total_size;

                drop(meta_guard);

                if changed {
                    // Update metadata
                    if let Ok(mut meta_guard) = self.metadata.write() {
                        *meta_guard = Some(current_meta);
                    }
                }

                return Ok(changed);
            }
        }

        // First time - store metadata
        if let Ok(mut meta_guard) = self.metadata.write() {
            *meta_guard = Some(current_meta);
        }

        Ok(false) // No change on first check
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cache_database_list() {
        let temp_dir = TempDir::new().unwrap();
        let cache = MetadataCache::new(temp_dir.path().to_path_buf()).unwrap();

        let databases = vec![DatabaseInfo {
            name: "test/db1".to_string(),
            version: "v1".to_string(),
            created_at: chrono::Utc::now(),
            chunk_count: 10,
            sequence_count: 100,
            total_size: 1000,
            reduction_profiles: vec![],
        }];

        // Cache should be empty initially
        assert!(cache.get_database_list().is_none());

        // Set cache
        cache.set_database_list(databases.clone()).unwrap();

        // Should be able to retrieve
        let cached = cache.get_database_list().unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].name, "test/db1");
    }

    #[test]
    fn test_cache_expiration() {
        let temp_dir = TempDir::new().unwrap();
        let cache = MetadataCache::new(temp_dir.path().to_path_buf())
            .unwrap()
            .with_ttl(1); // 1 second TTL

        let databases = vec![];
        cache.set_database_list(databases).unwrap();

        // Should be cached immediately
        assert!(cache.get_database_list().is_some());

        // Wait for expiration
        std::thread::sleep(Duration::from_secs(2));

        // Should be expired now
        assert!(cache.get_database_list().is_none());
    }

    #[test]
    fn test_cache_invalidation() {
        let temp_dir = TempDir::new().unwrap();
        let cache = MetadataCache::new(temp_dir.path().to_path_buf()).unwrap();

        let databases = vec![];
        cache.set_database_list(databases).unwrap();

        // Should be cached
        assert!(cache.get_database_list().is_some());

        // Invalidate
        cache.invalidate_all();

        // Should be cleared
        assert!(cache.get_database_list().is_none());
    }
}
