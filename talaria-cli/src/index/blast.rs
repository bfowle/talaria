/// BLAST-specific optimizations
use talaria_bio::sequence::Sequence;

#[allow(dead_code)]
pub struct BlastOptimizer;

impl BlastOptimizer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }

    #[allow(dead_code)]
    pub fn optimize_for_blast(&self, sequences: &mut Vec<Sequence>) {
        // BLAST benefits from diverse sequences being well-distributed
        // Simple shuffle for diversity
        sequences.sort_by_key(|s| s.len());
    }
}
