/// Delta compression for canonical sequences
use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::storage::sequence::SequenceStorage;
use crate::types::{
    CanonicalSequence, SHA256Hash,
};

/// Trait for delta compression algorithms
pub trait DeltaCompressor: Send + Sync {
    /// Compute delta between two sequences
    fn compute_delta(&self, reference: &[u8], target: &[u8]) -> Result<Delta>;

    /// Reconstruct sequence from reference and delta
    fn apply_delta(&self, reference: &[u8], delta: &Delta) -> Result<Vec<u8>>;

    /// Estimate compression ratio
    fn estimate_ratio(&self, reference: &[u8], target: &[u8]) -> f32;
}

/// Delta operations for sequence transformation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DeltaOp {
    /// Copy bytes from reference
    Copy { offset: usize, length: usize },
    /// Insert new bytes
    Insert { data: Vec<u8> },
    /// Skip bytes in reference
    Skip { length: usize },
}

/// Delta between two sequences
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Delta {
    /// Operations to transform reference to target
    pub ops: Vec<DeltaOp>,
    /// Size of original sequence
    pub original_size: usize,
    /// Size of delta
    pub delta_size: usize,
    /// Compression ratio
    pub compression_ratio: f32,
}

/// Canonical delta chunk - references canonical sequences
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CanonicalTemporalDeltaChunk {
    /// Hash of the reference canonical sequence
    pub reference_hash: SHA256Hash,

    /// Deltas from this reference to other sequences
    pub deltas: Vec<CanonicalDelta>,

    /// Statistics
    pub total_sequences: usize,
    pub average_compression: f32,
    pub space_saved: usize,
}

/// Delta for a single canonical sequence
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CanonicalDelta {
    /// Hash of the target canonical sequence
    pub target_hash: SHA256Hash,

    /// Delta operations
    pub delta: Delta,

    /// Metadata
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Myers diff algorithm for biological sequences
pub struct MyersDeltaCompressor {
    _max_distance: usize,
}

impl MyersDeltaCompressor {
    pub fn new(max_distance: usize) -> Self {
        Self { _max_distance: max_distance }
    }

    fn compute_lcs(&self, a: &[u8], b: &[u8]) -> Vec<(usize, usize, usize)> {
        // Simplified LCS for demonstration
        // In production, use Myers' algorithm or similar
        let mut lcs = Vec::new();
        let mut i = 0;
        let mut j = 0;

        while i < a.len() && j < b.len() {
            // Find next matching region
            if a[i] == b[j] {
                let start_i = i;
                let start_j = j;
                while i < a.len() && j < b.len() && a[i] == b[j] {
                    i += 1;
                    j += 1;
                }
                lcs.push((start_i, start_j, i - start_i));
            } else {
                // Skip non-matching
                if i < a.len() - 1 && j < b.len() - 1 {
                    if a[i + 1] == b[j] {
                        i += 1;
                    } else if a[i] == b[j + 1] {
                        j += 1;
                    } else {
                        i += 1;
                        j += 1;
                    }
                } else {
                    break;
                }
            }
        }

        lcs
    }
}

impl DeltaCompressor for MyersDeltaCompressor {
    fn compute_delta(&self, reference: &[u8], target: &[u8]) -> Result<Delta> {
        let mut ops = Vec::new();
        let lcs = self.compute_lcs(reference, target);

        let mut ref_pos = 0;
        let mut tgt_pos = 0;

        for (ref_match, tgt_match, length) in lcs {
            // Insert any new bytes before the match
            if tgt_match > tgt_pos {
                ops.push(DeltaOp::Insert {
                    data: target[tgt_pos..tgt_match].to_vec(),
                });
            }

            // Skip any reference bytes before the match
            if ref_match > ref_pos {
                ops.push(DeltaOp::Skip {
                    length: ref_match - ref_pos,
                });
            }

            // Copy the matching region
            ops.push(DeltaOp::Copy {
                offset: ref_match,
                length,
            });

            ref_pos = ref_match + length;
            tgt_pos = tgt_match + length;
        }

        // Handle remaining target bytes
        if tgt_pos < target.len() {
            ops.push(DeltaOp::Insert {
                data: target[tgt_pos..].to_vec(),
            });
        }

        // Calculate sizes
        let delta_size: usize = ops.iter().map(|op| match op {
            DeltaOp::Copy { .. } => 16, // Size of copy instruction
            DeltaOp::Insert { data } => 8 + data.len(),
            DeltaOp::Skip { .. } => 8,
        }).sum();

        let compression_ratio = if target.len() > 0 {
            delta_size as f32 / target.len() as f32
        } else {
            1.0
        };

        Ok(Delta {
            ops,
            original_size: target.len(),
            delta_size,
            compression_ratio,
        })
    }

    fn apply_delta(&self, reference: &[u8], delta: &Delta) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(delta.original_size);

        for op in &delta.ops {
            match op {
                DeltaOp::Copy { offset, length } => {
                    if offset + length <= reference.len() {
                        result.extend_from_slice(&reference[*offset..*offset + *length]);
                    } else {
                        anyhow::bail!("Invalid copy operation: out of bounds");
                    }
                }
                DeltaOp::Insert { data } => {
                    result.extend_from_slice(data);
                }
                DeltaOp::Skip { .. } => {
                    // Skip operation doesn't add to result
                }
            }
        }

        Ok(result)
    }

    fn estimate_ratio(&self, reference: &[u8], target: &[u8]) -> f32 {
        // Quick estimation based on similarity
        let max_len = reference.len().max(target.len());
        if max_len == 0 {
            return 1.0;
        }

        let matches = reference.iter()
            .zip(target.iter())
            .filter(|(a, b)| a == b)
            .count();

        1.0 - (matches as f32 / max_len as f32)
    }
}

/// Manager for canonical delta compression
pub struct CanonicalDeltaManager {
    storage: SequenceStorage,
    deltas_dir: PathBuf,
    compressor: Box<dyn DeltaCompressor>,
}

impl CanonicalDeltaManager {
    pub fn new(storage: SequenceStorage, base_path: &Path) -> Result<Self> {
        let deltas_dir = base_path.join("deltas");
        std::fs::create_dir_all(&deltas_dir)?;

        Ok(Self {
            storage,
            deltas_dir,
            compressor: Box::new(MyersDeltaCompressor::new(1000)),
        })
    }

    /// Select optimal reference sequences for a set of sequences
    pub fn select_references(
        &self,
        sequence_hashes: &[SHA256Hash],
        max_references: usize,
    ) -> Result<Vec<SHA256Hash>> {
        // Simple selection: choose sequences that minimize total delta size
        // In production, use clustering or more sophisticated algorithms

        if sequence_hashes.len() <= max_references {
            return Ok(sequence_hashes.to_vec());
        }

        // For now, just take evenly distributed sequences
        let mut references = Vec::new();
        let step = sequence_hashes.len() / max_references;

        for i in (0..sequence_hashes.len()).step_by(step) {
            references.push(sequence_hashes[i].clone());
            if references.len() >= max_references {
                break;
            }
        }

        Ok(references)
    }

    /// Compute deltas for a set of sequences
    pub fn compute_deltas_for_set(
        &self,
        reference_hash: &SHA256Hash,
        target_hashes: &[SHA256Hash],
    ) -> Result<CanonicalTemporalDeltaChunk> {
        // Load reference sequence
        let reference = self.storage.load_canonical(reference_hash)?;

        let mut deltas = Vec::new();
        let mut total_saved = 0;

        for target_hash in target_hashes {
            if target_hash == reference_hash {
                continue; // Skip self-reference
            }

            // Load target sequence
            let target = self.storage.load_canonical(target_hash)?;

            // Compute delta
            let delta = self.compressor.compute_delta(
                &reference.sequence,
                &target.sequence,
            )?;

            // Only use delta if it saves space
            if delta.compression_ratio < 0.8 {
                total_saved += target.sequence.len() - delta.delta_size;

                deltas.push(CanonicalDelta {
                    target_hash: target_hash.clone(),
                    delta,
                    created_at: chrono::Utc::now(),
                });
            }
        }

        let average_compression = if !deltas.is_empty() {
            deltas.iter().map(|d| d.delta.compression_ratio).sum::<f32>() / deltas.len() as f32
        } else {
            1.0
        };

        Ok(CanonicalTemporalDeltaChunk {
            reference_hash: reference_hash.clone(),
            deltas,
            total_sequences: target_hashes.len(),
            average_compression,
            space_saved: total_saved,
        })
    }

    /// Store a delta chunk
    pub fn store_delta_chunk(&self, chunk: &CanonicalTemporalDeltaChunk) -> Result<()> {
        let path = self.get_delta_path(&chunk.reference_hash);

        // Create parent directories
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Serialize and compress
        let data = rmp_serde::to_vec(chunk)?;
        let compressed = zstd::encode_all(&data[..], 3)?;
        std::fs::write(&path, compressed)?;

        Ok(())
    }

    /// Load a delta chunk
    pub fn load_delta_chunk(&self, reference_hash: &SHA256Hash) -> Result<Option<CanonicalTemporalDeltaChunk>> {
        let path = self.get_delta_path(reference_hash);

        if !path.exists() {
            return Ok(None);
        }

        let compressed = std::fs::read(&path)?;
        let data = zstd::decode_all(&compressed[..])?;
        let chunk = rmp_serde::from_slice(&data)?;

        Ok(Some(chunk))
    }

    /// Reconstruct a sequence using delta
    pub fn reconstruct_sequence(&self, target_hash: &SHA256Hash) -> Result<CanonicalSequence> {
        // First, check if we have the sequence directly
        if let Ok(seq) = self.storage.load_canonical(target_hash) {
            return Ok(seq);
        }

        // Look for delta that can reconstruct this sequence
        for entry in std::fs::read_dir(&self.deltas_dir)? {
            let entry = entry?;
            if entry.path().extension() == Some(std::ffi::OsStr::new("delta")) {
                // Load delta chunk
                let compressed = std::fs::read(entry.path())?;
                let data = zstd::decode_all(&compressed[..])?;
                let chunk: CanonicalTemporalDeltaChunk = rmp_serde::from_slice(&data)?;

                // Check if this chunk has our target
                for delta in &chunk.deltas {
                    if delta.target_hash == *target_hash {
                        // Load reference and apply delta
                        let reference = self.storage.load_canonical(&chunk.reference_hash)?;
                        let reconstructed = self.compressor.apply_delta(
                            &reference.sequence,
                            &delta.delta,
                        )?;

                        return Ok(CanonicalSequence {
                            sequence_hash: target_hash.clone(),
                            sequence: reconstructed,
                            length: delta.delta.original_size,
                            sequence_type: reference.sequence_type,
                            checksum: reference.checksum, // Should recalculate
                            first_seen: reference.first_seen,
                            last_seen: chrono::Utc::now(),
                        });
                    }
                }
            }
        }

        anyhow::bail!("Cannot reconstruct sequence: {}", target_hash)
    }

    fn get_delta_path(&self, reference_hash: &SHA256Hash) -> PathBuf {
        let hex = reference_hash.to_hex();
        self.deltas_dir.join(format!("{}.delta", hex))
    }
}

// Benefits of canonical delta compression:
// 1. Deltas computed once per sequence pair, not per database
// 2. Works across all databases - if sequence A and B appear in multiple databases,
//    delta is computed once
// 3. Reference selection can be optimized globally
// 4. Reconstruction is independent of database source
// 5. Can use more sophisticated algorithms since we only compute once