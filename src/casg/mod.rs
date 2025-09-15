/// Content-Addressed Sequence Graph (CASG) System
///
/// A modern approach to sequence database management using content-addressing,
/// Merkle DAGs, and taxonomy-aware chunking for efficient storage and verification.

pub mod manifest;
pub mod storage;
pub mod merkle;
pub mod chunker;
pub mod assembler;
pub mod verifier;
pub mod taxonomy;
pub mod taxonomy_manifest;
pub mod temporal;
pub mod types;
pub mod reduction;
pub mod delta;
pub mod processing_state;
pub mod cloud;

pub use types::*;
pub use manifest::Manifest;
pub use storage::CASGStorage;
pub use merkle::MerkleDAG;
pub use chunker::TaxonomicChunker;
pub use assembler::FastaAssembler;
pub use verifier::{CASGVerifier, VerificationResult};
pub use taxonomy::TaxonomyManager;
pub use taxonomy_manifest::{TaxonomyManifest, TaxonomySource};
pub use temporal::TemporalIndex;
pub use processing_state::{ProcessingState, ProcessingStateManager, OperationType, SourceInfo};

#[cfg(test)]
mod tests;

#[cfg(test)]
pub use tests::{VersionIdentifier, VersionInfo};

use anyhow::Result;
use std::path::Path;

/// Main CASG repository manager
pub struct CASGRepository {
    pub storage: CASGStorage,
    pub manifest: Manifest,
    pub taxonomy: TaxonomyManager,
    pub temporal: TemporalIndex,
}

impl CASGRepository {
    /// Initialize a new CASG repository
    pub fn init(base_path: &Path) -> Result<Self> {
        let storage = CASGStorage::new(base_path)?;
        let manifest = Manifest::new_with_path(base_path);
        let taxonomy = TaxonomyManager::new(base_path)?;
        let temporal = TemporalIndex::new(base_path)?;

        Ok(Self {
            storage,
            manifest,
            taxonomy,
            temporal,
        })
    }

    /// Open an existing CASG repository
    pub fn open(base_path: &Path) -> Result<Self> {
        let storage = CASGStorage::open(base_path)?;

        // Try to load manifest, but create new if it doesn't exist
        let manifest = Manifest::load(base_path).unwrap_or_else(|_| Manifest::new_with_path(base_path));

        // Try to load taxonomy, but create new if it doesn't exist
        let taxonomy = TaxonomyManager::load(base_path).unwrap_or_else(|_| {
            TaxonomyManager::new(base_path).expect("Failed to create taxonomy manager")
        });

        // Try to load temporal index, but create new if it doesn't exist
        let temporal = TemporalIndex::load(base_path).unwrap_or_else(|_| {
            TemporalIndex::new(base_path).expect("Failed to create temporal index")
        });

        Ok(Self {
            storage,
            manifest,
            taxonomy,
            temporal,
        })
    }

    /// Check for updates using manifest ETags
    pub async fn check_updates(&self) -> Result<bool> {
        self.manifest.check_remote_updates().await
    }

    /// Sync with remote repository
    pub async fn sync(&mut self) -> Result<SyncResult> {
        // Check for manifest updates
        if !self.check_updates().await? {
            return Ok(SyncResult::NoUpdates);
        }

        // Download new manifest
        let new_manifest = self.manifest.fetch_remote().await?;

        // Compute diff
        let diff = self.manifest.diff(&new_manifest)?;

        // Download only changed chunks
        let downloaded = self.storage.fetch_chunks(&diff.new_chunks).await?;

        // Update manifest
        self.manifest = new_manifest;
        self.manifest.save()?;

        Ok(SyncResult::Updated {
            chunks_downloaded: downloaded.len(),
            bytes_transferred: downloaded.iter().map(|c| c.size).sum(),
        })
    }

    /// Extract sequences for a taxonomic group
    pub fn extract_taxon(&self, taxon: &str) -> Result<Vec<crate::bio::sequence::Sequence>> {
        let chunks = self.taxonomy.get_chunks_for_taxon(taxon)?;
        let assembler = FastaAssembler::new(&self.storage);
        assembler.assemble_from_chunks(&chunks)
    }

    /// Verify integrity of the repository
    pub fn verify(&self) -> Result<VerificationResult> {
        let manifest_data = self.manifest.get_data()
            .ok_or_else(|| anyhow::anyhow!("No manifest loaded"))?;
        let verifier = CASGVerifier::new(&self.storage, manifest_data);
        verifier.verify_all()
    }

    /// Get discrepancies between taxonomy and sequences
    pub fn get_discrepancies(&self) -> Result<Vec<TaxonomicDiscrepancy>> {
        self.taxonomy.detect_discrepancies(&self.storage)
    }

    /// Get taxonomy root for manifest
    pub fn get_taxonomy_root(&self) -> Result<MerkleHash> {
        let manifest_data = self.manifest.get_data()
            .ok_or_else(|| anyhow::anyhow!("No manifest loaded"))?;

        // Calculate Merkle root from taxonomy version hashes in chunks
        if manifest_data.chunk_index.is_empty() {
            // Empty tree has a special root
            return Ok(SHA256Hash::compute(b"EMPTY_TAXONOMY_ROOT"));
        }

        // Use a placeholder for taxonomy versions since ChunkMetadata doesn't have this field
        // In a real implementation, this would come from loading the actual chunks
        let mut taxonomy_hashes: Vec<SHA256Hash> = vec![manifest_data.taxonomy_root.clone()];

        // Sort for deterministic ordering
        taxonomy_hashes.sort();
        taxonomy_hashes.dedup();

        // Build Merkle tree
        self.compute_merkle_root(taxonomy_hashes)
    }

    /// Get sequence root for manifest
    pub fn get_sequence_root(&self) -> Result<MerkleHash> {
        let manifest_data = self.manifest.get_data()
            .ok_or_else(|| anyhow::anyhow!("No manifest loaded"))?;

        // Calculate Merkle root from content hashes of all chunks
        if manifest_data.chunk_index.is_empty() {
            // Empty tree has a special root
            return Ok(SHA256Hash::compute(b"EMPTY_SEQUENCE_ROOT"));
        }

        // Collect content hashes from all chunks (already sorted in manifest)
        let sequence_hashes: Vec<SHA256Hash> = manifest_data.chunk_index
            .iter()
            .map(|chunk| chunk.hash.clone())
            .collect();

        // Build Merkle tree
        self.compute_merkle_root(sequence_hashes)
    }

    /// Compute Merkle root from a list of hashes
    fn compute_merkle_root(&self, mut hashes: Vec<SHA256Hash>) -> Result<MerkleHash> {
        if hashes.is_empty() {
            return Ok(SHA256Hash::compute(b"EMPTY"));
        }

        if hashes.len() == 1 {
            return Ok(hashes[0].clone());
        }

        // Build tree level by level
        while hashes.len() > 1 {
            let mut next_level = Vec::new();

            // Process pairs of hashes
            for chunk in hashes.chunks(2) {
                let combined = if chunk.len() == 2 {
                    // Combine two hashes
                    let mut data = Vec::new();
                    data.extend_from_slice(&chunk[0].0);
                    data.extend_from_slice(&chunk[1].0);
                    SHA256Hash::compute(&data)
                } else {
                    // Odd number - duplicate the last hash
                    let mut data = Vec::new();
                    data.extend_from_slice(&chunk[0].0);
                    data.extend_from_slice(&chunk[0].0);
                    SHA256Hash::compute(&data)
                };
                next_level.push(combined);
            }

            hashes = next_level;
        }

        Ok(hashes[0].clone())
    }
}

#[derive(Debug)]
pub enum SyncResult {
    NoUpdates,
    Updated {
        chunks_downloaded: usize,
        bytes_transferred: usize,
    },
}

