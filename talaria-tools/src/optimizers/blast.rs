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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_sequences() -> Vec<Sequence> {
        vec![
            Sequence::new("tiny".to_string(), b"A".to_vec()),
            Sequence::new("small".to_string(), b"ATCG".to_vec()),
            Sequence::new("medium".to_string(), b"ATCGATCG".to_vec()),
            Sequence::new("large".to_string(), b"ATCGATCGATCGATCG".to_vec()),
            Sequence::new("small2".to_string(), b"GCTA".to_vec()),
        ]
    }

    #[test]
    fn test_blast_optimizer_creation() {
        let optimizer = BlastOptimizer::new();
        // Simple creation test
        let _ = optimizer;
    }

    #[test]
    fn test_optimize_for_blast_sorts_by_length() {
        let optimizer = BlastOptimizer::new();
        let mut sequences = create_test_sequences();

        // Scramble the order first
        sequences.reverse();

        optimizer.optimize_for_blast(&mut sequences);

        // Should be sorted by length in ascending order
        assert_eq!(sequences[0].id, "tiny"); // 1 bp
                                             // Next two are both 4bp, could be in either order
        assert!(sequences[1].id == "small" || sequences[1].id == "small2");
        assert!(sequences[2].id == "small" || sequences[2].id == "small2");
        assert_ne!(sequences[1].id, sequences[2].id); // But they should be different
        assert_eq!(sequences[3].id, "medium"); // 8 bp
        assert_eq!(sequences[4].id, "large"); // 16 bp

        // Verify actual lengths are ascending
        for i in 0..sequences.len() - 1 {
            assert!(sequences[i].len() <= sequences[i + 1].len());
        }
    }

    #[test]
    fn test_optimize_for_blast_empty() {
        let optimizer = BlastOptimizer::new();
        let mut sequences = Vec::new();

        optimizer.optimize_for_blast(&mut sequences);

        assert!(sequences.is_empty());
    }

    #[test]
    fn test_optimize_for_blast_single() {
        let optimizer = BlastOptimizer::new();
        let mut sequences = vec![Sequence::new("only".to_string(), b"ATCGATCG".to_vec())];

        optimizer.optimize_for_blast(&mut sequences);

        assert_eq!(sequences.len(), 1);
        assert_eq!(sequences[0].id, "only");
    }

    #[test]
    fn test_optimize_for_blast_same_length_stable() {
        let optimizer = BlastOptimizer::new();
        let mut sequences = vec![
            Sequence::new("first".to_string(), b"AAAA".to_vec()),
            Sequence::new("second".to_string(), b"TTTT".to_vec()),
            Sequence::new("third".to_string(), b"GGGG".to_vec()),
            Sequence::new("fourth".to_string(), b"CCCC".to_vec()),
        ];

        let original_order: Vec<String> = sequences.iter().map(|s| s.id.clone()).collect();
        optimizer.optimize_for_blast(&mut sequences);

        // All have same length, order should be preserved (stable sort)
        let new_order: Vec<String> = sequences.iter().map(|s| s.id.clone()).collect();
        assert_eq!(original_order, new_order);
    }

    #[test]
    fn test_optimize_for_blast_diverse_lengths() {
        let optimizer = BlastOptimizer::new();
        let mut sequences = Vec::new();

        // Create sequences with exponentially growing lengths for diversity
        for i in 0..10 {
            let len = 1 << i; // 1, 2, 4, 8, 16, 32, etc.
            let seq = vec![b'A'; len];
            sequences.push(Sequence::new(format!("seq_{}", len), seq));
        }

        // Scramble
        sequences.reverse();

        optimizer.optimize_for_blast(&mut sequences);

        // Should be sorted by length ascending
        assert_eq!(sequences[0].len(), 1);
        assert_eq!(sequences[1].len(), 2);
        assert_eq!(sequences[2].len(), 4);
        assert_eq!(sequences[9].len(), 512);

        // Verify proper ordering
        for i in 0..sequences.len() - 1 {
            assert!(sequences[i].len() <= sequences[i + 1].len());
        }
    }

    #[test]
    fn test_optimize_for_blast_with_empty_sequences() {
        let optimizer = BlastOptimizer::new();
        let mut sequences = vec![
            Sequence::new("normal".to_string(), b"ATCG".to_vec()),
            Sequence::new("empty".to_string(), vec![]),
            Sequence::new("also_normal".to_string(), b"GC".to_vec()),
            Sequence::new("also_empty".to_string(), vec![]),
        ];

        optimizer.optimize_for_blast(&mut sequences);

        // Empty sequences should be first, then sorted by length
        assert_eq!(sequences[0].len(), 0);
        assert_eq!(sequences[1].len(), 0);
        assert_eq!(sequences[2].len(), 2); // "GC"
        assert_eq!(sequences[3].len(), 4); // "ATCG"
    }
}
