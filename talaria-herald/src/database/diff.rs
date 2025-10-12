use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use talaria_bio::parse_fasta;
use talaria_bio::sequence::Sequence;
use talaria_utils::report::{
    ComparisonResult, ModifiedSequence, RenamedSequence, SequenceChange, SequenceInfo,
};

#[derive(Debug, Clone)]
pub struct DatabaseDiffer {
    similarity_threshold: f64,
    headers_only: bool,
    threads: usize,
}

impl DatabaseDiffer {
    pub fn new() -> Self {
        Self {
            similarity_threshold: 0.95,
            headers_only: false,
            threads: rayon::current_num_threads(),
        }
    }

    pub fn with_similarity_threshold(mut self, threshold: f64) -> Self {
        self.similarity_threshold = threshold;
        self
    }

    pub fn with_headers_only(mut self, headers_only: bool) -> Self {
        self.headers_only = headers_only;
        self
    }

    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }

    pub fn compare(&self, old_path: &Path, new_path: &Path) -> Result<ComparisonResult> {
        tracing::debug!("Loading old database...");
        let old_sequences = self.load_database(old_path)?;

        tracing::debug!("Loading new database...");
        let new_sequences = self.load_database(new_path)?;

        tracing::info!(
            "Comparing {} vs {} sequences...",
            old_sequences.len(),
            new_sequences.len()
        );

        let mut result = ComparisonResult::new(
            old_path.to_path_buf(),
            new_path.to_path_buf(),
            old_sequences.len(),
            new_sequences.len(),
        );

        let old_ids: HashSet<String> = old_sequences.keys().cloned().collect();
        let new_ids: HashSet<String> = new_sequences.keys().cloned().collect();

        // Find added sequences
        for id in new_ids.difference(&old_ids) {
            if let Some(seq) = new_sequences.get(id) {
                result.added.push(sequence_info_from_sequence(seq));
            }
        }

        // Find removed sequences
        for id in old_ids.difference(&new_ids) {
            if let Some(seq) = old_sequences.get(id) {
                result.removed.push(sequence_info_from_sequence(seq));
            }
        }

        // Find modified or unchanged sequences
        for id in old_ids.intersection(&new_ids) {
            let old_seq = &old_sequences[id];
            let new_seq = &new_sequences[id];

            if self.headers_only {
                // Compare only headers
                if old_seq.description != new_seq.description {
                    result.modified.push(ModifiedSequence {
                        old: sequence_info_from_sequence(old_seq),
                        new: sequence_info_from_sequence(new_seq),
                        similarity: 0.0,
                        changes: vec![SequenceChange::HeaderChanged],
                    });
                } else {
                    result.unchanged_count += 1;
                }
            } else {
                // Full sequence comparison
                if old_seq.sequence != new_seq.sequence {
                    let similarity =
                        self.calculate_similarity(&old_seq.sequence, &new_seq.sequence);
                    let changes = self.detect_changes(old_seq, new_seq);

                    result.modified.push(ModifiedSequence {
                        old: sequence_info_from_sequence(old_seq),
                        new: sequence_info_from_sequence(new_seq),
                        similarity,
                        changes,
                    });
                } else if old_seq.description != new_seq.description {
                    // Same sequence, different header
                    result.renamed.push(RenamedSequence {
                        old_id: old_seq.id.clone(),
                        new_id: new_seq.id.clone(),
                        old_description: old_seq.description.clone(),
                        new_description: new_seq.description.clone(),
                    });
                } else {
                    result.unchanged_count += 1;
                }
            }
        }

        // Calculate statistics
        result.calculate_statistics(&old_sequences, &new_sequences, |s| s.len(), |s| s.taxon_id);

        Ok(result)
    }

    fn load_database(&self, path: &Path) -> Result<HashMap<String, Sequence>> {
        let sequences_vec =
            parse_fasta(path).map_err(|e| anyhow::anyhow!("Failed to parse FASTA: {}", e))?;

        let mut sequences = HashMap::new();
        for seq in sequences_vec {
            sequences.insert(seq.id.clone(), seq);
        }

        Ok(sequences)
    }

    fn calculate_similarity(&self, seq1: &[u8], seq2: &[u8]) -> f64 {
        if seq1.is_empty() || seq2.is_empty() {
            return 0.0;
        }

        let len1 = seq1.len();
        let len2 = seq2.len();

        if (len1 as f64 - len2 as f64).abs() / len1.max(len2) as f64 > 0.5 {
            // Very different lengths, likely low similarity
            return 0.0;
        }

        // Simple identity calculation for now
        let min_len = len1.min(len2);
        let matches = seq1
            .iter()
            .zip(seq2.iter())
            .take(min_len)
            .filter(|(a, b)| a == b)
            .count();

        matches as f64 / len1.max(len2) as f64
    }

    fn detect_changes(&self, old: &Sequence, new: &Sequence) -> Vec<SequenceChange> {
        let mut changes = Vec::new();

        if old.description != new.description {
            changes.push(SequenceChange::HeaderChanged);
        }

        let len_diff = new.sequence.len() as i64 - old.sequence.len() as i64;
        if len_diff > 0 {
            changes.push(SequenceChange::Extended(len_diff as usize));
        } else if len_diff < 0 {
            changes.push(SequenceChange::Truncated((-len_diff) as usize));
        }

        // Check for mutations (simplified)
        if old.sequence.len() == new.sequence.len() {
            let mutations = old
                .sequence
                .iter()
                .zip(new.sequence.iter())
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .count();

            if mutations > 0 {
                changes.push(SequenceChange::Mutations(mutations));
            }
        }

        changes
    }
}

// Helper function to create SequenceInfo from Sequence
fn sequence_info_from_sequence(seq: &Sequence) -> SequenceInfo {
    SequenceInfo::new(
        seq.id.clone(),
        seq.description.clone(),
        seq.sequence.len(),
        seq.taxon_id,
    )
}
