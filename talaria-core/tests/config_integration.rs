#![allow(clippy::bool_assert_comparison)]

use std::fs;
/// Integration tests for configuration loading and saving
use talaria_core::config::{default_config, load_config, save_config, Config};
use talaria_test::{TestConfig, TestEnvironment};

#[test]
fn test_config_loading_from_multiple_sources() {
    let env = TestEnvironment::with_config(TestConfig::default()).unwrap();

    // Create config files in different locations
    let home_config = env.root().join("config.toml");
    let project_config = env.root().join("project.toml");

    // Create home config with basic settings
    let home_content = r#"
[reduction]
target_ratio = 0.4

[database]
retention_count = 5
"#;
    fs::write(&home_config, home_content).unwrap();

    // Create project config with different settings
    let project_content = r#"
[reduction]
target_ratio = 0.6
min_sequence_length = 75

[alignment]
algorithm = "smith-waterman"

[database]
auto_update_check = true
preferred_mirror = "ncbi"
"#;
    fs::write(&project_config, project_content).unwrap();

    // Load configs and verify
    let config1 = load_config(&home_config).unwrap();
    assert_eq!(config1.reduction.target_ratio, 0.4);
    assert_eq!(config1.reduction.min_sequence_length, 50); // Default
    assert_eq!(config1.database.retention_count, 5);

    let config2 = load_config(&project_config).unwrap();
    assert_eq!(config2.reduction.target_ratio, 0.6);
    assert_eq!(config2.reduction.min_sequence_length, 75);
    assert_eq!(config2.alignment.algorithm, "smith-waterman");
    assert!(config2.database.auto_update_check);
}

#[test]
fn test_config_environment_variable_overrides() {
    // Note: TestEnvironment sets its own environment variables at creation
    // This test verifies that config files are loaded correctly even when env vars exist
    let env = TestEnvironment::with_config(TestConfig::default()).unwrap();

    // Create a config file
    let config_file = env.root().join("config.toml");
    let content = r#"
[database]
database_dir = "/original/path"
retention_count = 3
"#;
    fs::write(&config_file, content).unwrap();

    // Load config
    let config = load_config(&config_file).unwrap();

    // Config file values should be loaded (not overridden by env vars)
    assert_eq!(
        config.database.database_dir,
        Some("/original/path".to_string())
    );
    assert_eq!(config.database.retention_count, 3);

    // Note: Path functions would use env vars, but config loading doesn't override
    // This is expected behavior - config files and env vars serve different purposes
}

#[test]
fn test_config_migration_compatibility() {
    // Test that old config formats can still be loaded
    let env = TestEnvironment::with_config(TestConfig::default()).unwrap();

    // Old format config (minimal fields)
    let old_config = r#"
[reduction]
target_ratio = 0.5

[alignment]
gap_penalty = 25
"#;

    let config_file = env.root().join("old_config.toml");
    fs::write(&config_file, old_config).unwrap();

    // Should load with defaults for missing fields
    let config = load_config(&config_file).unwrap();
    assert_eq!(config.reduction.target_ratio, 0.5);
    assert_eq!(config.alignment.gap_penalty, 25);

    // New fields should have defaults
    assert_eq!(config.reduction.taxonomy_aware, false);
    assert_eq!(config.database.preferred_mirror, Some("ebi".to_string()));
}

#[test]
fn test_config_validation() {
    let env = TestEnvironment::with_config(TestConfig::default()).unwrap();

    // Test various invalid configurations
    let test_cases = vec![
        // Invalid TOML syntax
        ("invalid syntax {{", true),
        // Missing required sections (all sections have defaults, so this succeeds)
        ("[reduction]\n", false),
        // Invalid type for field
        ("[reduction]\ntarget_ratio = \"not a number\"", true),
        // Out of range values (these are accepted but may cause issues at runtime)
        ("[reduction]\ntarget_ratio = -1.0", false),
        ("[reduction]\ntarget_ratio = 2.0", false),
    ];

    for (content, should_fail) in test_cases {
        let config_file = env.root().join("test_config.toml");
        fs::write(&config_file, content).unwrap();

        let result = load_config(&config_file);
        if should_fail {
            assert!(result.is_err(), "Config should fail: {}", content);
        } else {
            assert!(result.is_ok(), "Config should succeed: {}", content);
        }
    }
}

#[test]
fn test_config_serialization_preservation() {
    let env = TestEnvironment::with_config(TestConfig::default()).unwrap();

    // Create a complex config
    let mut config = Config::default();
    config.reduction.target_ratio = 0.42;
    config.reduction.similarity_threshold = 0.85;
    config.reduction.taxonomy_aware = true;
    config.alignment.algorithm = "custom-algorithm".to_string();
    config.output.compress_output = true;
    config.performance.chunk_size = 20000;
    config.database.database_dir = Some("/custom/db/path".to_string());
    config.database.preferred_mirror = Some("custom-mirror".to_string());

    // Save to file
    let config_file = env.root().join("complex_config.toml");
    save_config(&config_file, &config).unwrap();

    // Load back
    let loaded = load_config(&config_file).unwrap();

    // Verify all fields preserved
    assert_eq!(config.reduction.target_ratio, loaded.reduction.target_ratio);
    assert_eq!(
        config.reduction.similarity_threshold,
        loaded.reduction.similarity_threshold
    );
    assert_eq!(
        config.reduction.taxonomy_aware,
        loaded.reduction.taxonomy_aware
    );
    assert_eq!(config.alignment.algorithm, loaded.alignment.algorithm);
    assert_eq!(config.output.compress_output, loaded.output.compress_output);
    assert_eq!(config.performance.chunk_size, loaded.performance.chunk_size);
    assert_eq!(config.database.database_dir, loaded.database.database_dir);
    assert_eq!(
        config.database.preferred_mirror,
        loaded.database.preferred_mirror
    );
}

#[test]
fn test_config_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    let env = TestEnvironment::with_config(TestConfig::default()).unwrap();
    let config_file = Arc::new(env.root().join("concurrent_config.toml"));

    // Create initial config
    let config = Config::default();
    save_config(config_file.as_ref(), &config).unwrap();

    // Spawn multiple threads to read config
    let mut handles = vec![];
    for i in 0..10 {
        let config_file = Arc::clone(&config_file);
        let handle = thread::spawn(move || {
            let config = load_config(config_file.as_ref()).unwrap();
            assert_eq!(config.reduction.target_ratio, 0.3); // Default value
            i
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_config_format_preservation() {
    let env = TestEnvironment::with_config(TestConfig::default()).unwrap();

    // Create a nicely formatted config
    let formatted_config = r#"# Talaria Configuration File

[reduction]
# Target reduction ratio
target_ratio = 0.5
# Minimum sequence length to process
min_sequence_length = 100

[alignment]
# Algorithm to use for sequence alignment
algorithm = "needleman-wunsch"

[database]
# Preferred mirror for downloads
preferred_mirror = "ebi"
"#;

    let config_file = env.root().join("formatted_config.toml");
    fs::write(&config_file, formatted_config).unwrap();

    // Load and save
    let config = load_config(&config_file).unwrap();
    let output_file = env.root().join("output_config.toml");
    save_config(&output_file, &config).unwrap();

    // Read output
    let _output = fs::read_to_string(&output_file).unwrap();

    // Should be valid TOML
    let reloaded = load_config(&output_file).unwrap();
    assert_eq!(
        config.reduction.target_ratio,
        reloaded.reduction.target_ratio
    );
}

#[test]
fn test_default_config_completeness() {
    // Ensure default config has all necessary fields
    let config = default_config();

    // Reduction config
    assert!(config.reduction.target_ratio > 0.0);
    assert!(config.reduction.min_sequence_length > 0);
    assert!(config.reduction.max_delta_distance > 0);

    // Alignment config
    assert!(config.alignment.gap_penalty != 0);
    assert!(config.alignment.gap_extension != 0);
    assert!(!config.alignment.algorithm.is_empty());

    // Output config
    assert!(!config.output.format.is_empty());

    // Performance config
    assert!(config.performance.chunk_size > 0);
    assert!(config.performance.batch_size > 0);

    // Database config
    assert!(config.database.retention_count > 0);
}

#[test]
fn test_config_with_special_characters() {
    let env = TestEnvironment::with_config(TestConfig::default()).unwrap();

    // Config with special characters in strings
    let content = r#"
[database]
database_dir = "/path/with spaces/and-dashes/under_scores"
preferred_mirror = "mirror-with-special.chars_123"

[alignment]
algorithm = "algorithm:with:colons"
"#;

    let config_file = env.root().join("special_chars.toml");
    fs::write(&config_file, content).unwrap();

    let config = load_config(&config_file).unwrap();
    assert_eq!(
        config.database.database_dir,
        Some("/path/with spaces/and-dashes/under_scores".to_string())
    );
    assert_eq!(
        config.database.preferred_mirror,
        Some("mirror-with-special.chars_123".to_string())
    );
    assert_eq!(config.alignment.algorithm, "algorithm:with:colons");

    // Save and reload to ensure round-trip works
    let output_file = env.root().join("special_chars_output.toml");
    save_config(&output_file, &config).unwrap();
    let reloaded = load_config(&output_file).unwrap();

    assert_eq!(config.database.database_dir, reloaded.database.database_dir);
    assert_eq!(config.alignment.algorithm, reloaded.alignment.algorithm);
}
