/// RocksDB storage backend implementation for Herald
///
/// This module provides high-performance storage using RocksDB's
/// LSM-tree architecture for optimal write performance and scalability.
use anyhow::{anyhow, Context, Result};
use rocksdb::{
    backup::{BackupEngine, BackupEngineOptions, RestoreOptions},
    BlockBasedOptions, BoundColumnFamily, Cache, ColumnFamilyDescriptor, DBWithThreadMode,
    IteratorMode, MultiThreaded, Options, WriteBatch, WriteOptions, DB,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::types::{CanonicalSequence, SequenceRepresentations, SequenceStorageBackend};
use talaria_core::types::{SHA256Hash, TaxonId};
use talaria_core::StorageStats;

/// Column family names for different data types
pub mod cf_names {
    pub const DEFAULT: &str = "default";
    pub const SEQUENCES: &str = "sequences";
    pub const REPRESENTATIONS: &str = "representations";
    pub const MANIFESTS: &str = "manifests";
    pub const INDICES: &str = "indices";
    pub const MERKLE: &str = "merkle";
    pub const TEMPORAL: &str = "temporal";
}

/// RocksDB configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocksDBConfig {
    /// Base path for RocksDB data
    pub path: PathBuf,

    /// Write buffer size in MB (default: 256)
    pub write_buffer_size_mb: usize,

    /// Maximum number of write buffers (default: 4)
    pub max_write_buffer_number: usize,

    /// Target file size in MB (default: 256)
    pub target_file_size_mb: usize,

    /// Maximum background jobs (default: 16)
    pub max_background_jobs: i32,

    /// Block cache size in MB (default: 2048)
    pub block_cache_size_mb: usize,

    /// Bloom filter bits per key (default: 10)
    pub bloom_filter_bits: f64,

    /// Enable statistics collection (default: false)
    pub enable_statistics: bool,

    /// Compression algorithm (default: "zstd")
    pub compression: String,

    /// Compression level (default: 3)
    pub compression_level: i32,

    /// Optimization preset identifier (for tracking)
    pub optimize_for: String,
}

impl Default for RocksDBConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("~/.talaria/databases/rocksdb"),
            write_buffer_size_mb: 256,
            max_write_buffer_number: 4,
            target_file_size_mb: 256,
            max_background_jobs: 16,
            block_cache_size_mb: 2048,
            bloom_filter_bits: 15.0, // Increased from 10.0 for better accuracy
            enable_statistics: false,
            compression: "zstd".to_string(),
            compression_level: 3,
            optimize_for: "default".to_string(),
        }
    }
}

/// RocksDB storage backend
pub struct RocksDBBackend {
    /// RocksDB instance with multi-threaded column family support
    pub db: Arc<DBWithThreadMode<MultiThreaded>>,

    /// Configuration (kept for future use)
    #[allow(dead_code)]
    config: RocksDBConfig,

    /// Write options for batch operations
    write_opts: WriteOptions,
}

impl RocksDBBackend {
    // Chunk storage methods (for direct use by HeraldStorage)

    /// Check if a chunk exists
    pub fn chunk_exists(&self, hash: &SHA256Hash) -> Result<bool> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        Ok(self.db.get_cf(&cf, hash.as_bytes())?.is_some())
    }

    /// Store a chunk
    pub fn store_chunk(&self, hash: &SHA256Hash, data: &[u8]) -> Result<()> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        self.db
            .put_cf_opt(&cf, hash.as_bytes(), data, &self.write_opts)?;
        Ok(())
    }

    /// Store multiple chunks in a batch for better performance
    pub fn store_chunks_batch(&self, chunks: &[(SHA256Hash, Vec<u8>)]) -> Result<()> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        let mut batch = WriteBatch::default();

        for (hash, data) in chunks {
            batch.put_cf(&cf, hash.as_bytes(), data);
        }

        // Check if bulk import mode is enabled via environment variable
        let use_bulk_mode = std::env::var("TALARIA_BULK_IMPORT_MODE")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        if use_bulk_mode {
            // Use optimized write options for bulk imports:
            // - Disable WAL for maximum write throughput (~3-5x faster)
            // - No sync (fsync happens at end of import)
            let mut bulk_write_opts = WriteOptions::default();
            bulk_write_opts.set_sync(false);
            bulk_write_opts.disable_wal(true);
            self.db.write_opt(batch, &bulk_write_opts)?;
        } else {
            // Use normal write options (WAL enabled for durability)
            self.db.write_opt(batch, &self.write_opts)?;
        }

        Ok(())
    }

    /// Load a chunk
    pub fn load_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        self.db
            .get_cf(&cf, hash.as_bytes())?
            .ok_or_else(|| anyhow!("Chunk not found: {}", hash))
    }

    /// Get chunk size
    pub fn get_chunk_size(&self, hash: &SHA256Hash) -> Result<Option<usize>> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        Ok(self.db.get_cf(&cf, hash.as_bytes())?.map(|v| v.len()))
    }

    /// List all chunk hashes
    pub fn list_all_chunks(&self) -> Result<Vec<SHA256Hash>> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        let mut hashes = Vec::new();

        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for item in iter {
            let (key, _value) = item?;
            let hash = SHA256Hash::from_bytes(&key)?;
            hashes.push(hash);
        }

        Ok(hashes)
    }

    /// Delete a chunk from storage
    pub fn delete_chunk(&self, hash: &SHA256Hash) -> Result<()> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        self.db.delete_cf(&cf, hash.as_bytes())?;
        Ok(())
    }

    /// Delete multiple chunks in a batch for better performance
    pub fn delete_chunks_batch(&self, hashes: &[SHA256Hash]) -> Result<()> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        let mut batch = WriteBatch::default();

        for hash in hashes {
            batch.delete_cf(&cf, hash.as_bytes());
        }

        self.db.write_opt(batch, &self.write_opts)?;
        Ok(())
    }

    // Index operations for sequence indices stored in RocksDB

    /// Store an index entry
    pub fn put_index(&self, key: &str, value: &[u8]) -> Result<()> {
        let cf = self.cf_handle(cf_names::INDICES)?;
        self.db
            .put_cf_opt(&cf, key.as_bytes(), value, &self.write_opts)?;
        Ok(())
    }

    /// Get an index entry
    pub fn get_index(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let cf = self.cf_handle(cf_names::INDICES)?;
        Ok(self.db.get_cf(&cf, key.as_bytes())?)
    }

    /// Delete an index entry
    pub fn delete_index(&self, key: &str) -> Result<()> {
        let cf = self.cf_handle(cf_names::INDICES)?;
        self.db.delete_cf(&cf, key.as_bytes())?;
        Ok(())
    }

    /// Append to an index list (for taxonomy and database indices)
    pub fn append_to_index_list(&self, key: &str, hash: &SHA256Hash) -> Result<()> {
        let cf = self.cf_handle(cf_names::INDICES)?;

        // Get existing list or create new
        let mut hashes: Vec<SHA256Hash> = if let Some(data) = self.db.get_cf(&cf, key.as_bytes())? {
            bincode::deserialize(&data)?
        } else {
            Vec::new()
        };

        // Add hash if not already present
        if !hashes.contains(hash) {
            hashes.push(*hash);
            let data = bincode::serialize(&hashes)?;
            self.db
                .put_cf_opt(&cf, key.as_bytes(), &data, &self.write_opts)?;
        }

        Ok(())
    }

    /// Get an index list
    pub fn get_index_list(&self, key: &str) -> Result<Vec<SHA256Hash>> {
        let cf = self.cf_handle(cf_names::INDICES)?;

        if let Some(data) = self.db.get_cf(&cf, key.as_bytes())? {
            Ok(bincode::deserialize(&data)?)
        } else {
            Ok(Vec::new())
        }
    }

    /// Iterate over all indices with a given prefix
    pub fn iterate_index_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let cf = self.cf_handle(cf_names::INDICES)?;
        let mut results = Vec::new();

        let iter = self.db.prefix_iterator_cf(&cf, prefix.as_bytes());
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key).to_string();
            if !key_str.starts_with(prefix) {
                break; // Reached end of prefix range
            }
            results.push((key_str, value.to_vec()));
        }

        Ok(results)
    }

    /// Flush all pending writes to disk
    pub fn flush(&self) -> Result<()> {
        // Flush all column families
        let cfs = vec![
            cf_names::SEQUENCES,
            cf_names::REPRESENTATIONS,
            cf_names::MANIFESTS,
            cf_names::INDICES,
            cf_names::MERKLE,
            cf_names::TEMPORAL,
        ];

        for cf_name in cfs {
            let cf = self.cf_handle(cf_name)?;
            self.db.flush_cf(&cf)?;
        }
        Ok(())
    }

    /// Compact all column families to compress uncompressed L0/L1 data
    /// This is critical after bulk writes to ensure data is properly compressed
    pub fn compact(&self) -> Result<()> {
        use rocksdb::{BottommostLevelCompaction, CompactOptions};

        let cfs = vec![
            cf_names::SEQUENCES,
            cf_names::REPRESENTATIONS,
            cf_names::MANIFESTS,
            cf_names::INDICES,
            cf_names::MERKLE,
            cf_names::TEMPORAL,
        ];

        // Configure compaction options to force bottommost level compaction
        // This ensures ALL data gets compressed, not just upper levels
        let mut compact_opts = CompactOptions::default();
        compact_opts.set_bottommost_level_compaction(BottommostLevelCompaction::ForceOptimized);
        compact_opts.set_exclusive_manual_compaction(true);

        for cf_name in cfs {
            let cf = self.cf_handle(cf_name)?;
            eprintln!("  Compacting column family: {}", cf_name);
            // Compact the entire key range for this column family
            // ForceOptimized ensures bottommost level is compacted without double-compacting
            self.db
                .compact_range_cf_opt(&cf, None::<&[u8]>, None::<&[u8]>, &compact_opts);
        }
        Ok(())
    }

    /// Store a manifest entry
    pub fn put_manifest(&self, key: &str, value: &[u8]) -> Result<()> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        self.db
            .put_cf_opt(&cf, key.as_bytes(), value, &self.write_opts)?;
        Ok(())
    }

    /// Get a manifest entry
    pub fn get_manifest(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        Ok(self.db.get_cf(&cf, key.as_bytes())?)
    }

    /// List all manifests with prefix "manifest:"
    pub fn list_manifests(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        let mut manifests = Vec::new();

        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for item in iter {
            let (key, value) = item?;
            if let Ok(key_str) = String::from_utf8(key.to_vec()) {
                manifests.push((key_str, value.to_vec()));
            }
        }

        Ok(manifests)
    }

    /// List manifest keys with a specific prefix (more efficient than list_manifests for large datasets)
    pub fn list_manifest_keys_with_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        let mut keys = Vec::new();

        // Use prefix iterator for efficiency
        let iter = self.db.prefix_iterator_cf(&cf, prefix.as_bytes());
        for item in iter {
            let (key, _) = item?;
            if let Ok(key_str) = String::from_utf8(key.to_vec()) {
                // Check if key still has our prefix (prefix_iterator can overshoot)
                if key_str.starts_with(prefix) {
                    keys.push(key_str);
                } else {
                    break; // We've moved past our prefix
                }
            }
        }

        Ok(keys)
    }

    /// Delete a manifest entry
    pub fn delete_manifest(&self, key: &str) -> Result<()> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        self.db.delete_cf(&cf, key.as_bytes())?;
        Ok(())
    }

    /// Store lightweight database metadata for fast listing
    /// Key format: "db_meta:{source}:{dataset}"
    /// Stored in INDICES column family for performance
    pub fn put_database_metadata(
        &self,
        source: &str,
        dataset: &str,
        metadata: &[u8],
    ) -> Result<()> {
        let key = format!("db_meta:{}:{}", source, dataset);
        let cf = self.cf_handle(cf_names::INDICES)?;
        self.db
            .put_cf_opt(&cf, key.as_bytes(), metadata, &self.write_opts)?;
        Ok(())
    }

    /// Get database metadata
    pub fn get_database_metadata(&self, source: &str, dataset: &str) -> Result<Option<Vec<u8>>> {
        let key = format!("db_meta:{}:{}", source, dataset);
        let cf = self.cf_handle(cf_names::INDICES)?;
        Ok(self.db.get_cf(&cf, key.as_bytes())?)
    }

    /// List all database metadata entries (fast - only returns small metadata, not full manifests)
    pub fn list_database_metadata(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let cf = self.cf_handle(cf_names::INDICES)?;
        let mut metadata_list = Vec::new();

        let iter = self.db.prefix_iterator_cf(&cf, b"db_meta:");
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key).to_string();
            if !key_str.starts_with("db_meta:") {
                break; // Reached end of prefix range
            }
            metadata_list.push((key_str, value.to_vec()));
        }

        Ok(metadata_list)
    }

    /// Delete database metadata
    pub fn delete_database_metadata(&self, source: &str, dataset: &str) -> Result<()> {
        let key = format!("db_meta:{}:{}", source, dataset);
        let cf = self.cf_handle(cf_names::INDICES)?;
        self.db.delete_cf(&cf, key.as_bytes())?;
        Ok(())
    }

    /// Iterate over manifests with a given prefix
    pub fn iterate_manifest_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let cf = self.cf_handle(cf_names::MANIFESTS)?;
        let mut results = Vec::new();

        let iter = self.db.prefix_iterator_cf(&cf, prefix.as_bytes());
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key).to_string();
            if !key_str.starts_with(prefix) {
                break; // Reached end of prefix range
            }
            results.push((key_str, value.to_vec()));
        }

        Ok(results)
    }
    /// Create a new RocksDB backend with default configuration
    pub fn new(path: &Path) -> Result<Self> {
        let config = RocksDBConfig {
            path: path.to_path_buf(),
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Create with custom config but override path
    pub fn new_with_config(path: &Path, mut config: RocksDBConfig) -> Result<Self> {
        config.path = path.to_path_buf();
        Self::with_config(config)
    }

    /// Create a new RocksDB backend with custom configuration
    pub fn with_config(config: RocksDBConfig) -> Result<Self> {
        // Expand tilde in path
        let path = Self::expand_path(&config.path)?;

        // Create directory if it doesn't exist
        std::fs::create_dir_all(&path)?;

        // Configure column families
        let column_families = [
            cf_names::DEFAULT,
            cf_names::SEQUENCES,
            cf_names::REPRESENTATIONS,
            cf_names::MANIFESTS,
            cf_names::INDICES,
            cf_names::MERKLE,
            cf_names::TEMPORAL,
        ];

        // Create options for each column family
        let cf_descriptors: Vec<ColumnFamilyDescriptor> = column_families
            .iter()
            .map(|name| {
                let cf_opts = Self::create_cf_options(&config, name);
                ColumnFamilyDescriptor::new(*name, cf_opts)
            })
            .collect();

        // Create database options
        let db_opts = Self::create_db_options(&config)?;

        // Open or create database
        let db = DB::open_cf_descriptors(&db_opts, &path, cf_descriptors).map_err(|e| {
            eprintln!("RocksDB open error details: {:?}", e);
            anyhow::anyhow!(
                "Failed to open RocksDB at path: {}. Error: {}",
                path.display(),
                e
            )
        })?;

        // Configure write options
        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(false); // Don't sync on every write for performance
        write_opts.disable_wal(false); // Keep WAL for durability

        Ok(Self {
            db: Arc::new(db),
            config,
            write_opts,
        })
    }

    /// Create database options
    fn create_db_options(config: &RocksDBConfig) -> Result<Options> {
        let mut opts = Options::default();

        // Basic options
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_max_open_files(10000);

        // Performance options
        opts.set_max_background_jobs(config.max_background_jobs);
        opts.set_bytes_per_sync(1024 * 1024); // 1MB
        opts.increase_parallelism(num_cpus::get() as i32);

        // Compaction settings - ensure auto-compaction is active
        opts.set_level_compaction_dynamic_level_bytes(true);
        opts.set_max_bytes_for_level_base(512 * 1024 * 1024); // 512MB
        opts.set_max_bytes_for_level_multiplier(10.0);

        // Write buffer configuration
        opts.set_write_buffer_size(config.write_buffer_size_mb * 1024 * 1024);
        opts.set_max_write_buffer_number(config.max_write_buffer_number as i32);

        // Enable statistics if requested
        if config.enable_statistics {
            opts.enable_statistics();
        }

        // Compression
        let compression_type = match config.compression.as_str() {
            "zstd" => rocksdb::DBCompressionType::Zstd,
            "lz4" => rocksdb::DBCompressionType::Lz4,
            "snappy" => rocksdb::DBCompressionType::Snappy,
            "none" => rocksdb::DBCompressionType::None,
            _ => rocksdb::DBCompressionType::Zstd,
        };
        opts.set_compression_type(compression_type);

        // Set compression for each level
        // CRITICAL: Enable compression on ALL levels to prevent massive storage bloat
        // Previous config had L0/L1 uncompressed which caused 234GB of uncompressed data!
        let compression_per_level = vec![
            compression_type, // L0: Compress immediately (fixed from None)
            compression_type, // L1: Compress (fixed from None)
            compression_type, // L2+: Full compression
            compression_type,
            compression_type,
            compression_type,
            compression_type,
        ];
        opts.set_compression_per_level(&compression_per_level);

        Ok(opts)
    }

    /// Create column family options
    fn create_cf_options(config: &RocksDBConfig, cf_name: &str) -> Options {
        let mut opts = Options::default();

        // Basic options
        opts.set_write_buffer_size(config.write_buffer_size_mb * 1024 * 1024);
        opts.set_max_write_buffer_number(config.max_write_buffer_number as i32);
        opts.set_target_file_size_base((config.target_file_size_mb * 1024 * 1024) as u64);

        // Block-based table options
        let mut block_opts = BlockBasedOptions::default();

        // Block cache
        let cache = Cache::new_lru_cache(config.block_cache_size_mb * 1024 * 1024);
        block_opts.set_block_cache(&cache);

        // Bloom filter for faster lookups
        // Increased from 10.0 → 15.0 bits per key for better accuracy
        // This reduces false positive rate from ~1% to ~0.03%
        if config.bloom_filter_bits > 0.0 {
            block_opts.set_bloom_filter(config.bloom_filter_bits, false);
        }

        // Use block-based table
        opts.set_block_based_table_factory(&block_opts);

        // Optimize for specific column families
        match cf_name {
            cf_names::SEQUENCES => {
                // Sequences are large and accessed randomly
                opts.set_compression_type(rocksdb::DBCompressionType::Zstd);
                opts.set_bottommost_compression_type(rocksdb::DBCompressionType::Zstd);
            }
            cf_names::INDICES => {
                // Indices store large amounts of sequence->hash mappings
                // CRITICAL: Enable compression - these files can be 200MB+ each!
                opts.set_compression_type(rocksdb::DBCompressionType::Zstd);
                opts.optimize_for_point_lookup(512); // 512MB block cache
            }
            cf_names::MANIFESTS => {
                // Manifests are medium-sized and heavily queried for deduplication
                opts.set_compression_type(rocksdb::DBCompressionType::Zstd);

                // Use ribbon filter for manifests: better space efficiency than bloom
                // Ribbon filters use ~30% less memory for same false positive rate
                let mut manifest_block_opts = BlockBasedOptions::default();
                let cache = Cache::new_lru_cache(config.block_cache_size_mb * 1024 * 1024);
                manifest_block_opts.set_block_cache(&cache);
                manifest_block_opts.set_ribbon_filter(15.0); // Ribbon filter instead of bloom
                opts.set_block_based_table_factory(&manifest_block_opts);
            }
            _ => {
                // Default compression
                opts.set_compression_type(rocksdb::DBCompressionType::Zstd);
            }
        }

        opts
    }

    /// Expand tilde in path
    fn expand_path(path: &Path) -> Result<PathBuf> {
        let path_str = path.to_str().ok_or_else(|| anyhow!("Invalid path"))?;

        if path_str.starts_with("~") {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .context("Could not determine home directory")?;

            let expanded = path_str.replacen("~", &home, 1);
            Ok(PathBuf::from(expanded))
        } else {
            Ok(path.to_path_buf())
        }
    }

    /// Get a column family handle
    fn cf_handle(&self, name: &str) -> Result<Arc<BoundColumnFamily<'_>>> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| anyhow!("Column family '{}' not found", name))
    }

    /// Serialize value for storage
    fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>> {
        rmp_serde::to_vec(value).context("Failed to serialize value")
    }

    /// Deserialize value from storage
    fn deserialize<T: for<'de> Deserialize<'de>>(data: &[u8]) -> Result<T> {
        rmp_serde::from_slice(data).context("Failed to deserialize value")
    }

    // Index management methods for sequence cleanup

    /// Get hash by sequence ID
    pub fn get_hash_by_id(&self, id: &str) -> Result<SHA256Hash> {
        let cf = self.cf_handle(cf_names::INDICES)?;
        let key = format!("id:{}", id);
        match self.db.get_cf(&cf, key.as_bytes())? {
            Some(data) => SHA256Hash::from_bytes(&data),
            None => Err(anyhow::anyhow!("ID not found in index: {}", id)),
        }
    }

    /// Remove ID from index
    pub fn remove_id_index(&self, id: &str) -> Result<()> {
        let cf = self.cf_handle(cf_names::INDICES)?;
        let key = format!("id:{}", id);
        self.db.delete_cf(&cf, key.as_bytes())?;
        Ok(())
    }

    /// Get all hashes for a taxon
    pub fn get_hashes_by_taxon(&self, taxon_id: TaxonId) -> Result<Vec<SHA256Hash>> {
        let cf = self.cf_handle(cf_names::INDICES)?;
        let key = format!("taxon:{}", taxon_id.0);
        match self.db.get_cf(&cf, key.as_bytes())? {
            Some(data) => {
                // Deserialize list of hashes
                let hashes: Vec<SHA256Hash> = Self::deserialize(&data)?;
                Ok(hashes)
            }
            None => Ok(Vec::new()),
        }
    }

    /// Remove taxon from index
    pub fn remove_taxon_index(&self, taxon_id: TaxonId) -> Result<()> {
        let cf = self.cf_handle(cf_names::INDICES)?;
        let key = format!("taxon:{}", taxon_id.0);
        self.db.delete_cf(&cf, key.as_bytes())?;
        Ok(())
    }

    /// Update taxon index with new hash list
    pub fn update_taxon_index(&self, taxon_id: TaxonId, hashes: Vec<SHA256Hash>) -> Result<()> {
        let cf = self.cf_handle(cf_names::INDICES)?;
        let key = format!("taxon:{}", taxon_id.0);
        let value = Self::serialize(&hashes)?;
        self.db
            .put_cf_opt(&cf, key.as_bytes(), value, &self.write_opts)?;
        Ok(())
    }

    /// Get similar sequences (placeholder for now)
    pub fn get_similar_sequences(
        &self,
        _hash: &SHA256Hash,
        _threshold: f64,
    ) -> Result<Vec<SHA256Hash>> {
        // Similarity index not implemented yet in RocksDB
        // Would require a more complex index structure
        Ok(Vec::new())
    }

    /// Remove similarity edge (placeholder for now)
    pub fn remove_similarity_edge(&self, _from: &SHA256Hash, _to: &SHA256Hash) -> Result<()> {
        // Similarity index not implemented yet
        Ok(())
    }

    /// Get a sequence by hash
    pub fn get_sequence(&self, hash: &SHA256Hash) -> Result<CanonicalSequence> {
        let cf = self.cf_handle(cf_names::SEQUENCES)?;
        match self.db.get_cf(&cf, hash.as_bytes())? {
            Some(data) => Self::deserialize(&data),
            None => Err(anyhow::anyhow!("Sequence not found: {}", hash)),
        }
    }

    // ========== Backup Operations ==========

    /// Create a new backup of the database
    ///
    /// # Arguments
    /// * `backup_dir` - Directory to store the backup
    /// * `flush_before_backup` - Whether to flush memtable to disk before backup
    ///
    /// # Returns
    /// Backup ID of the created backup
    pub fn create_backup<P: AsRef<Path>>(
        &self,
        backup_dir: P,
        flush_before_backup: bool,
    ) -> Result<u32> {
        // Create backup directory if it doesn't exist
        let backup_path = backup_dir.as_ref();
        std::fs::create_dir_all(backup_path).context("Failed to create backup directory")?;

        // Initialize backup engine
        let mut backup_opts = BackupEngineOptions::new(backup_path)?;
        backup_opts.set_max_background_operations(4);

        let mut backup_engine = BackupEngine::open(&backup_opts, &rocksdb::Env::new()?)?;

        // Flush database if requested (ensures all data is persisted)
        if flush_before_backup {
            self.flush()?;
        }

        // Create the backup
        backup_engine
            .create_new_backup_flush(&self.db, flush_before_backup)
            .context("Failed to create backup")?;

        // Get info about all backups to return the latest ID
        let backup_info = backup_engine.get_backup_info();
        let backup_id = backup_info
            .last()
            .map(|info| info.backup_id)
            .ok_or_else(|| anyhow!("No backup created"))?;

        Ok(backup_id)
    }

    /// Restore database from the latest backup
    ///
    /// # Arguments
    /// * `backup_dir` - Directory containing backups
    /// * `restore_dir` - Directory to restore the database to
    ///
    /// # Note
    /// This will overwrite any existing data in restore_dir
    pub fn restore_from_latest_backup<P: AsRef<Path>>(backup_dir: P, restore_dir: P) -> Result<()> {
        let backup_path = backup_dir.as_ref();
        let restore_path = restore_dir.as_ref();

        // Create restore directory if it doesn't exist
        std::fs::create_dir_all(restore_path).context("Failed to create restore directory")?;

        // Initialize backup engine
        let backup_opts = BackupEngineOptions::new(backup_path)?;
        let mut backup_engine = BackupEngine::open(&backup_opts, &rocksdb::Env::new()?)?;

        // Restore from latest backup
        let restore_opts = RestoreOptions::default();
        backup_engine
            .restore_from_latest_backup(restore_path, restore_path, &restore_opts)
            .context("Failed to restore from backup")?;

        Ok(())
    }

    /// List all available backups
    ///
    /// # Arguments
    /// * `backup_dir` - Directory containing backups
    ///
    /// # Returns
    /// Vector of (backup_id, timestamp, size_bytes) tuples
    pub fn list_backups<P: AsRef<Path>>(backup_dir: P) -> Result<Vec<(u32, i64, u64)>> {
        let backup_path = backup_dir.as_ref();

        if !backup_path.exists() {
            return Ok(Vec::new());
        }

        // Initialize backup engine
        let backup_opts = BackupEngineOptions::new(backup_path)?;
        let backup_engine = BackupEngine::open(&backup_opts, &rocksdb::Env::new()?)?;

        // Get backup info
        let backup_info = backup_engine.get_backup_info();
        let backups = backup_info
            .into_iter()
            .map(|info| (info.backup_id, info.timestamp, info.size))
            .collect();

        Ok(backups)
    }

    /// Verify a specific backup
    ///
    /// # Arguments
    /// * `backup_dir` - Directory containing backups
    /// * `backup_id` - ID of backup to verify
    ///
    /// # Note
    /// This checks that files exist and sizes match, but does not verify checksums
    pub fn verify_backup<P: AsRef<Path>>(backup_dir: P, backup_id: u32) -> Result<()> {
        let backup_path = backup_dir.as_ref();

        // Initialize backup engine
        let backup_opts = BackupEngineOptions::new(backup_path)?;
        let backup_engine = BackupEngine::open(&backup_opts, &rocksdb::Env::new()?)?;

        // Verify the backup
        backup_engine
            .verify_backup(backup_id)
            .context(format!("Backup verification failed for ID {}", backup_id))?;

        Ok(())
    }

    /// Delete old backups, keeping only the specified number of recent backups
    ///
    /// # Arguments
    /// * `backup_dir` - Directory containing backups
    /// * `num_backups_to_keep` - Number of most recent backups to keep
    pub fn purge_old_backups<P: AsRef<Path>>(
        backup_dir: P,
        num_backups_to_keep: usize,
    ) -> Result<()> {
        let backup_path = backup_dir.as_ref();

        // Initialize backup engine
        let backup_opts = BackupEngineOptions::new(backup_path)?;
        let mut backup_engine = BackupEngine::open(&backup_opts, &rocksdb::Env::new()?)?;

        // Purge old backups
        backup_engine
            .purge_old_backups(num_backups_to_keep)
            .context("Failed to purge old backups")?;

        Ok(())
    }

    /// Get the database path for this backend
    pub fn db_path(&self) -> &Path {
        &self.config.path
    }
}

// Define a trait for RocksDB index operations to avoid impl on Arc directly
pub trait RocksDBIndexOps {
    fn put_index(&self, key: &str, value: &[u8]) -> Result<()>;
    fn get_index(&self, key: &str) -> Result<Option<Vec<u8>>>;
    fn delete_index(&self, key: &str) -> Result<()>;
    fn append_to_index_list(&self, key: &str, hash: &SHA256Hash) -> Result<()>;
    fn get_index_list(&self, key: &str) -> Result<Vec<SHA256Hash>>;
    fn put_manifest(&self, key: &str, value: &[u8]) -> Result<()>;
    fn get_manifest(&self, key: &str) -> Result<Option<Vec<u8>>>;
    fn list_manifests(&self) -> Result<Vec<(String, Vec<u8>)>>;
    fn flush(&self) -> Result<()>;
    fn compact(&self) -> Result<()>;
}

// Implement the trait for Arc<RocksDBBackend> to allow shared access
impl RocksDBIndexOps for Arc<RocksDBBackend> {
    fn put_index(&self, key: &str, value: &[u8]) -> Result<()> {
        (**self).put_index(key, value)
    }

    fn get_index(&self, key: &str) -> Result<Option<Vec<u8>>> {
        (**self).get_index(key)
    }

    fn delete_index(&self, key: &str) -> Result<()> {
        (**self).delete_index(key)
    }

    fn append_to_index_list(&self, key: &str, hash: &SHA256Hash) -> Result<()> {
        (**self).append_to_index_list(key, hash)
    }

    fn get_index_list(&self, key: &str) -> Result<Vec<SHA256Hash>> {
        (**self).get_index_list(key)
    }

    fn put_manifest(&self, key: &str, value: &[u8]) -> Result<()> {
        (**self).put_manifest(key, value)
    }

    fn get_manifest(&self, key: &str) -> Result<Option<Vec<u8>>> {
        (**self).get_manifest(key)
    }

    fn list_manifests(&self) -> Result<Vec<(String, Vec<u8>)>> {
        (**self).list_manifests()
    }

    fn flush(&self) -> Result<()> {
        (**self).flush()
    }

    fn compact(&self) -> Result<()> {
        (**self).compact()
    }
}

// Implement for Arc<RocksDBBackend> to allow shared ownership
impl SequenceStorageBackend for Arc<RocksDBBackend> {
    fn sequence_exists(&self, hash: &SHA256Hash) -> Result<bool> {
        (**self).sequence_exists(hash)
    }

    fn sequences_exist_batch(&self, hashes: &[SHA256Hash]) -> Result<Vec<bool>> {
        (**self).sequences_exist_batch(hashes)
    }

    fn store_canonical(&self, sequence: &CanonicalSequence) -> Result<()> {
        (**self).store_canonical(sequence)
    }

    fn store_canonical_batch(&self, sequences: &[CanonicalSequence]) -> Result<()> {
        (**self).store_canonical_batch(sequences)
    }

    fn load_canonical(&self, hash: &SHA256Hash) -> Result<CanonicalSequence> {
        (**self).load_canonical(hash)
    }

    fn store_representations(&self, representations: &SequenceRepresentations) -> Result<()> {
        (**self).store_representations(representations)
    }

    fn load_representations(&self, hash: &SHA256Hash) -> Result<SequenceRepresentations> {
        (**self).load_representations(hash)
    }

    fn get_stats(&self) -> Result<StorageStats> {
        (**self).get_stats()
    }

    fn list_all_hashes(&self) -> Result<Vec<SHA256Hash>> {
        (**self).list_all_hashes()
    }

    fn get_sequence_size(&self, hash: &SHA256Hash) -> Result<usize> {
        (**self).get_sequence_size(hash)
    }

    fn remove_sequence(&self, hash: &SHA256Hash) -> Result<()> {
        (**self).remove_sequence(hash)
    }

    fn flush(&self) -> Result<()> {
        (**self).flush()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        (**self).as_any()
    }
}

impl SequenceStorageBackend for RocksDBBackend {
    fn sequence_exists(&self, hash: &SHA256Hash) -> Result<bool> {
        let cf = self.cf_handle(cf_names::SEQUENCES)?;
        Ok(self.db.get_cf(&cf, hash.as_bytes())?.is_some())
    }

    /// Batch check existence - MUCH faster with MultiGet
    fn sequences_exist_batch(&self, hashes: &[SHA256Hash]) -> Result<Vec<bool>> {
        let cf = self.cf_handle(cf_names::SEQUENCES)?;

        // Prepare keys
        let keys: Vec<Vec<u8>> = hashes.iter().map(|h| h.as_bytes().to_vec()).collect();

        // Use multi_get_cf for batch retrieval
        let results = self
            .db
            .multi_get_cf(keys.iter().map(|k| (&cf, k.as_slice())).collect::<Vec<_>>());

        // Convert results to existence flags
        Ok(results
            .into_iter()
            .map(|r| r.map(|v| v.is_some()).unwrap_or(false))
            .collect())
    }

    fn store_canonical(&self, sequence: &CanonicalSequence) -> Result<()> {
        let cf = self.cf_handle(cf_names::SEQUENCES)?;
        let key = sequence.sequence_hash.as_bytes();
        let value = Self::serialize(sequence)?;

        self.db.put_cf_opt(&cf, key, value, &self.write_opts)?;
        Ok(())
    }

    fn store_canonical_batch(&self, sequences: &[CanonicalSequence]) -> Result<()> {
        let cf = self.cf_handle(cf_names::SEQUENCES)?;

        // Use WriteBatch for atomic batch writes
        let mut batch = WriteBatch::default();

        for sequence in sequences {
            let key = sequence.sequence_hash.as_bytes();
            let value = Self::serialize(sequence)?;
            batch.put_cf(&cf, key, value);
        }

        self.db.write_opt(batch, &self.write_opts)?;
        Ok(())
    }

    fn load_canonical(&self, hash: &SHA256Hash) -> Result<CanonicalSequence> {
        let cf = self.cf_handle(cf_names::SEQUENCES)?;
        let key = hash.as_bytes();

        let data = self
            .db
            .get_cf(&cf, key)?
            .ok_or_else(|| anyhow!("Sequence not found: {}", hash))?;

        Self::deserialize(&data)
    }

    fn store_representations(&self, representations: &SequenceRepresentations) -> Result<()> {
        let cf = self.cf_handle(cf_names::REPRESENTATIONS)?;
        let key = representations.canonical_hash.as_bytes();
        let value = Self::serialize(representations)?;

        self.db.put_cf_opt(&cf, key, value, &self.write_opts)?;
        Ok(())
    }

    fn load_representations(&self, hash: &SHA256Hash) -> Result<SequenceRepresentations> {
        let cf = self.cf_handle(cf_names::REPRESENTATIONS)?;
        let key = hash.as_bytes();

        // Try to load existing representations
        if let Some(data) = self.db.get_cf(&cf, key)? {
            Self::deserialize(&data)
        } else {
            // Return empty representations if none exist
            Ok(SequenceRepresentations {
                canonical_hash: *hash,
                representations: Vec::new(),
            })
        }
    }

    fn get_stats(&self) -> Result<StorageStats> {
        use rocksdb::properties;

        // Use RocksDB approximate statistics for instant results
        // This is MUCH faster than iterating through millions of sequences
        let cf = self.cf_handle(cf_names::SEQUENCES)?;

        // Get approximate number of keys (very fast, O(1))
        let total_sequences = self
            .db
            .property_int_value_cf(&cf, properties::ESTIMATE_NUM_KEYS)?
            .unwrap_or(0) as usize;

        // Get approximate total size of all SST files
        let total_size = self
            .db
            .property_int_value_cf(&cf, properties::TOTAL_SST_FILES_SIZE)?
            .unwrap_or(0) as usize;

        // Get approximate count for representations
        let rep_cf = self.cf_handle(cf_names::REPRESENTATIONS)?;
        let total_representations = self
            .db
            .property_int_value_cf(&rep_cf, properties::ESTIMATE_NUM_KEYS)?
            .unwrap_or(0) as usize;

        Ok(StorageStats {
            total_chunks: total_sequences, // Each sequence is a "chunk" in RocksDB
            total_size,
            compressed_chunks: total_sequences, // All stored with compression
            deduplication_ratio: 1.0,           // Will be calculated based on representations
            total_sequences: Some(total_sequences),
            total_representations: Some(total_representations),
        })
    }

    fn list_all_hashes(&self) -> Result<Vec<SHA256Hash>> {
        let cf = self.cf_handle(cf_names::SEQUENCES)?;
        let mut hashes = Vec::new();

        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for item in iter {
            let (key, _value) = item?;
            let hash = SHA256Hash::from_bytes(&key)?;
            hashes.push(hash);
        }

        Ok(hashes)
    }

    fn get_sequence_size(&self, hash: &SHA256Hash) -> Result<usize> {
        let cf = self.cf_handle(cf_names::SEQUENCES)?;
        let key = hash.as_bytes();

        if let Some(value) = self.db.get_cf(&cf, key)? {
            Ok(value.len())
        } else {
            Err(anyhow!("Sequence not found: {}", hash))
        }
    }

    fn remove_sequence(&self, hash: &SHA256Hash) -> Result<()> {
        let seq_cf = self.cf_handle(cf_names::SEQUENCES)?;
        let rep_cf = self.cf_handle(cf_names::REPRESENTATIONS)?;
        let key = hash.as_bytes();

        // Remove from both column families
        self.db.delete_cf(&seq_cf, key)?;
        self.db.delete_cf(&rep_cf, key)?;
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        // Flush all column families
        let cfs = vec![
            cf_names::SEQUENCES,
            cf_names::REPRESENTATIONS,
            cf_names::MANIFESTS,
            cf_names::INDICES,
        ];

        for cf_name in cfs {
            if let Ok(cf) = self.cf_handle(cf_name) {
                self.db.flush_cf(&cf)?;
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Drop for RocksDBBackend {
    fn drop(&mut self) {
        // RocksDB will be properly closed when Arc reference count reaches 0
        // Flush any pending writes
        if let Ok(cf) = self.cf_handle(cf_names::SEQUENCES) {
            let _ = self.db.flush_cf(&cf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use talaria_core::types::SequenceType;
    use tempfile::TempDir;

    #[test]
    fn test_rocksdb_backend_creation() {
        let temp_dir = TempDir::new().unwrap();
        let backend = RocksDBBackend::new(temp_dir.path()).unwrap();

        // Should have all column families
        assert!(backend.cf_handle(cf_names::SEQUENCES).is_ok());
        assert!(backend.cf_handle(cf_names::REPRESENTATIONS).is_ok());
        assert!(backend.cf_handle(cf_names::MANIFESTS).is_ok());
    }

    #[test]
    fn test_sequence_storage_and_retrieval() {
        let temp_dir = TempDir::new().unwrap();
        let backend = RocksDBBackend::new(temp_dir.path()).unwrap();

        // Create a test sequence
        let sequence = CanonicalSequence {
            sequence_hash: SHA256Hash::compute(b"ATCG"),
            sequence: b"ATCG".to_vec(),
            length: 4,
            sequence_type: SequenceType::DNA,
            checksum: 12345,
            first_seen: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
        };

        // Store and retrieve
        backend.store_canonical(&sequence).unwrap();
        assert!(backend.sequence_exists(&sequence.sequence_hash).unwrap());

        let loaded = backend.load_canonical(&sequence.sequence_hash).unwrap();
        assert_eq!(loaded.sequence, sequence.sequence);
    }

    #[test]
    fn test_batch_existence_checking() {
        let temp_dir = TempDir::new().unwrap();
        let backend = RocksDBBackend::new(temp_dir.path()).unwrap();

        // Create test sequences
        let hashes: Vec<SHA256Hash> = (0..100).map(|i| SHA256Hash::compute(&[i as u8])).collect();

        // Store half of them
        for (i, hash) in hashes.iter().enumerate().take(50) {
            let sequence = CanonicalSequence {
                sequence_hash: *hash,
                sequence: vec![i as u8],
                length: 1,
                sequence_type: SequenceType::DNA,
                checksum: i as u64,
                first_seen: chrono::Utc::now(),
                last_seen: chrono::Utc::now(),
            };
            backend.store_canonical(&sequence).unwrap();
        }

        // Batch check existence
        let exists = backend.sequences_exist_batch(&hashes).unwrap();

        // First 50 should exist, last 50 should not
        for (i, exists_flag) in exists.iter().enumerate() {
            if i < 50 {
                assert!(*exists_flag, "Sequence {} should exist", i);
            } else {
                assert!(!*exists_flag, "Sequence {} should not exist", i);
            }
        }
    }
}
