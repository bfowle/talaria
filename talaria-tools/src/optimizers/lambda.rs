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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_sequences() -> Vec<Sequence> {
        vec![
            Sequence::new("seq1".to_string(), b"ATCG".to_vec()).with_taxon(100),
            Sequence::new("seq2".to_string(), b"GCTA".to_vec()).with_taxon(50),
            Sequence::new("seq3".to_string(), b"TTAA".to_vec()).with_taxon(75),
            Sequence::new("seq4".to_string(), b"CCGG".to_vec()),  // No taxon
            Sequence::new("seq5".to_string(), b"AATT".to_vec()).with_taxon(50),
        ]
    }

    #[test]
    fn test_lambda_optimizer_creation() {
        let optimizer = LambdaOptimizer::new();
        // Simple creation test
        let _ = optimizer;
    }

    #[test]
    fn test_optimize_for_lambda_sorts_by_taxon() {
        let optimizer = LambdaOptimizer::new();
        let mut sequences = create_test_sequences();

        optimizer.optimize_for_lambda(&mut sequences);

        // Should be sorted by taxon_id (None/0 first, then ascending)
        assert_eq!(sequences[0].id, "seq4"); // No taxon (treated as 0)
        assert_eq!(sequences[1].id, "seq2"); // Taxon 50
        assert_eq!(sequences[2].id, "seq5"); // Taxon 50
        assert_eq!(sequences[3].id, "seq3"); // Taxon 75
        assert_eq!(sequences[4].id, "seq1"); // Taxon 100
    }

    #[test]
    fn test_optimize_for_lambda_empty() {
        let optimizer = LambdaOptimizer::new();
        let mut sequences = Vec::new();

        optimizer.optimize_for_lambda(&mut sequences);

        assert!(sequences.is_empty());
    }

    #[test]
    fn test_optimize_for_lambda_single() {
        let optimizer = LambdaOptimizer::new();
        let mut sequences = vec![
            Sequence::new("only".to_string(), b"ATCG".to_vec()).with_taxon(42)
        ];

        optimizer.optimize_for_lambda(&mut sequences);

        assert_eq!(sequences.len(), 1);
        assert_eq!(sequences[0].id, "only");
    }

    #[test]
    fn test_prepare_taxonomy_mapping() {
        let optimizer = LambdaOptimizer::new();
        let sequences = create_test_sequences();

        let mapping = optimizer.prepare_taxonomy_mapping(&sequences);

        assert_eq!(mapping.len(), 4); // Only 4 have taxon IDs
        assert!(mapping.contains(&("seq1".to_string(), 100)));
        assert!(mapping.contains(&("seq2".to_string(), 50)));
        assert!(mapping.contains(&("seq3".to_string(), 75)));
        assert!(mapping.contains(&("seq5".to_string(), 50)));

        // seq4 should not be in mapping (no taxon)
        assert!(!mapping.iter().any(|(id, _)| id == "seq4"));
    }

    #[test]
    fn test_prepare_taxonomy_mapping_empty() {
        let optimizer = LambdaOptimizer::new();
        let sequences = Vec::new();

        let mapping = optimizer.prepare_taxonomy_mapping(&sequences);

        assert!(mapping.is_empty());
    }

    #[test]
    fn test_prepare_taxonomy_mapping_no_taxons() {
        let optimizer = LambdaOptimizer::new();
        let sequences = vec![
            Sequence::new("seq1".to_string(), b"ATCG".to_vec()),
            Sequence::new("seq2".to_string(), b"GCTA".to_vec()),
        ];

        let mapping = optimizer.prepare_taxonomy_mapping(&sequences);

        assert!(mapping.is_empty());
    }

    #[test]
    fn test_stable_sort_for_same_taxon() {
        let optimizer = LambdaOptimizer::new();
        let mut sequences = vec![
            Sequence::new("first_50".to_string(), b"AAAA".to_vec()).with_taxon(50),
            Sequence::new("second_50".to_string(), b"TTTT".to_vec()).with_taxon(50),
            Sequence::new("third_50".to_string(), b"GGGG".to_vec()).with_taxon(50),
        ];

        let original_order: Vec<String> = sequences.iter().map(|s| s.id.clone()).collect();
        optimizer.optimize_for_lambda(&mut sequences);

        // All have same taxon, so order should be preserved (stable sort)
        let new_order: Vec<String> = sequences.iter().map(|s| s.id.clone()).collect();
        assert_eq!(original_order, new_order);
    }
}
