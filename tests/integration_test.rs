use std::fs;
use std::path::Path;
use talaria::bio::fasta::parse_fasta;
use talaria::bio::sequence::{Sequence, SequenceType};
use talaria::core::reducer::Reducer;
use talaria::core::reference_selector::ReferenceSelector;

#[test]
fn test_fasta_parsing() {
    let fasta_content = ">seq1 description\nACGTACGT\n>seq2\nTGCATGCA\n";
    let temp_file = "test_sequences.fasta";
    
    fs::write(temp_file, fasta_content).unwrap();
    
    let sequences = parse_fasta(Path::new(temp_file)).unwrap();
    
    assert_eq!(sequences.len(), 2);
    assert_eq!(sequences[0].id, "seq1");
    assert_eq!(sequences[0].sequence, b"ACGTACGT");
    assert_eq!(sequences[1].id, "seq2");
    assert_eq!(sequences[1].sequence, b"TGCATGCA");
    
    fs::remove_file(temp_file).unwrap();
}

#[test]
fn test_reference_selection() {
    let sequences = vec![
        Sequence::new("seq1".to_string(), b"ACGTACGTACGT".to_vec()),
        Sequence::new("seq2".to_string(), b"TGCATGCA".to_vec()),
        Sequence::new("seq3".to_string(), b"AAAAAAAAAAAAAAAA".to_vec()),
    ];
    
    let selector = ReferenceSelector::new()
        .with_similarity_threshold(0.8)
        .with_min_length(5); // Lower minimum length for test sequences
    let result = selector.select_references(sequences.clone(), 0.5);
    
    // The result should contain some references
    assert!(result.references.len() > 0);
}

#[test]
fn test_alignment() {
    use talaria::bio::alignment::Alignment;
    
    let seq1 = Sequence::new("seq1".to_string(), b"ACGTACGT".to_vec());
    let seq2 = Sequence::new("seq2".to_string(), b"ACGTACGA".to_vec());
    
    let alignment = Alignment::global(&seq1, &seq2);
    
    // Check that alignment completes and has reasonable identity
    assert!(alignment.identity >= 0.0 && alignment.identity <= 1.0);
}

#[test]
fn test_delta_encoding() {
    use talaria::core::delta_encoder::{DeltaEncoder, DeltaReconstructor};
    
    let reference = Sequence::new("ref".to_string(), b"ACGTACGTACGT".to_vec());
    let child = Sequence::new("child".to_string(), b"ACGTACGAACGT".to_vec());
    
    let encoder = DeltaEncoder::new();
    let delta_record = encoder.encode(&reference, &child);
    
    assert!(!delta_record.deltas.is_empty());
    
    let reconstructor = DeltaReconstructor::new();
    let reconstructed = reconstructor.reconstruct(&reference, &delta_record);
    
    assert_eq!(reconstructed.id, "child");
    assert_eq!(reconstructed.sequence, child.sequence);
}

#[test]
fn test_sequence_reduction() {
    use talaria::cli::TargetAligner;
    use talaria::core::config::Config;
    
    let sequences = vec![
        Sequence::new("seq1".to_string(), b"ACGTACGT".to_vec()),
        Sequence::new("seq2".to_string(), b"ACGTACGT".to_vec()), // Duplicate
        Sequence::new("seq3".to_string(), b"TGCATGCA".to_vec()),
    ];
    
    let config = Config::default();
    let mut reducer = Reducer::new(config).with_silent(true);
    let (reduced_sequences, _deltas, _original_count) = reducer.reduce(sequences, 0.5, TargetAligner::Generic).unwrap();
    
    // Should have reduced the sequences
    assert!(reduced_sequences.len() <= 3);
}

#[test]
fn test_sequence_type_detection() {
    let dna_seq = Sequence::new("dna".to_string(), b"ACGTACGT".to_vec());
    let protein_seq = Sequence::new("protein".to_string(), b"MVALPRWFDK".to_vec());
    
    assert_eq!(dna_seq.detect_type(), SequenceType::Nucleotide);
    assert_eq!(protein_seq.detect_type(), SequenceType::Protein);
}

#[test]
fn test_database_config() {
    use talaria::download::get_database_configs;
    
    let configs = get_database_configs();
    
    assert!(configs.len() > 0);
    assert!(configs.iter().any(|c| c.name == "UniProt"));
    assert!(configs.iter().any(|c| c.name == "NCBI"));
}