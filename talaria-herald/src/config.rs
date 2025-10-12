/// Configuration system for HERALD storage backends
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use talaria_storage::backend::RocksDBConfig;

/// Main HERALD configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeraldConfig {
    /// Storage backend configuration
    pub storage: StorageConfig,
    /// Performance tuning options
    pub performance: PerformanceConfig,
    /// Migration settings
    pub migration: MigrationConfig,
}

impl Default for HeraldConfig {
    fn default() -> Self {
        Self {
            storage: StorageConfig::default(),
            performance: PerformanceConfig::default(),
            migration: MigrationConfig::default(),
        }
    }
}

/// Storage configuration section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// RocksDB configuration
    #[serde(default)]
    pub rocksdb: RocksDBConfig,

    /// Bloom filter configuration
    #[serde(default)]
    pub bloom_filter: BloomFilterConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            rocksdb: RocksDBConfig::default(),
            bloom_filter: BloomFilterConfig::default(),
        }
    }
}

/// Bloom filter configuration for sequence indices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloomFilterConfig {
    /// Expected number of sequences (for sizing bloom filter)
    pub expected_sequences: usize,

    /// Target false positive rate (0.0 - 1.0)
    /// Lower values = more memory but fewer false positives
    /// Recommended: 0.001 (0.1%) for general use, 0.0001 (0.01%) for large datasets
    pub false_positive_rate: f64,

    /// How often to persist bloom filter to disk (in seconds)
    /// Set to 0 to disable automatic persistence
    pub persist_interval_seconds: u64,

    /// Enable bloom filter statistics tracking
    pub enable_statistics: bool,
}

impl Default for BloomFilterConfig {
    fn default() -> Self {
        Self {
            expected_sequences: 100_000_000, // Default for large datasets like UniRef50
            false_positive_rate: 0.001,      // 0.1% FP rate
            persist_interval_seconds: 300,   // Save every 5 minutes
            enable_statistics: false,
        }
    }
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Number of threads for parallel processing
    pub threads: Option<usize>,
    /// Batch size for bulk operations
    pub batch_size: usize,
    /// Enable verbose logging
    pub verbose: bool,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            threads: None, // Use num_cpus by default
            batch_size: 10000,
            verbose: false,
        }
    }
}

/// Migration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    /// Enable automatic migration
    pub auto_migrate: bool,
    /// Verification sample size
    pub verify_sample_size: usize,
    /// Keep old data after successful migration
    pub preserve_old_data: bool,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            auto_migrate: false,
            verify_sample_size: 1000,
            preserve_old_data: true,
        }
    }
}

impl HeraldConfig {
    /// Load configuration from environment and/or file
    pub fn load() -> Result<Self> {
        // Start with defaults
        let mut config = Self::default();

        // Check for config file
        if let Ok(config_path) = env::var("TALARIA_HERALD_CONFIG") {
            config = Self::from_file(Path::new(&config_path))?;
        } else if let Ok(home) = env::var("TALARIA_HOME") {
            let default_config = PathBuf::from(home).join("herald.toml");
            if default_config.exists() {
                config = Self::from_file(&default_config)?;
            }
        }

        // Override with environment variables
        config.apply_env_overrides()?;

        Ok(config)
    }

    /// Load from TOML file
    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {:?}", path))?;
        toml::from_str(&contents).with_context(|| format!("Failed to parse config from {:?}", path))
    }

    /// Save to TOML file
    pub fn save(&self, path: &Path) -> Result<()> {
        let contents = toml::to_string_pretty(self)?;
        fs::write(path, contents).with_context(|| format!("Failed to write config to {:?}", path))
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) -> Result<()> {
        // RocksDB settings
        if let Ok(val) = env::var("TALARIA_ROCKSDB_WRITE_BUFFER_MB") {
            self.storage.rocksdb.write_buffer_size_mb = val.parse()?;
        }
        if let Ok(val) = env::var("TALARIA_ROCKSDB_CACHE_MB") {
            self.storage.rocksdb.block_cache_size_mb = val.parse()?;
        }

        // Performance
        if let Ok(val) = env::var("TALARIA_THREADS") {
            self.performance.threads = Some(val.parse()?);
        }
        if let Ok(val) = env::var("TALARIA_BATCH_SIZE") {
            self.performance.batch_size = val.parse()?;
        }
        if let Ok(val) = env::var("TALARIA_VERBOSE") {
            self.performance.verbose = val.parse().unwrap_or(true);
        }

        // Migration
        if let Ok(val) = env::var("TALARIA_AUTO_MIGRATE") {
            self.migration.auto_migrate = val.parse().unwrap_or(false);
        }

        Ok(())
    }

    /// Get RocksDB configuration
    pub fn get_rocksdb_config(&self) -> RocksDBConfig {
        self.storage.rocksdb.clone()
    }

    /// Get thread count (uses num_cpus if not specified)
    pub fn get_thread_count(&self) -> usize {
        self.performance.threads.unwrap_or_else(num_cpus::get)
    }
}

/// Generate a default configuration file
pub fn generate_default_config(path: &Path) -> Result<()> {
    let config = HeraldConfig::default();
    config.save(path)?;
    tracing::info!("Generated default config at {:?}", path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_serialization() {
        let config = HeraldConfig::default();
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: HeraldConfig = toml::from_str(&serialized).unwrap();

        assert_eq!(
            config.storage.rocksdb.write_buffer_size_mb,
            deserialized.storage.rocksdb.write_buffer_size_mb
        );
    }

    #[test]
    fn test_config_file_loading() {
        let config = HeraldConfig {
            storage: StorageConfig {
                rocksdb: RocksDBConfig {
                    write_buffer_size_mb: 128,
                    max_write_buffer_number: 4,
                    block_cache_size_mb: 1024,
                    bloom_filter_bits: 12.0,
                    compression: "zstd".to_string(),
                    ..Default::default()
                },
                bloom_filter: Default::default(),
            },
            performance: PerformanceConfig::default(),
            migration: MigrationConfig::default(),
        };

        let temp_file = NamedTempFile::new().unwrap();
        config.save(temp_file.path()).unwrap();

        let loaded = HeraldConfig::from_file(temp_file.path()).unwrap();
        assert_eq!(loaded.storage.rocksdb.write_buffer_size_mb, 128);
    }

    #[test]
    fn test_env_overrides() {
        std::env::set_var("TALARIA_ROCKSDB_CACHE_MB", "2048");

        let mut config = HeraldConfig::default();
        config.apply_env_overrides().unwrap();

        assert_eq!(config.storage.rocksdb.block_cache_size_mb, 2048);

        // Clean up
        std::env::remove_var("TALARIA_ROCKSDB_CACHE_MB");
    }

    #[test]
    fn test_rocksdb_config() {
        let config = HeraldConfig {
            storage: StorageConfig {
                rocksdb: RocksDBConfig {
                    write_buffer_size_mb: 128,
                    ..Default::default()
                },
                bloom_filter: Default::default(),
            },
            performance: PerformanceConfig::default(),
            migration: MigrationConfig::default(),
        };

        let rocksdb_config = config.get_rocksdb_config();
        assert_eq!(rocksdb_config.write_buffer_size_mb, 128);
    }
}
