#![allow(clippy::len_zero)]

use std::time::Instant;
/// Integration tests for sequence alignment functionality
use talaria_bio::alignment::{
    Alignment, Delta, NeedlemanWunsch, NucleotideMatrix, ScoringMatrix, BLOSUM62,
};
use talaria_bio::sequence::Sequence;

#[test]
fn test_exact_match_alignment() {
    // Test perfect alignment
    let ref_seq = Sequence::new("ref".to_string(), b"ATGCATGCATGC".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"ATGCATGCATGC".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    assert_eq!(alignment.identity, 1.0);
    assert_eq!(alignment.deltas.len(), 0);
    assert_eq!(alignment.ref_aligned, b"ATGCATGCATGC");
    assert_eq!(alignment.query_aligned, b"ATGCATGCATGC");
}

#[test]
fn test_single_mismatch_alignment() {
    // Test alignment with one mismatch
    let ref_seq = Sequence::new("ref".to_string(), b"ATGCATGC".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"ATGGATGC".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    assert!(alignment.identity > 0.8);
    assert_eq!(alignment.deltas.len(), 1);
    assert_eq!(
        alignment.deltas[0],
        Delta {
            position: 3,
            reference: b'C',
            query: b'G'
        }
    );
}

#[test]
fn test_insertion_alignment() {
    // Test alignment with insertion
    let ref_seq = Sequence::new("ref".to_string(), b"ATGCATGC".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"ATGCAAATGC".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    assert!(alignment.identity < 1.0);
    assert!(alignment.deltas.len() > 0);
    assert!(alignment.query_aligned.len() > alignment.ref_aligned.len() - 2); // Allow for some variation
}

#[test]
fn test_deletion_alignment() {
    // Test alignment with deletion
    let ref_seq = Sequence::new("ref".to_string(), b"ATGCAAATGC".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"ATGCATGC".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    assert!(alignment.identity < 1.0);
    assert!(alignment.deltas.len() > 0);
}

#[test]
fn test_complex_alignment() {
    // Test alignment with multiple differences
    let ref_seq = Sequence::new("ref".to_string(), b"ATGCATGCATGCATGC".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"ATGGATCCATGCTTGC".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    assert!(alignment.identity > 0.5);
    assert!(alignment.identity < 0.9);
    assert!(alignment.deltas.len() > 1);
}

#[test]
fn test_protein_alignment() {
    // Test protein sequence alignment using BLOSUM62
    let ref_seq = Sequence::new("ref".to_string(), b"ACDEFGHIKLMNPQRSTVWY".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"ACDEFGHIKLMNPQRSTVWY".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    assert_eq!(alignment.identity, 1.0);
    assert_eq!(alignment.deltas.len(), 0);
}

#[test]
fn test_protein_with_substitutions() {
    // Test protein alignment with substitutions
    let ref_seq = Sequence::new("ref".to_string(), b"ACDEFGHIKLM".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"ACDEFGHLKLM".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    assert!(alignment.identity > 0.8);
    assert!(alignment.deltas.len() > 0);
}

#[test]
fn test_empty_sequence_handling() {
    // Test edge case: empty sequences
    let ref_seq = Sequence::new("ref".to_string(), b"ATGC".to_vec());
    let query_seq = Sequence::new("query".to_string(), vec![]);

    let alignment = Alignment::global(&ref_seq, &query_seq);

    assert_eq!(alignment.identity, 0.0);
}

#[test]
fn test_single_base_alignment() {
    // Test edge case: single base sequences
    let ref_seq = Sequence::new("ref".to_string(), b"A".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"A".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    assert_eq!(alignment.identity, 1.0);
    assert_eq!(alignment.deltas.len(), 0);
}

#[test]
fn test_gap_penalties() {
    // Test that gap penalties work correctly
    let aligner = NeedlemanWunsch::new(NucleotideMatrix::new());

    // Sequence with gaps should have lower score
    let ref_seq = b"ATGCATGCATGC";
    let query_with_gap = b"ATGC____ATGC";
    let query_no_gap = b"ATGCATGCATGC";

    let alignment_gap = aligner.align(ref_seq, query_with_gap);
    let alignment_no_gap = aligner.align(ref_seq, query_no_gap);

    assert!(alignment_no_gap.score > alignment_gap.score);
}

#[test]
fn test_scoring_matrix_consistency() {
    // Test that scoring matrices give expected results
    let nuc_matrix = NucleotideMatrix::new();
    let blosum = BLOSUM62::new();

    // Nucleotide matches should score positively
    assert!(nuc_matrix.score(b'A', b'A') > 0);
    assert!(nuc_matrix.score(b'T', b'T') > 0);
    assert!(nuc_matrix.score(b'G', b'G') > 0);
    assert!(nuc_matrix.score(b'C', b'C') > 0);

    // Mismatches should score negatively
    assert!(nuc_matrix.score(b'A', b'T') < 0);
    assert!(nuc_matrix.score(b'G', b'C') < 0);

    // BLOSUM62 should handle amino acids
    assert!(blosum.score(b'W', b'W') > 0); // Tryptophan identity
    assert!(blosum.score(b'C', b'C') > 0); // Cysteine identity
}

#[test]
fn test_large_sequence_alignment() {
    // Test performance with larger sequences
    let ref_seq = Sequence::new("ref".to_string(), b"ATGC".repeat(100));
    let mut query_bytes = b"ATGC".repeat(100);

    // Introduce some mutations (make sure they actually change the base)
    // Position 50 = index 50, "ATGC" pattern means position 50%4=2 is 'G', change to 'A'
    query_bytes[50] = b'A';
    // Position 100 = index 100, position 100%4=0 is 'A', change to 'G'
    query_bytes[100] = b'G';
    // Position 150 = index 150, position 150%4=2 is 'G', change to 'T'
    query_bytes[150] = b'T';

    let query_seq = Sequence::new("query".to_string(), query_bytes);

    let start = Instant::now();
    let alignment = Alignment::global(&ref_seq, &query_seq);
    let duration = start.elapsed();

    // Should complete in reasonable time
    assert!(duration.as_secs() < 5);
    assert!(alignment.identity > 0.95); // Most bases still match
    assert!(alignment.deltas.len() >= 3); // At least our 3 mutations
}

#[test]
fn test_ambiguous_base_handling() {
    // Test alignment with ambiguous bases
    let ref_seq = Sequence::new("ref".to_string(), b"ATGCNATGC".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"ATGCAATGC".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    // Should handle N as wildcard
    assert!(alignment.identity > 0.8);
}

#[test]
fn test_case_insensitive_alignment() {
    // Test that alignment handles mixed case
    let ref_seq = Sequence::new("ref".to_string(), b"ATGCATGC".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"atgcatgc".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    // Should treat as identical after normalization
    assert!(alignment.identity > 0.99);
}

#[test]
fn test_alignment_string_generation() {
    // Test that alignment string is correctly generated
    let ref_seq = Sequence::new("ref".to_string(), b"ATGC".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"ATGC".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    // Perfect match should have all pipe characters
    assert!(alignment.alignment_string.iter().all(|&c| c == b'|'));

    // Test with mismatch
    let ref_seq2 = Sequence::new("ref".to_string(), b"ATGC".to_vec());
    let query_seq2 = Sequence::new("query".to_string(), b"ATTC".to_vec());

    let alignment2 = Alignment::global(&ref_seq2, &query_seq2);

    // Should have mix of pipes and mismatches
    assert!(alignment2.alignment_string.contains(&b'|'));
    assert!(alignment2.alignment_string.contains(&b'X'));
}

#[test]
fn test_delta_extraction() {
    // Test that deltas are correctly extracted
    let ref_seq = Sequence::new("ref".to_string(), b"ATGCATGC".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"ATGGATCC".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    // Should have deltas for the differences
    assert!(alignment.deltas.len() > 0);

    for delta in &alignment.deltas {
        // Each delta should have valid position
        assert!(delta.position < alignment.ref_aligned.len());
        // Reference and query should be different
        assert_ne!(delta.reference, delta.query);
    }
}

#[test]
fn test_symmetric_alignment() {
    // Test that alignment is reasonably symmetric
    let seq1 = Sequence::new("seq1".to_string(), b"ATGCATGCATGC".to_vec());
    let seq2 = Sequence::new("seq2".to_string(), b"ATGGATGGATGG".to_vec());

    let alignment1 = Alignment::global(&seq1, &seq2);
    let alignment2 = Alignment::global(&seq2, &seq1);

    // Scores might differ slightly due to gap penalties, but identity should be similar
    assert!((alignment1.identity - alignment2.identity).abs() < 0.1);
}

#[test]
fn test_repeated_sequence_alignment() {
    // Test alignment of sequences with repeats
    let ref_seq = Sequence::new("ref".to_string(), b"AAAAAAAAAA".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"AAAAAAAA".to_vec());

    let alignment = Alignment::global(&ref_seq, &query_seq);

    assert!(alignment.identity > 0.7);
    assert_eq!(
        alignment
            .deltas
            .iter()
            .filter(|d| d.reference == b'-' || d.query == b'-')
            .count(),
        2
    );
}

#[test]
fn test_reverse_complement_detection() {
    // Test that reverse complements are not automatically aligned
    // (This tests expected behavior - they should NOT match well)
    let ref_seq = Sequence::new("ref".to_string(), b"AAAAAAAA".to_vec());
    let query_seq = Sequence::new("query".to_string(), b"TTTTTTTT".to_vec()); // reverse complement

    let alignment = Alignment::global(&ref_seq, &query_seq);

    // Should not have high identity (all mismatches)
    assert!(alignment.identity < 0.2);
}
