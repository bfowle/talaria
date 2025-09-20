/// Trait for comparing manifests to determine differences
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::casg::types::{ChunkMetadata, SHA256Hash, TemporalManifest};

/// Type of change detected
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    /// Chunk was added
    Added,
    /// Chunk was removed
    Removed,
    /// Chunk was modified (different hash)
    Modified,
    /// Chunk was moved (same hash, different location)
    Moved,
}

/// A single chunk change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChange {
    /// Type of change
    pub change_type: ChangeType,
    /// Old chunk information (if applicable)
    pub old_chunk: Option<ChunkMetadata>,
    /// New chunk information (if applicable)
    pub new_chunk: Option<ChunkMetadata>,
    /// Affected sequence IDs
    pub affected_sequences: Vec<String>,
    /// Size difference in bytes (positive = growth)
    pub size_delta: i64,
}

/// Result of a manifest diff operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResult {
    /// All detected changes
    pub changes: Vec<ChunkChange>,
    /// Summary statistics
    pub stats: DiffStats,
    /// Upgrade requirements (if any)
    pub upgrade_requirements: Vec<String>,
}

/// Statistics about the diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffStats {
    /// Number of chunks added
    pub chunks_added: usize,
    /// Number of chunks removed
    pub chunks_removed: usize,
    /// Number of chunks modified
    pub chunks_modified: usize,
    /// Number of chunks moved
    pub chunks_moved: usize,
    /// Total size change in bytes
    pub total_size_delta: i64,
    /// Number of affected sequences
    pub sequences_affected: usize,
    /// Percentage of manifest that changed
    pub change_percentage: f32,
}

/// Options for diff operation
#[derive(Debug, Clone, Default)]
pub struct DiffOptions {
    /// Include detailed sequence information
    pub include_sequences: bool,
    /// Only show changes of specific types
    pub filter_types: Option<Vec<ChangeType>>,
    /// Minimum size change to report (bytes)
    pub min_size_change: Option<usize>,
    /// Generate patch file for migration
    pub generate_patch: bool,
}

/// Trait for comparing manifests
#[async_trait]
pub trait TemporalManifestDiffer: Send + Sync {
    /// Compare two manifests and return differences
    async fn diff(
        &self,
        old: &TemporalManifest,
        new: &TemporalManifest,
        options: DiffOptions,
    ) -> Result<DiffResult>;

    /// Compare manifests from files
    async fn diff_files(
        &self,
        old_path: &PathBuf,
        new_path: &PathBuf,
        options: DiffOptions,
    ) -> Result<DiffResult>;

    /// Generate a patch that can be applied to migrate from old to new
    async fn generate_patch(&self, diff: &DiffResult) -> Result<Vec<u8>>;

    /// Apply a patch to a manifest
    async fn apply_patch(&self, manifest: &mut TemporalManifest, patch: &[u8]) -> Result<()>;

    /// Estimate the cost of applying a diff (time, bandwidth, etc.)
    async fn estimate_cost(&self, diff: &DiffResult) -> Result<MigrationCost>;

    /// Verify that a diff was applied correctly
    async fn verify_diff(
        &self,
        manifest: &TemporalManifest,
        expected_hash: &SHA256Hash,
    ) -> Result<bool>;
}

/// Cost estimation for applying a diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationCost {
    /// Estimated download size in bytes
    pub download_bytes: usize,
    /// Estimated time in seconds
    pub estimated_seconds: u64,
    /// Number of chunks to download
    pub chunks_to_download: usize,
    /// Number of chunks to remove
    pub chunks_to_remove: usize,
    /// Whether a full re-download is recommended
    pub recommend_full_download: bool,
}

/// Standard implementation of TemporalManifestDiffer
pub struct StandardTemporalManifestDiffer;

impl StandardTemporalManifestDiffer {
    pub fn new() -> Self {
        Self
    }

    fn calculate_changes(
        &self,
        old: &TemporalManifest,
        new: &TemporalManifest,
    ) -> Vec<ChunkChange> {
        let mut changes = Vec::new();

        // Build hash maps for efficient lookup
        let old_chunks: HashMap<SHA256Hash, &ChunkMetadata> = old
            .chunk_index
            .iter()
            .map(|c| (c.hash.clone(), c))
            .collect();

        let new_chunks: HashMap<SHA256Hash, &ChunkMetadata> = new
            .chunk_index
            .iter()
            .map(|c| (c.hash.clone(), c))
            .collect();

        // Find removed and modified chunks
        for (hash, old_chunk) in &old_chunks {
            if let Some(new_chunk) = new_chunks.get(hash) {
                // Check if chunk was moved or modified
                if old_chunk.size != new_chunk.size {
                    changes.push(ChunkChange {
                        change_type: ChangeType::Moved,
                        old_chunk: Some((*old_chunk).clone()),
                        new_chunk: Some((*new_chunk).clone()),
                        affected_sequences: Vec::new(),
                        size_delta: 0,
                    });
                }
            } else {
                // Chunk was removed
                changes.push(ChunkChange {
                    change_type: ChangeType::Removed,
                    old_chunk: Some((*old_chunk).clone()),
                    new_chunk: None,
                    affected_sequences: Vec::new(),
                    size_delta: -(old_chunk.size as i64),
                });
            }
        }

        // Find added chunks
        for (hash, new_chunk) in &new_chunks {
            if !old_chunks.contains_key(hash) {
                changes.push(ChunkChange {
                    change_type: ChangeType::Added,
                    old_chunk: None,
                    new_chunk: Some((*new_chunk).clone()),
                    affected_sequences: Vec::new(),
                    size_delta: new_chunk.size as i64,
                });
            }
        }

        changes
    }

    fn calculate_stats(
        &self,
        changes: &[ChunkChange],
        old: &TemporalManifest,
        new: &TemporalManifest,
    ) -> DiffStats {
        let chunks_added = changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Added)
            .count();
        let chunks_removed = changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Removed)
            .count();
        let chunks_modified = changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Modified)
            .count();
        let chunks_moved = changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Moved)
            .count();

        let total_size_delta: i64 = changes.iter().map(|c| c.size_delta).sum();

        let total_chunks = old.chunk_index.len().max(new.chunk_index.len());
        let change_percentage = if total_chunks > 0 {
            (changes.len() as f32 / total_chunks as f32) * 100.0
        } else {
            0.0
        };

        DiffStats {
            chunks_added,
            chunks_removed,
            chunks_modified,
            chunks_moved,
            total_size_delta,
            sequences_affected: 0, // Would need sequence mapping
            change_percentage,
        }
    }
}

#[async_trait]
impl TemporalManifestDiffer for StandardTemporalManifestDiffer {
    async fn diff(
        &self,
        old: &TemporalManifest,
        new: &TemporalManifest,
        options: DiffOptions,
    ) -> Result<DiffResult> {
        let mut changes = self.calculate_changes(old, new);

        // Apply filters if specified
        if let Some(ref filter_types) = options.filter_types {
            changes.retain(|c| filter_types.contains(&c.change_type));
        }

        if let Some(min_size) = options.min_size_change {
            changes.retain(|c| c.size_delta.abs() >= min_size as i64);
        }

        let stats = self.calculate_stats(&changes, old, new);

        // Check for upgrade requirements
        let mut upgrade_requirements = Vec::new();
        if old.version != new.version {
            upgrade_requirements.push(format!(
                "TemporalManifest version upgrade from {} to {}",
                old.version, new.version
            ));
        }

        Ok(DiffResult {
            changes,
            stats,
            upgrade_requirements,
        })
    }

    async fn diff_files(
        &self,
        old_path: &PathBuf,
        new_path: &PathBuf,
        options: DiffOptions,
    ) -> Result<DiffResult> {
        // Load manifests from files
        let old_data = std::fs::read(old_path)?;
        let new_data = std::fs::read(new_path)?;

        // Auto-detect format and deserialize
        use crate::casg::format::FormatDetector;
        let _old_format = FormatDetector::detect(old_path);
        let _new_format = FormatDetector::detect(new_path);

        // For now, use JSON deserialization as a fallback
        let old: TemporalManifest = serde_json::from_slice(&old_data)?;
        let new: TemporalManifest = serde_json::from_slice(&new_data)?;

        self.diff(&old, &new, options).await
    }

    async fn generate_patch(&self, diff: &DiffResult) -> Result<Vec<u8>> {
        // Generate a patch file that describes the changes
        let patch = serde_json::to_vec_pretty(diff)?;
        Ok(patch)
    }

    async fn apply_patch(&self, manifest: &mut TemporalManifest, patch: &[u8]) -> Result<()> {
        let diff: DiffResult = serde_json::from_slice(patch)?;

        // Apply changes to manifest
        for change in &diff.changes {
            match change.change_type {
                ChangeType::Added => {
                    if let Some(ref new_chunk) = change.new_chunk {
                        manifest.chunk_index.push(new_chunk.clone());
                    }
                }
                ChangeType::Removed => {
                    if let Some(ref old_chunk) = change.old_chunk {
                        manifest.chunk_index.retain(|c| c.hash != old_chunk.hash);
                    }
                }
                ChangeType::Modified | ChangeType::Moved => {
                    if let (Some(ref old_chunk), Some(ref new_chunk)) =
                        (&change.old_chunk, &change.new_chunk)
                    {
                        if let Some(chunk) = manifest
                            .chunk_index
                            .iter_mut()
                            .find(|c| c.hash == old_chunk.hash)
                        {
                            *chunk = new_chunk.clone();
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn estimate_cost(&self, diff: &DiffResult) -> Result<MigrationCost> {
        let chunks_to_download = diff.stats.chunks_added + diff.stats.chunks_modified;
        let chunks_to_remove = diff.stats.chunks_removed;

        // Estimate download size (assume average chunk size)
        let avg_chunk_size = 100_000; // 100KB average
        let download_bytes = chunks_to_download * avg_chunk_size;

        // Estimate time (assume 10MB/s download speed)
        let download_speed = 10_000_000; // 10MB/s
        let estimated_seconds = (download_bytes / download_speed).max(1) as u64;

        // Recommend full download if more than 50% changed
        let recommend_full_download = diff.stats.change_percentage > 50.0;

        Ok(MigrationCost {
            download_bytes,
            estimated_seconds,
            chunks_to_download,
            chunks_to_remove,
            recommend_full_download,
        })
    }

    async fn verify_diff(
        &self,
        manifest: &TemporalManifest,
        expected_hash: &SHA256Hash,
    ) -> Result<bool> {
        // Calculate hash of manifest and compare
        let manifest_bytes = serde_json::to_vec(manifest)?;
        let actual_hash = SHA256Hash::compute(&manifest_bytes);

        Ok(actual_hash == *expected_hash)
    }
}
