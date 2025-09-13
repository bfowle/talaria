/// Generic index optimization

use crate::bio::sequence::Sequence;

pub struct GenericOptimizer;

impl GenericOptimizer {
    pub fn new() -> Self {
        Self
    }
    
    pub fn optimize(&self, sequences: &mut Vec<Sequence>) {
        // Generic optimization: sort by length for better cache locality
        sequences.sort_by_key(|s| std::cmp::Reverse(s.len()));
    }
}