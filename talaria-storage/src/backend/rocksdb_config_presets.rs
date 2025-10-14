/// Optimized RocksDB configuration presets for different use cases
use super::rocksdb_backend::RocksDBConfig;
use std::path::PathBuf;

impl RocksDBConfig {
    /// High-performance configuration for batch processing
    /// Optimized for UniRef50-scale operations (50k+ sequences)
    pub fn high_performance() -> Self {
        Self {
            path: PathBuf::new(),            // Will be overridden
            write_buffer_size_mb: 256,       // Large write buffer for batch writes
            max_write_buffer_number: 6,      // More buffers for parallel writes
            block_cache_size_mb: 4096,       // 4GB cache for hot data
            bloom_filter_bits: 10.0,         // Standard bloom filter
            compression: "zstd".to_string(), // Best compression ratio
            compression_level: 3,            // Balanced compression
            max_background_jobs: 16,         // Max parallelism
            target_file_size_mb: 256,        // Larger SST files
            enable_statistics: false,        // Disable for performance
            optimize_for: "batch".to_string(),
        }
    }

    /// Memory-optimized configuration for limited resources
    pub fn memory_optimized() -> Self {
        Self {
            path: PathBuf::new(),
            write_buffer_size_mb: 64,   // Smaller buffers
            max_write_buffer_number: 2, // Minimum buffers
            block_cache_size_mb: 512,   // 512MB cache
            bloom_filter_bits: 10.0,
            compression: "zstd".to_string(), // Good compression
            compression_level: 6,            // Higher compression
            max_background_jobs: 4,          // Limited parallelism
            target_file_size_mb: 64,         // Smaller files
            enable_statistics: false,
            optimize_for: "memory".to_string(),
        }
    }

    /// Balanced configuration for general use
    pub fn balanced() -> Self {
        Self {
            path: PathBuf::new(),
            write_buffer_size_mb: 128,
            max_write_buffer_number: 4,
            block_cache_size_mb: 2048, // 2GB cache
            bloom_filter_bits: 10.0,
            compression: "zstd".to_string(),
            compression_level: 3,
            max_background_jobs: 8,
            target_file_size_mb: 128,
            enable_statistics: true, // Enable for monitoring
            optimize_for: "balanced".to_string(),
        }
    }

    /// SSD-optimized configuration
    pub fn ssd_optimized() -> Self {
        Self {
            path: PathBuf::new(),
            write_buffer_size_mb: 128,
            max_write_buffer_number: 4,
            block_cache_size_mb: 1024,
            bloom_filter_bits: 10.0,
            compression: "lz4".to_string(), // Faster compression for SSD
            compression_level: 1,           // Minimal compression
            max_background_jobs: 16,        // High parallelism
            target_file_size_mb: 512,       // Larger files for SSD
            enable_statistics: false,
            optimize_for: "ssd".to_string(),
        }
    }

    /// Development/testing configuration
    pub fn development() -> Self {
        Self {
            path: PathBuf::new(),
            write_buffer_size_mb: 64,
            max_write_buffer_number: 2,
            block_cache_size_mb: 256,
            bloom_filter_bits: 10.0,
            compression: "snappy".to_string(), // Fast compression
            compression_level: 1,
            max_background_jobs: 2,
            target_file_size_mb: 64,
            enable_statistics: true, // Enable for debugging
            optimize_for: "dev".to_string(),
        }
    }

    /// Apply optimizations based on detected hardware
    pub fn auto_tune(&mut self) {
        use num_cpus;
        use sysinfo::{System, SystemExt};

        let cpus = num_cpus::get();
        let mut sys = System::new_all();
        sys.refresh_memory();
        let total_memory_mb = sys.total_memory() / 1024 / 1024;

        // Adjust based on CPU cores
        self.max_background_jobs = (cpus / 2).clamp(2, 32) as i32;

        // Adjust cache based on available memory
        // Use ~25% of system memory for block cache, max 8GB
        let suggested_cache = (total_memory_mb / 4).min(8192) as usize;
        self.block_cache_size_mb = suggested_cache;

        // Adjust write buffers based on memory
        if total_memory_mb > 16384 {
            // >16GB RAM
            self.write_buffer_size_mb = 256;
            self.max_write_buffer_number = 6;
        } else if total_memory_mb > 8192 {
            // >8GB RAM
            self.write_buffer_size_mb = 128;
            self.max_write_buffer_number = 4;
        } else {
            // Limited memory
            self.write_buffer_size_mb = 64;
            self.max_write_buffer_number = 2;
        }

        tracing::info!(
            "Auto-tuned RocksDB: {} CPUs, {}MB RAM -> {}MB cache, {} background jobs",
            cpus,
            total_memory_mb,
            self.block_cache_size_mb,
            self.max_background_jobs
        );
    }

    /// Configure for specific workload patterns
    pub fn optimize_for_workload(&mut self, pattern: WorkloadPattern) {
        match pattern {
            WorkloadPattern::BulkLoad => {
                // Optimize for initial database population
                self.max_write_buffer_number = 8;
                self.write_buffer_size_mb = 512;
                self.compression_level = 1; // Fast compression
                self.max_background_jobs = 32;
            }
            WorkloadPattern::PointLookups => {
                // Optimize for individual sequence lookups
                self.bloom_filter_bits = 15.0; // Stronger bloom filter
                self.block_cache_size_mb *= 2;
            }
            WorkloadPattern::RangeScans => {
                // Optimize for scanning many sequences
                self.target_file_size_mb = 512;
                self.compression_level = 6; // Better compression for scans
            }
            WorkloadPattern::MixedReadWrite => {
                // Balanced for concurrent reads and writes
                self.max_write_buffer_number = 4;
                self.write_buffer_size_mb = 128;
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum WorkloadPattern {
    BulkLoad,
    PointLookups,
    RangeScans,
    MixedReadWrite,
}

/// Performance monitoring for RocksDB
#[allow(dead_code)]
pub struct RocksDBMonitor {
    pub write_amplification: f64,
    pub read_amplification: f64,
    pub space_amplification: f64,
    pub cache_hit_rate: f64,
    pub compaction_pending_bytes: u64,
}

impl RocksDBMonitor {
    /// Analyze current performance and suggest optimizations
    pub fn suggest_optimizations(&self, current_config: &RocksDBConfig) -> Vec<String> {
        let mut suggestions = Vec::new();

        // Check write amplification
        if self.write_amplification > 10.0 {
            suggestions.push(format!(
                "High write amplification ({:.1}x). Consider increasing write_buffer_size to {}MB",
                self.write_amplification,
                current_config.write_buffer_size_mb * 2
            ));
        }

        // Check cache hit rate
        if self.cache_hit_rate < 0.8 {
            suggestions.push(format!(
                "Low cache hit rate ({:.1}%). Consider increasing block_cache_size to {}MB",
                self.cache_hit_rate * 100.0,
                current_config.block_cache_size_mb * 2
            ));
        }

        // Check compaction
        if self.compaction_pending_bytes > 1_000_000_000 {
            // 1GB
            suggestions.push(format!(
                "Large compaction backlog ({}MB). Consider increasing max_background_jobs to {}",
                self.compaction_pending_bytes / 1_024 / 1_024,
                current_config.max_background_jobs * 2
            ));
        }

        // Check space amplification
        if self.space_amplification > 1.5 {
            suggestions.push(format!(
                "High space amplification ({:.1}x). Consider enabling compression or increasing compression_level",
                self.space_amplification
            ));
        }

        suggestions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_presets() {
        let high_perf = RocksDBConfig::high_performance();
        assert_eq!(high_perf.write_buffer_size_mb, 256);
        assert_eq!(high_perf.max_background_jobs, 16);

        let memory_opt = RocksDBConfig::memory_optimized();
        assert_eq!(memory_opt.write_buffer_size_mb, 64);
        assert_eq!(memory_opt.block_cache_size_mb, 512);

        let balanced = RocksDBConfig::balanced();
        assert!(balanced.enable_statistics);
    }

    #[test]
    fn test_workload_optimization() {
        let mut config = RocksDBConfig::balanced();
        config.optimize_for_workload(WorkloadPattern::BulkLoad);
        assert_eq!(config.max_write_buffer_number, 8);

        let mut config = RocksDBConfig::balanced();
        config.optimize_for_workload(WorkloadPattern::PointLookups);
        assert_eq!(config.bloom_filter_bits, 15.0);
    }

    #[test]
    fn test_monitor_suggestions() {
        let config = RocksDBConfig::balanced();
        let monitor = RocksDBMonitor {
            write_amplification: 15.0,
            read_amplification: 2.0,
            space_amplification: 1.2,
            cache_hit_rate: 0.6,
            compaction_pending_bytes: 2_000_000_000,
        };

        let suggestions = monitor.suggest_optimizations(&config);
        assert!(!suggestions.is_empty());
        assert!(suggestions
            .iter()
            .any(|s| s.contains("write amplification")));
        assert!(suggestions.iter().any(|s| s.contains("cache hit rate")));
    }
}
