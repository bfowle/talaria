use crate::sequence::{Sequence, SequenceType};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::Mutex;

/// Comprehensive statistics for biological sequences
#[derive(Clone)]
pub struct SequenceStats {
    // Basic metrics
    pub total_sequences: usize,
    pub total_length: usize,
    pub average_length: f64,
    pub median_length: usize,
    pub min_length: usize,
    pub max_length: usize,
    pub n50: usize,
    pub n90: usize,

    // Composition
    pub gc_content: f64,
    pub at_content: f64,
    pub nucleotide_frequencies: HashMap<u8, f64>,
    pub amino_acid_frequencies: HashMap<u8, f64>,

    // Complexity
    pub shannon_entropy: f64,
    pub simpson_diversity: f64,
    pub low_complexity_percentage: f64,
    pub ambiguous_bases: usize,
    pub gap_count: usize,

    // Distribution
    pub length_distribution: Vec<(String, usize)>,
    pub gc_distribution: Vec<(String, usize)>,
    pub type_distribution: HashMap<SequenceType, usize>,
    pub primary_type: SequenceType, // Primary sequence type in dataset

    // Internal cache for performance
    sequence_gc_values: Vec<f64>,
}

impl std::fmt::Debug for SequenceStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SequenceStats")
            .field("total_sequences", &self.total_sequences)
            .field("total_length", &self.total_length)
            .field("average_length", &self.average_length)
            .field("median_length", &self.median_length)
            .field("min_length", &self.min_length)
            .field("max_length", &self.max_length)
            .field("n50", &self.n50)
            .field("n90", &self.n90)
            .field("gc_content", &self.gc_content)
            .field("at_content", &self.at_content)
            .field("nucleotide_frequencies", &self.nucleotide_frequencies)
            .field("amino_acid_frequencies", &self.amino_acid_frequencies)
            .field("shannon_entropy", &self.shannon_entropy)
            .field("simpson_diversity", &self.simpson_diversity)
            .field("low_complexity_percentage", &self.low_complexity_percentage)
            .field("ambiguous_bases", &self.ambiguous_bases)
            .field("gap_count", &self.gap_count)
            .field("length_distribution", &self.length_distribution)
            .field("gc_distribution", &self.gc_distribution)
            .field("type_distribution", &self.type_distribution)
            .field("primary_type", &self.primary_type)
            .finish()
    }
}

impl SequenceStats {
    pub fn calculate(sequences: &[Sequence]) -> Self {
        Self::calculate_with_progress(sequences, false)
    }

    pub fn calculate_with_progress(sequences: &[Sequence], show_progress: bool) -> Self {
        let mut stats = Self::default();

        if sequences.is_empty() {
            return stats;
        }

        // Create progress bar if requested
        let pb = if show_progress {
            let pb = ProgressBar::new(sequences.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("##-"),
            );
            pb.set_message("Calculating statistics...");
            Some(pb)
        } else {
            None
        };

        // Basic metrics
        stats.total_sequences = sequences.len();

        let mut lengths: Vec<usize> = sequences.iter().map(|s| s.len()).collect();
        lengths.sort_unstable();

        stats.total_length = lengths.iter().sum();
        stats.average_length = stats.total_length as f64 / stats.total_sequences as f64;
        stats.min_length = *lengths.first().unwrap_or(&0);
        stats.max_length = *lengths.last().unwrap_or(&0);
        stats.median_length = lengths[lengths.len() / 2];

        // Calculate N50 and N90
        stats.n50 = calculate_nx(&lengths, stats.total_length, 50);
        stats.n90 = calculate_nx(&lengths, stats.total_length, 90);

        // Parallel composition analysis using Rayon
        let total_gc = Mutex::new(0usize);
        let total_at = Mutex::new(0usize);
        let nuc_counts = Mutex::new(HashMap::new());
        let aa_counts = Mutex::new(HashMap::new());
        let type_counts = Mutex::new(HashMap::new());
        let gap_count = Mutex::new(0usize);
        let ambiguous_bases = Mutex::new(0usize);
        let gc_values = Mutex::new(Vec::with_capacity(sequences.len()));

        sequences.par_iter().enumerate().for_each(|(i, seq)| {
            let seq_type = seq.detect_type();
            type_counts
                .lock()
                .unwrap()
                .entry(seq_type)
                .and_modify(|e| *e += 1)
                .or_insert(1);

            let mut local_gc = 0;
            let mut local_at = 0;
            let mut local_gaps = 0;
            let mut local_ambiguous = 0;
            let mut local_nuc_counts = HashMap::new();
            let mut local_aa_counts = HashMap::new();
            let mut seq_total_bases = 0;

            for &base in &seq.sequence {
                let upper = base.to_ascii_uppercase();

                match seq_type {
                    SequenceType::Nucleotide => {
                        *local_nuc_counts.entry(upper).or_insert(0) += 1;
                        match upper {
                            b'G' | b'C' => {
                                local_gc += 1;
                                seq_total_bases += 1;
                            }
                            b'A' | b'T' | b'U' => {
                                local_at += 1;
                                seq_total_bases += 1;
                            }
                            b'-' => local_gaps += 1,
                            b'N' | b'X' => local_ambiguous += 1,
                            _ => {}
                        }
                    }
                    SequenceType::Protein => {
                        *local_aa_counts.entry(upper).or_insert(0) += 1;
                        if upper == b'X' || upper == b'*' {
                            local_ambiguous += 1;
                        }
                        if upper == b'-' {
                            local_gaps += 1;
                        }
                    }
                }
            }

            // Calculate and store per-sequence GC content
            let seq_gc = if seq_total_bases > 0 {
                (local_gc as f64 / seq_total_bases as f64) * 100.0
            } else {
                0.0
            };
            gc_values.lock().unwrap().push((i, seq_gc));

            // Update global counters
            *total_gc.lock().unwrap() += local_gc;
            *total_at.lock().unwrap() += local_at;
            *gap_count.lock().unwrap() += local_gaps;
            *ambiguous_bases.lock().unwrap() += local_ambiguous;

            // Merge local counts into global
            for (k, v) in local_nuc_counts {
                *nuc_counts.lock().unwrap().entry(k).or_insert(0) += v;
            }
            for (k, v) in local_aa_counts {
                *aa_counts.lock().unwrap().entry(k).or_insert(0) += v;
            }

            if let Some(ref pb) = pb {
                if i % 100 == 0 {
                    pb.set_position(i as u64);
                }
            }
        });

        if let Some(ref pb) = pb {
            pb.finish_with_message("Statistics calculated!");
        }

        // Extract values from mutexes
        let total_gc = *total_gc.lock().unwrap();
        let total_at = *total_at.lock().unwrap();
        let nuc_counts = nuc_counts.into_inner().unwrap();
        let aa_counts = aa_counts.into_inner().unwrap();
        let type_counts = type_counts.into_inner().unwrap();
        stats.gap_count = *gap_count.lock().unwrap();
        stats.ambiguous_bases = *ambiguous_bases.lock().unwrap();
        let mut gc_values = gc_values.into_inner().unwrap();
        gc_values.sort_by_key(|&(i, _)| i);
        stats.sequence_gc_values = gc_values.into_iter().map(|(_, gc)| gc).collect();

        // Calculate frequencies
        if total_gc + total_at > 0 {
            stats.gc_content = (total_gc as f64 / (total_gc + total_at) as f64) * 100.0;
            stats.at_content = (total_at as f64 / (total_gc + total_at) as f64) * 100.0;
        }

        let total_nucs: usize = nuc_counts.values().sum();
        if total_nucs > 0 {
            for (nuc, count) in nuc_counts {
                stats
                    .nucleotide_frequencies
                    .insert(nuc, (count as f64 / total_nucs as f64) * 100.0);
            }
        }

        let total_aas: usize = aa_counts.values().sum();
        if total_aas > 0 {
            for (aa, count) in aa_counts {
                stats
                    .amino_acid_frequencies
                    .insert(aa, (count as f64 / total_aas as f64) * 100.0);
            }
        }

        // Calculate Shannon entropy based on sequence type
        // Check if majority are protein or nucleotide sequences
        let protein_count = type_counts.get(&SequenceType::Protein).unwrap_or(&0);
        let nucleotide_count = type_counts.get(&SequenceType::Nucleotide).unwrap_or(&0);

        if protein_count > nucleotide_count && !stats.amino_acid_frequencies.is_empty() {
            stats.shannon_entropy = calculate_shannon_entropy(&stats.amino_acid_frequencies);
        } else if !stats.nucleotide_frequencies.is_empty() {
            stats.shannon_entropy = calculate_shannon_entropy(&stats.nucleotide_frequencies);
        } else {
            stats.shannon_entropy = 0.0;
        }

        // Calculate Simpson's diversity
        stats.simpson_diversity = calculate_simpson_diversity(&lengths);

        // Length distribution
        stats.length_distribution = calculate_length_distribution(&lengths);

        // GC distribution (using cached values)
        stats.gc_distribution = calculate_gc_distribution_cached(&stats.sequence_gc_values);

        // Low complexity regions (sample-based)
        if let Some(ref pb) = pb {
            pb.set_message("Estimating sequence complexity...");
        }
        stats.low_complexity_percentage = estimate_low_complexity(sequences);

        stats.type_distribution = type_counts.clone();

        // Determine primary sequence type
        let protein_count = type_counts.get(&SequenceType::Protein).unwrap_or(&0);
        let nucleotide_count = type_counts.get(&SequenceType::Nucleotide).unwrap_or(&0);
        stats.primary_type = if protein_count > nucleotide_count {
            SequenceType::Protein
        } else {
            SequenceType::Nucleotide
        };

        stats
    }
}

impl Default for SequenceStats {
    fn default() -> Self {
        Self {
            total_sequences: 0,
            total_length: 0,
            average_length: 0.0,
            median_length: 0,
            min_length: 0,
            max_length: 0,
            n50: 0,
            n90: 0,
            gc_content: 0.0,
            at_content: 0.0,
            nucleotide_frequencies: HashMap::new(),
            amino_acid_frequencies: HashMap::new(),
            shannon_entropy: 0.0,
            simpson_diversity: 0.0,
            low_complexity_percentage: 0.0,
            ambiguous_bases: 0,
            gap_count: 0,
            length_distribution: Vec::new(),
            gc_distribution: Vec::new(),
            type_distribution: HashMap::new(),
            primary_type: SequenceType::Nucleotide,
            sequence_gc_values: Vec::new(),
        }
    }
}

fn calculate_nx(lengths: &[usize], total_length: usize, percentage: usize) -> usize {
    let target = (total_length as f64 * percentage as f64 / 100.0) as usize;
    let mut cumulative = 0;

    for &length in lengths.iter().rev() {
        cumulative += length;
        if cumulative >= target {
            return length;
        }
    }

    0
}

fn calculate_shannon_entropy(frequencies: &HashMap<u8, f64>) -> f64 {
    let mut entropy = 0.0;

    for &freq in frequencies.values() {
        if freq > 0.0 {
            let p = freq / 100.0;
            entropy -= p * p.log2();
        }
    }

    entropy
}

fn calculate_simpson_diversity(lengths: &[usize]) -> f64 {
    if lengths.is_empty() {
        return 0.0;
    }

    let total: usize = lengths.iter().sum();
    let mut sum_squares = 0.0;

    for &length in lengths {
        let proportion = length as f64 / total as f64;
        sum_squares += proportion * proportion;
    }

    1.0 - sum_squares
}

fn calculate_length_distribution(lengths: &[usize]) -> Vec<(String, usize)> {
    let mut distribution = vec![
        ("0-100".to_string(), 0),
        ("100-500".to_string(), 0),
        ("500-1k".to_string(), 0),
        ("1k-5k".to_string(), 0),
        ("5k-10k".to_string(), 0),
        (">10k".to_string(), 0),
    ];

    for &length in lengths {
        if length < 100 {
            distribution[0].1 += 1;
        } else if length < 500 {
            distribution[1].1 += 1;
        } else if length < 1000 {
            distribution[2].1 += 1;
        } else if length < 5000 {
            distribution[3].1 += 1;
        } else if length < 10000 {
            distribution[4].1 += 1;
        } else {
            distribution[5].1 += 1;
        }
    }

    distribution
}

fn calculate_gc_distribution_cached(gc_values: &[f64]) -> Vec<(String, usize)> {
    let mut distribution = vec![
        ("0-20%".to_string(), 0),
        ("20-40%".to_string(), 0),
        ("40-60%".to_string(), 0),
        ("60-80%".to_string(), 0),
        ("80-100%".to_string(), 0),
    ];

    for &gc in gc_values {
        if gc < 20.0 {
            distribution[0].1 += 1;
        } else if gc < 40.0 {
            distribution[1].1 += 1;
        } else if gc < 60.0 {
            distribution[2].1 += 1;
        } else if gc < 80.0 {
            distribution[3].1 += 1;
        } else {
            distribution[4].1 += 1;
        }
    }

    distribution
}

fn calculate_sequence_gc(seq: &Sequence) -> f64 {
    let mut gc_count = 0;
    let mut total_count = 0;

    for &base in &seq.sequence {
        match base.to_ascii_uppercase() {
            b'G' | b'C' => {
                gc_count += 1;
                total_count += 1;
            }
            b'A' | b'T' | b'U' => {
                total_count += 1;
            }
            _ => {}
        }
    }

    if total_count > 0 {
        (gc_count as f64 / total_count as f64) * 100.0
    } else {
        0.0
    }
}

fn estimate_low_complexity(sequences: &[Sequence]) -> f64 {
    // Sample-based estimation for performance
    // For large datasets, sample up to 1000 sequences
    let sample_size = sequences.len().min(1000);
    if sample_size == 0 {
        return 0.0;
    }

    let mut low_complexity_bases = 0;
    let mut total_bases = 0;

    // Sample sequences evenly across the dataset
    let step = sequences.len().max(1) / sample_size.max(1);
    let sampled_sequences: Vec<&Sequence> = sequences
        .iter()
        .step_by(step.max(1))
        .take(sample_size)
        .collect();

    for seq in sampled_sequences {
        // Skip very short sequences
        if seq.sequence.len() < 20 {
            continue;
        }

        // Sample windows from each sequence (not all windows)
        let window_step = (seq.sequence.len() / 100).max(1);

        for window in seq.sequence.windows(20).step_by(window_step) {
            total_bases += 1;

            // Simple low complexity detection: if >70% of window is same base
            let mut counts = [0u8; 256];
            for &base in window {
                let idx = base as usize;
                counts[idx] = counts[idx].saturating_add(1);
            }

            let max_count = *counts.iter().max().unwrap_or(&0);
            if max_count > 14 {
                // 70% of 20
                low_complexity_bases += 1;
            }
        }
    }

    if total_bases > 0 {
        (low_complexity_bases as f64 / total_bases as f64) * 100.0
    } else {
        0.0
    }
}

// Add gc_content method to Sequence for compatibility
impl Sequence {
    pub fn gc_content(&self) -> f64 {
        calculate_sequence_gc(self)
    }
}
