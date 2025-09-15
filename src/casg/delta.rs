/// Delta chunk implementation for CASG
///
/// Stores delta-encoded sequences as content-addressed chunks,
/// enabling efficient storage and deduplication of similar sequences.

use crate::casg::types::*;
use crate::bio::sequence::Sequence;
use crate::core::delta_encoder::{DeltaRecord, DeltaRange};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A chunk containing delta-encoded sequences
///
/// Delta chunks store the differences between child sequences and their
/// reference sequences, enabling efficient compression and deduplication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaChunk {
    /// Content hash of this chunk
    pub content_hash: SHA256Hash,

    /// Hash of the reference chunk these deltas are based on
    pub reference_chunk_hash: SHA256Hash,

    /// Delta records for child sequences
    pub delta_records: Vec<DeltaRecord>,

    /// Chunk metadata
    pub metadata: DeltaChunkMetadata,

    /// Compressed representation if applicable
    pub compressed_data: Option<Vec<u8>>,
}

/// Metadata for a delta chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaChunkMetadata {
    /// Number of child sequences in this chunk
    pub child_count: usize,

    /// Total number of delta operations
    pub total_delta_ops: usize,

    /// Average delta operations per child
    pub avg_delta_ops: f32,

    /// Maximum delta operations for any child
    pub max_delta_ops: usize,

    /// Uncompressed size in bytes
    pub uncompressed_size: usize,

    /// Compressed size if compression was used
    pub compressed_size: Option<usize>,

    /// Compression algorithm used
    pub compression: Option<CompressionType>,

    /// Taxonomic IDs of children if available
    pub child_taxon_ids: Vec<TaxonId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionType {
    Zstd,
    Gzip,
    Lz4,
    None,
}

impl DeltaChunk {
    /// Create a new delta chunk from delta records
    pub fn new(
        reference_chunk_hash: SHA256Hash,
        delta_records: Vec<DeltaRecord>,
    ) -> Result<Self> {
        // Calculate metadata
        let child_count = delta_records.len();
        let total_delta_ops: usize = delta_records
            .iter()
            .map(|d| d.deltas.len())
            .sum();

        let max_delta_ops = delta_records
            .iter()
            .map(|d| d.deltas.len())
            .max()
            .unwrap_or(0);

        let avg_delta_ops = if child_count > 0 {
            total_delta_ops as f32 / child_count as f32
        } else {
            0.0
        };

        let child_taxon_ids: Vec<TaxonId> = delta_records
            .iter()
            .filter_map(|d| d.taxon_id.map(TaxonId))
            .collect();

        // Serialize for size calculation and hashing
        let serialized = serde_json::to_vec(&delta_records)?;
        let uncompressed_size = serialized.len();

        // Compute content hash
        let content_hash = SHA256Hash::compute(&serialized);

        // Try compression
        let (compressed_data, compressed_size, compression) =
            Self::try_compress(&serialized)?;

        let metadata = DeltaChunkMetadata {
            child_count,
            total_delta_ops,
            avg_delta_ops,
            max_delta_ops,
            uncompressed_size,
            compressed_size,
            compression,
            child_taxon_ids,
        };

        Ok(Self {
            content_hash,
            reference_chunk_hash,
            delta_records,
            metadata,
            compressed_data,
        })
    }

    /// Try to compress the data and return the best result
    fn try_compress(data: &[u8]) -> Result<(Option<Vec<u8>>, Option<usize>, Option<CompressionType>)> {
        // Use gzip compression which is already available
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)?;
        let compressed = encoder.finish()?;

        // Only use compression if it saves >10% space
        if compressed.len() < (data.len() * 9 / 10) {
            Ok((
                Some(compressed.clone()),
                Some(compressed.len()),
                Some(CompressionType::Gzip),
            ))
        } else {
            Ok((None, None, None))
        }
    }

    /// Decompress the chunk data if compressed
    pub fn decompress(&self) -> Result<Vec<u8>> {
        match (&self.compressed_data, &self.metadata.compression) {
            (Some(compressed), Some(CompressionType::Gzip)) => {
                use flate2::read::GzDecoder;
                use std::io::Read;

                let mut decoder = GzDecoder::new(compressed.as_slice());
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)?;
                Ok(decompressed)
            }
            (Some(_compressed), Some(CompressionType::Zstd)) => {
                // Zstd not currently supported, return error
                Err(anyhow::anyhow!("Zstd compression not currently supported"))
            }
            _ => {
                // Return serialized delta records if not compressed
                Ok(serde_json::to_vec(&self.delta_records)?)
            }
        }
    }

    /// Get the effective size (compressed if available, otherwise uncompressed)
    pub fn effective_size(&self) -> usize {
        self.metadata.compressed_size
            .unwrap_or(self.metadata.uncompressed_size)
    }

    /// Check if a specific child ID is in this chunk
    pub fn contains_child(&self, child_id: &str) -> bool {
        self.delta_records
            .iter()
            .any(|d| d.child_id == child_id)
    }

    /// Get the delta record for a specific child
    pub fn get_child_delta(&self, child_id: &str) -> Option<&DeltaRecord> {
        self.delta_records
            .iter()
            .find(|d| d.child_id == child_id)
    }

    /// Split a large delta chunk into smaller chunks
    pub fn split(&self, max_chunk_size: usize) -> Result<Vec<DeltaChunk>> {
        if self.effective_size() <= max_chunk_size {
            return Ok(vec![self.clone()]);
        }

        // Split delta records into groups
        let mut chunks = Vec::new();
        let mut current_batch = Vec::new();
        let mut current_size = 0;

        for record in &self.delta_records {
            let record_size = serde_json::to_vec(record)?.len();

            if current_size + record_size > max_chunk_size && !current_batch.is_empty() {
                // Create a chunk from current batch
                chunks.push(DeltaChunk::new(
                    self.reference_chunk_hash.clone(),
                    current_batch.clone(),
                )?);
                current_batch.clear();
                current_size = 0;
            }

            current_batch.push(record.clone());
            current_size += record_size;
        }

        // Don't forget the last batch
        if !current_batch.is_empty() {
            chunks.push(DeltaChunk::new(
                self.reference_chunk_hash.clone(),
                current_batch,
            )?);
        }

        Ok(chunks)
    }

    /// Merge multiple delta chunks that share the same reference
    pub fn merge(chunks: Vec<DeltaChunk>) -> Result<Self> {
        if chunks.is_empty() {
            return Err(anyhow::anyhow!("Cannot merge empty chunk list"));
        }

        // Verify all chunks share the same reference
        let reference_hash = chunks[0].reference_chunk_hash.clone();
        if !chunks.iter().all(|c| c.reference_chunk_hash == reference_hash) {
            return Err(anyhow::anyhow!("Cannot merge chunks with different references"));
        }

        // Combine all delta records
        let mut all_records = Vec::new();
        for chunk in chunks {
            all_records.extend(chunk.delta_records);
        }

        DeltaChunk::new(reference_hash, all_records)
    }
}

/// Reconstructor for delta-encoded sequences
pub struct DeltaReconstructor {
    reference_cache: HashMap<SHA256Hash, Vec<Sequence>>,
}

impl DeltaReconstructor {
    pub fn new() -> Self {
        Self {
            reference_cache: HashMap::new(),
        }
    }

    /// Reconstruct a child sequence from a delta chunk and reference sequence
    pub fn reconstruct_sequence(
        &self,
        delta_record: &DeltaRecord,
        reference: &Sequence,
    ) -> Result<Sequence> {
        // Start with the reference sequence
        let mut reconstructed = reference.sequence.clone();

        // Apply delta operations
        for delta in &delta_record.deltas {
            Self::apply_delta(&mut reconstructed, delta)?;
        }

        // Apply header changes if present
        let description = if let Some(header_change) = &delta_record.header_change {
            header_change.new_description.clone()
        } else {
            // Use reference description if no change tracked
            reference.description.clone()
        };

        Ok(Sequence {
            id: delta_record.child_id.clone(),
            description,
            sequence: reconstructed,
            taxon_id: delta_record.taxon_id,
        })
    }

    /// Apply a single delta operation to a sequence
    fn apply_delta(sequence: &mut Vec<u8>, delta: &DeltaRange) -> Result<()> {
        // Validate range
        if delta.end >= sequence.len() {
            return Err(anyhow::anyhow!(
                "Delta range ({}-{}) exceeds sequence length {}",
                delta.start, delta.end, sequence.len()
            ));
        }

        // Apply substitution
        let range_len = delta.end - delta.start + 1;
        if delta.substitution.len() != range_len {
            return Err(anyhow::anyhow!(
                "Delta substitution length {} doesn't match range length {}",
                delta.substitution.len(), range_len
            ));
        }

        for (i, &byte) in delta.substitution.iter().enumerate() {
            sequence[delta.start + i] = byte;
        }

        Ok(())
    }

    /// Reconstruct all sequences from a delta chunk
    pub fn reconstruct_chunk(
        &mut self,
        delta_chunk: &DeltaChunk,
        reference_sequences: Vec<Sequence>,
    ) -> Result<Vec<Sequence>> {
        // Cache reference sequences
        self.reference_cache.insert(
            delta_chunk.reference_chunk_hash.clone(),
            reference_sequences.clone(),
        );

        // Build reference map by ID
        let ref_map: HashMap<String, &Sequence> = reference_sequences
            .iter()
            .map(|s| (s.id.clone(), s))
            .collect();

        // Reconstruct each child
        let mut reconstructed = Vec::new();
        for delta_record in &delta_chunk.delta_records {
            let reference = ref_map.get(&delta_record.reference_id)
                .ok_or_else(|| anyhow::anyhow!(
                    "Reference sequence {} not found",
                    delta_record.reference_id
                ))?;

            reconstructed.push(self.reconstruct_sequence(delta_record, reference)?);
        }

        Ok(reconstructed)
    }

    /// Clear the reference cache to free memory
    pub fn clear_cache(&mut self) {
        self.reference_cache.clear();
    }
}

/// Index for efficient delta chunk queries
pub struct DeltaIndex {
    /// Map from child ID to chunk hash
    child_to_chunk: HashMap<String, SHA256Hash>,

    /// Map from reference chunk to delta chunks
    reference_to_deltas: HashMap<SHA256Hash, Vec<SHA256Hash>>,

    /// Map from taxon ID to delta chunks
    taxon_to_chunks: HashMap<TaxonId, Vec<SHA256Hash>>,
}

impl DeltaIndex {
    pub fn new() -> Self {
        Self {
            child_to_chunk: HashMap::new(),
            reference_to_deltas: HashMap::new(),
            taxon_to_chunks: HashMap::new(),
        }
    }

    /// Add a delta chunk to the index
    pub fn add_chunk(&mut self, chunk: &DeltaChunk) {
        // Index by child IDs
        for record in &chunk.delta_records {
            self.child_to_chunk.insert(
                record.child_id.clone(),
                chunk.content_hash.clone(),
            );
        }

        // Index by reference
        self.reference_to_deltas
            .entry(chunk.reference_chunk_hash.clone())
            .or_insert_with(Vec::new)
            .push(chunk.content_hash.clone());

        // Index by taxon
        for taxon_id in &chunk.metadata.child_taxon_ids {
            self.taxon_to_chunks
                .entry(*taxon_id)
                .or_insert_with(Vec::new)
                .push(chunk.content_hash.clone());
        }
    }

    /// Find the chunk containing a specific child sequence
    pub fn find_chunk_for_child(&self, child_id: &str) -> Option<&SHA256Hash> {
        self.child_to_chunk.get(child_id)
    }

    /// Find all delta chunks for a reference chunk
    pub fn find_deltas_for_reference(&self, reference_hash: &SHA256Hash) -> Vec<SHA256Hash> {
        self.reference_to_deltas
            .get(reference_hash)
            .cloned()
            .unwrap_or_default()
    }

    /// Find all delta chunks for a taxon
    pub fn find_chunks_for_taxon(&self, taxon_id: TaxonId) -> Vec<SHA256Hash> {
        self.taxon_to_chunks
            .get(&taxon_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Get statistics about the index
    pub fn stats(&self) -> DeltaIndexStats {
        DeltaIndexStats {
            total_children: self.child_to_chunk.len(),
            total_references: self.reference_to_deltas.len(),
            total_taxa: self.taxon_to_chunks.len(),
            avg_deltas_per_reference: if self.reference_to_deltas.is_empty() {
                0.0
            } else {
                let total: usize = self.reference_to_deltas.values().map(|v| v.len()).sum();
                total as f32 / self.reference_to_deltas.len() as f32
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeltaIndexStats {
    pub total_children: usize,
    pub total_references: usize,
    pub total_taxa: usize,
    pub avg_deltas_per_reference: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_chunk_creation() {
        let ref_hash = SHA256Hash::compute(b"reference");

        let delta_records = vec![
            DeltaRecord {
                child_id: "child1".to_string(),
                reference_id: "ref1".to_string(),
                taxon_id: Some(9606),
                deltas: vec![
                    DeltaRange {
                        start: 10,
                        end: 15,
                        substitution: vec![b'A'; 6],
                    },
                ],
                header_change: None,
            },
        ];

        let chunk = DeltaChunk::new(ref_hash, delta_records).unwrap();

        assert_eq!(chunk.metadata.child_count, 1);
        assert_eq!(chunk.metadata.total_delta_ops, 1);
        assert!(chunk.contains_child("child1"));
    }

    #[test]
    fn test_delta_reconstruction() {
        let reference = Sequence {
            id: "ref1".to_string(),
            description: Some("Reference".to_string()),
            sequence: b"ACGTACGTACGT".to_vec(),
            taxon_id: None,
        };

        let delta_record = DeltaRecord {
            child_id: "child1".to_string(),
            reference_id: "ref1".to_string(),
            taxon_id: Some(9606),
            deltas: vec![
                DeltaRange {
                    start: 0,
                    end: 3,
                    substitution: b"TTTT".to_vec(),
                },
            ],
            header_change: None,
        };

        let reconstructor = DeltaReconstructor::new();
        let reconstructed = reconstructor.reconstruct_sequence(&delta_record, &reference).unwrap();

        assert_eq!(reconstructed.id, "child1");
        assert_eq!(&reconstructed.sequence[0..4], b"TTTT");
        assert_eq!(&reconstructed.sequence[4..], b"ACGTACGT");
    }

    #[test]
    fn test_delta_index() {
        let mut index = DeltaIndex::new();

        let chunk = DeltaChunk::new(
            SHA256Hash::compute(b"ref"),
            vec![
                DeltaRecord {
                    child_id: "child1".to_string(),
                    reference_id: "ref1".to_string(),
                    taxon_id: Some(9606),
                    deltas: vec![],
                    header_change: None,
                },
            ],
        ).unwrap();

        index.add_chunk(&chunk);

        assert_eq!(
            index.find_chunk_for_child("child1"),
            Some(&chunk.content_hash)
        );
    }
}