//! Core utilities and types shared across all Talaria crates

pub mod config;
pub mod error;
pub mod system;
pub mod types;

// Re-export commonly used types
pub use error::{TalariaError, TalariaResult, VerificationError, VerificationErrorType};
pub use config::{Config, load_config, save_config};

// Re-export core types
pub use types::{
    SHA256Hash, TaxonId, TaxonomyDataSource,
    ChunkInfo, ChunkMetadata, DeltaChunk, ChunkType,
    DatabaseSource, DatabaseSourceInfo, UniProtDatabase, NCBIDatabase,
    StorageStats, GCResult, GarbageCollectionStats, DetailedStorageStats,
    TaxonomyStats, SyncResult, RemoteStatus,
    OutputFormat,
    SequenceType,
};

// Re-export system utilities
pub use system::{
    // Path functions
    talaria_home,
    talaria_databases_dir,
    talaria_tools_dir,
    talaria_cache_dir,
    talaria_workspace_dir,
    generate_utc_timestamp,
    // Version functions
    parse_version,
    is_compatible,
    current_version,
};

/// Version information for the Talaria project
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");