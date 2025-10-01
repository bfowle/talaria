use std::collections::{HashMap, HashSet};
/// MMseqs2-specific optimizations for sensitive protein search
///
/// MMseqs2 uses profile searches and cascaded clustering.
/// Our optimization focuses on:
/// 1. Profile-aware reduction preserving HMM diversity
/// 2. Selecting cluster representatives at multiple identity levels
/// 3. Optimizing for k-mer prefiltering
/// 4. Maintaining sensitivity levels (s1-s7.5)
use talaria_bio::sequence::Sequence;

#[allow(dead_code)]
pub struct MMseqs2Optimizer {
    /// Clustering steps for cascaded clustering
    clustering_steps: Vec<f64>,
    /// Sensitivity level (1.0 to 7.5)
    sensitivity: f64,
    /// Whether to optimize for profile searches
    profile_mode: bool,
    /// K-mer size for prefiltering
    kmer_size: usize,
}

impl MMseqs2Optimizer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            // MMseqs2 default cascaded clustering: 90%, 70%, 50%, 30%
            clustering_steps: vec![0.9, 0.7, 0.5, 0.3],
            sensitivity: 5.7, // Default sensitivity
            profile_mode: false,
            kmer_size: 7, // Default k-mer size
        }
    }

    #[allow(dead_code)]
    pub fn with_sensitivity(mut self, sensitivity: f64) -> Self {
        self.sensitivity = sensitivity.clamp(1.0, 7.5);
        // Adjust k-mer size based on sensitivity
        self.kmer_size = if sensitivity < 4.0 {
            6
        } else if sensitivity < 6.0 {
            7
        } else {
            8
        };
        self
    }

    #[allow(dead_code)]
    pub fn with_profile_mode(mut self, enabled: bool) -> Self {
        self.profile_mode = enabled;
        self
    }

    #[allow(dead_code)]
    pub fn with_clustering_steps(mut self, steps: Vec<f64>) -> Self {
        self.clustering_steps = steps;
        self
    }

    #[allow(dead_code)]
    pub fn optimize_for_mmseqs2(&self, sequences: &mut Vec<Sequence>) {
        // MMseqs2-specific optimizations

        // 1. Apply cascaded clustering representation
        self.apply_cascaded_clustering(sequences);

        // 2. Optimize k-mer representation for prefiltering
        self.optimize_kmer_representation(sequences);

        // 3. If profile mode, ensure profile diversity
        if self.profile_mode {
            self.ensure_profile_diversity(sequences);
        }

        // 4. Sort for optimal memory access patterns
        self.sort_for_memory_efficiency(sequences);
    }

    /// Apply MMseqs2's cascaded clustering approach
    #[allow(dead_code)]
    fn apply_cascaded_clustering(&self, sequences: &mut Vec<Sequence>) {
        let mut representatives = Vec::new();
        let mut remaining = sequences.clone();

        for &threshold in &self.clustering_steps {
            let (reps, rest) = self.cluster_at_threshold(&remaining, threshold);
            representatives.extend(reps);
            remaining = rest;

            // Stop if we've selected enough representatives
            if representatives.len() >= sequences.len() / 3 {
                break;
            }
        }

        // Add some remaining sequences to maintain diversity
        let diversity_count = (sequences.len() / 10).min(remaining.len());
        representatives.extend(remaining.into_iter().take(diversity_count));

        *sequences = representatives;
    }

    /// Cluster sequences at a specific identity threshold
    #[allow(dead_code)]
    fn cluster_at_threshold(
        &self,
        sequences: &[Sequence],
        threshold: f64,
    ) -> (Vec<Sequence>, Vec<Sequence>) {
        let mut representatives = Vec::new();
        let mut non_representatives = Vec::new();
        let mut clustered = HashSet::new();

        // Sort by length for better clustering
        let mut sorted_seqs = sequences.to_vec();
        sorted_seqs.sort_by_key(|s| std::cmp::Reverse(s.len()));

        for seq in sorted_seqs {
            if clustered.contains(&seq.id) {
                continue;
            }

            // This sequence becomes a representative
            representatives.push(seq.clone());
            clustered.insert(seq.id.clone());

            // Find similar sequences (simplified similarity check)
            for other in sequences {
                if !clustered.contains(&other.id) && self.is_similar(&seq, other, threshold) {
                    clustered.insert(other.id.clone());
                    non_representatives.push(other.clone());
                }
            }
        }

        (representatives, non_representatives)
    }

    /// Check if two sequences are similar (simplified version)
    #[allow(dead_code)]
    fn is_similar(&self, seq1: &Sequence, seq2: &Sequence, threshold: f64) -> bool {
        // Quick length check
        let len_ratio = seq1.len().min(seq2.len()) as f64 / seq1.len().max(seq2.len()) as f64;
        if len_ratio < threshold * 0.8 {
            return false;
        }

        // K-mer similarity check
        let kmers1 = self.extract_kmers(&seq1.sequence);
        let kmers2 = self.extract_kmers(&seq2.sequence);

        let intersection = kmers1.intersection(&kmers2).count();
        let union = kmers1.len() + kmers2.len() - intersection;

        if union == 0 {
            return false;
        }

        (intersection as f64 / union as f64) >= threshold * 0.7
    }

    /// Extract k-mers for similarity computation
    #[allow(dead_code)]
    fn extract_kmers(&self, sequence: &[u8]) -> HashSet<Vec<u8>> {
        if sequence.len() < self.kmer_size {
            return HashSet::new();
        }

        sequence
            .windows(self.kmer_size)
            .map(|w| w.to_vec())
            .collect()
    }

    /// Optimize k-mer representation for MMseqs2's prefiltering
    #[allow(dead_code)]
    fn optimize_kmer_representation(&self, sequences: &mut Vec<Sequence>) {
        // Count k-mer frequencies across all sequences
        let mut kmer_counts: HashMap<Vec<u8>, usize> = HashMap::new();

        for seq in sequences.iter() {
            for kmer in self.extract_kmers(&seq.sequence) {
                *kmer_counts.entry(kmer).or_insert(0) += 1;
            }
        }

        // Score sequences by k-mer diversity
        let scores: Vec<(usize, f64)> = sequences
            .iter()
            .enumerate()
            .map(|(i, seq)| {
                let kmers = self.extract_kmers(&seq.sequence);
                let score = kmers
                    .iter()
                    .map(|k| {
                        let count = kmer_counts.get(k).copied().unwrap_or(1);
                        1.0 / count as f64 // Rare k-mers are more valuable
                    })
                    .sum::<f64>();
                (i, score)
            })
            .collect();

        // Sort by k-mer diversity score
        let mut scored_sequences: Vec<_> = scores
            .into_iter()
            .map(|(i, score)| (score, sequences[i].clone()))
            .collect();
        scored_sequences.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

        *sequences = scored_sequences.into_iter().map(|(_, seq)| seq).collect();
    }

    /// Ensure profile diversity for profile searches
    #[allow(dead_code)]
    fn ensure_profile_diversity(&self, sequences: &mut Vec<Sequence>) {
        // Group sequences by similar lengths (profiles work better with similar lengths)
        let mut length_groups: HashMap<usize, Vec<Sequence>> = HashMap::new();

        for seq in sequences.drain(..) {
            let length_bin = (seq.len() / 50) * 50; // Bin by 50 AA increments
            length_groups.entry(length_bin).or_default().push(seq);
        }

        // Select representatives from each length group
        for (_, group) in length_groups {
            let representatives = self.select_profile_representatives(group);
            sequences.extend(representatives);
        }
    }

    /// Select diverse representatives for profile building
    #[allow(dead_code)]
    fn select_profile_representatives(&self, mut group: Vec<Sequence>) -> Vec<Sequence> {
        if group.len() <= 3 {
            return group;
        }

        // Sort by sequence diversity (simplified)
        group.sort_by_cached_key(|seq| {
            let unique_chars = seq.sequence.iter().collect::<HashSet<_>>().len();
            std::cmp::Reverse(unique_chars)
        });

        // Take diverse subset
        let count = (group.len() as f64 * 0.4).ceil() as usize;
        group.into_iter().take(count).collect()
    }

    /// Sort sequences for optimal memory access in MMseqs2
    #[allow(dead_code)]
    fn sort_for_memory_efficiency(&self, sequences: &mut Vec<Sequence>) {
        // MMseqs2 benefits from sequences sorted by length within taxonomic groups
        sequences.sort_by_key(|seq| (seq.taxon_id.unwrap_or(0), seq.len()));
    }

    /// Get MMseqs2-specific parameters
    #[allow(dead_code)]
    pub fn get_reduction_params(&self) -> MMseqs2Params {
        MMseqs2Params {
            sensitivity: self.sensitivity,
            clustering_mode: if self.clustering_steps.len() > 1 {
                ClusteringMode::Cascaded(self.clustering_steps.clone())
            } else {
                ClusteringMode::Single(self.clustering_steps[0])
            },
            kmer_size: self.kmer_size,
            profile_mode: self.profile_mode,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct MMseqs2Params {
    pub sensitivity: f64,
    pub clustering_mode: ClusteringMode,
    pub kmer_size: usize,
    pub profile_mode: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum ClusteringMode {
    Single(f64),
    Cascaded(Vec<f64>),
}

impl Default for MMseqs2Optimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate MMseqs2 database creation commands
#[allow(dead_code)]
pub fn generate_mmseqs2_commands(fasta_path: &str, params: &MMseqs2Params) -> Vec<String> {
    let mut commands = Vec::new();
    let db_name = fasta_path.replace(".fasta", "").replace(".fa", "");

    // Create MMseqs2 database
    commands.push(format!("mmseqs createdb {} {}_db", fasta_path, db_name));

    // Create index with specified sensitivity
    commands.push(format!(
        "mmseqs createindex {}_db {}_idx --sensitivity {} -k {}",
        db_name, db_name, params.sensitivity, params.kmer_size
    ));

    // Clustering command if cascaded
    if let ClusteringMode::Cascaded(ref steps) = params.clustering_mode {
        for (i, &threshold) in steps.iter().enumerate() {
            commands.push(format!(
                "mmseqs cluster {}_db {}_clu_{} {}_tmp --min-seq-id {}",
                db_name, db_name, i, db_name, threshold
            ));
        }
    }

    // Profile search command if in profile mode
    if params.profile_mode {
        commands.push(format!(
            "mmseqs search {}_db {}_db {}_results {}_tmp -s {} --num-iterations 3",
            db_name, db_name, db_name, db_name, params.sensitivity
        ));
    } else {
        commands.push(format!(
            "mmseqs search {}_db {}_db {}_results {}_tmp -s {}",
            db_name, db_name, db_name, db_name, params.sensitivity
        ));
    }

    // Convert results to readable format
    commands.push(format!(
        "mmseqs convertalis {}_db {}_db {}_results {}_results.m8",
        db_name, db_name, db_name, db_name
    ));

    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clustering_steps() {
        let optimizer = MMseqs2Optimizer::new();
        assert_eq!(optimizer.clustering_steps, vec![0.9, 0.7, 0.5, 0.3]);
    }

    #[test]
    fn test_sensitivity_clamping() {
        let optimizer = MMseqs2Optimizer::new().with_sensitivity(10.0);
        assert_eq!(optimizer.sensitivity, 7.5);

        let optimizer = MMseqs2Optimizer::new().with_sensitivity(0.5);
        assert_eq!(optimizer.sensitivity, 1.0);
    }
}
