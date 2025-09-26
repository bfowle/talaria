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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_sequences() -> Vec<Sequence> {
        vec![
            Sequence::new("short".to_string(), b"AT".to_vec()),
            Sequence::new("medium".to_string(), b"ATCGATCG".to_vec()),
            Sequence::new("long".to_string(), b"ATCGATCGATCGATCG".to_vec()),
            Sequence::new("tiny".to_string(), b"A".to_vec()),
            Sequence::new("medium2".to_string(), b"GCTAGCTA".to_vec()),
        ]
    }

    #[test]
    fn test_generic_optimizer_creation() {
        let optimizer = GenericOptimizer::new();
        // Simple creation test
        let _ = optimizer;
    }

    #[test]
    fn test_optimize_sorts_by_length_descending() {
        let optimizer = GenericOptimizer::new();
        let mut sequences = create_test_sequences();

        optimizer.optimize(&mut sequences);

        // Should be sorted by length in descending order (longest first)
        assert_eq!(sequences[0].id, "long");    // 16 bp
        assert_eq!(sequences[1].id, "medium");  // 8 bp
        assert_eq!(sequences[2].id, "medium2"); // 8 bp
        assert_eq!(sequences[3].id, "short");   // 2 bp
        assert_eq!(sequences[4].id, "tiny");    // 1 bp

        // Verify actual lengths are descending
        for i in 0..sequences.len() - 1 {
            assert!(sequences[i].len() >= sequences[i + 1].len());
        }
    }

    #[test]
    fn test_optimize_empty() {
        let optimizer = GenericOptimizer::new();
        let mut sequences = Vec::new();

        optimizer.optimize(&mut sequences);

        assert!(sequences.is_empty());
    }

    #[test]
    fn test_optimize_single() {
        let optimizer = GenericOptimizer::new();
        let mut sequences = vec![
            Sequence::new("only".to_string(), b"ATCGATCG".to_vec())
        ];

        optimizer.optimize(&mut sequences);

        assert_eq!(sequences.len(), 1);
        assert_eq!(sequences[0].id, "only");
    }

    #[test]
    fn test_optimize_same_length() {
        let optimizer = GenericOptimizer::new();
        let mut sequences = vec![
            Sequence::new("first".to_string(), b"AAAA".to_vec()),
            Sequence::new("second".to_string(), b"TTTT".to_vec()),
            Sequence::new("third".to_string(), b"GGGG".to_vec()),
            Sequence::new("fourth".to_string(), b"CCCC".to_vec()),
        ];

        let original_order: Vec<String> = sequences.iter().map(|s| s.id.clone()).collect();
        optimizer.optimize(&mut sequences);

        // All have same length, so order should be preserved (stable sort)
        let new_order: Vec<String> = sequences.iter().map(|s| s.id.clone()).collect();
        assert_eq!(original_order, new_order);
    }

    #[test]
    fn test_optimize_with_empty_sequences() {
        let optimizer = GenericOptimizer::new();
        let mut sequences = vec![
            Sequence::new("empty".to_string(), vec![]),
            Sequence::new("normal".to_string(), b"ATCG".to_vec()),
            Sequence::new("also_empty".to_string(), vec![]),
        ];

        optimizer.optimize(&mut sequences);

        // Normal sequence should be first (length 4), empty sequences last
        assert_eq!(sequences[0].id, "normal");
        assert_eq!(sequences[0].len(), 4);
        assert_eq!(sequences[1].len(), 0);
        assert_eq!(sequences[2].len(), 0);
    }

    #[test]
    fn test_cache_locality_optimization() {
        // This test verifies the intention of the optimization
        let optimizer = GenericOptimizer::new();
        let mut sequences = Vec::new();

        // Create sequences with varying sizes
        for i in 1..=100 {
            let seq = vec![b'A'; i];
            sequences.push(Sequence::new(format!("seq{}", i), seq));
        }

        optimizer.optimize(&mut sequences);

        // Verify longest sequences come first (better for cache prefetching)
        assert_eq!(sequences[0].len(), 100);
        assert_eq!(sequences[99].len(), 1);

        // Verify strict descending order
        for i in 0..sequences.len() - 1 {
            assert!(
                sequences[i].len() >= sequences[i + 1].len(),
                "Sequence at {} (len {}) should be >= sequence at {} (len {})",
                i, sequences[i].len(), i + 1, sequences[i + 1].len()
            );
        }
    }
}
