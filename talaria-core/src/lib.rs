//! Core utilities and types shared across all Talaria crates

pub mod error;
pub mod paths;
pub mod config;
pub mod version;

// Re-export commonly used types
pub use error::{TalariaError, TalariaResult};
pub use paths::{
    talaria_home,
    talaria_databases_dir,
    talaria_tools_dir,
    talaria_cache_dir,
    talaria_workspace_dir,
    generate_utc_timestamp,
};
pub use config::{Config, load_config, save_config};
pub use version::{parse_version, is_compatible, current_version};

/// Version information for the Talaria project
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");