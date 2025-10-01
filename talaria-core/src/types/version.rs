//! Version and update related types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Database version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseVersionInfo {
    pub timestamp: String,
    pub created_at: DateTime<Utc>,
    pub upstream_version: Option<String>,
    pub source: String,
    pub dataset: String,
    pub aliases: Vec<String>,
    pub chunk_count: usize,
    pub sequence_count: usize,
    pub total_size: u64,
}

/// Temporal version information for bi-temporal tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalVersionInfo {
    pub version: String,
    pub timestamp: DateTime<Utc>,
    pub version_type: String,
    pub sequence_root: String,
    pub taxonomy_root: String,
    pub chunk_count: usize,
    pub sequence_count: usize,
    pub changes: Vec<String>,
    pub parent_version: Option<String>,
}

/// Update status for checking database updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub updates_available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub changes_summary: String,
    pub estimated_download_size: usize,
}
