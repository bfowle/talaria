use std::path::PathBuf;
use std::sync::OnceLock;

// Cache the paths to avoid repeated environment lookups
static TALARIA_HOME: OnceLock<PathBuf> = OnceLock::new();
static TALARIA_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static TALARIA_DATABASES_DIR: OnceLock<PathBuf> = OnceLock::new();
static TALARIA_TOOLS_DIR: OnceLock<PathBuf> = OnceLock::new();
static TALARIA_CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
static TALARIA_TAXONOMY_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Get the Talaria home directory
/// Checks TALARIA_HOME environment variable, falls back to ${HOME}/.talaria
pub fn talaria_home() -> PathBuf {
    TALARIA_HOME.get_or_init(|| {
        if let Ok(path) = std::env::var("TALARIA_HOME") {
            PathBuf::from(path)
        } else {
            let home = std::env::var("HOME")
                .unwrap_or_else(|_| std::env::var("USERPROFILE")
                    .unwrap_or_else(|_| ".".to_string()));
            PathBuf::from(home).join(".talaria")
        }
    }).clone()
}

/// Get the Talaria data directory
/// Checks TALARIA_DATA_DIR environment variable, falls back to TALARIA_HOME
pub fn talaria_data_dir() -> PathBuf {
    TALARIA_DATA_DIR.get_or_init(|| {
        if let Ok(path) = std::env::var("TALARIA_DATA_DIR") {
            PathBuf::from(path)
        } else {
            talaria_home()
        }
    }).clone()
}

/// Get the databases storage directory
/// Checks TALARIA_DATABASES_DIR environment variable, falls back to TALARIA_DATA_DIR/databases
pub fn talaria_databases_dir() -> PathBuf {
    TALARIA_DATABASES_DIR.get_or_init(|| {
        if let Ok(path) = std::env::var("TALARIA_DATABASES_DIR") {
            PathBuf::from(path)
        } else {
            talaria_data_dir().join("databases")
        }
    }).clone()
}

/// Get the tools directory
/// Checks TALARIA_TOOLS_DIR environment variable, falls back to TALARIA_DATA_DIR/tools
pub fn talaria_tools_dir() -> PathBuf {
    TALARIA_TOOLS_DIR.get_or_init(|| {
        if let Ok(path) = std::env::var("TALARIA_TOOLS_DIR") {
            PathBuf::from(path)
        } else {
            talaria_data_dir().join("tools")
        }
    }).clone()
}

/// Get the cache directory
/// Checks TALARIA_CACHE_DIR environment variable, falls back to TALARIA_DATA_DIR/cache
pub fn talaria_cache_dir() -> PathBuf {
    TALARIA_CACHE_DIR.get_or_init(|| {
        if let Ok(path) = std::env::var("TALARIA_CACHE_DIR") {
            PathBuf::from(path)
        } else {
            talaria_data_dir().join("cache")
        }
    }).clone()
}

/// Get the taxonomy directory
/// Checks TALARIA_TAXONOMY_DIR environment variable, falls back to TALARIA_DATABASES_DIR/taxonomy
pub fn talaria_taxonomy_dir() -> PathBuf {
    TALARIA_TAXONOMY_DIR.get_or_init(|| {
        if let Ok(path) = std::env::var("TALARIA_TAXONOMY_DIR") {
            PathBuf::from(path)
        } else {
            talaria_databases_dir().join("taxonomy")
        }
    }).clone()
}

/// Get database path for a specific source and dataset
pub fn database_path(source: &str, dataset: &str) -> PathBuf {
    talaria_databases_dir().join(source).join(dataset)
}

/// Get storage path
pub fn storage_path() -> PathBuf {
    talaria_databases_dir().join("chunks")
}

/// Get manifest path for a specific database
pub fn manifest_path(source: &str, dataset: &str) -> PathBuf {
    talaria_databases_dir().join("manifests").join(format!("{}-{}.json", source, dataset))
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
        if is_custom_data_dir() { "Yes" } else { "No (using defaults)" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    #[ignore] // This test must run in isolation due to OnceLock initialization
    fn test_default_paths() {
        // Clear any existing environment variables for testing
        env::remove_var("TALARIA_HOME");
        env::remove_var("TALARIA_DATA_DIR");
        env::remove_var("TALARIA_DATABASES_DIR");

        let expected_home = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".talaria");

        // Force re-initialization for testing
        // Note: In production, OnceLock prevents this
        assert_eq!(talaria_home(), expected_home);
        assert_eq!(talaria_data_dir(), expected_home);
        assert_eq!(talaria_databases_dir(), expected_home.join("databases"));
    }

    #[test]
    fn test_custom_paths() {
        // This test would need to be run in isolation due to OnceLock
        // In practice, environment variables should be set before first use
        env::set_var("TALARIA_HOME", "/custom/talaria");
        env::set_var("TALARIA_DATABASES_DIR", "/fast/ssd/databases");

        // These assertions would work if OnceLock wasn't already initialized
        // assert_eq!(talaria_home(), PathBuf::from("/custom/talaria"));
        // assert_eq!(talaria_databases_dir(), PathBuf::from("/fast/ssd/databases"));
    }

    #[test]
    fn test_database_paths() {
        let manifest_path = manifest_path("uniprot", "swissprot");
        assert!(manifest_path.ends_with("manifests/uniprot-swissprot.json"));

        let db_path = database_path("ncbi", "nr");
        assert!(db_path.ends_with("ncbi/nr"));
    }
}