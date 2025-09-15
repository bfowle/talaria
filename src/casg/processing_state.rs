/// Processing state management for resumable CASG operations
///
/// This module tracks the state of ongoing CASG operations (downloads, chunking, updates)
/// to enable safe resumption after interruptions. It validates that the same version
/// is being resumed to ensure data consistency.

use crate::casg::types::SHA256Hash;
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum age for a resumable state (7 days)
const MAX_STATE_AGE_DAYS: i64 = 7;

/// State tracking for CASG processing operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingState {
    /// Type of operation being performed
    pub operation: OperationType,

    /// Hash of the manifest being processed
    pub manifest_hash: SHA256Hash,

    /// Version string of the manifest
    pub manifest_version: String,

    /// Total number of chunks to process
    pub total_chunks: usize,

    /// Set of successfully completed chunk hashes
    pub completed_chunks: HashSet<SHA256Hash>,

    /// When this operation started
    pub started_at: DateTime<Utc>,

    /// Last time the state was updated
    pub last_updated: DateTime<Utc>,

    /// Information about the source being processed
    pub source_info: SourceInfo,

    /// Optional checkpoint data for complex operations
    pub checkpoint_data: Option<serde_json::Value>,
}

/// Type of CASG operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationType {
    /// Initial download and chunking of a database
    InitialDownload,

    /// Incremental update fetching new/modified chunks
    IncrementalUpdate,

    /// Converting downloaded data to chunks
    Chunking,

    /// Updating taxonomy data
    TaxonomyUpdate,

    /// Database reduction operation
    Reduction { profile: String },
}

/// Information about the data source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Database name (e.g., "uniprot/swissprot")
    pub database: String,

    /// Source URL if applicable
    pub source_url: Option<String>,

    /// ETag for version tracking
    pub etag: Option<String>,

    /// Size of the operation if known
    pub total_size_bytes: Option<u64>,
}

impl ProcessingState {
    /// Create a new processing state
    pub fn new(
        operation: OperationType,
        manifest_hash: SHA256Hash,
        manifest_version: String,
        total_chunks: usize,
        source_info: SourceInfo,
    ) -> Self {
        let now = Utc::now();
        Self {
            operation,
            manifest_hash,
            manifest_version,
            total_chunks,
            completed_chunks: HashSet::new(),
            started_at: now,
            last_updated: now,
            source_info,
            checkpoint_data: None,
        }
    }

    /// Mark a chunk as completed
    pub fn mark_chunk_completed(&mut self, chunk_hash: SHA256Hash) {
        self.completed_chunks.insert(chunk_hash);
        self.last_updated = Utc::now();
    }

    /// Mark multiple chunks as completed
    pub fn mark_chunks_completed(&mut self, chunk_hashes: &[SHA256Hash]) {
        for hash in chunk_hashes {
            self.completed_chunks.insert(hash.clone());
        }
        self.last_updated = Utc::now();
    }

    /// Get the number of remaining chunks
    pub fn remaining_chunks(&self) -> usize {
        self.total_chunks.saturating_sub(self.completed_chunks.len())
    }

    /// Get completion percentage
    pub fn completion_percentage(&self) -> f32 {
        if self.total_chunks == 0 {
            return 100.0;
        }
        (self.completed_chunks.len() as f32 / self.total_chunks as f32) * 100.0
    }

    /// Check if the state is too old to resume
    pub fn is_expired(&self) -> bool {
        let age = Utc::now() - self.last_updated;
        age > Duration::days(MAX_STATE_AGE_DAYS)
    }

    /// Check if this state can be resumed with the given manifest
    pub fn can_resume_with(&self, manifest_hash: &SHA256Hash, manifest_version: &str) -> bool {
        !self.is_expired()
            && self.manifest_hash == *manifest_hash
            && self.manifest_version == manifest_version
    }

    /// Check if the operation is complete
    pub fn is_complete(&self) -> bool {
        self.completed_chunks.len() >= self.total_chunks
    }

    /// Update checkpoint data for complex operations
    pub fn update_checkpoint(&mut self, data: serde_json::Value) {
        self.checkpoint_data = Some(data);
        self.last_updated = Utc::now();
    }

    /// Get a summary of the current state
    pub fn summary(&self) -> String {
        format!(
            "{:?} operation: {}/{} chunks complete ({:.1}%), started {}",
            self.operation,
            self.completed_chunks.len(),
            self.total_chunks,
            self.completion_percentage(),
            self.started_at.format("%Y-%m-%d %H:%M:%S")
        )
    }
}

/// Manager for processing state persistence
pub struct ProcessingStateManager {
    state_dir: PathBuf,
}

impl ProcessingStateManager {
    /// Create a new state manager
    pub fn new(base_path: &Path) -> Result<Self> {
        let state_dir = base_path.join(".processing_states");
        fs::create_dir_all(&state_dir)
            .context("Failed to create processing state directory")?;

        Ok(Self { state_dir })
    }

    /// Save a processing state
    pub fn save_state(&self, state: &ProcessingState, operation_id: &str) -> Result<()> {
        let state_file = self.state_dir.join(format!("{}.json", operation_id));
        let content = serde_json::to_string_pretty(state)
            .context("Failed to serialize processing state")?;

        fs::write(&state_file, content)
            .context("Failed to write processing state file")?;

        Ok(())
    }

    /// Load a processing state
    pub fn load_state(&self, operation_id: &str) -> Result<Option<ProcessingState>> {
        let state_file = self.state_dir.join(format!("{}.json", operation_id));

        if !state_file.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&state_file)
            .context("Failed to read processing state file")?;

        let state: ProcessingState = serde_json::from_str(&content)
            .context("Failed to deserialize processing state")?;

        // Check if state is expired
        if state.is_expired() {
            // Clean up expired state
            self.delete_state(operation_id)?;
            return Ok(None);
        }

        Ok(Some(state))
    }

    /// Delete a processing state
    pub fn delete_state(&self, operation_id: &str) -> Result<()> {
        let state_file = self.state_dir.join(format!("{}.json", operation_id));

        if state_file.exists() {
            fs::remove_file(&state_file)
                .context("Failed to delete processing state file")?;
        }

        Ok(())
    }

    /// List all available processing states
    pub fn list_states(&self) -> Result<Vec<(String, ProcessingState)>> {
        let mut states = Vec::new();

        if !self.state_dir.exists() {
            return Ok(states);
        }

        for entry in fs::read_dir(&self.state_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(operation_id) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(Some(state)) = self.load_state(operation_id) {
                        states.push((operation_id.to_string(), state));
                    }
                }
            }
        }

        Ok(states)
    }

    /// Clean up all expired states
    pub fn cleanup_expired(&self) -> Result<usize> {
        let mut cleaned = 0;

        for (operation_id, state) in self.list_states()? {
            if state.is_expired() {
                self.delete_state(&operation_id)?;
                cleaned += 1;
            }
        }

        Ok(cleaned)
    }

    /// Generate an operation ID from source info
    pub fn generate_operation_id(
        database: &str,
        operation: &OperationType,
    ) -> String {
        let op_suffix = match operation {
            OperationType::InitialDownload => "initial",
            OperationType::IncrementalUpdate => "update",
            OperationType::Chunking => "chunk",
            OperationType::TaxonomyUpdate => "taxonomy",
            OperationType::Reduction { profile } => &format!("reduce_{}", profile),
        };

        format!("{}_{}", database.replace('/', "_"), op_suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processing_state_completion() {
        let source_info = SourceInfo {
            database: "test_db".to_string(),
            source_url: None,
            etag: None,
            total_size_bytes: None,
        };

        let mut state = ProcessingState::new(
            OperationType::InitialDownload,
            SHA256Hash::compute(b"test"),
            "v1.0".to_string(),
            10,
            source_info,
        );

        assert_eq!(state.remaining_chunks(), 10);
        assert_eq!(state.completion_percentage(), 0.0);

        // Mark some chunks as completed
        for i in 0..5 {
            state.mark_chunk_completed(SHA256Hash::compute(&[i]));
        }

        assert_eq!(state.remaining_chunks(), 5);
        assert_eq!(state.completion_percentage(), 50.0);
        assert!(!state.is_complete());

        // Complete all chunks
        for i in 5..10 {
            state.mark_chunk_completed(SHA256Hash::compute(&[i]));
        }

        assert_eq!(state.remaining_chunks(), 0);
        assert_eq!(state.completion_percentage(), 100.0);
        assert!(state.is_complete());
    }

    #[test]
    fn test_state_resumability() {
        let source_info = SourceInfo {
            database: "test_db".to_string(),
            source_url: None,
            etag: Some("etag123".to_string()),
            total_size_bytes: Some(1000000),
        };

        let state = ProcessingState::new(
            OperationType::IncrementalUpdate,
            SHA256Hash::compute(b"manifest"),
            "v2.0".to_string(),
            100,
            source_info,
        );

        // Should be resumable with same manifest
        assert!(state.can_resume_with(
            &SHA256Hash::compute(b"manifest"),
            "v2.0"
        ));

        // Should not be resumable with different manifest
        assert!(!state.can_resume_with(
            &SHA256Hash::compute(b"different"),
            "v2.0"
        ));

        // Should not be resumable with different version
        assert!(!state.can_resume_with(
            &SHA256Hash::compute(b"manifest"),
            "v3.0"
        ));
    }

    #[test]
    fn test_operation_id_generation() {
        assert_eq!(
            ProcessingStateManager::generate_operation_id(
                "uniprot/swissprot",
                &OperationType::InitialDownload
            ),
            "uniprot_swissprot_initial"
        );

        assert_eq!(
            ProcessingStateManager::generate_operation_id(
                "ncbi/nr",
                &OperationType::Reduction {
                    profile: "blast-30".to_string()
                }
            ),
            "ncbi_nr_reduce_blast-30"
        );
    }
}