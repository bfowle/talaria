/// Kraken-specific optimizations

use crate::bio::sequence::Sequence;

pub struct KrakenOptimizer;

impl KrakenOptimizer {
    pub fn new() -> Self {
        Self
    }
    
    pub fn optimize_for_kraken(&self, sequences: &mut Vec<Sequence>) {
        // Kraken uses k-mers, so ensure good k-mer coverage
        // Sort by taxonomy for better k-mer locality
        sequences.sort_by_key(|s| (s.taxon_id.unwrap_or(0), s.len()));
    }
}