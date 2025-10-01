//! Chunk type definitions

use serde::{Deserialize, Serialize};

use super::hash::SHA256Hash;
use super::taxonomy::TaxonId;

/// Basic chunk information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkInfo {
    pub hash: SHA256Hash,
    pub size: usize,
}

impl ChunkInfo {
    pub fn new(hash: SHA256Hash, size: usize) -> Self {
        Self { hash, size }
    }
}

/// Extended chunk metadata with taxonomy information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkMetadata {
    pub hash: SHA256Hash,
    pub size: usize,
    pub taxon_ids: Vec<TaxonId>,
    pub sequence_count: usize,
    pub compressed_size: Option<usize>,
    pub compression_ratio: Option<f32>,
}

impl ChunkMetadata {
    pub fn new(hash: SHA256Hash, size: usize) -> Self {
        Self {
            hash,
            size,
            taxon_ids: Vec::new(),
            sequence_count: 0,
            compressed_size: None,
            compression_ratio: None,
        }
    }

    pub fn with_taxonomy(hash: SHA256Hash, size: usize, taxon_ids: Vec<TaxonId>) -> Self {
        Self {
            hash,
            size,
            taxon_ids,
            sequence_count: 0,
            compressed_size: None,
            compression_ratio: None,
        }
    }

    /// Check if this chunk contains sequences from a specific taxon
    pub fn contains_taxon(&self, taxon_id: TaxonId) -> bool {
        self.taxon_ids.contains(&taxon_id)
    }

    /// Get the number of unique taxa in this chunk
    pub fn taxon_count(&self) -> usize {
        self.taxon_ids.len()
    }
}

/// Delta-encoded chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaChunk {
    pub reference_hash: SHA256Hash,
    pub deltas: Vec<u8>,
}

impl DeltaChunk {
    pub fn new(reference_hash: SHA256Hash, deltas: Vec<u8>) -> Self {
        Self {
            reference_hash,
            deltas,
        }
    }

    /// Get the size of the delta data
    pub fn delta_size(&self) -> usize {
        self.deltas.len()
    }
}

/// Chunk type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkType {
    /// Standard data chunk
    Data,
    /// Reference chunk for delta encoding
    Reference,
    /// Delta-encoded chunk
    Delta,
    /// Metadata chunk
    Metadata,
    /// Index chunk
    Index,
}

impl ChunkType {
    pub fn is_reference(&self) -> bool {
        matches!(self, ChunkType::Reference)
    }

    pub fn is_delta(&self) -> bool {
        matches!(self, ChunkType::Delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_metadata() {
        let hash = SHA256Hash::default();
        let metadata =
            ChunkMetadata::with_taxonomy(hash, 1024, vec![TaxonId::HUMAN, TaxonId::MOUSE]);

        assert!(metadata.contains_taxon(TaxonId::HUMAN));
        assert!(!metadata.contains_taxon(TaxonId::ECOLI));
        assert_eq!(metadata.taxon_count(), 2);
    }

    #[test]
    fn test_delta_chunk() {
        let ref_hash = SHA256Hash::compute(b"reference");
        let deltas = vec![1, 2, 3, 4, 5];
        let chunk = DeltaChunk::new(ref_hash, deltas);

        assert_eq!(chunk.delta_size(), 5);
        assert_eq!(chunk.reference_hash, ref_hash);
    }
}
