//! Database management functionality for SEQUOIA

pub mod cache;
pub mod diff;
pub mod manager;
pub mod manager_resume;
pub mod manager_unified_progress;

#[cfg(test)]
mod manager_test;

pub use diff::DatabaseDiffer;
pub use manager::DatabaseManager;

// Re-export comparison types from talaria-utils for convenience
pub use talaria_utils::report::{
    ComparisonResult, DatabaseStatistics, ModifiedSequence, RenamedSequence, SequenceChange,
    SequenceInfo,
};

// Result types for database operations
#[derive(Debug)]
pub enum DownloadResult {
    UpToDate,
    AlreadyExists {
        total_chunks: usize,
        total_size: u64,
    },
    Updated {
        chunks_added: usize,
        chunks_updated: usize,
        chunks_removed: usize,
        size_difference: i64,
    },
    Downloaded {
        total_chunks: usize,
        total_size: u64,
    },
    InitialDownload {
        total_chunks: usize,
        total_size: u64,
    },
}

#[derive(Debug)]
pub enum TaxonomyUpdateResult {
    UpToDate,
    Updated {
        nodes_updated: bool,
        names_updated: bool,
        merged_updated: bool,
        deleted_updated: bool,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatabaseInfo {
    pub name: String,
    pub source: String,
    pub version: String,
    pub chunks: usize,
    pub sequences: usize,
    pub size: u64,
}
