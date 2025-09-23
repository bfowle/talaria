/// LAMBDA-specific optimizations for index building
use talaria_bio::sequence::Sequence;

#[allow(dead_code)]
pub struct LambdaOptimizer;

impl LambdaOptimizer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }

    #[allow(dead_code)]
    pub fn optimize_for_lambda(&self, sequences: &mut Vec<Sequence>) {
        // Sort sequences by taxon ID for better locality in LAMBDA
        sequences.sort_by_key(|s| s.taxon_id.unwrap_or(0));
    }

    #[allow(dead_code)]
    pub fn prepare_taxonomy_mapping(&self, sequences: &[Sequence]) -> Vec<(String, u32)> {
        sequences
            .iter()
            .filter_map(|s| s.taxon_id.map(|t| (s.id.clone(), t)))
            .collect()
    }
}
