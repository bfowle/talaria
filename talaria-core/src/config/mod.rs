//! Configuration types for Talaria

use serde::{Deserialize, Serialize};
use std::path::Path;
use crate::TalariaError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub reduction: ReductionConfig,
    #[serde(default)]
    pub alignment: AlignmentConfig,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub performance: PerformanceConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReductionConfig {
    #[serde(default = "default_target_ratio")]
    pub target_ratio: f64,
    #[serde(default = "default_min_sequence_length")]
    pub min_sequence_length: usize,
    #[serde(default = "default_max_delta_distance")]
    pub max_delta_distance: usize,
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f64,
    #[serde(default = "default_taxonomy_aware")]
    pub taxonomy_aware: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentConfig {
    #[serde(default = "default_gap_penalty")]
    pub gap_penalty: i32,
    #[serde(default = "default_gap_extension")]
    pub gap_extension: i32,
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_include_metadata")]
    pub include_metadata: bool,
    #[serde(default = "default_compress_output")]
    pub compress_output: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_cache_alignments")]
    pub cache_alignments: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Base directory for storing downloaded databases
    #[serde(default)]
    pub database_dir: Option<String>,
    /// Number of old versions to retain (0 = keep all)
    #[serde(default = "default_retention_count")]
    pub retention_count: usize,
    /// Automatically check for updates
    #[serde(default = "default_auto_update_check")]
    pub auto_update_check: bool,
    /// Preferred mirror for downloads (e.g., "ebi", "uniprot", "ncbi")
    #[serde(default = "default_preferred_mirror")]
    pub preferred_mirror: Option<String>,
}

// Default value functions
fn default_target_ratio() -> f64 { 0.3 }
fn default_min_sequence_length() -> usize { 50 }
fn default_max_delta_distance() -> usize { 100 }
fn default_similarity_threshold() -> f64 { 0.0 }
fn default_taxonomy_aware() -> bool { false }
fn default_gap_penalty() -> i32 { 20 }
fn default_gap_extension() -> i32 { 10 }
fn default_algorithm() -> String { "needleman-wunsch".to_string() }
fn default_format() -> String { "fasta".to_string() }
fn default_include_metadata() -> bool { true }
fn default_compress_output() -> bool { false }
fn default_chunk_size() -> usize { 10000 }
fn default_batch_size() -> usize { 1000 }
fn default_cache_alignments() -> bool { true }
fn default_retention_count() -> usize { 3 }
fn default_auto_update_check() -> bool { false }
fn default_preferred_mirror() -> Option<String> { Some("ebi".to_string()) }

impl Default for ReductionConfig {
    fn default() -> Self {
        Self {
            target_ratio: default_target_ratio(),
            min_sequence_length: default_min_sequence_length(),
            max_delta_distance: default_max_delta_distance(),
            similarity_threshold: default_similarity_threshold(),
            taxonomy_aware: default_taxonomy_aware(),
        }
    }
}

impl Default for AlignmentConfig {
    fn default() -> Self {
        Self {
            gap_penalty: default_gap_penalty(),
            gap_extension: default_gap_extension(),
            algorithm: default_algorithm(),
        }
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: default_format(),
            include_metadata: default_include_metadata(),
            compress_output: default_compress_output(),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            chunk_size: default_chunk_size(),
            batch_size: default_batch_size(),
            cache_alignments: default_cache_alignments(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            database_dir: None,
            retention_count: default_retention_count(),
            auto_update_check: default_auto_update_check(),
            preferred_mirror: default_preferred_mirror(),
        }
    }
}

pub fn default_config() -> Config {
    Config::default()
}

pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config, TalariaError> {
    let contents = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&contents)
        .map_err(|e| TalariaError::Configuration(format!("Failed to parse config: {}", e)))?;
    Ok(config)
}

pub fn save_config<P: AsRef<Path>>(path: P, config: &Config) -> Result<(), TalariaError> {
    let contents = toml::to_string_pretty(config)
        .map_err(|e| TalariaError::Configuration(format!("Failed to serialize config: {}", e)))?;
    std::fs::write(path, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_default_config() {
        let config = Config::default();

        // Test reduction defaults
        assert_eq!(config.reduction.target_ratio, 0.3);
        assert_eq!(config.reduction.min_sequence_length, 50);
        assert_eq!(config.reduction.max_delta_distance, 100);
        assert_eq!(config.reduction.similarity_threshold, 0.0);
        assert!(!config.reduction.taxonomy_aware);

        // Test alignment defaults
        assert_eq!(config.alignment.gap_penalty, 20);
        assert_eq!(config.alignment.gap_extension, 10);
        assert_eq!(config.alignment.algorithm, "needleman-wunsch");

        // Test output defaults
        assert_eq!(config.output.format, "fasta");
        assert!(config.output.include_metadata);
        assert!(!config.output.compress_output);

        // Test performance defaults
        assert_eq!(config.performance.chunk_size, 10000);
        assert_eq!(config.performance.batch_size, 1000);
        assert!(config.performance.cache_alignments);

        // Test database defaults
        assert_eq!(config.database.database_dir, None);
        assert_eq!(config.database.retention_count, 3);
        assert!(!config.database.auto_update_check);
        assert_eq!(config.database.preferred_mirror, Some("ebi".to_string()));
    }

    #[test]
    fn test_default_config_function() {
        let config1 = Config::default();
        let config2 = default_config();

        // Both should produce identical configs
        assert_eq!(config1.reduction.target_ratio, config2.reduction.target_ratio);
        assert_eq!(config1.alignment.algorithm, config2.alignment.algorithm);
    }

    #[test]
    fn test_load_valid_config() {
        let toml_content = r#"
[reduction]
target_ratio = 0.5
min_sequence_length = 100
max_delta_distance = 200
similarity_threshold = 0.8
taxonomy_aware = true

[alignment]
gap_penalty = 30
gap_extension = 15
algorithm = "smith-waterman"

[output]
format = "json"
include_metadata = false
compress_output = true

[performance]
chunk_size = 5000
batch_size = 500
cache_alignments = false

[database]
database_dir = "/custom/path"
retention_count = 5
auto_update_check = true
preferred_mirror = "ncbi"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", toml_content).unwrap();

        let config = load_config(temp_file.path()).unwrap();

        assert_eq!(config.reduction.target_ratio, 0.5);
        assert_eq!(config.reduction.min_sequence_length, 100);
        assert_eq!(config.reduction.max_delta_distance, 200);
        assert_eq!(config.reduction.similarity_threshold, 0.8);
        assert!(config.reduction.taxonomy_aware);

        assert_eq!(config.alignment.gap_penalty, 30);
        assert_eq!(config.alignment.gap_extension, 15);
        assert_eq!(config.alignment.algorithm, "smith-waterman");

        assert_eq!(config.output.format, "json");
        assert!(!config.output.include_metadata);
        assert!(config.output.compress_output);

        assert_eq!(config.performance.chunk_size, 5000);
        assert_eq!(config.performance.batch_size, 500);
        assert!(!config.performance.cache_alignments);

        assert_eq!(config.database.database_dir, Some("/custom/path".to_string()));
        assert_eq!(config.database.retention_count, 5);
        assert!(config.database.auto_update_check);
        assert_eq!(config.database.preferred_mirror, Some("ncbi".to_string()));
    }

    #[test]
    fn test_load_partial_config() {
        // Test that missing fields use defaults
        let toml_content = r#"
[reduction]
target_ratio = 0.7

[alignment]
algorithm = "custom"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", toml_content).unwrap();

        let config = load_config(temp_file.path()).unwrap();

        // Specified values
        assert_eq!(config.reduction.target_ratio, 0.7);
        assert_eq!(config.alignment.algorithm, "custom");

        // Default values for unspecified fields
        assert_eq!(config.reduction.min_sequence_length, 50);
        assert_eq!(config.alignment.gap_penalty, 20);
        assert_eq!(config.output.format, "fasta");
        assert_eq!(config.performance.chunk_size, 10000);
    }

    #[test]
    fn test_load_invalid_config() {
        let toml_content = "this is not valid TOML {{";

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", toml_content).unwrap();

        let result = load_config(temp_file.path());
        assert!(result.is_err());

        match result.unwrap_err() {
            TalariaError::Configuration(msg) => {
                assert!(msg.contains("Failed to parse config"));
            }
            _ => panic!("Expected Configuration error"),
        }
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = load_config("/nonexistent/path/to/config.toml");
        assert!(result.is_err());

        match result.unwrap_err() {
            TalariaError::Io(_) => {}, // Expected
            _ => panic!("Expected Io error"),
        }
    }

    #[test]
    fn test_save_config() {
        let config = Config::default();
        let temp_file = NamedTempFile::new().unwrap();

        save_config(temp_file.path(), &config).unwrap();

        // Verify file exists and can be loaded
        assert!(temp_file.path().exists());
        let loaded_config = load_config(temp_file.path()).unwrap();

        assert_eq!(config.reduction.target_ratio, loaded_config.reduction.target_ratio);
        assert_eq!(config.alignment.algorithm, loaded_config.alignment.algorithm);
    }

    #[test]
    fn test_config_round_trip() {
        let mut config = Config::default();
        config.reduction.target_ratio = 0.42;
        config.alignment.algorithm = "test-algorithm".to_string();
        config.output.compress_output = true;
        config.database.preferred_mirror = Some("test-mirror".to_string());

        let temp_file = NamedTempFile::new().unwrap();

        // Save and reload
        save_config(temp_file.path(), &config).unwrap();
        let loaded = load_config(temp_file.path()).unwrap();

        // Verify all fields match
        assert_eq!(config.reduction.target_ratio, loaded.reduction.target_ratio);
        assert_eq!(config.alignment.algorithm, loaded.alignment.algorithm);
        assert_eq!(config.output.compress_output, loaded.output.compress_output);
        assert_eq!(config.database.preferred_mirror, loaded.database.preferred_mirror);
    }

    #[test]
    fn test_save_config_permission_error() {
        let config = Config::default();
        let result = save_config("/root/cannot_write_here.toml", &config);

        assert!(result.is_err());
        match result.unwrap_err() {
            TalariaError::Io(_) => {}, // Expected
            _ => panic!("Expected Io error for permission denied"),
        }
    }

    #[test]
    fn test_config_serialization_edge_cases() {
        let mut config = Config::default();

        // Test edge values (avoiding usize::MAX which can't serialize as u64 in TOML)
        config.reduction.target_ratio = 0.0;
        config.reduction.min_sequence_length = 0;
        config.reduction.max_delta_distance = 1_000_000_000;  // Large but serializable
        config.alignment.gap_penalty = i32::MIN;
        config.alignment.gap_extension = i32::MAX;

        let temp_file = NamedTempFile::new().unwrap();
        save_config(temp_file.path(), &config).unwrap();
        let loaded = load_config(temp_file.path()).unwrap();

        assert_eq!(config.reduction.target_ratio, loaded.reduction.target_ratio);
        assert_eq!(config.reduction.max_delta_distance, loaded.reduction.max_delta_distance);
        assert_eq!(config.alignment.gap_penalty, loaded.alignment.gap_penalty);
        assert_eq!(config.alignment.gap_extension, loaded.alignment.gap_extension);
    }
}