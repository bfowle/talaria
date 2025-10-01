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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_sequences() -> Vec<Sequence> {
        vec![
            Sequence::new("seq1".to_string(), b"ATCG".to_vec()).with_taxon(100),
            Sequence::new("seq2".to_string(), b"GC".to_vec()).with_taxon(50),
            Sequence::new("seq3".to_string(), b"ATCGATCG".to_vec()).with_taxon(50),
            Sequence::new("seq4".to_string(), b"TTAA".to_vec()), // No taxon
            Sequence::new("seq5".to_string(), b"GGCCTTAA".to_vec()).with_taxon(100),
            Sequence::new("seq6".to_string(), b"A".to_vec()), // No taxon
        ]
    }

    #[test]
    fn test_kraken_optimizer_creation() {
        let optimizer = KrakenOptimizer::new();
        // Simple creation test
        let _ = optimizer;
    }

    #[test]
    fn test_optimize_for_kraken_sorts_by_taxon_and_length() {
        let optimizer = KrakenOptimizer::new();
        let mut sequences = create_test_sequences();

        optimizer.optimize_for_kraken(&mut sequences);

        // Should be sorted by (taxon_id, length)
        // taxon 0 (None) comes first, sorted by length
        assert_eq!(sequences[0].id, "seq6"); // No taxon, length 1
        assert_eq!(sequences[1].id, "seq4"); // No taxon, length 4

        // taxon 50, sorted by length
        assert_eq!(sequences[2].id, "seq2"); // Taxon 50, length 2
        assert_eq!(sequences[3].id, "seq3"); // Taxon 50, length 8

        // taxon 100, sorted by length
        assert_eq!(sequences[4].id, "seq1"); // Taxon 100, length 4
        assert_eq!(sequences[5].id, "seq5"); // Taxon 100, length 8
    }

    #[test]
    fn test_optimize_for_kraken_empty() {
        let optimizer = KrakenOptimizer::new();
        let mut sequences = Vec::new();

        optimizer.optimize_for_kraken(&mut sequences);

        assert!(sequences.is_empty());
    }

    #[test]
    fn test_optimize_for_kraken_single() {
        let optimizer = KrakenOptimizer::new();
        let mut sequences =
            vec![Sequence::new("only".to_string(), b"ATCG".to_vec()).with_taxon(42)];

        optimizer.optimize_for_kraken(&mut sequences);

        assert_eq!(sequences.len(), 1);
        assert_eq!(sequences[0].id, "only");
    }

    #[test]
    fn test_optimize_for_kraken_no_taxons() {
        let optimizer = KrakenOptimizer::new();
        let mut sequences = vec![
            Sequence::new("long".to_string(), b"ATCGATCGATCG".to_vec()),
            Sequence::new("short".to_string(), b"AT".to_vec()),
            Sequence::new("medium".to_string(), b"ATCGATCG".to_vec()),
        ];

        optimizer.optimize_for_kraken(&mut sequences);

        // All have taxon 0 (None), should be sorted by length only
        assert_eq!(sequences[0].id, "short"); // length 2
        assert_eq!(sequences[1].id, "medium"); // length 8
        assert_eq!(sequences[2].id, "long"); // length 12
    }

    #[test]
    fn test_optimize_for_kraken_same_taxon_same_length() {
        let optimizer = KrakenOptimizer::new();
        let mut sequences = vec![
            Sequence::new("first".to_string(), b"AAAA".to_vec()).with_taxon(50),
            Sequence::new("second".to_string(), b"TTTT".to_vec()).with_taxon(50),
            Sequence::new("third".to_string(), b"GGGG".to_vec()).with_taxon(50),
        ];

        let original_order: Vec<String> = sequences.iter().map(|s| s.id.clone()).collect();
        optimizer.optimize_for_kraken(&mut sequences);

        // All have same taxon and length, order should be preserved
        let new_order: Vec<String> = sequences.iter().map(|s| s.id.clone()).collect();
        assert_eq!(original_order, new_order);
    }

    #[test]
    fn test_optimize_for_kraken_kmer_locality() {
        // Test that sequences with same taxon are grouped together
        // which improves k-mer locality for Kraken
        let optimizer = KrakenOptimizer::new();
        let mut sequences = Vec::new();

        // Create sequences alternating between taxons
        for i in 0..20 {
            let taxon = if i % 2 == 0 { 10 } else { 20 };
            let seq = vec![b'A'; i + 1];
            sequences.push(Sequence::new(format!("seq_{}", i), seq).with_taxon(taxon));
        }

        optimizer.optimize_for_kraken(&mut sequences);

        // Verify taxons are grouped together
        let mut current_taxon = sequences[0].taxon_id.unwrap_or(0);
        let mut taxon_changes = 0;

        for seq in &sequences[1..] {
            let taxon = seq.taxon_id.unwrap_or(0);
            if taxon != current_taxon {
                taxon_changes += 1;
                current_taxon = taxon;
            }
        }

        // Should have at most 1 taxon change (from 10 to 20)
        assert_eq!(taxon_changes, 1, "Sequences should be grouped by taxon");
    }

    #[test]
    fn test_optimize_for_kraken_mixed_taxons() {
        let optimizer = KrakenOptimizer::new();
        let mut sequences = vec![
            Sequence::new("t100_l10".to_string(), vec![b'A'; 10]).with_taxon(100),
            Sequence::new("none_l5".to_string(), vec![b'T'; 5]),
            Sequence::new("t50_l15".to_string(), vec![b'G'; 15]).with_taxon(50),
            Sequence::new("t100_l5".to_string(), vec![b'C'; 5]).with_taxon(100),
            Sequence::new("t50_l10".to_string(), vec![b'A'; 10]).with_taxon(50),
            Sequence::new("none_l10".to_string(), vec![b'T'; 10]),
        ];

        optimizer.optimize_for_kraken(&mut sequences);

        // Verify ordering: first by taxon (0, 50, 100), then by length within taxon
        assert_eq!(sequences[0].taxon_id, None);
        assert_eq!(sequences[0].len(), 5);

        assert_eq!(sequences[1].taxon_id, None);
        assert_eq!(sequences[1].len(), 10);

        assert_eq!(sequences[2].taxon_id, Some(50));
        assert_eq!(sequences[2].len(), 10);

        assert_eq!(sequences[3].taxon_id, Some(50));
        assert_eq!(sequences[3].len(), 15);

        assert_eq!(sequences[4].taxon_id, Some(100));
        assert_eq!(sequences[4].len(), 5);

        assert_eq!(sequences[5].taxon_id, Some(100));
        assert_eq!(sequences[5].len(), 10);
    }
}
