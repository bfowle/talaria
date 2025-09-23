/// Generic index optimization
use talaria_bio::sequence::Sequence;

#[allow(dead_code)]
pub struct GenericOptimizer;

impl GenericOptimizer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }

    #[allow(dead_code)]
    pub fn optimize(&self, sequences: &mut Vec<Sequence>) {
        // Generic optimization: sort by length for better cache locality
        sequences.sort_by_key(|s| std::cmp::Reverse(s.len()));
    }
}
