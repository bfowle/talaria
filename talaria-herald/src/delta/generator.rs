use crate::delta::DeltaGenerator as DeltaGeneratorTrait;
pub use crate::delta::DeltaGeneratorConfig;
use crate::types::{SHA256HashExt, *};
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use talaria_bio::compression::{DeltaEncoder, DeltaRecord};
/// Delta generation engine for HERALD integration
///
/// This module bridges the gap between the reducer's delta encoder and HERALD's
/// content-addressed storage, converting delta records into HERALD delta chunks.
use talaria_bio::sequence::Sequence;

/// Delta generator for creating HERALD delta chunks
pub struct DeltaGenerator {
    config: DeltaGeneratorConfig,
    encoder: DeltaEncoder,
}

impl DeltaGenerator {
    pub fn new(config: DeltaGeneratorConfig) -> Self {
        Self {
            config,
            encoder: DeltaEncoder::new(),
        }
    }

    /// Generate delta chunks from sequences with references
    pub fn generate_delta_chunks(
        &mut self,
        sequences: &[Sequence],
        references: &[Sequence],
        reference_chunk_hash: SHA256Hash,
    ) -> Result<Vec<TemporalDeltaChunk>> {
        // Generate delta records for each sequence
        let mut delta_records = Vec::new();
        let mut full_sequences = Vec::new();

        for seq in sequences {
            // Select best reference based on similarity
            let best_ref = self.select_best_reference(seq, references)?;

            // Generate delta encoding
            let delta_record = self.encoder.encode(seq, best_ref);

            // Check if delta is efficient based on operation count
            if delta_record.deltas.len() <= self.config.max_delta_ops_threshold {
                delta_records.push(delta_record);
            } else {
                // Fall back to full sequence storage only if too many operations
                full_sequences.push(seq.clone());
            }
        }

        // Batch delta records into chunks
        let chunks = self.batch_into_chunks(delta_records, reference_chunk_hash)?;

        // Handle full sequences separately if needed
        if !full_sequences.is_empty() {
            // These would be stored as regular HERALD chunks, not delta chunks
            // For now, we'll skip them in delta generation
            tracing::info!(
                "Skipping {} sequences that don't meet delta criteria",
                full_sequences.len()
            );
        }

        Ok(chunks)
    }

    /// Batch delta records into appropriately sized chunks
    fn batch_into_chunks(
        &self,
        delta_records: Vec<DeltaRecord>,
        reference_chunk_hash: SHA256Hash,
    ) -> Result<Vec<TemporalDeltaChunk>> {
        if delta_records.is_empty() {
            return Ok(Vec::new());
        }

        let mut chunks = Vec::new();
        let mut current_batch = Vec::new();
        let mut current_size = 0;

        for record in delta_records {
            let record_size = self.estimate_record_size(&record);

            // Check if adding this record would exceed limits
            if !current_batch.is_empty()
                && (current_size + record_size > self.config.max_chunk_size
                    || current_batch.len() >= self.config.target_sequences_per_chunk)
            {
                // Create chunk from current batch
                chunks.push(self.create_delta_chunk(current_batch, reference_chunk_hash.clone())?);
                current_batch = Vec::new();
                current_size = 0;
            }

            current_size += record_size;
            current_batch.push(record);
        }

        // Don't forget the last batch
        if !current_batch.is_empty() {
            chunks.push(self.create_delta_chunk(current_batch, reference_chunk_hash)?);
        }

        Ok(chunks)
    }

    /// Create a HERALD delta chunk from delta records
    fn create_delta_chunk(
        &self,
        delta_records: Vec<DeltaRecord>,
        reference_chunk_hash: SHA256Hash,
    ) -> Result<TemporalDeltaChunk> {
        // Extract metadata
        let taxon_ids: Vec<TaxonId> = delta_records
            .iter()
            .filter_map(|r| r.taxon_id.map(TaxonId))
            .collect();

        // Calculate statistics
        let total_ops: usize = delta_records.iter().map(|r| r.deltas.len()).sum();
        let _max_ops = delta_records
            .iter()
            .map(|r| r.deltas.len())
            .max()
            .unwrap_or(0);
        let _avg_ops = if !delta_records.is_empty() {
            total_ops as f32 / delta_records.len() as f32
        } else {
            0.0
        };

        // Convert to HERALD delta operations
        let mut delta_operations = Vec::new();
        for record in &delta_records {
            delta_operations.push(self.convert_to_herald_operation(record)?);
        }

        // Calculate sizes
        let serialized = serde_json::to_vec(&delta_operations)?;
        let original_size = serialized.len();

        // Try compression
        let (_compressed_data, compressed_size) = if self.config.enable_compression {
            use flate2::write::GzEncoder;
            use flate2::Compression;
            use std::io::Write;

            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&serialized)?;
            let compressed = encoder.finish()?;

            if compressed.len() < original_size {
                (Some(compressed.clone()), compressed.len())
            } else {
                (None, original_size)
            }
        } else {
            (None, original_size)
        };

        let compression_ratio = compressed_size as f32 / original_size as f32;

        // Create the delta chunk using the new HERALD types
        let chunk_type = ChunkClassification::Delta {
            reference_hash: reference_chunk_hash.clone(),
            compression_ratio,
        };

        // Build sequence references
        let sequences: Vec<SequenceRef> = delta_records
            .iter()
            .enumerate()
            .map(|(i, record)| SequenceRef {
                chunk_hash: SHA256Hash::zero(), // Will be set when chunk is stored
                offset: i * 1000,               // Approximate offset
                length: record.deltas.iter().map(|d| d.substitution.len()).sum(),
                sequence_id: record.child_id.clone(),
            })
            .collect();

        // Compute content hash
        let content_hash = SHA256Hash::compute(&serialized);

        // Use the HERALD TemporalDeltaChunk type that's defined in types.rs
        let delta_chunk = TemporalDeltaChunk {
            content_hash: content_hash.clone(),
            reference_hash: reference_chunk_hash,
            chunk_type,
            taxonomy_version: SHA256Hash::zero(), // Will be set by HERALD
            taxon_ids,
            deltas: delta_operations,
            sequences,
            created_at: Utc::now(),
            valid_from: Utc::now(),
            valid_until: None,
            original_size,
            compressed_size,
            compression_ratio,
        };

        Ok(delta_chunk)
    }

    /// Convert a delta record to HERALD delta operation
    fn convert_to_herald_operation(&self, record: &DeltaRecord) -> Result<DeltaOperation> {
        // If no deltas, it's a direct reference
        if record.deltas.is_empty() {
            return Ok(DeltaOperation::UseReference {
                sequence_id: record.child_id.clone(),
                reference_offset: 0,
                length: 0, // Will be determined from reference
            });
        }

        // Convert delta ranges to sequence edits
        let mut edits = Vec::new();
        for delta in &record.deltas {
            // Each delta range is a substitution
            for (i, &byte) in delta.substitution.iter().enumerate() {
                edits.push(SeqEdit::Substitute {
                    pos: delta.start + i,
                    new_base: byte,
                });
            }
        }

        Ok(DeltaOperation::Modify {
            sequence_id: record.child_id.clone(),
            reference_offset: 0,
            operations: edits,
        })
    }

    /// Estimate the size of a delta record
    fn estimate_record_size(&self, record: &DeltaRecord) -> usize {
        // Simple estimation based on components
        let id_size = record.child_id.len() + record.reference_id.len();
        let delta_size: usize = record
            .deltas
            .iter()
            .map(|d| 8 + d.substitution.len()) // positions + data
            .sum();
        let header_size = record
            .header_change
            .as_ref()
            .map(|h| h.new_description.as_ref().map(|d| d.len()).unwrap_or(0))
            .unwrap_or(0);

        id_size + delta_size + header_size + 32 // Add overhead
    }

    /// Generate incremental update chunks
    pub fn generate_incremental_update(
        &mut self,
        old_sequences: &[Sequence],
        new_sequences: &[Sequence],
        old_chunk_hash: SHA256Hash,
    ) -> Result<Vec<TemporalDeltaChunk>> {
        // Map old sequences by ID for quick lookup
        let old_map: HashMap<String, &Sequence> =
            old_sequences.iter().map(|s| (s.id.clone(), s)).collect();

        let mut delta_records = Vec::new();
        let mut new_inserts = Vec::new();
        let mut deletions = Vec::new();

        // Process new/modified sequences
        for new_seq in new_sequences {
            if let Some(old_seq) = old_map.get(&new_seq.id) {
                // Sequence exists - check for modifications
                if old_seq.sequence != new_seq.sequence {
                    // Generate delta for modification
                    let delta = self.encoder.encode(new_seq, old_seq);
                    delta_records.push(delta);
                }
            } else {
                // New sequence
                new_inserts.push(new_seq.clone());
            }
        }

        // Find deleted sequences
        for old_seq in old_sequences {
            if !new_sequences.iter().any(|s| s.id == old_seq.id) {
                deletions.push(old_seq.id.clone());
            }
        }

        // Create delta chunks for updates
        let mut chunks = self.batch_into_chunks(delta_records, old_chunk_hash.clone())?;

        // Add insertion operations
        if !new_inserts.is_empty() {
            let insert_ops: Vec<DeltaOperation> = new_inserts
                .into_iter()
                .map(|seq| DeltaOperation::Insert {
                    sequence_id: seq.id,
                    data: seq.sequence,
                })
                .collect();

            // Create insertion chunk
            let insert_chunk = self.create_operation_chunk(insert_ops, old_chunk_hash.clone())?;
            chunks.push(insert_chunk);
        }

        // Add deletion operations
        if !deletions.is_empty() {
            let delete_ops: Vec<DeltaOperation> = deletions
                .into_iter()
                .map(|id| DeltaOperation::Delete { sequence_id: id })
                .collect();

            // Create deletion chunk
            let delete_chunk = self.create_operation_chunk(delete_ops, old_chunk_hash.clone())?;
            chunks.push(delete_chunk);
        }

        Ok(chunks)
    }

    /// Create a chunk from delta operations
    fn create_operation_chunk(
        &self,
        operations: Vec<DeltaOperation>,
        reference_hash: SHA256Hash,
    ) -> Result<TemporalDeltaChunk> {
        let serialized = serde_json::to_vec(&operations)?;
        let content_hash = SHA256Hash::compute(&serialized);

        let delta_chunk = TemporalDeltaChunk {
            content_hash,
            reference_hash: reference_hash.clone(),
            chunk_type: ChunkClassification::Delta {
                reference_hash,
                compression_ratio: 1.0,
            },
            taxonomy_version: SHA256Hash::zero(),
            taxon_ids: Vec::new(),
            deltas: operations,
            sequences: Vec::new(),
            created_at: Utc::now(),
            valid_from: Utc::now(),
            valid_until: None,
            original_size: serialized.len(),
            compressed_size: serialized.len(),
            compression_ratio: 1.0,
        };

        Ok(delta_chunk)
    }

    /// Select the best reference for a sequence based on similarity
    fn select_best_reference<'a>(
        &self,
        sequence: &Sequence,
        references: &'a [Sequence],
    ) -> Result<&'a Sequence> {
        if references.is_empty() {
            return Err(anyhow::anyhow!("No references available"));
        }

        // For small sets, check all references
        if references.len() <= 10 {
            let mut best_ref = &references[0];
            let mut best_score = 0.0;

            for reference in references {
                let score = self.compute_similarity_score(sequence, reference);
                if score > best_score {
                    best_score = score;
                    best_ref = reference;
                }
            }

            return Ok(best_ref);
        }

        // For larger sets, use sampling to avoid O(n) comparison
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();

        // Sample 10 random references
        let sample_size = std::cmp::min(10, references.len());
        let mut indices: Vec<usize> = (0..references.len()).collect();
        indices.shuffle(&mut rng);
        indices.truncate(sample_size);

        let mut best_ref = &references[indices[0]];
        let mut best_score = 0.0;

        for &idx in &indices {
            let reference = &references[idx];
            let score = self.compute_similarity_score(sequence, reference);
            if score > best_score {
                best_score = score;
                best_ref = reference;
            }
        }

        Ok(best_ref)
    }

    /// Compute similarity score between two sequences
    fn compute_similarity_score(&self, seq1: &Sequence, seq2: &Sequence) -> f64 {
        // Use k-mer based similarity for efficiency
        let k = 3; // Use 3-mers
        let kmers1 = self.extract_kmers(&seq1.sequence, k);
        let kmers2 = self.extract_kmers(&seq2.sequence, k);

        // Jaccard similarity
        let intersection = kmers1.intersection(&kmers2).count();
        let union = kmers1.union(&kmers2).count();

        if union == 0 {
            return 0.0;
        }

        intersection as f64 / union as f64
    }

    /// Extract k-mers from a sequence
    fn extract_kmers(&self, sequence: &[u8], k: usize) -> std::collections::HashSet<Vec<u8>> {
        use std::collections::HashSet;

        let mut kmers = HashSet::new();
        if sequence.len() < k {
            return kmers;
        }

        for i in 0..=sequence.len() - k {
            kmers.insert(sequence[i..i + k].to_vec());
        }

        kmers
    }
}

// Implement the DeltaGenerator trait
impl DeltaGeneratorTrait for DeltaGenerator {
    fn generate_deltas(
        &mut self,
        sequences: &[Sequence],
        references: &[Sequence],
        reference_hash: SHA256Hash,
    ) -> Result<Vec<TemporalDeltaChunk>> {
        self.generate_delta_chunks(sequences, references, reference_hash)
    }

    fn set_config(&mut self, config: DeltaGeneratorConfig) {
        self.config = config;
    }

    fn get_config(&self) -> &DeltaGeneratorConfig {
        &self.config
    }
}

#[test]
fn test_delta_generator_creation() {
    let config = DeltaGeneratorConfig::default();
    let _generator = DeltaGenerator::new(config);
    // DeltaGenerator created successfully
}

#[test]
fn test_incremental_update_detection() {
    let mut generator = DeltaGenerator::new(DeltaGeneratorConfig::default());

    let old_sequences = vec![
        Sequence::new("seq1".to_string(), b"ACGTACGT".to_vec()),
        Sequence::new("seq2".to_string(), b"TTTTGGGG".to_vec()),
    ];

    let new_sequences = vec![
        Sequence::new("seq1".to_string(), b"ACGTACGT".to_vec()), // Unchanged
        Sequence::new("seq2".to_string(), b"TTTTAAAA".to_vec()), // Modified
        Sequence::new("seq3".to_string(), b"CCCCCCCC".to_vec()), // New
    ];

    let old_hash = SHA256Hash::compute(b"old_chunk");
    let chunks = generator
        .generate_incremental_update(&old_sequences, &new_sequences, old_hash)
        .unwrap();

    // Should have chunks for modifications and insertions
    assert!(!chunks.is_empty());
}
