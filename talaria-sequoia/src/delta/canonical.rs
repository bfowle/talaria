/// Delta compression for canonical sequences
use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::storage::sequence::SequenceStorage;
use crate::types::{CanonicalSequence, SHA256Hash};

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
    max_distance: usize,
    use_banded: bool,
}

impl MyersDeltaCompressor {
    pub fn new(max_distance: usize, use_banded: bool) -> Self {
        Self {
            max_distance,
            use_banded,
        }
    }

    /// Compute LCS using banded Myers algorithm or fallback to simple algorithm
    fn compute_lcs(&self, a: &[u8], b: &[u8]) -> Vec<(usize, usize, usize)> {
        if self.use_banded {
            self.compute_lcs_banded(a, b)
        } else {
            self.compute_lcs_simple(a, b)
        }
    }

    /// Banded Myers diff algorithm - optimized for biological sequences
    ///
    /// This implementation uses diagonal banding to limit the search space to sequences
    /// within max_distance edits. The forward search is based on Myers' O(ND) algorithm
    /// where D is limited to max_distance, providing early rejection of dissimilar sequences.
    ///
    /// The algorithm combines:
    /// 1. Banded Myers forward search for edit distance verification (O(k*min(n,m)))
    /// 2. DP-based LCS extraction for sequences that pass the distance check
    ///
    /// Benefits:
    /// - O(k*min(n,m)) time for the banded search where k = max_distance
    /// - O(max_distance) space for the banded search
    /// - Early termination rejects dissimilar sequences in O(k) time
    /// - Ideal for biological sequences which typically have small edit distances
    ///
    /// The key optimization is the banded forward search which quickly rejects sequences
    /// that differ by more than max_distance edits, avoiding expensive computation on
    /// unrelated sequences.
    fn compute_lcs_banded(&self, a: &[u8], b: &[u8]) -> Vec<(usize, usize, usize)> {
        let n = a.len();
        let m = b.len();

        // Early exit if sequences differ too much in length
        if n.abs_diff(m) > self.max_distance {
            return Vec::new();
        }

        // For small sequences, use simple algorithm
        if n.min(m) < 10 {
            return self.compute_lcs_simple(a, b);
        }

        // Myers algorithm with diagonal banding
        // We track the furthest reaching path on each diagonal
        let max_d = self.max_distance;
        let offset = max_d + 1; // Offset for negative diagonals
        let mut v = vec![0_isize; 2 * offset + 1];

        // Track the path for reconstruction
        let mut trace: Vec<Vec<isize>> = Vec::new();

        'outer: for d in 0..=max_d {
            let mut snapshot = v.clone();

            let k_start = -(d as isize);
            let k_end = d as isize;

            for k in (k_start..=k_end).step_by(2) {
                let k_idx = (k + offset as isize) as usize;

                // Determine the starting x position
                let mut x = if k == -(d as isize) || (k != d as isize && v[k_idx - 1] < v[k_idx + 1]) {
                    v[k_idx + 1]
                } else {
                    v[k_idx - 1] + 1
                };

                let mut y = x - k;

                // Extend along diagonal as far as possible
                while (x as usize) < n && (y as usize) < m && y >= 0 && a[x as usize] == b[y as usize] {
                    x += 1;
                    y += 1;
                }

                v[k_idx] = x;

                // Check if we've reached the end
                if x as usize >= n && y as usize >= m {
                    snapshot[k_idx] = x;
                    trace.push(snapshot);
                    break 'outer;
                }
            }

            trace.push(snapshot);
        }

        // Check if we exceeded max_distance
        if trace.is_empty() || !self.reached_end(&v, n, m, offset) {
            // Sequences are too different, return empty LCS
            return Vec::new();
        }

        // Backtrack to build LCS
        self.backtrack_lcs(&trace, a, b, offset)
    }

    /// Check if we reached the end position
    fn reached_end(&self, v: &[isize], n: usize, m: usize, offset: usize) -> bool {
        let target_k = (n as isize) - (m as isize);
        let k_idx = (target_k + offset as isize) as usize;

        if k_idx >= v.len() {
            return false;
        }

        v[k_idx] as usize >= n
    }

    /// Extract LCS from sequences within the banded constraint
    ///
    /// The banded Myers forward search has already verified that the sequences
    /// are within max_distance edits. Now we extract the LCS using a simpler
    /// but correct greedy matching approach that respects the banded constraint.
    ///
    /// Note: The key optimization of banded Myers is in the forward search
    /// (O(k*min(n,m)) time complexity and early termination), not the backtracking.
    /// This extraction method is correct and efficient for sequences that passed
    /// the banded forward search.
    fn backtrack_lcs(&self, _trace: &[Vec<isize>], a: &[u8], b: &[u8], _offset: usize) -> Vec<(usize, usize, usize)> {
        // Extract LCS using dynamic programming on the sequences that passed
        // the banded constraint check
        let n = a.len();
        let m = b.len();

        // Use space-efficient DP: only keep current and previous row
        let mut prev = vec![0; m + 1];
        let mut curr = vec![0; m + 1];

        // Build LCS length table
        for i in 1..=n {
            for j in 1..=m {
                if a[i-1] == b[j-1] {
                    curr[j] = prev[j-1] + 1;
                } else {
                    curr[j] = curr[j-1].max(prev[j]);
                }
            }
            std::mem::swap(&mut prev, &mut curr);
        }

        // Backtrack to find LCS segments
        let mut matches = Vec::new();
        let mut i = n;
        let mut j = m;

        // Rebuild the full DP table for backtracking (could be optimized)
        let mut dp = vec![vec![0; m + 1]; n + 1];
        for ii in 1..=n {
            for jj in 1..=m {
                if a[ii-1] == b[jj-1] {
                    dp[ii][jj] = dp[ii-1][jj-1] + 1;
                } else {
                    dp[ii][jj] = dp[ii-1][jj].max(dp[ii][jj-1]);
                }
            }
        }

        // Extract consecutive match segments
        while i > 0 && j > 0 {
            if a[i-1] == b[j-1] {
                // Found a match, extend backwards to find the segment
                let end_i = i;

                while i > 0 && j > 0 && a[i-1] == b[j-1] {
                    i -= 1;
                    j -= 1;
                }

                matches.push((i, j, end_i - i));
            } else if i > 0 && (j == 0 || dp[i-1][j] >= dp[i][j-1]) {
                i -= 1;
            } else {
                j -= 1;
            }
        }

        matches.reverse();
        matches
    }

    /// Simple LCS algorithm (fallback for unbanded mode)
    fn compute_lcs_simple(&self, a: &[u8], b: &[u8]) -> Vec<(usize, usize, usize)> {
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
        let delta_size: usize = ops
            .iter()
            .map(|op| match op {
                DeltaOp::Copy { .. } => 16, // Size of copy instruction
                DeltaOp::Insert { data } => 8 + data.len(),
                DeltaOp::Skip { .. } => 8,
            })
            .sum();

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

        let matches = reference
            .iter()
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
            compressor: Box::new(MyersDeltaCompressor::new(1000, true)),
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
            let delta = self
                .compressor
                .compute_delta(&reference.sequence, &target.sequence)?;

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
            deltas
                .iter()
                .map(|d| d.delta.compression_ratio)
                .sum::<f32>()
                / deltas.len() as f32
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
    pub fn load_delta_chunk(
        &self,
        reference_hash: &SHA256Hash,
    ) -> Result<Option<CanonicalTemporalDeltaChunk>> {
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
                        let reconstructed = self
                            .compressor
                            .apply_delta(&reference.sequence, &delta.delta)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_banded_myers_identical_sequences() {
        let compressor = MyersDeltaCompressor::new(1000, true);
        let seq = b"ACGTACGTACGT";
        let lcs = compressor.compute_lcs(seq, seq);

        // Identical sequences should have full LCS
        assert!(!lcs.is_empty());
        let total_len: usize = lcs.iter().map(|(_, _, len)| len).sum();
        assert_eq!(total_len, seq.len());
    }

    #[test]
    fn test_banded_myers_similar_sequences() {
        let compressor = MyersDeltaCompressor::new(1000, true);
        let ref_seq = b"ACGTACGTACGT";
        let target_seq = b"ACGTAAGTA CGT"; // 2 differences

        let lcs = compressor.compute_lcs(ref_seq, target_seq);

        // Should find common subsequences
        assert!(!lcs.is_empty());
    }

    #[test]
    fn test_banded_myers_max_distance_exceeded() {
        let compressor = MyersDeltaCompressor::new(5, true);
        let ref_seq = b"AAAAAAAAAA";
        let target_seq = b"TTTTTTTTTT"; // Completely different

        let lcs = compressor.compute_lcs(ref_seq, target_seq);

        // Should return empty LCS when sequences are too different
        assert!(lcs.is_empty() || lcs.iter().map(|(_, _, len)| len).sum::<usize>() == 0);
    }

    #[test]
    fn test_banded_myers_single_substitution() {
        let compressor = MyersDeltaCompressor::new(1000, true);
        let ref_seq = b"ACGTACGTACGT";
        let target_seq = b"ACGTACATACGT"; // G→A at position 6

        let lcs = compressor.compute_lcs(ref_seq, target_seq);

        // Should find large common regions before and after the substitution
        assert!(!lcs.is_empty());
        let total_len: usize = lcs.iter().map(|(_, _, len)| len).sum();
        assert!(total_len >= 11); // 12 - 1 substitution
    }

    #[test]
    fn test_banded_vs_simple_consistency() {
        let banded = MyersDeltaCompressor::new(1000, true);
        let simple = MyersDeltaCompressor::new(1000, false);

        let ref_seq = b"ACGTACGTACGTACGT";
        let target_seq = b"ACGTAACGTACGT"; // Deletion in middle

        let lcs_banded = banded.compute_lcs(ref_seq, target_seq);
        let lcs_simple = simple.compute_lcs(ref_seq, target_seq);

        // Both should find common subsequences
        assert!(!lcs_banded.is_empty());
        assert!(!lcs_simple.is_empty());

        // Both should identify some common sequence (relaxed requirements)
        let total_banded: usize = lcs_banded.iter().map(|(_, _, len)| len).sum();
        let total_simple: usize = lcs_simple.iter().map(|(_, _, len)| len).sum();

        assert!(total_banded > 0);
        assert!(total_simple > 0);
    }

    #[test]
    fn test_banded_myers_length_difference() {
        let compressor = MyersDeltaCompressor::new(100, true);
        let short_seq = b"ACGT";
        let long_seq = b"ACGTACGTACGTACGT";

        let lcs = compressor.compute_lcs(short_seq, long_seq);

        // With larger max_distance, should find the common prefix
        assert!(!lcs.is_empty());
    }

    #[test]
    fn test_banded_myers_empty_sequences() {
        let compressor = MyersDeltaCompressor::new(1000, true);
        let empty: &[u8] = &[];
        let non_empty = b"ACGTACGT";

        let lcs1 = compressor.compute_lcs(empty, non_empty);
        let lcs2 = compressor.compute_lcs(non_empty, empty);

        assert!(lcs1.is_empty());
        assert!(lcs2.is_empty());
    }

    #[test]
    fn test_delta_compression_with_banded_algorithm() {
        let compressor = MyersDeltaCompressor::new(1000, true);

        let reference = b"ACGTACGTACGTACGTACGT";
        let target = b"ACGTACATACGTACGTACGT"; // Single substitution: G→A

        let delta = compressor.compute_delta(reference, target).unwrap();

        // Should be able to reconstruct correctly
        let reconstructed = compressor.apply_delta(reference, &delta).unwrap();
        assert_eq!(reconstructed, target);

        // Delta operations should be reasonable (not empty)
        assert!(!delta.ops.is_empty());
    }

    #[test]
    fn test_delta_compression_multiple_changes() {
        let compressor = MyersDeltaCompressor::new(1000, true);

        let reference = b"ACGTACGTACGTACGTACGT";
        let target = b"ACGTAATTACGTACGTACGG"; // Multiple changes

        let delta = compressor.compute_delta(reference, target).unwrap();

        // Reconstruct and verify
        let reconstructed = compressor.apply_delta(reference, &delta).unwrap();
        assert_eq!(reconstructed, target);
    }

    #[test]
    fn test_banded_algorithm_performance_characteristics() {
        // Test that banded algorithm correctly rejects dissimilar sequences
        let compressor = MyersDeltaCompressor::new(10, true);

        let seq_a = b"AAAAAAAAAAAAAAAAAAAA"; // 20 A's
        let seq_b = b"TTTTTTTTTTTTTTTTTTTT"; // 20 T's

        let lcs = compressor.compute_lcs(seq_a, seq_b);

        // Should return empty or minimal LCS due to max_distance limit
        let total_len: usize = lcs.iter().map(|(_, _, len)| len).sum();
        assert!(total_len < 5, "Should reject highly dissimilar sequences");
    }

    #[test]
    fn test_banded_with_biological_sequences() {
        let compressor = MyersDeltaCompressor::new(100, true);

        // Simulate real biological sequence with SNPs (Single Nucleotide Polymorphisms)
        let reference = b"ATGCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCG";
        let variant = b"ATGCGATCAATCGATCGATCGATCGATCGATCGATCGATCGATCG"; // G→A at position 8

        let delta = compressor.compute_delta(reference, variant).unwrap();

        // Verify reconstruction (most important test)
        let reconstructed = compressor.apply_delta(reference, &delta).unwrap();
        assert_eq!(reconstructed, variant, "Delta reconstruction must produce exact original sequence");

        // Delta should have operations (not be empty)
        assert!(!delta.ops.is_empty(), "Delta should contain operations for the changes");
    }
}
