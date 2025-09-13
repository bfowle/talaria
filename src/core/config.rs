use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub reduction: ReductionConfig,
    pub alignment: AlignmentConfig,
    pub output: OutputConfig,
    pub performance: PerformanceConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReductionConfig {
    pub target_ratio: f64,
    pub min_sequence_length: usize,
    pub max_delta_distance: usize,
    pub similarity_threshold: f64,
    pub taxonomy_aware: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentConfig {
    pub gap_penalty: i32,
    pub gap_extension: i32,
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub format: String,
    pub include_metadata: bool,
    pub compress_output: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub chunk_size: usize,
    pub batch_size: usize,
    pub cache_alignments: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Base directory for storing downloaded databases
    pub database_dir: Option<String>,
    /// Number of old versions to retain (0 = keep all)
    pub retention_count: usize,
    /// Automatically check for updates
    pub auto_update_check: bool,
    /// Preferred mirror for downloads (e.g., "ebi", "uniprot", "ncbi")
    pub preferred_mirror: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            reduction: ReductionConfig {
                target_ratio: 0.3,
                min_sequence_length: 50,
                max_delta_distance: 100,
                similarity_threshold: 0.0,  // Disabled by default (0.0 = no similarity check)
                taxonomy_aware: false,      // Disabled by default
            },
            alignment: AlignmentConfig {
                gap_penalty: 20,
                gap_extension: 10,
                algorithm: "needleman-wunsch".to_string(),
            },
            output: OutputConfig {
                format: "fasta".to_string(),
                include_metadata: true,
                compress_output: false,
            },
            performance: PerformanceConfig {
                chunk_size: 10000,
                batch_size: 1000,
                cache_alignments: true,
            },
            database: DatabaseConfig {
                database_dir: None,  // Will default to ~/.talaria/databases/data/
                retention_count: 3,  // Keep 3 old versions by default
                auto_update_check: false,
                preferred_mirror: Some("ebi".to_string()),  // Use EBI mirror by default
            },
        }
    }
}

pub fn default_config() -> Config {
    Config::default()
}

pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config, crate::TalariaError> {
    let contents = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&contents)
        .map_err(|e| crate::TalariaError::Config(format!("Failed to parse config: {}", e)))?;
    Ok(config)
}

pub fn save_config<P: AsRef<Path>>(path: P, config: &Config) -> Result<(), crate::TalariaError> {
    let contents = toml::to_string_pretty(config)
        .map_err(|e| crate::TalariaError::Config(format!("Failed to serialize config: {}", e)))?;
    std::fs::write(path, contents)?;
    Ok(())
}