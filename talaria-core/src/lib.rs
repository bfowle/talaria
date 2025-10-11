//! Core utilities and types shared across all Talaria crates

pub mod audit;
pub mod config;
pub mod error;
pub mod system;
pub mod types;

// Re-export commonly used types
pub use config::{load_config, save_config, Config};
pub use error::{TalariaError, TalariaResult, VerificationError, VerificationErrorType};

// Re-export core types
pub use types::{
    ChunkInfo, ChunkMetadata, ChunkType, DatabaseSource, DatabaseSourceInfo, DeltaChunk,
    DetailedStorageStats, GCResult, GarbageCollectionStats, NCBIDatabase, OutputFormat,
    RemoteStatus, SHA256Hash, SequenceType, StorageStats, SyncResult, TargetAligner, TaxonId,
    TaxonomyDataSource, TaxonomyStats, UniProtDatabase,
};

// Re-export system utilities
pub use system::{
    current_version,
    generate_utc_timestamp,
    is_compatible,
    // Version functions
    parse_version,
    talaria_cache_dir,
    talaria_databases_dir,
    // Path functions
    talaria_home,
    talaria_tools_dir,
    talaria_workspace_dir,
};

/// Version information for the Talaria project
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");
