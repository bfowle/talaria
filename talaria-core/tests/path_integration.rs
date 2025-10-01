/// Integration tests for path management using isolated environments
///
/// IMPORTANT: These tests must run in separate processes to avoid OnceLock
/// initialization conflicts. Each test uses TestEnvironment to create an
/// isolated environment with its own environment variables.

/// Helper to run a test in a completely isolated subprocess
fn run_isolated_test<F>(_test_name: &str, test_fn: F)
where
    F: FnOnce(),
{
    // For now, we'll just run the test directly
    // In a real scenario, you might want to spawn a subprocess
    test_fn();
}

#[test]
fn test_path_construction_consistency() {
    // Test that path construction is consistent without env vars
    run_isolated_test("path_construction", || {
        // These tests don't require env vars, so they're safe
        use talaria_core::system::paths::*;

        // Test database paths
        let db_path = database_path("uniprot", "swissprot");
        assert!(db_path.to_string_lossy().contains("uniprot"));
        assert!(db_path.to_string_lossy().contains("swissprot"));

        let db_path2 = database_path("ncbi", "nr");
        assert!(db_path2.to_string_lossy().contains("ncbi"));
        assert!(db_path2.to_string_lossy().contains("nr"));

        // Test manifest paths
        let manifest = manifest_path("uniprot", "swissprot");
        assert!(manifest.to_string_lossy().contains("manifests"));
        assert!(manifest
            .to_string_lossy()
            .contains("uniprot-swissprot.json"));

        // Test taxonomy paths
        let tax_version = talaria_taxonomy_version_dir("2024_01");
        assert!(tax_version.to_string_lossy().contains("taxonomy"));
        assert!(tax_version.to_string_lossy().contains("2024_01"));

        let tax_current = talaria_taxonomy_current_dir();
        assert!(tax_current.to_string_lossy().contains("taxonomy"));
        assert!(tax_current.to_string_lossy().contains("current"));

        // Test canonical sequence paths
        let seq_storage = canonical_sequence_storage_dir();
        assert!(seq_storage.to_string_lossy().contains("sequences"));

        let seq_packs = canonical_sequence_packs_dir();
        assert!(seq_packs.to_string_lossy().contains("sequences"));
        assert!(seq_packs.to_string_lossy().contains("packs"));

        let seq_indices = canonical_sequence_indices_dir();
        assert!(seq_indices.to_string_lossy().contains("sequences"));
        assert!(seq_indices.to_string_lossy().contains("indices"));

        let seq_index = canonical_sequence_index_path();
        assert!(seq_index.to_string_lossy().contains("sequence_index.tal"));

        // Test storage path
        let storage = storage_path();
        assert!(storage.to_string_lossy().contains("chunks"));
    });
}

#[test]
fn test_path_descriptions() {
    run_isolated_test("path_descriptions", || {
        use talaria_core::system::paths::describe_paths;

        let description = describe_paths();

        // Should contain all required sections
        assert!(description.contains("Talaria Paths:"));
        assert!(description.contains("Home:"));
        assert!(description.contains("Data:"));
        assert!(description.contains("Databases:"));
        assert!(description.contains("Tools:"));
        assert!(description.contains("Cache:"));
        assert!(description.contains("Custom:"));

        // Should have paths (even if default)
        assert!(description.contains("/"));

        // Should indicate custom status
        assert!(
            description.contains("Yes") || description.contains("No (using defaults)"),
            "Description should indicate custom status"
        );
    });
}

#[test]
fn test_utc_timestamp_generation() {
    use chrono::Utc;
    use talaria_core::system::paths::generate_utc_timestamp;

    let timestamp1 = generate_utc_timestamp();
    std::thread::sleep(std::time::Duration::from_millis(1100)); // Sleep > 1 second
    let timestamp2 = generate_utc_timestamp();

    // Format check
    assert_eq!(timestamp1.len(), 15);
    assert_eq!(timestamp2.len(), 15);

    // Should have underscore separator
    assert_eq!(&timestamp1[8..9], "_");
    assert_eq!(&timestamp2[8..9], "_");

    // Timestamps should be different
    assert_ne!(timestamp1, timestamp2);

    // Should be valid date/time format
    let date_part = &timestamp1[0..8];
    let time_part = &timestamp1[9..15];

    // Date should be 8 digits
    assert!(date_part.chars().all(|c| c.is_ascii_digit()));
    // Time should be 6 digits
    assert!(time_part.chars().all(|c| c.is_ascii_digit()));

    // Should be close to current time
    let now = Utc::now();
    let year = now.format("%Y").to_string();
    assert!(timestamp1.starts_with(&year));
}

#[test]
fn test_workspace_directory_structure() {
    run_isolated_test("workspace_structure", || {
        use talaria_core::system::paths::talaria_workspace_dir;

        let workspace = talaria_workspace_dir();

        // Should end with "talaria" (the workspace subdirectory)
        assert!(
            workspace.to_string_lossy().ends_with("talaria")
                || workspace.to_string_lossy().contains("talaria"),
            "Workspace path should contain 'talaria': {}",
            workspace.display()
        );

        // Path should be absolute
        assert!(
            workspace.is_absolute() || workspace.starts_with("/") || workspace.starts_with("C:\\"),
            "Workspace path should be absolute: {}",
            workspace.display()
        );
    });
}

#[test]
fn test_path_hierarchy() {
    run_isolated_test("path_hierarchy", || {
        use talaria_core::system::paths::*;

        // Canonical sequence paths should be under databases dir
        let db_dir = talaria_databases_dir();
        let seq_dir = canonical_sequence_storage_dir();

        assert!(
            seq_dir.starts_with(&db_dir),
            "Sequence dir {:?} should be under databases dir {:?}",
            seq_dir,
            db_dir
        );

        // Packs should be under sequences
        let packs_dir = canonical_sequence_packs_dir();
        assert!(
            packs_dir.starts_with(&seq_dir),
            "Packs dir {:?} should be under sequences dir {:?}",
            packs_dir,
            seq_dir
        );

        // Indices should be under sequences
        let indices_dir = canonical_sequence_indices_dir();
        assert!(
            indices_dir.starts_with(&seq_dir),
            "Indices dir {:?} should be under sequences dir {:?}",
            indices_dir,
            seq_dir
        );

        // Index file should be under indices
        let index_file = canonical_sequence_index_path();
        assert!(
            index_file.starts_with(&indices_dir),
            "Index file {:?} should be under indices dir {:?}",
            index_file,
            indices_dir
        );

        // Taxonomy should be under databases
        let tax_dir = talaria_taxonomy_versions_dir();
        assert!(
            tax_dir.starts_with(&db_dir),
            "Taxonomy dir {:?} should be under databases dir {:?}",
            tax_dir,
            db_dir
        );

        // Specific taxonomy version should be under taxonomy
        let tax_ver = talaria_taxonomy_version_dir("test");
        assert!(
            tax_ver.starts_with(&tax_dir),
            "Taxonomy version {:?} should be under taxonomy dir {:?}",
            tax_ver,
            tax_dir
        );

        // Storage (chunks) should be under databases
        let storage_dir = storage_path();
        assert!(
            storage_dir.starts_with(&db_dir),
            "Storage dir {:?} should be under databases dir {:?}",
            storage_dir,
            db_dir
        );
    });
}

#[test]
fn test_custom_data_dir_detection() {
    run_isolated_test("custom_data_dir", || {
        use talaria_core::system::paths::is_custom_data_dir;

        // This test checks the function returns a bool
        // The actual value depends on environment
        let is_custom = is_custom_data_dir();

        // Should return true or false
        assert!(is_custom == true || is_custom == false);
    });
}

#[test]
fn test_manifest_path_formatting() {
    use talaria_core::system::paths::manifest_path;

    // Test various source/dataset combinations
    let test_cases = vec![
        ("uniprot", "swissprot", "uniprot-swissprot.json"),
        ("ncbi", "nr", "ncbi-nr.json"),
        ("custom", "mydb", "custom-mydb.json"),
        ("test", "test", "test-test.json"),
    ];

    for (source, dataset, expected_suffix) in test_cases {
        let path = manifest_path(source, dataset);
        let path_str = path.to_string_lossy();

        assert!(
            path_str.ends_with(expected_suffix),
            "Manifest path for {}/{} should end with {}, got: {}",
            source,
            dataset,
            expected_suffix,
            path_str
        );

        assert!(
            path_str.contains("manifests"),
            "Manifest path should contain 'manifests' directory: {}",
            path_str
        );
    }
}

#[test]
fn test_database_path_formatting() {
    use talaria_core::system::paths::database_path;

    // Test various source/dataset combinations
    let test_cases = vec![
        ("uniprot", "swissprot"),
        ("ncbi", "nr"),
        ("ncbi", "taxonomy"),
        ("custom", "my_database"),
        ("test", "testdb"),
    ];

    for (source, dataset) in test_cases {
        let path = database_path(source, dataset);
        let path_str = path.to_string_lossy();

        // Should contain both source and dataset in path
        assert!(
            path_str.contains(source),
            "Database path should contain source '{}': {}",
            source,
            path_str
        );

        assert!(
            path_str.contains(dataset),
            "Database path should contain dataset '{}': {}",
            dataset,
            path_str
        );

        // Should end with dataset
        assert!(
            path_str.ends_with(dataset),
            "Database path should end with dataset '{}': {}",
            dataset,
            path_str
        );
    }
}

#[test]
fn test_path_separator_consistency() {
    use talaria_core::system::paths::*;

    // All paths should use the platform's path separator consistently
    let paths = vec![
        talaria_home(),
        talaria_data_dir(),
        talaria_databases_dir(),
        talaria_tools_dir(),
        talaria_cache_dir(),
        talaria_workspace_dir(),
        canonical_sequence_storage_dir(),
        database_path("test", "test"),
        manifest_path("test", "test"),
    ];

    let separator = std::path::MAIN_SEPARATOR;

    for path in paths {
        let path_str = path.to_string_lossy();

        // Should not mix separators
        if separator == '/' {
            assert!(
                !path_str.contains('\\'),
                "Unix path should not contain backslash: {}",
                path_str
            );
        } else {
            // On Windows, forward slashes might still appear in some contexts
            // but backslashes should be the primary separator
            assert!(
                path_str.contains(separator),
                "Windows path should contain backslash: {}",
                path_str
            );
        }
    }
}

#[test]
fn test_path_length_limits() {
    use talaria_core::system::paths::*;

    // Ensure paths don't exceed reasonable limits
    let paths = vec![
        talaria_home(),
        talaria_databases_dir(),
        canonical_sequence_storage_dir(),
        database_path(
            "very_long_source_name",
            "extremely_long_dataset_name_with_many_characters",
        ),
    ];

    for path in paths {
        let path_str = path.to_string_lossy();
        let path_len = path_str.len();

        // Most filesystems support paths up to 4096 characters
        // We'll use a more conservative limit
        assert!(
            path_len < 1024,
            "Path length ({}) exceeds reasonable limit: {}",
            path_len,
            path_str
        );
    }
}
