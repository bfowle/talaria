use crate::bio::fasta::parse_fasta;
use crate::bio::sequence::Sequence;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;

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
        println!("Loading old database...");
        let old_sequences = self.load_database(old_path)?;

        println!("Loading new database...");
        let new_sequences = self.load_database(new_path)?;

        println!(
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
                result.added.push(SequenceInfo::from_sequence(seq));
            }
        }

        // Find removed sequences
        for id in old_ids.difference(&new_ids) {
            if let Some(seq) = old_sequences.get(id) {
                result.removed.push(SequenceInfo::from_sequence(seq));
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
                        old: SequenceInfo::from_sequence(old_seq),
                        new: SequenceInfo::from_sequence(new_seq),
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
                        old: SequenceInfo::from_sequence(old_seq),
                        new: SequenceInfo::from_sequence(new_seq),
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
        result.calculate_statistics(&old_sequences, &new_sequences);

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

#[derive(Debug, Clone)]
pub struct ComparisonResult {
    pub old_path: std::path::PathBuf,
    pub new_path: std::path::PathBuf,
    pub old_count: usize,
    pub new_count: usize,
    pub added: Vec<SequenceInfo>,
    pub removed: Vec<SequenceInfo>,
    pub modified: Vec<ModifiedSequence>,
    pub renamed: Vec<RenamedSequence>,
    pub unchanged_count: usize,
    pub statistics: DatabaseStatistics,
}

impl ComparisonResult {
    fn new(
        old_path: std::path::PathBuf,
        new_path: std::path::PathBuf,
        old_count: usize,
        new_count: usize,
    ) -> Self {
        Self {
            old_path,
            new_path,
            old_count,
            new_count,
            added: Vec::new(),
            removed: Vec::new(),
            modified: Vec::new(),
            renamed: Vec::new(),
            unchanged_count: 0,
            statistics: DatabaseStatistics::default(),
        }
    }

    fn calculate_statistics(
        &mut self,
        old_sequences: &HashMap<String, Sequence>,
        new_sequences: &HashMap<String, Sequence>,
    ) {
        // Calculate length statistics
        let old_lengths: Vec<usize> = old_sequences.values().map(|s| s.sequence.len()).collect();
        let new_lengths: Vec<usize> = new_sequences.values().map(|s| s.sequence.len()).collect();

        self.statistics.old_total_length = old_lengths.iter().sum();
        self.statistics.new_total_length = new_lengths.iter().sum();

        if !old_lengths.is_empty() {
            self.statistics.old_avg_length = self.statistics.old_total_length / old_lengths.len();
        }

        if !new_lengths.is_empty() {
            self.statistics.new_avg_length = self.statistics.new_total_length / new_lengths.len();
        }

        // Taxonomic statistics
        let old_taxa: HashSet<u32> = old_sequences.values().filter_map(|s| s.taxon_id).collect();
        let new_taxa: HashSet<u32> = new_sequences.values().filter_map(|s| s.taxon_id).collect();

        self.statistics.old_unique_taxa = old_taxa.len();
        self.statistics.new_unique_taxa = new_taxa.len();
        self.statistics.added_taxa = new_taxa.difference(&old_taxa).count();
        self.statistics.removed_taxa = old_taxa.difference(&new_taxa).count();
    }
}

#[derive(Debug, Clone, Default)]
pub struct DatabaseStatistics {
    pub old_total_length: usize,
    pub new_total_length: usize,
    pub old_avg_length: usize,
    pub new_avg_length: usize,
    pub old_unique_taxa: usize,
    pub new_unique_taxa: usize,
    pub added_taxa: usize,
    pub removed_taxa: usize,
}

#[derive(Debug, Clone)]
pub struct SequenceInfo {
    pub id: String,
    pub description: Option<String>,
    pub length: usize,
    pub taxon_id: Option<u32>,
}

impl SequenceInfo {
    fn from_sequence(seq: &Sequence) -> Self {
        Self {
            id: seq.id.clone(),
            description: seq.description.clone(),
            length: seq.sequence.len(),
            taxon_id: seq.taxon_id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModifiedSequence {
    pub old: SequenceInfo,
    pub new: SequenceInfo,
    pub similarity: f64,
    pub changes: Vec<SequenceChange>,
}

#[derive(Debug, Clone)]
pub struct RenamedSequence {
    pub old_id: String,
    pub new_id: String,
    pub old_description: Option<String>,
    pub new_description: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SequenceChange {
    HeaderChanged,
    Extended(usize),
    Truncated(usize),
    Mutations(usize),
}
