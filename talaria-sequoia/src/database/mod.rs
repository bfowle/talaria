//! Database management functionality for SEQUOIA

pub mod diff;
pub mod manager;

pub use diff::{DatabaseDiffer, ComparisonResult};
pub use manager::DatabaseManager;

// Result types for database operations
#[derive(Debug)]
pub enum DownloadResult {
    UpToDate,
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

#[derive(Debug)]
pub struct DatabaseInfo {
    pub name: String,
    pub source: String,
    pub version: String,
    pub chunks: usize,
    pub sequences: usize,
    pub size: u64,
}