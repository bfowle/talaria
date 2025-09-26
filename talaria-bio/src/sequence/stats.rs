use super::types::{Sequence, SequenceType};
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
                    SequenceType::Nucleotide | SequenceType::DNA | SequenceType::RNA => {
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
                    SequenceType::Unknown => {
                        // For unknown sequences, just count characters
                        if upper == b'-' {
                            local_gaps += 1;
                        } else if upper == b'N' || upper == b'X' {
                            local_ambiguous += 1;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_sequences() -> Vec<Sequence> {
        vec![
            // DNA sequences with different characteristics
            Sequence::new("seq1".to_string(), b"ATGCATGCATGC".to_vec()),  // 50% GC
            Sequence::new("seq2".to_string(), b"GGGGCCCC".to_vec()),      // 100% GC
            Sequence::new("seq3".to_string(), b"AAAATTTT".to_vec()),      // 0% GC
            Sequence::new("seq4".to_string(), b"ATGCNATGC".to_vec()),     // With ambiguous
            Sequence::new("seq5".to_string(), b"ATGC-ATGC".to_vec()),     // With gap
            // Protein sequences
            Sequence::new("seq6".to_string(), b"ACDEFGHIKLMNPQRSTVWY".to_vec()),
            Sequence::new("seq7".to_string(), b"AAAAAAAA".to_vec()),      // Low complexity
            // Various lengths
            Sequence::new("seq8".to_string(), b"AT".to_vec()),            // Very short
            Sequence::new("seq9".to_string(), b"ATGC".repeat(250)), // Long
        ]
    }

    #[test]
    fn test_basic_stats_calculation() {
        let sequences = create_test_sequences();
        let stats = SequenceStats::calculate(&sequences);

        assert_eq!(stats.total_sequences, 9);
        assert!(stats.total_length > 0);
        assert!(stats.average_length > 0.0);
        assert!(stats.min_length <= stats.max_length);
        assert!(stats.median_length > 0);
    }

    #[test]
    fn test_empty_sequences() {
        let sequences = vec![];
        let stats = SequenceStats::calculate(&sequences);

        assert_eq!(stats.total_sequences, 0);
        assert_eq!(stats.total_length, 0);
        assert_eq!(stats.average_length, 0.0);
        assert_eq!(stats.n50, 0);
        assert_eq!(stats.n90, 0);
    }

    #[test]
    fn test_gc_content_calculation() {
        let sequences = vec![
            Sequence::new("all_gc".to_string(), b"GGGGCCCC".to_vec()),
            Sequence::new("no_gc".to_string(), b"AAAATTTT".to_vec()),
            Sequence::new("half_gc".to_string(), b"ATGCATGC".to_vec()),
        ];

        let stats = SequenceStats::calculate(&sequences);

        // Overall GC content should be (8 + 0 + 4) / (8 + 8 + 8) = 12/24 = 50%
        assert!((stats.gc_content - 50.0).abs() < 0.1);
        assert!((stats.at_content - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_individual_sequence_gc_content() {
        let seq1 = Sequence::new("test".to_string(), b"GGCC".to_vec());
        assert_eq!(seq1.gc_content(), 100.0);

        let seq2 = Sequence::new("test".to_string(), b"AATT".to_vec());
        assert_eq!(seq2.gc_content(), 0.0);

        let seq3 = Sequence::new("test".to_string(), b"ATGC".to_vec());
        assert_eq!(seq3.gc_content(), 50.0);

        let seq4 = Sequence::new("test".to_string(), b"ATGCN-XX".to_vec());
        assert_eq!(seq4.gc_content(), 50.0); // Only counts ATGC
    }

    #[test]
    fn test_n50_calculation() {
        // Create sequences with specific lengths for predictable N50
        let sequences = vec![
            Sequence::new("s1".to_string(), vec![b'A'; 100]),
            Sequence::new("s2".to_string(), vec![b'A'; 200]),
            Sequence::new("s3".to_string(), vec![b'A'; 300]),
            Sequence::new("s4".to_string(), vec![b'A'; 400]),
        ];

        let stats = SequenceStats::calculate(&sequences);

        // Total length = 1000, half = 500
        // N50 should be 300 (sequences ≥300 cover >50% of total)
        assert_eq!(stats.n50, 300);
    }

    #[test]
    fn test_nx_calculation() {
        let lengths = vec![100, 200, 300, 400];
        let total = 1000;

        assert_eq!(calculate_nx(&lengths, total, 50), 300);
        assert_eq!(calculate_nx(&lengths, total, 90), 200);  // 90% of 1000 = 900, need 200+300+400
        assert_eq!(calculate_nx(&lengths, total, 10), 400);
    }

    #[test]
    fn test_shannon_entropy() {
        // Equal frequencies = maximum entropy
        let mut frequencies = HashMap::new();
        frequencies.insert(b'A', 25.0);
        frequencies.insert(b'T', 25.0);
        frequencies.insert(b'G', 25.0);
        frequencies.insert(b'C', 25.0);

        let entropy = calculate_shannon_entropy(&frequencies);
        assert!((entropy - 2.0).abs() < 0.01); // log2(4) = 2

        // Single base = minimum entropy
        let mut single = HashMap::new();
        single.insert(b'A', 100.0);

        let entropy_single = calculate_shannon_entropy(&single);
        assert_eq!(entropy_single, 0.0);
    }

    #[test]
    fn test_simpson_diversity() {
        let lengths = vec![100, 100, 100, 100];
        let diversity = calculate_simpson_diversity(&lengths);
        assert!((diversity - 0.75).abs() < 0.01); // 1 - 4*(0.25^2) = 0.75

        let single = vec![1000];
        let diversity_single = calculate_simpson_diversity(&single);
        assert_eq!(diversity_single, 0.0); // No diversity

        let empty: Vec<usize> = vec![];
        let diversity_empty = calculate_simpson_diversity(&empty);
        assert_eq!(diversity_empty, 0.0);
    }

    #[test]
    fn test_length_distribution() {
        let lengths = vec![50, 150, 750, 1500, 7500, 15000];
        let distribution = calculate_length_distribution(&lengths);

        assert_eq!(distribution[0].1, 1); // 0-100: 50
        assert_eq!(distribution[1].1, 1); // 100-500: 150
        assert_eq!(distribution[2].1, 1); // 500-1k: 750
        assert_eq!(distribution[3].1, 1); // 1k-5k: 1500
        assert_eq!(distribution[4].1, 1); // 5k-10k: 7500
        assert_eq!(distribution[5].1, 1); // >10k: 15000
    }

    #[test]
    fn test_gc_distribution() {
        let gc_values = vec![10.0, 30.0, 50.0, 70.0, 90.0];
        let distribution = calculate_gc_distribution_cached(&gc_values);

        assert_eq!(distribution[0].1, 1); // 0-20%: 10.0
        assert_eq!(distribution[1].1, 1); // 20-40%: 30.0
        assert_eq!(distribution[2].1, 1); // 40-60%: 50.0
        assert_eq!(distribution[3].1, 1); // 60-80%: 70.0
        assert_eq!(distribution[4].1, 1); // 80-100%: 90.0
    }

    #[test]
    fn test_ambiguous_bases_and_gaps() {
        let sequences = vec![
            Sequence::new("s1".to_string(), b"ATGCNNNN".to_vec()),    // 4 ambiguous
            Sequence::new("s2".to_string(), b"ATGC----".to_vec()),    // 4 gaps
            Sequence::new("s3".to_string(), b"ACDEFX**".to_vec()),    // 3 ambiguous protein
        ];

        let stats = SequenceStats::calculate(&sequences);

        assert_eq!(stats.ambiguous_bases, 7); // 4 N's + 1 X + 2 *'s
        assert_eq!(stats.gap_count, 4);
    }

    #[test]
    fn test_nucleotide_frequencies() {
        let sequences = vec![
            Sequence::new("s1".to_string(), b"AAAA".to_vec()),
            Sequence::new("s2".to_string(), b"TTTT".to_vec()),
            Sequence::new("s3".to_string(), b"GGGG".to_vec()),
            Sequence::new("s4".to_string(), b"CCCC".to_vec()),
        ];

        let stats = SequenceStats::calculate(&sequences);

        // Each base should be 25%
        assert!((stats.nucleotide_frequencies.get(&b'A').unwrap() - 25.0).abs() < 0.1);
        assert!((stats.nucleotide_frequencies.get(&b'T').unwrap() - 25.0).abs() < 0.1);
        assert!((stats.nucleotide_frequencies.get(&b'G').unwrap() - 25.0).abs() < 0.1);
        assert!((stats.nucleotide_frequencies.get(&b'C').unwrap() - 25.0).abs() < 0.1);
    }

    #[test]
    fn test_amino_acid_frequencies() {
        let sequences = vec![
            Sequence::new("p1".to_string(), b"EEEEEEEE".to_vec()), // Glutamate - clearly protein (E is in protein_chars)
            Sequence::new("p2".to_string(), b"FFFFFFFF".to_vec()), // Phenylalanine - clearly protein (F is in protein_chars)
        ];

        let stats = SequenceStats::calculate(&sequences);

        // Each AA should be 50%
        assert!((stats.amino_acid_frequencies.get(&b'E').unwrap() - 50.0).abs() < 0.1);
        assert!((stats.amino_acid_frequencies.get(&b'F').unwrap() - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_type_distribution() {
        let sequences = vec![
            Sequence::new("dna1".to_string(), b"ATGCATGC".to_vec()),
            Sequence::new("dna2".to_string(), b"GCTAGCTA".to_vec()),
            Sequence::new("prot1".to_string(), b"ACDEFGHIKLMNPQRSTVWY".to_vec()),
        ];

        let stats = SequenceStats::calculate(&sequences);

        assert_eq!(*stats.type_distribution.get(&SequenceType::Nucleotide).unwrap_or(&0), 2);
        assert_eq!(*stats.type_distribution.get(&SequenceType::Protein).unwrap_or(&0), 1);
        assert_eq!(stats.primary_type, SequenceType::Nucleotide);
    }

    #[test]
    fn test_low_complexity_estimation() {
        let sequences = vec![
            Sequence::new("low".to_string(), b"A".repeat(100).to_vec()), // Very low complexity
            Sequence::new("high".to_string(), b"ATGCATGCATGCATGCATGC".to_vec()), // High complexity
        ];

        let low_complexity = estimate_low_complexity(&sequences);

        // First sequence should contribute to low complexity score
        assert!(low_complexity > 0.0);
    }

    #[test]
    fn test_case_insensitive_processing() {
        let sequences = vec![
            Sequence::new("mixed".to_string(), b"atgcATGC".to_vec()),
        ];

        let stats = SequenceStats::calculate(&sequences);

        // Should process both uppercase and lowercase correctly
        assert_eq!(stats.gc_content, 50.0);
    }

    #[test]
    fn test_rna_sequences() {
        let sequences = vec![
            Sequence::new("rna".to_string(), b"AUGCAUGC".to_vec()),
        ];

        let stats = SequenceStats::calculate(&sequences);

        // Should handle U as T
        assert!(stats.nucleotide_frequencies.contains_key(&b'U'));
        assert!(stats.at_content > 0.0);
    }

    #[test]
    fn test_with_progress() {
        let sequences = create_test_sequences();

        // Should not panic with progress bar
        let stats = SequenceStats::calculate_with_progress(&sequences, true);
        assert_eq!(stats.total_sequences, 9);

        // Should work without progress bar too
        let stats_no_progress = SequenceStats::calculate_with_progress(&sequences, false);
        assert_eq!(stats_no_progress.total_sequences, 9);
    }

    #[test]
    fn test_median_calculation() {
        let sequences = vec![
            Sequence::new("s1".to_string(), vec![b'A'; 100]),
            Sequence::new("s2".to_string(), vec![b'A'; 200]),
            Sequence::new("s3".to_string(), vec![b'A'; 300]),
        ];

        let stats = SequenceStats::calculate(&sequences);
        assert_eq!(stats.median_length, 200);

        // Test with even number
        let sequences_even = vec![
            Sequence::new("s1".to_string(), vec![b'A'; 100]),
            Sequence::new("s2".to_string(), vec![b'A'; 200]),
            Sequence::new("s3".to_string(), vec![b'A'; 300]),
            Sequence::new("s4".to_string(), vec![b'A'; 400]),
        ];

        let stats_even = SequenceStats::calculate(&sequences_even);
        assert_eq!(stats_even.median_length, 300); // Takes upper middle value
    }

    #[test]
    fn test_parallel_consistency() {
        // Ensure parallel processing gives consistent results
        let sequences = create_test_sequences();

        let stats1 = SequenceStats::calculate(&sequences);
        let stats2 = SequenceStats::calculate(&sequences);

        assert_eq!(stats1.total_sequences, stats2.total_sequences);
        assert_eq!(stats1.total_length, stats2.total_length);
        assert!((stats1.gc_content - stats2.gc_content).abs() < 0.01);
        assert_eq!(stats1.n50, stats2.n50);
    }
}
