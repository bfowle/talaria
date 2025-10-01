//! Custom assertions for testing
//!
//! Provides specialized assertions for bioinformatics data.

use std::collections::HashSet;
use talaria_core::types::{SHA256Hash, TaxonId};

/// Assert that two sequences are similar within a threshold
pub fn assert_sequence_similarity(seq1: &str, seq2: &str, min_similarity: f64) {
    assert_eq!(seq1.len(), seq2.len(), "Sequences must have same length");

    let matches = seq1
        .chars()
        .zip(seq2.chars())
        .filter(|(a, b)| a == b)
        .count();

    let similarity = matches as f64 / seq1.len() as f64;

    assert!(
        similarity >= min_similarity,
        "Sequence similarity {:.2}% is below threshold {:.2}%",
        similarity * 100.0,
        min_similarity * 100.0
    );
}

/// Assert that a FASTA content is valid
pub fn assert_valid_fasta(content: &str) {
    let lines: Vec<&str> = content.lines().collect();
    assert!(!lines.is_empty(), "FASTA content is empty");

    let mut has_header = false;
    let mut has_sequence = false;

    for line in lines {
        if line.starts_with('>') {
            assert!(!line[1..].trim().is_empty(), "Empty FASTA header found");
            has_header = true;
        } else if !line.is_empty() {
            assert!(
                line.chars()
                    .all(|c| "ATGCNRYKMSWBDHV-".contains(c.to_ascii_uppercase())),
                "Invalid sequence character found: {}",
                line
            );
            has_sequence = true;
        }
    }

    assert!(has_header, "No FASTA headers found");
    assert!(has_sequence, "No sequences found");
}

/// Assert that sequences are properly deduplicated
pub fn assert_deduplicated(hashes: &[SHA256Hash]) {
    let unique: HashSet<_> = hashes.iter().collect();
    assert_eq!(
        hashes.len(),
        unique.len(),
        "Found {} duplicate hashes in supposedly deduplicated set",
        hashes.len() - unique.len()
    );
}

/// Assert that a taxonomy filter matches expected sequences
pub fn assert_taxonomy_filter(
    sequences: &[(String, Option<TaxonId>)],
    filter: impl Fn(Option<TaxonId>) -> bool,
    expected_count: usize,
) {
    let matched: Vec<_> = sequences
        .iter()
        .filter(|(_, taxon)| filter(*taxon))
        .collect();

    assert_eq!(
        matched.len(),
        expected_count,
        "Taxonomy filter matched {} sequences, expected {}",
        matched.len(),
        expected_count
    );
}

/// Assert storage statistics are within expected ranges
pub fn assert_storage_stats(total_sequences: usize, unique_sequences: usize, min_dedup_ratio: f64) {
    let dedup_ratio = if total_sequences > 0 {
        (total_sequences - unique_sequences) as f64 / total_sequences as f64
    } else {
        0.0
    };

    assert!(
        dedup_ratio >= min_dedup_ratio,
        "Deduplication ratio {:.2}% is below minimum {:.2}%",
        dedup_ratio * 100.0,
        min_dedup_ratio * 100.0
    );
}

/// Assert that a file size is within expected range
pub fn assert_file_size(size: u64, min_size: u64, max_size: u64) {
    assert!(
        size >= min_size,
        "File size {} bytes is below minimum {} bytes",
        size,
        min_size
    );
    assert!(
        size <= max_size,
        "File size {} bytes exceeds maximum {} bytes",
        size,
        max_size
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_similarity_assertion() {
        assert_sequence_similarity("ATGC", "ATGC", 1.0);
        assert_sequence_similarity("ATGC", "ATGG", 0.75);
    }

    #[test]
    #[should_panic(expected = "below threshold")]
    fn test_sequence_similarity_fails() {
        assert_sequence_similarity("ATGC", "CGTA", 0.5);
    }

    #[test]
    fn test_valid_fasta_assertion() {
        assert_valid_fasta(">seq1\nATGC\n>seq2\nGCTA");
    }

    #[test]
    #[should_panic(expected = "Invalid sequence character")]
    fn test_invalid_fasta_character() {
        assert_valid_fasta(">seq1\nATGX");
    }
}
