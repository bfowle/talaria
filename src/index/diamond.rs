/// Diamond-specific optimizations for fast protein alignment
///
/// Diamond uses a double-indexing strategy with seeds and reduced alphabet.
/// Our optimization focuses on:
/// 1. Maintaining seed diversity for Diamond's prefiltering
/// 2. Clustering at 90% identity to remove redundancy
/// 3. Preserving taxonomic representation for metagenomics
/// 4. Optimizing for block-aligning algorithm

use crate::bio::sequence::Sequence;
use std::collections::{HashMap, HashSet};

pub struct DiamondOptimizer {
    /// Identity threshold for clustering (Diamond default: 90%)
    clustering_threshold: f64,
    /// Minimum seed coverage to maintain
    min_seed_coverage: f64,
    /// Whether to preserve taxonomy diversity
    preserve_taxonomy: bool,
}

impl DiamondOptimizer {
    pub fn new() -> Self {
        Self {
            clustering_threshold: 0.9,
            min_seed_coverage: 0.95,
            preserve_taxonomy: true,
        }
    }
    
    pub fn with_clustering_threshold(mut self, threshold: f64) -> Self {
        self.clustering_threshold = threshold;
        self
    }
    
    pub fn with_seed_coverage(mut self, coverage: f64) -> Self {
        self.min_seed_coverage = coverage;
        self
    }
    
    pub fn optimize_for_diamond(&self, sequences: &mut Vec<Sequence>) {
        // Diamond-specific optimizations
        
        // 1. Group sequences by taxonomy for diversity preservation
        if self.preserve_taxonomy {
            self.group_by_taxonomy(sequences);
        }
        
        // 2. Sort by sequence complexity (Diamond performs better with complex sequences first)
        self.sort_by_complexity(sequences);
        
        // 3. Ensure seed diversity for Diamond's double-indexing
        self.ensure_seed_diversity(sequences);
    }
    
    /// Group sequences by taxonomy to ensure diverse representation
    fn group_by_taxonomy(&self, sequences: &mut Vec<Sequence>) {
        let mut taxonomy_groups: HashMap<u32, Vec<Sequence>> = HashMap::new();
        let mut no_taxonomy = Vec::new();
        
        // Group by taxonomy
        for seq in sequences.drain(..) {
            if let Some(taxon_id) = seq.taxon_id {
                taxonomy_groups.entry(taxon_id).or_insert_with(Vec::new).push(seq);
            } else {
                no_taxonomy.push(seq);
            }
        }
        
        // Interleave sequences from different taxonomic groups
        let max_group_size = taxonomy_groups.values().map(|v| v.len()).max().unwrap_or(0);
        
        for i in 0..max_group_size {
            for group in taxonomy_groups.values_mut() {
                if i < group.len() {
                    sequences.push(group[i].clone());
                }
            }
        }
        
        // Add sequences without taxonomy at the end
        sequences.extend(no_taxonomy);
    }
    
    /// Sort sequences by complexity (entropy) for better Diamond performance
    fn sort_by_complexity(&self, sequences: &mut Vec<Sequence>) {
        sequences.sort_by_cached_key(|seq| {
            let complexity = self.calculate_sequence_complexity(&seq.sequence);
            std::cmp::Reverse((complexity * 1000.0) as u64)
        });
    }
    
    /// Calculate sequence complexity using Shannon entropy
    fn calculate_sequence_complexity(&self, sequence: &[u8]) -> f64 {
        if sequence.is_empty() {
            return 0.0;
        }
        
        let mut freq = HashMap::new();
        for &aa in sequence {
            *freq.entry(aa).or_insert(0) += 1;
        }
        
        let len = sequence.len() as f64;
        let mut entropy = 0.0;
        
        for count in freq.values() {
            let p = *count as f64 / len;
            if p > 0.0 {
                entropy -= p * p.log2();
            }
        }
        
        entropy / 4.32 // Normalize to 0-1 range (log2(20) ≈ 4.32 for amino acids)
    }
    
    /// Ensure sufficient seed diversity for Diamond's indexing
    fn ensure_seed_diversity(&self, sequences: &mut Vec<Sequence>) {
        // Diamond uses spaced seeds of length 12-15
        const SEED_LENGTH: usize = 12;
        
        let mut seed_coverage = HashSet::new();
        let mut selected = Vec::new();
        
        // First pass: select sequences that add new seeds
        for seq in sequences.iter() {
            if seq.sequence.len() < SEED_LENGTH {
                continue;
            }
            
            let mut new_seeds = 0;
            for window in seq.sequence.windows(SEED_LENGTH) {
                let seed = window.to_vec();
                if !seed_coverage.contains(&seed) {
                    new_seeds += 1;
                }
            }
            
            // Select if adds significant new seeds
            if new_seeds > 10 || selected.is_empty() {
                for window in seq.sequence.windows(SEED_LENGTH) {
                    seed_coverage.insert(window.to_vec());
                }
                selected.push(seq.clone());
            }
        }
        
        *sequences = selected;
    }
    
    /// Get Diamond-specific parameters for reduction
    pub fn get_reduction_params(&self) -> DiamondParams {
        DiamondParams {
            sensitivity: DiamondSensitivity::Sensitive,
            block_size: 2.0, // Default Diamond block size in billions
            index_chunks: 4,  // Number of index chunks
            clustering_threshold: self.clustering_threshold,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiamondParams {
    pub sensitivity: DiamondSensitivity,
    pub block_size: f64,
    pub index_chunks: usize,
    pub clustering_threshold: f64,
}

#[derive(Debug, Clone)]
pub enum DiamondSensitivity {
    Fast,
    Mid,
    Sensitive,
    MoreSensitive,
    VerySensitive,
    UltraSensitive,
}

impl Default for DiamondOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Create optimized Diamond database commands
pub fn generate_diamond_commands(fasta_path: &str, params: &DiamondParams) -> Vec<String> {
    let mut commands = Vec::new();
    
    // Make Diamond database
    let mut makedb_cmd = format!("diamond makedb --in {} --db {}.dmnd", fasta_path, fasta_path);
    
    // Add block size if specified
    makedb_cmd.push_str(&format!(" --block-size {}", params.block_size));
    
    commands.push(makedb_cmd);
    
    // Generate example search command
    let sensitivity_flag = match params.sensitivity {
        DiamondSensitivity::Fast => "",
        DiamondSensitivity::Mid => "--mid-sensitive",
        DiamondSensitivity::Sensitive => "--sensitive",
        DiamondSensitivity::MoreSensitive => "--more-sensitive",
        DiamondSensitivity::VerySensitive => "--very-sensitive",
        DiamondSensitivity::UltraSensitive => "--ultra-sensitive",
    };
    
    let search_cmd = format!(
        "diamond blastp --db {}.dmnd --query queries.fasta --out results.m8 {}",
        fasta_path, sensitivity_flag
    );
    
    commands.push(search_cmd);
    
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_complexity_calculation() {
        let optimizer = DiamondOptimizer::new();
        
        // Uniform sequence (low complexity)
        let uniform = b"AAAAAAAAAAAAAAAAAAAA";
        let uniform_complexity = optimizer.calculate_sequence_complexity(uniform);
        assert!(uniform_complexity < 0.2);
        
        // Random sequence (high complexity)
        let random = b"ACDEFGHIKLMNPQRSTVWY";
        let random_complexity = optimizer.calculate_sequence_complexity(random);
        assert!(random_complexity > 0.8);
    }
}