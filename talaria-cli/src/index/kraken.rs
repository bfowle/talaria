/// Kraken-specific optimizations
use talaria_bio::sequence::Sequence;

#[allow(dead_code)]
pub struct KrakenOptimizer;

impl KrakenOptimizer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }

    #[allow(dead_code)]
    pub fn optimize_for_kraken(&self, sequences: &mut Vec<Sequence>) {
        // Kraken uses k-mers, so ensure good k-mer coverage
        // Sort by taxonomy for better k-mer locality
        sequences.sort_by_key(|s| (s.taxon_id.unwrap_or(0), s.len()));
    }
}
