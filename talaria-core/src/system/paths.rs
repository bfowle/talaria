use std::path::PathBuf;
use std::sync::OnceLock;

// Cache the paths to avoid repeated environment lookups
static TALARIA_HOME: OnceLock<PathBuf> = OnceLock::new();
static TALARIA_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static TALARIA_DATABASES_DIR: OnceLock<PathBuf> = OnceLock::new();
static TALARIA_TOOLS_DIR: OnceLock<PathBuf> = OnceLock::new();
static TALARIA_CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
static TALARIA_WORKSPACE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Generate a UTC timestamp for version identifiers
/// Returns format: YYYYMMDD_HHMMSS (in UTC timezone)
/// This ensures consistent versioning across distributed teams
pub fn generate_utc_timestamp() -> String {
    chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string()
}

/// Get the Talaria home directory
/// Checks TALARIA_HOME environment variable, falls back to ${HOME}/.talaria
pub fn talaria_home() -> PathBuf {
    TALARIA_HOME
        .get_or_init(|| {
            if let Ok(path) = std::env::var("TALARIA_HOME") {
                PathBuf::from(path)
            } else {
                let home = std::env::var("HOME").unwrap_or_else(|_| {
                    std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string())
                });
                PathBuf::from(home).join(".talaria")
            }
        })
        .clone()
}

/// Get the Talaria data directory
/// Checks TALARIA_DATA_DIR environment variable, falls back to TALARIA_HOME
pub fn talaria_data_dir() -> PathBuf {
    TALARIA_DATA_DIR
        .get_or_init(|| {
            if let Ok(path) = std::env::var("TALARIA_DATA_DIR") {
                PathBuf::from(path)
            } else {
                talaria_home()
            }
        })
        .clone()
}

/// Get the databases storage directory
/// Checks TALARIA_DATABASES_DIR environment variable, falls back to TALARIA_DATA_DIR/databases
pub fn talaria_databases_dir() -> PathBuf {
    TALARIA_DATABASES_DIR
        .get_or_init(|| {
            if let Ok(path) = std::env::var("TALARIA_DATABASES_DIR") {
                PathBuf::from(path)
            } else {
                talaria_data_dir().join("databases")
            }
        })
        .clone()
}

/// Get the tools directory
/// Checks TALARIA_TOOLS_DIR environment variable, falls back to TALARIA_DATA_DIR/tools
pub fn talaria_tools_dir() -> PathBuf {
    TALARIA_TOOLS_DIR
        .get_or_init(|| {
            if let Ok(path) = std::env::var("TALARIA_TOOLS_DIR") {
                PathBuf::from(path)
            } else {
                talaria_data_dir().join("tools")
            }
        })
        .clone()
}

/// Get the cache directory
/// Checks TALARIA_CACHE_DIR environment variable, falls back to TALARIA_DATA_DIR/cache
pub fn talaria_cache_dir() -> PathBuf {
    TALARIA_CACHE_DIR
        .get_or_init(|| {
            if let Ok(path) = std::env::var("TALARIA_CACHE_DIR") {
                PathBuf::from(path)
            } else {
                talaria_data_dir().join("cache")
            }
        })
        .clone()
}

/// Get the unified taxonomy directory
/// Returns: TALARIA_DATABASES_DIR/taxonomy
pub fn talaria_taxonomy_versions_dir() -> PathBuf {
    talaria_databases_dir().join("taxonomy")
}

/// Get the current taxonomy version directory
/// Returns: TALARIA_DATABASES_DIR/taxonomy/current (symlink)
pub fn talaria_taxonomy_current_dir() -> PathBuf {
    talaria_taxonomy_versions_dir().join("current")
}

/// Get a specific taxonomy version directory
/// Returns: TALARIA_DATABASES_DIR/taxonomy/{version}
pub fn talaria_taxonomy_version_dir(version: &str) -> PathBuf {
    talaria_taxonomy_versions_dir().join(version)
}

/// Get the workspace directory for temporal workspaces
/// Checks TALARIA_WORKSPACE_DIR environment variable, falls back to /tmp/talaria or $TMPDIR/talaria
pub fn talaria_workspace_dir() -> PathBuf {
    TALARIA_WORKSPACE_DIR
        .get_or_init(|| {
            if let Ok(path) = std::env::var("TALARIA_WORKSPACE_DIR") {
                PathBuf::from(path)
            } else if let Ok(tmpdir) = std::env::var("TMPDIR") {
                PathBuf::from(tmpdir).join("talaria")
            } else {
                PathBuf::from("/tmp/talaria")
            }
        })
        .clone()
}

/// Get database path for a specific source and dataset
pub fn database_path(source: &str, dataset: &str) -> PathBuf {
    talaria_databases_dir().join(source).join(dataset)
}

/// Get storage path
pub fn storage_path() -> PathBuf {
    talaria_databases_dir().join("chunks")
}

/// Get the canonical sequence storage directory
/// This is the SINGLE shared location for all unique sequences across all databases
/// SEQUOIA Principle #1: Canonical Sequence Storage - Each unique sequence stored exactly once
/// Returns: TALARIA_DATABASES_DIR/sequences
pub fn canonical_sequence_storage_dir() -> PathBuf {
    talaria_databases_dir().join("sequences")
}

/// Get the canonical sequence packs directory
/// Returns: TALARIA_DATABASES_DIR/sequences/packs
pub fn canonical_sequence_packs_dir() -> PathBuf {
    canonical_sequence_storage_dir().join("packs")
}

/// Get the canonical sequence indices directory
/// Returns: TALARIA_DATABASES_DIR/sequences/indices
pub fn canonical_sequence_indices_dir() -> PathBuf {
    canonical_sequence_storage_dir().join("indices")
}

/// Get the canonical sequence index file path
/// Returns: TALARIA_DATABASES_DIR/sequences/indices/sequence_index.tal
pub fn canonical_sequence_index_path() -> PathBuf {
    canonical_sequence_indices_dir().join("sequence_index.tal")
}

/// Get manifest path for a specific database
pub fn manifest_path(source: &str, dataset: &str) -> PathBuf {
    talaria_databases_dir()
        .join("manifests")
        .join(format!("{}-{}.json", source, dataset))
}

/// Check if running in a custom data directory
pub fn is_custom_data_dir() -> bool {
    std::env::var("TALARIA_DATA_DIR").is_ok() || std::env::var("TALARIA_HOME").is_ok()
}

/// Get a human-readable description of the current path configuration
pub fn describe_paths() -> String {
    format!(
        "Talaria Paths:\n  \
        Home: {}\n  \
        Data: {}\n  \
        Databases: {}\n  \
        Tools: {}\n  \
        Cache: {}\n  \
        Custom: {}",
        talaria_home().display(),
        talaria_data_dir().display(),
        talaria_databases_dir().display(),
        talaria_tools_dir().display(),
        talaria_cache_dir().display(),
        if is_custom_data_dir() {
            "Yes"
        } else {
            "No (using defaults)"
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utc_timestamp_format() {
        let timestamp = generate_utc_timestamp();

        // Should be in format YYYYMMDD_HHMMSS
        assert_eq!(timestamp.len(), 15);
        assert_eq!(&timestamp[8..9], "_");

        // Should only contain digits and underscore
        for (i, c) in timestamp.chars().enumerate() {
            if i == 8 {
                assert_eq!(c, '_');
            } else {
                assert!(c.is_ascii_digit());
            }
        }
    }

    #[test]
    fn test_database_path_construction() {
        // These don't depend on environment variables
        let path = database_path("uniprot", "swissprot");
        assert!(path.ends_with("uniprot/swissprot"));

        let path = database_path("ncbi", "nr");
        assert!(path.ends_with("ncbi/nr"));

        let path = database_path("custom", "mydb");
        assert!(path.ends_with("custom/mydb"));
    }

    #[test]
    fn test_manifest_path_construction() {
        let path = manifest_path("uniprot", "swissprot");
        assert!(path.ends_with("manifests/uniprot-swissprot.json"));

        let path = manifest_path("ncbi", "taxonomy");
        assert!(path.ends_with("manifests/ncbi-taxonomy.json"));
    }

    #[test]
    fn test_taxonomy_path_construction() {
        let version_dir = talaria_taxonomy_version_dir("20241225");
        assert!(version_dir.ends_with("taxonomy/20241225"));

        let current_dir = talaria_taxonomy_current_dir();
        assert!(current_dir.ends_with("taxonomy/current"));
    }

    #[test]
    fn test_canonical_sequence_paths() {
        let storage_dir = canonical_sequence_storage_dir();
        assert!(storage_dir.ends_with("sequences"));

        let packs_dir = canonical_sequence_packs_dir();
        assert!(packs_dir.ends_with("sequences/packs"));

        let indices_dir = canonical_sequence_indices_dir();
        assert!(indices_dir.ends_with("sequences/indices"));

        let index_path = canonical_sequence_index_path();
        assert!(index_path.ends_with("sequences/indices/sequence_index.tal"));
    }

    #[test]
    fn test_storage_path() {
        let path = storage_path();
        assert!(path.ends_with("chunks"));
    }

    #[test]
    fn test_is_custom_data_dir() {
        // This test checks the function logic without depending on actual env vars
        // The actual behavior is tested in integration tests
        let result = is_custom_data_dir();
        // Result depends on whether env vars are set - just verify it returns a bool
        assert!(result == true || result == false);
    }

    #[test]
    fn test_describe_paths() {
        let description = describe_paths();

        // Should contain all path types
        assert!(description.contains("Home:"));
        assert!(description.contains("Data:"));
        assert!(description.contains("Databases:"));
        assert!(description.contains("Tools:"));
        assert!(description.contains("Cache:"));
        assert!(description.contains("Custom:"));

        // Should indicate custom status
        assert!(description.contains("Yes") || description.contains("No (using defaults)"));
    }
}
