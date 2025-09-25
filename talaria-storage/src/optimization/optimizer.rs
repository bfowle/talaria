/// Trait for storage optimization strategies
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// TODO: Update when talaria-sequoia is extracted
// use talaria_sequoia::types::SHA256Hash;
use crate::core::types::SHA256Hash;

/// Storage optimization strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageStrategy {
    /// Remove duplicates across databases
    Deduplication,
    /// Compress chunks
    Compression,
    /// Delta encoding between versions
    DeltaEncoding,
    /// Archive old versions
    Archival,
    /// Cache frequently accessed chunks
    Caching,
    /// Repack small chunks into larger ones
    Repacking,
    /// Remove unused chunks
    GarbageCollection,
}

/// Optimization result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    /// Strategy used
    pub strategy: StorageStrategy,
    /// Space saved in bytes
    pub space_saved: i64,
    /// Space used before optimization
    pub space_before: usize,
    /// Space used after optimization
    pub space_after: usize,
    /// Number of chunks affected
    pub chunks_affected: usize,
    /// Time taken in seconds
    pub duration_seconds: u64,
    /// Details about the optimization
    pub details: HashMap<String, String>,
}

/// Optimization options
#[derive(Debug, Clone, Default)]
pub struct OptimizationOptions {
    /// Strategies to apply
    pub strategies: Vec<StorageStrategy>,
    /// Target space savings (bytes)
    pub target_savings: Option<usize>,
    /// Maximum time to spend (seconds)
    pub max_duration: Option<u64>,
    /// Dry run mode
    pub dry_run: bool,
    /// Preserve N most recent versions
    pub preserve_versions: usize,
    /// Compression level (1-9)
    pub compression_level: Option<u32>,
    /// Minimum chunk size for repacking
    pub min_chunk_size: Option<usize>,
}

/// Storage analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageAnalysis {
    /// Total storage used
    pub total_size: usize,
    /// Number of chunks
    pub chunk_count: usize,
    /// Duplicate chunks found
    pub duplicate_chunks: Vec<DuplicateChunk>,
    /// Compressible chunks
    pub compressible_chunks: Vec<CompressibleChunk>,
    /// Unused chunks
    pub unused_chunks: Vec<SHA256Hash>,
    /// Potential space savings
    pub potential_savings: HashMap<StorageStrategy, usize>,
    /// Recommended strategies
    pub recommended_strategies: Vec<StorageStrategy>,
}

/// Duplicate chunk information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateChunk {
    /// Chunk hash
    pub hash: SHA256Hash,
    /// Number of duplicates
    pub count: usize,
    /// Size of each duplicate
    pub size: usize,
    /// Locations where duplicates exist
    pub locations: Vec<PathBuf>,
}

/// Compressible chunk information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressibleChunk {
    /// Chunk hash
    pub hash: SHA256Hash,
    /// Current size
    pub current_size: usize,
    /// Estimated compressed size
    pub compressed_size: usize,
    /// Compression ratio
    pub ratio: f32,
}

/// Trait for storage optimization
#[async_trait]
pub trait StorageOptimizer: Send + Sync {
    /// Analyze storage usage
    async fn analyze(&self) -> Result<StorageAnalysis>;

    /// Optimize storage with given options
    async fn optimize(&mut self, options: OptimizationOptions) -> Result<Vec<OptimizationResult>>;

    /// Apply a specific optimization strategy
    async fn apply_strategy(&mut self, strategy: StorageStrategy) -> Result<OptimizationResult>;

    /// Deduplicate chunks across databases
    async fn deduplicate(&mut self) -> Result<OptimizationResult>;

    /// Compress chunks
    async fn compress_chunks(&mut self, level: u32) -> Result<OptimizationResult>;

    /// Create delta-encoded chunks
    async fn create_deltas(&mut self, base_version: &str) -> Result<OptimizationResult>;

    /// Archive old versions
    async fn archive_old(&mut self, keep_recent: usize) -> Result<OptimizationResult>;

    /// Optimize cache for frequently accessed chunks
    async fn optimize_cache(&mut self) -> Result<OptimizationResult>;

    /// Repack small chunks
    async fn repack_chunks(&mut self, target_size: usize) -> Result<OptimizationResult>;

    /// Remove unused chunks
    async fn garbage_collect(&mut self) -> Result<OptimizationResult>;

    /// Estimate optimization impact
    async fn estimate_impact(&self, strategy: StorageStrategy) -> Result<usize>;

    /// Verify storage integrity after optimization
    async fn verify_integrity(&self) -> Result<bool>;
}

/// Standard implementation of StorageOptimizer
pub struct StandardStorageOptimizer {
    _base_path: PathBuf,
    chunks_dir: PathBuf,
    /// Cache of chunk metadata
    chunk_cache: HashMap<SHA256Hash, ChunkInfo>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ChunkInfo {
    hash: SHA256Hash,
    size: usize,
    compressed: bool,
    _access_count: usize,
    _last_accessed: Option<chrono::DateTime<chrono::Utc>>,
    references: Vec<PathBuf>,
}

impl StandardStorageOptimizer {
    pub fn new(base_path: PathBuf) -> Self {
        let chunks_dir = base_path.join("chunks");
        Self {
            _base_path: base_path,
            chunks_dir,
            chunk_cache: HashMap::new(),
        }
    }

    async fn scan_chunks(&mut self) -> Result<()> {
        self.chunk_cache.clear();

        // Scan chunks directory
        for entry in std::fs::read_dir(&self.chunks_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let file_name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("");

                if let Ok(hash) = SHA256Hash::from_hex(file_name) {
                    let metadata = entry.metadata()?;
                    let compressed = path.extension().map(|e| e == "gz").unwrap_or(false);

                    self.chunk_cache.insert(
                        hash,
                        ChunkInfo {
                            hash,
                            size: metadata.len() as usize,
                            compressed,
                            _access_count: 0,
                            _last_accessed: None,
                            references: vec![path],
                        },
                    );
                }
            }
        }

        Ok(())
    }

    fn find_duplicates(&self) -> Vec<DuplicateChunk> {
        let mut hash_locations: HashMap<SHA256Hash, Vec<PathBuf>> = HashMap::new();

        // Group chunks by hash
        for (hash, info) in &self.chunk_cache {
            hash_locations
                .entry(*hash)
                .or_default()
                .extend(info.references.clone());
        }

        // Find duplicates
        let mut duplicates = Vec::new();
        for (hash, locations) in hash_locations {
            if locations.len() > 1 {
                if let Some(info) = self.chunk_cache.get(&hash) {
                    duplicates.push(DuplicateChunk {
                        hash,
                        count: locations.len(),
                        size: info.size,
                        locations,
                    });
                }
            }
        }

        duplicates
    }

    fn find_compressible(&self) -> Vec<CompressibleChunk> {
        let mut compressible = Vec::new();

        for (hash, info) in &self.chunk_cache {
            if !info.compressed && info.size > 1024 {
                // Estimate compression ratio (conservative)
                let estimated_compressed = info.size / 3;
                compressible.push(CompressibleChunk {
                    hash: *hash,
                    current_size: info.size,
                    compressed_size: estimated_compressed,
                    ratio: estimated_compressed as f32 / info.size as f32,
                });
            }
        }

        compressible
    }

    async fn compress_chunk(&self, hash: &SHA256Hash, level: u32) -> Result<usize> {
        let chunk_path = self.chunks_dir.join(hash.to_hex());
        let compressed_path = chunk_path.with_extension("gz");

        if chunk_path.exists() && !compressed_path.exists() {
            let data = std::fs::read(&chunk_path)?;

            // Compress data
            use flate2::write::GzEncoder;
            use flate2::Compression;
            use std::io::Write;

            let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
            encoder.write_all(&data)?;
            let compressed = encoder.finish()?;

            let saved = data.len() - compressed.len();
            std::fs::write(&compressed_path, compressed)?;

            // Remove uncompressed version
            std::fs::remove_file(&chunk_path)?;

            Ok(saved)
        } else {
            Ok(0)
        }
    }
}

#[async_trait]
impl StorageOptimizer for StandardStorageOptimizer {
    async fn analyze(&self) -> Result<StorageAnalysis> {
        let mut total_size = 0;
        let mut chunk_count = 0;

        // Calculate total size
        for info in self.chunk_cache.values() {
            total_size += info.size;
            chunk_count += 1;
        }

        let duplicate_chunks = self.find_duplicates();
        let compressible_chunks = self.find_compressible();

        // Calculate potential savings
        let mut potential_savings = HashMap::new();

        // Deduplication savings
        let dedup_savings: usize = duplicate_chunks
            .iter()
            .map(|d| d.size * (d.count - 1))
            .sum();
        potential_savings.insert(StorageStrategy::Deduplication, dedup_savings);

        // Compression savings
        let compression_savings: usize = compressible_chunks
            .iter()
            .map(|c| c.current_size - c.compressed_size)
            .sum();
        potential_savings.insert(StorageStrategy::Compression, compression_savings);

        // Recommend strategies
        let mut recommended = Vec::new();
        if dedup_savings > 1_000_000 {
            recommended.push(StorageStrategy::Deduplication);
        }
        if compression_savings > 5_000_000 {
            recommended.push(StorageStrategy::Compression);
        }

        Ok(StorageAnalysis {
            total_size,
            chunk_count,
            duplicate_chunks,
            compressible_chunks,
            unused_chunks: Vec::new(),
            potential_savings,
            recommended_strategies: recommended,
        })
    }

    async fn optimize(&mut self, options: OptimizationOptions) -> Result<Vec<OptimizationResult>> {
        let mut results = Vec::new();

        // Scan chunks first
        self.scan_chunks().await?;

        for strategy in &options.strategies {
            if options.dry_run {
                let impact = self.estimate_impact(*strategy).await?;
                results.push(OptimizationResult {
                    strategy: *strategy,
                    space_saved: impact as i64,
                    space_before: 0,
                    space_after: 0,
                    chunks_affected: 0,
                    duration_seconds: 0,
                    details: HashMap::new(),
                });
            } else {
                let result = self.apply_strategy(*strategy).await?;
                results.push(result);
            }

            // Check if target savings reached
            if let Some(target) = options.target_savings {
                let total_saved: i64 = results.iter().map(|r| r.space_saved).sum();
                if total_saved >= target as i64 {
                    break;
                }
            }
        }

        Ok(results)
    }

    async fn apply_strategy(&mut self, strategy: StorageStrategy) -> Result<OptimizationResult> {
        match strategy {
            StorageStrategy::Deduplication => self.deduplicate().await,
            StorageStrategy::Compression => self.compress_chunks(6).await,
            StorageStrategy::DeltaEncoding => self.create_deltas("current").await,
            StorageStrategy::Archival => self.archive_old(3).await,
            StorageStrategy::Caching => self.optimize_cache().await,
            StorageStrategy::Repacking => self.repack_chunks(1_000_000).await,
            StorageStrategy::GarbageCollection => self.garbage_collect().await,
        }
    }

    async fn deduplicate(&mut self) -> Result<OptimizationResult> {
        let start = std::time::Instant::now();
        let duplicates = self.find_duplicates();
        let mut space_saved = 0;
        let mut chunks_affected = 0;

        for dup in &duplicates {
            // Keep first, remove rest
            for location in dup.locations.iter().skip(1) {
                if location.exists() {
                    std::fs::remove_file(location)?;
                    space_saved += dup.size;
                    chunks_affected += 1;
                }
            }
        }

        Ok(OptimizationResult {
            strategy: StorageStrategy::Deduplication,
            space_saved: space_saved as i64,
            space_before: 0,
            space_after: 0,
            chunks_affected,
            duration_seconds: start.elapsed().as_secs(),
            details: HashMap::new(),
        })
    }

    async fn compress_chunks(&mut self, level: u32) -> Result<OptimizationResult> {
        let start = std::time::Instant::now();
        let compressible = self.find_compressible();
        let mut total_saved = 0;
        let mut chunks_affected = 0;

        for chunk in &compressible {
            let saved = self.compress_chunk(&chunk.hash, level).await?;
            total_saved += saved;
            if saved > 0 {
                chunks_affected += 1;
            }
        }

        Ok(OptimizationResult {
            strategy: StorageStrategy::Compression,
            space_saved: total_saved as i64,
            space_before: 0,
            space_after: 0,
            chunks_affected,
            duration_seconds: start.elapsed().as_secs(),
            details: HashMap::new(),
        })
    }

    async fn create_deltas(&mut self, _base_version: &str) -> Result<OptimizationResult> {
        // Implementation would create delta-encoded chunks
        Ok(OptimizationResult {
            strategy: StorageStrategy::DeltaEncoding,
            space_saved: 0,
            space_before: 0,
            space_after: 0,
            chunks_affected: 0,
            duration_seconds: 0,
            details: HashMap::new(),
        })
    }

    async fn archive_old(&mut self, _keep_recent: usize) -> Result<OptimizationResult> {
        // Implementation would archive old versions
        Ok(OptimizationResult {
            strategy: StorageStrategy::Archival,
            space_saved: 0,
            space_before: 0,
            space_after: 0,
            chunks_affected: 0,
            duration_seconds: 0,
            details: HashMap::new(),
        })
    }

    async fn optimize_cache(&mut self) -> Result<OptimizationResult> {
        // Implementation would optimize cache
        Ok(OptimizationResult {
            strategy: StorageStrategy::Caching,
            space_saved: 0,
            space_before: 0,
            space_after: 0,
            chunks_affected: 0,
            duration_seconds: 0,
            details: HashMap::new(),
        })
    }

    async fn repack_chunks(&mut self, _target_size: usize) -> Result<OptimizationResult> {
        // Implementation would repack small chunks
        Ok(OptimizationResult {
            strategy: StorageStrategy::Repacking,
            space_saved: 0,
            space_before: 0,
            space_after: 0,
            chunks_affected: 0,
            duration_seconds: 0,
            details: HashMap::new(),
        })
    }

    async fn garbage_collect(&mut self) -> Result<OptimizationResult> {
        let start = std::time::Instant::now();

        // Find chunks not referenced by any manifest
        // This is simplified - would need to check all manifests
        let space_saved = 0;
        let chunks_removed = 0;

        // For now, just return empty result
        Ok(OptimizationResult {
            strategy: StorageStrategy::GarbageCollection,
            space_saved,
            space_before: 0,
            space_after: 0,
            chunks_affected: chunks_removed,
            duration_seconds: start.elapsed().as_secs(),
            details: HashMap::new(),
        })
    }

    async fn estimate_impact(&self, strategy: StorageStrategy) -> Result<usize> {
        match strategy {
            StorageStrategy::Deduplication => {
                let duplicates = self.find_duplicates();
                Ok(duplicates.iter().map(|d| d.size * (d.count - 1)).sum())
            }
            StorageStrategy::Compression => {
                let compressible = self.find_compressible();
                Ok(compressible
                    .iter()
                    .map(|c| c.current_size - c.compressed_size)
                    .sum())
            }
            _ => Ok(0),
        }
    }

    async fn verify_integrity(&self) -> Result<bool> {
        // Verify all chunks are valid
        for (hash, info) in &self.chunk_cache {
            for path in &info.references {
                if !path.exists() {
                    return Ok(false);
                }

                // Verify hash matches
                let data = std::fs::read(path)?;
                let actual_hash = SHA256Hash::compute(&data);
                if actual_hash != *hash {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }
}
