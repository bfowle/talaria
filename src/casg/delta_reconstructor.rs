/// Delta reconstruction for CASG delta chunks
///
/// This module provides efficient reconstruction of sequences from delta chunks,
/// including reference caching, parallel reconstruction, and chain management.

use crate::bio::sequence::Sequence;
use crate::casg::types::*;
use crate::casg::delta::traits::DeltaReconstructor as DeltaReconstructorTrait;
use anyhow::Result;
use dashmap::DashMap;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Configuration for delta reconstruction
#[derive(Debug, Clone)]
pub struct ReconstructorConfig {
    /// Maximum number of cached references
    pub max_cache_size: usize,
    /// Enable parallel reconstruction
    pub parallel: bool,
    /// Maximum delta chain depth before warning
    pub max_chain_depth: usize,
}

impl Default for ReconstructorConfig {
    fn default() -> Self {
        Self {
            max_cache_size: 100,
            parallel: true,
            max_chain_depth: 3,
        }
    }
}

/// Delta reconstructor for efficient sequence reconstruction
pub struct DeltaReconstructor {
    config: ReconstructorConfig,
    reference_cache: Arc<DashMap<SHA256Hash, Vec<Sequence>>>,
    chain_depth_cache: Arc<DashMap<SHA256Hash, usize>>,
}

impl DeltaReconstructor {
    /// Create a new delta reconstructor
    pub fn new(config: ReconstructorConfig) -> Self {
        Self {
            config,
            reference_cache: Arc::new(DashMap::new()),
            chain_depth_cache: Arc::new(DashMap::new()),
        }
    }

    /// Create with default configuration
    pub fn default() -> Self {
        Self::new(ReconstructorConfig::default())
    }

    /// Reconstruct sequences from a delta chunk
    pub fn reconstruct_chunk(
        &self,
        delta_chunk: &DeltaChunk,
        reference_sequences: Vec<Sequence>,
    ) -> Result<Vec<Sequence>> {
        // Cache the reference sequences
        self.cache_references(&delta_chunk.reference_hash, reference_sequences.clone())?;

        // Build reference map by ID
        let ref_map: HashMap<String, &Sequence> = reference_sequences
            .iter()
            .map(|s| (s.id.clone(), s))
            .collect();

        // Reconstruct sequences based on delta operations
        let sequences = if self.config.parallel {
            self.reconstruct_parallel(delta_chunk, &ref_map)?
        } else {
            self.reconstruct_sequential(delta_chunk, &ref_map)?
        };

        Ok(sequences)
    }

    /// Sequential reconstruction
    fn reconstruct_sequential(
        &self,
        delta_chunk: &DeltaChunk,
        ref_map: &HashMap<String, &Sequence>,
    ) -> Result<Vec<Sequence>> {
        let mut sequences = Vec::new();

        for delta_op in &delta_chunk.deltas {
            match self.apply_delta_operation(delta_op, ref_map)? {
                Some(seq) => sequences.push(seq),
                None => {} // Deleted sequence
            }
        }

        Ok(sequences)
    }

    /// Parallel reconstruction using rayon
    fn reconstruct_parallel(
        &self,
        delta_chunk: &DeltaChunk,
        ref_map: &HashMap<String, &Sequence>,
    ) -> Result<Vec<Sequence>> {
        let sequences: Result<Vec<Option<Sequence>>> = delta_chunk.deltas
            .par_iter()
            .map(|delta_op| self.apply_delta_operation(delta_op, ref_map))
            .collect();

        Ok(sequences?
            .into_iter()
            .filter_map(|s| s)
            .collect())
    }

    /// Apply a single delta operation
    fn apply_delta_operation(
        &self,
        operation: &DeltaOperation,
        ref_map: &HashMap<String, &Sequence>,
    ) -> Result<Option<Sequence>> {
        match operation {
            DeltaOperation::UseReference { sequence_id, reference_offset, length } => {
                // Find the reference sequence
                let ref_seq = ref_map.values()
                    .find(|s| s.sequence.len() >= reference_offset + length)
                    .ok_or_else(|| anyhow::anyhow!("No suitable reference found for {}", sequence_id))?;

                // Extract the subsequence if needed
                let sequence = if *reference_offset == 0 && *length == ref_seq.sequence.len() {
                    ref_seq.sequence.clone()
                } else {
                    ref_seq.sequence[*reference_offset..*reference_offset + *length].to_vec()
                };

                Ok(Some(Sequence {
                    id: sequence_id.clone(),
                    description: ref_seq.description.clone(),
                    sequence,
                    taxon_id: ref_seq.taxon_id,
                }))
            }

            DeltaOperation::Insert { sequence_id, data } => {
                // New sequence not based on any reference
                Ok(Some(Sequence {
                    id: sequence_id.clone(),
                    description: None,
                    sequence: data.clone(),
                    taxon_id: None,
                }))
            }

            DeltaOperation::Modify { sequence_id, reference_offset: _, operations } => {
                // Find reference sequence to modify
                let ref_seq = ref_map.get(sequence_id)
                    .or_else(|| ref_map.values().next())
                    .ok_or_else(|| anyhow::anyhow!("No reference found for modification of {}", sequence_id))?;

                // Start with reference sequence
                let mut sequence = ref_seq.sequence.clone();

                // Apply edit operations
                for edit in operations {
                    self.apply_sequence_edit(&mut sequence, edit)?;
                }

                Ok(Some(Sequence {
                    id: sequence_id.clone(),
                    description: ref_seq.description.clone(),
                    sequence,
                    taxon_id: ref_seq.taxon_id,
                }))
            }

            DeltaOperation::Delete { sequence_id: _ } => {
                // Tombstone - sequence was deleted
                Ok(None)
            }
        }
    }

    /// Apply a sequence edit operation
    fn apply_sequence_edit(&self, sequence: &mut Vec<u8>, edit: &SeqEdit) -> Result<()> {
        match edit {
            SeqEdit::Substitute { pos, new_base } => {
                if *pos >= sequence.len() {
                    return Err(anyhow::anyhow!("Substitute position {} out of range", pos));
                }
                sequence[*pos] = *new_base;
            }

            SeqEdit::Insert { pos, bases } => {
                if *pos > sequence.len() {
                    return Err(anyhow::anyhow!("Insert position {} out of range", pos));
                }
                // Insert at position
                for (i, &base) in bases.iter().enumerate() {
                    sequence.insert(*pos + i, base);
                }
            }

            SeqEdit::Delete { pos, count } => {
                if *pos + *count > sequence.len() {
                    return Err(anyhow::anyhow!("Delete range {}..{} out of range", pos, pos + count));
                }
                // Remove bases
                for _ in 0..*count {
                    sequence.remove(*pos);
                }
            }
        }
        Ok(())
    }

    /// Cache reference sequences
    fn cache_references(&self, hash: &SHA256Hash, sequences: Vec<Sequence>) -> Result<()> {
        // Check cache size and evict if needed
        if self.reference_cache.len() >= self.config.max_cache_size {
            // Simple LRU-like eviction: remove first entry
            if let Some(entry) = self.reference_cache.iter().next() {
                let key = entry.key().clone();
                drop(entry);
                self.reference_cache.remove(&key);
            }
        }

        self.reference_cache.insert(hash.clone(), sequences);
        Ok(())
    }

    /// Get cached references
    pub fn get_cached_references(&self, hash: &SHA256Hash) -> Option<Vec<Sequence>> {
        self.reference_cache.get(hash).map(|r| r.clone())
    }

    /// Clear all caches
    pub fn clear_cache(&self) {
        self.reference_cache.clear();
        self.chain_depth_cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        CacheStats {
            reference_cache_size: self.reference_cache.len(),
            chain_depth_cache_size: self.chain_depth_cache.len(),
            max_cache_size: self.config.max_cache_size,
        }
    }

    /// Check delta chain depth
    pub fn check_chain_depth(&self, delta_chunk: &DeltaChunk) -> usize {
        // Check if we have a cached depth
        if let Some(depth) = self.chain_depth_cache.get(&delta_chunk.content_hash) {
            return *depth;
        }

        // Calculate depth based on chunk type
        let depth = match &delta_chunk.chunk_type {
            ChunkType::Delta { .. } => {
                // This is a delta, so depth is at least 1
                // In a real implementation, we'd check the reference's depth
                1
            }
            _ => 0,
        };

        self.chain_depth_cache.insert(delta_chunk.content_hash.clone(), depth);
        depth
    }

    /// Reconstruct sequences from multiple delta chunks
    pub fn reconstruct_multiple(
        &self,
        delta_chunks: Vec<&DeltaChunk>,
        reference_provider: impl Fn(&SHA256Hash) -> Result<Vec<Sequence>>,
    ) -> Result<Vec<Sequence>> {
        let mut all_sequences = Vec::new();

        for chunk in delta_chunks {
            // Get references for this chunk
            let references = if let Some(cached) = self.get_cached_references(&chunk.reference_hash) {
                cached
            } else {
                reference_provider(&chunk.reference_hash)?
            };

            // Reconstruct this chunk
            let sequences = self.reconstruct_chunk(chunk, references)?;
            all_sequences.extend(sequences);
        }

        Ok(all_sequences)
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub reference_cache_size: usize,
    pub chain_depth_cache_size: usize,
    pub max_cache_size: usize,
}

/// Delta chain manager for preventing long chains
pub struct DeltaChainManager {
    max_depth: usize,
    chain_map: HashMap<SHA256Hash, ChainInfo>,
}

#[derive(Debug, Clone)]
struct ChainInfo {
    depth: usize,
    #[allow(dead_code)]
    reference: SHA256Hash,
    children: Vec<SHA256Hash>,
}

impl DeltaChainManager {
    pub fn new(max_depth: usize) -> Self {
        Self {
            max_depth,
            chain_map: HashMap::new(),
        }
    }

    /// Add a delta chunk to the chain tracker
    pub fn add_chunk(&mut self, chunk: &DeltaChunk) {
        if let ChunkType::Delta { reference_hash, .. } = &chunk.chunk_type {
            let depth = self.chain_map
                .get(reference_hash)
                .map(|info| info.depth + 1)
                .unwrap_or(1);

            let info = ChainInfo {
                depth,
                reference: reference_hash.clone(),
                children: Vec::new(),
            };

            self.chain_map.insert(chunk.content_hash.clone(), info);

            // Update parent's children list
            if let Some(parent) = self.chain_map.get_mut(reference_hash) {
                parent.children.push(chunk.content_hash.clone());
            }
        }
    }

    /// Check if a chunk needs rebasing
    pub fn needs_rebase(&self, chunk_hash: &SHA256Hash) -> bool {
        self.chain_map
            .get(chunk_hash)
            .map(|info| info.depth > self.max_depth)
            .unwrap_or(false)
    }

    /// Get all chunks that need rebasing
    pub fn get_rebase_candidates(&self) -> Vec<SHA256Hash> {
        self.chain_map
            .iter()
            .filter(|(_, info)| info.depth > self.max_depth)
            .map(|(hash, _)| hash.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconstructor_creation() {
        let reconstructor = DeltaReconstructor::default();
        let stats = reconstructor.cache_stats();
        assert_eq!(stats.reference_cache_size, 0);
        assert_eq!(stats.max_cache_size, 100);
    }

    #[test]
    fn test_sequence_edit_substitute() {
        let reconstructor = DeltaReconstructor::default();
        let mut sequence = b"ACGT".to_vec();

        let edit = SeqEdit::Substitute { pos: 1, new_base: b'T' };
        reconstructor.apply_sequence_edit(&mut sequence, &edit).unwrap();

        assert_eq!(sequence, b"ATGT");
    }

    #[test]
    fn test_sequence_edit_insert() {
        let reconstructor = DeltaReconstructor::default();
        let mut sequence = b"ACGT".to_vec();

        let edit = SeqEdit::Insert { pos: 2, bases: vec![b'A', b'A'] };
        reconstructor.apply_sequence_edit(&mut sequence, &edit).unwrap();

        assert_eq!(sequence, b"ACAAGT");
    }

    #[test]
    fn test_sequence_edit_delete() {
        let reconstructor = DeltaReconstructor::default();
        let mut sequence = b"ACGTACGT".to_vec();

        let edit = SeqEdit::Delete { pos: 2, count: 4 };
        reconstructor.apply_sequence_edit(&mut sequence, &edit).unwrap();

        assert_eq!(sequence, b"ACGT");
    }

    #[test]
    fn test_chain_manager() {
        let mut manager = DeltaChainManager::new(2);

        let chunk = DeltaChunk {
            content_hash: SHA256Hash::compute(b"chunk1"),
            reference_hash: SHA256Hash::compute(b"ref"),
            chunk_type: ChunkType::Delta {
                reference_hash: SHA256Hash::compute(b"ref"),
                compression_ratio: 0.5,
            },
            taxonomy_version: SHA256Hash::zero(),
            taxon_ids: Vec::new(),
            deltas: Vec::new(),
            sequences: Vec::new(),
            created_at: chrono::Utc::now(),
            valid_from: chrono::Utc::now(),
            valid_until: None,
            original_size: 1000,
            compressed_size: 500,
            compression_ratio: 0.5,
        };

        manager.add_chunk(&chunk);
        assert!(!manager.needs_rebase(&chunk.content_hash));
    }
}

// Implement the DeltaReconstructor trait
impl DeltaReconstructorTrait for DeltaReconstructor {
    fn reconstruct_sequences(
        &self,
        delta_chunk: &DeltaChunk,
        reference_chunk: &TaxonomyAwareChunk,
    ) -> Result<Vec<Sequence>> {
        // For now, we need to convert SequenceRef to actual Sequences
        // In a real implementation, we'd load sequences from storage
        // based on the SequenceRef chunk_hash and offset
        let reference_sequences: Vec<Sequence> = reference_chunk.sequences
            .iter()
            .map(|seq_ref| Sequence {
                id: seq_ref.sequence_id.clone(),
                description: None,
                sequence: Vec::new(), // Would be loaded from storage
                taxon_id: None,
            })
            .collect();

        // Use the existing reconstruct_chunk method
        self.reconstruct_chunk(delta_chunk, reference_sequences)
    }

    fn apply_delta_operations(
        &self,
        operations: &[DeltaOperation],
        reference_data: &[u8],
    ) -> Result<Vec<u8>> {
        // Create a temporary sequence from reference data
        let ref_seq = Sequence {
            id: "temp_ref".to_string(),
            description: None,
            sequence: reference_data.to_vec(),
            taxon_id: None,
        };

        let ref_map = HashMap::from([("temp_ref".to_string(), &ref_seq)]);
        let mut result_data = Vec::new();

        for operation in operations {
            if let Some(seq) = self.apply_delta_operation(operation, &ref_map)? {
                result_data.extend_from_slice(&seq.sequence);
            }
        }

        Ok(result_data)
    }

    fn apply_operation(
        &self,
        operation: &DeltaOperation,
        current_data: &mut Vec<u8>,
        reference_data: &[u8],
    ) -> Result<()> {
        match operation {
            DeltaOperation::UseReference { reference_offset, length, .. } => {
                // Replace current data with reference substring
                current_data.clear();
                let end = (*reference_offset + *length).min(reference_data.len());
                current_data.extend_from_slice(&reference_data[*reference_offset..end]);
            }

            DeltaOperation::Insert { data, .. } => {
                // Replace with new data
                current_data.clear();
                current_data.extend_from_slice(data);
            }

            DeltaOperation::Modify { operations, .. } => {
                // Apply sequence edits
                for edit in operations {
                    self.apply_sequence_edit(current_data, edit)?;
                }
            }

            DeltaOperation::Delete { .. } => {
                // Clear the data
                current_data.clear();
            }
        }

        Ok(())
    }

    fn validate_reconstruction(
        &self,
        original_hash: &SHA256Hash,
        reconstructed: &[u8],
    ) -> Result<bool> {
        // Compute hash of reconstructed data
        let reconstructed_hash = SHA256Hash::compute(reconstructed);
        Ok(reconstructed_hash == *original_hash)
    }

    fn name(&self) -> &str {
        "DeltaReconstructor"
    }
}