/// BLAST-specific optimizations

use crate::bio::sequence::Sequence;

pub struct BlastOptimizer;

impl BlastOptimizer {
    pub fn new() -> Self {
        Self
    }
    
    pub fn optimize_for_blast(&self, sequences: &mut Vec<Sequence>) {
        // BLAST benefits from diverse sequences being well-distributed
        // Simple shuffle for diversity
        sequences.sort_by_key(|s| s.len());
    }
}