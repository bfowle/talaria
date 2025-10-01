use std::fs;
use std::io::Write;
/// Integration tests for FASTA parsing and writing
use talaria_bio::formats::fasta::{parse_fasta, parse_fasta_parallel, write_fasta};
use talaria_bio::sequence::Sequence;
use tempfile::NamedTempFile;

#[test]
fn test_fasta_round_trip() {
    // Create test sequences
    let sequences = vec![
        Sequence::new("seq1".to_string(), b"ATGCATGCATGC".to_vec())
            .with_description("Test DNA sequence".to_string()),
        Sequence::new("seq2".to_string(), b"ACDEFGHIKLMNPQRSTVWY".to_vec())
            .with_description("Test protein sequence".to_string()),
        Sequence::new("seq3".to_string(), b"ATGCNNNATGC".to_vec())
            .with_description("DNA with N".to_string()),
    ];

    // Write to temporary file
    let temp_file = NamedTempFile::new().unwrap();
    write_fasta(temp_file.path(), &sequences).unwrap();

    // Read back and verify
    let parsed = parse_fasta(temp_file.path()).unwrap();
    assert_eq!(parsed.len(), sequences.len());

    for (original, parsed) in sequences.iter().zip(parsed.iter()) {
        assert_eq!(original.id, parsed.id);
        assert_eq!(original.sequence, parsed.sequence);
        assert_eq!(original.description, parsed.description);
    }
}

#[test]
fn test_parallel_fasta_parsing() {
    // Create a larger test file
    let mut temp_file = NamedTempFile::new().unwrap();

    // Write 1000 sequences
    for i in 0..1000 {
        writeln!(temp_file, ">seq_{} Test sequence {}", i, i).unwrap();
        writeln!(temp_file, "ATGCATGCATGCATGCATGC").unwrap();
    }
    temp_file.flush().unwrap();

    // Parse serially
    let serial = parse_fasta(temp_file.path()).unwrap();

    // Parse in parallel
    let parallel = parse_fasta_parallel(temp_file.path(), 1024 * 1024).unwrap();

    // Results should be identical
    assert_eq!(serial.len(), 1000);
    assert_eq!(parallel.len(), 1000);

    for (s, p) in serial.iter().zip(parallel.iter()) {
        assert_eq!(s.id, p.id);
        assert_eq!(s.sequence, p.sequence);
        assert_eq!(s.description, p.description);
    }
}

#[test]
fn test_malformed_fasta_handling() {
    // Test various malformed FASTA formats
    let long_seq = format!(">seq1\n{}\n", "A".repeat(100000));
    let test_cases = vec![
        // Missing '>' at start
        ("seq1\nATGCATGC\n", 0),
        // Empty file
        ("", 0),
        // Only headers, no sequences
        (">seq1\n>seq2\n>seq3\n", 0),
        // Mixed valid and invalid
        (">seq1\nATGC\nInvalid line without >\n>seq2\nGCTA\n", 2),
        // Very long lines
        (long_seq.as_str(), 1),
    ];

    for (content, expected_count) in test_cases {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", content).unwrap();
        temp_file.flush().unwrap();

        let result = parse_fasta(temp_file.path());
        if expected_count > 0 {
            assert!(result.is_ok());
            let sequences = result.unwrap();
            assert_eq!(sequences.len(), expected_count);
        }
    }
}

#[test]
fn test_compressed_fasta_parsing() {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    // Create compressed FASTA
    let temp_file = NamedTempFile::new().unwrap();
    let temp_gz_path = format!("{}.gz", temp_file.path().display());

    {
        let file = fs::File::create(&temp_gz_path).unwrap();
        let mut gz = GzEncoder::new(file, Compression::default());

        gz.write_all(b">seq1 Compressed sequence\n").unwrap();
        gz.write_all(b"ATGCATGCATGC\n").unwrap();
        gz.write_all(b">seq2 Another one\n").unwrap();
        gz.write_all(b"GCTAGCTAGCTA\n").unwrap();
        gz.finish().unwrap();
    }

    // Parse compressed file
    let sequences = parse_fasta(&temp_gz_path).unwrap();

    assert_eq!(sequences.len(), 2);
    assert_eq!(sequences[0].id, "seq1");
    assert_eq!(sequences[0].sequence, b"ATGCATGCATGC");
    assert_eq!(sequences[1].id, "seq2");
    assert_eq!(sequences[1].sequence, b"GCTAGCTAGCTA");

    // Clean up
    fs::remove_file(temp_gz_path).ok();
}

#[test]
fn test_fasta_with_wrapped_sequences() {
    // Test FASTA with sequences wrapped at different lengths
    let content = r#">seq1 Wrapped at 60
ATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGC
ATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGC
>seq2 Wrapped at 80
ATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGC
ATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGC
>seq3 Single line
ATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGC
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    write!(temp_file, "{}", content).unwrap();
    temp_file.flush().unwrap();

    let sequences = parse_fasta(temp_file.path()).unwrap();
    assert_eq!(sequences.len(), 3);

    // Check sequence lengths - they were wrapped differently but should be parsed correctly
    assert_eq!(sequences[0].sequence.len(), 120); // 2 lines of 60 chars
    assert_eq!(sequences[1].sequence.len(), 152); // 2 lines: 76 + 76 chars
    assert_eq!(sequences[2].sequence.len(), 120); // single line
}

#[test]
fn test_fasta_with_special_characters() {
    // Test handling of various special characters in headers
    let content = r#">sp|P12345|GFP_ECOLI Green fluorescent protein OS=Escherichia coli OX=562 GN=gfp PE=1 SV=2
MSKGEELFTGVVPILVELDGDVNGHKFSVSGEGEGDATYGKLTLKFICTTGKLPVPWPTL
>gi|123456789|ref|NP_123456.1| hypothetical protein [Bacillus subtilis]
MAEIKDAQRRAFEQLQAAGVTTEDSAIYQCHVDGLTAEQIAEGKITVGQVVQLPLQIEA
>tr|Q9Y6K1|Q9Y6K1_HUMAN DNA-binding protein OS=Homo sapiens (Human) OX=9606
MGSSHHHHHHSSGLVPRGSHMASMTGGQQMGRGSEF
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    write!(temp_file, "{}", content).unwrap();
    temp_file.flush().unwrap();

    let sequences = parse_fasta(temp_file.path()).unwrap();
    assert_eq!(sequences.len(), 3);

    // Check that IDs are correctly extracted
    assert_eq!(sequences[0].id, "sp|P12345|GFP_ECOLI");
    assert_eq!(sequences[1].id, "gi|123456789|ref|NP_123456.1|");
    assert_eq!(sequences[2].id, "tr|Q9Y6K1|Q9Y6K1_HUMAN");

    // Check descriptions
    assert!(sequences[0]
        .description
        .as_ref()
        .unwrap()
        .contains("Green fluorescent protein"));
    assert!(sequences[1]
        .description
        .as_ref()
        .unwrap()
        .contains("hypothetical protein"));
    assert!(sequences[2]
        .description
        .as_ref()
        .unwrap()
        .contains("DNA-binding protein"));
}

#[test]
fn test_empty_sequences_filtered() {
    // Test that empty sequences are filtered out
    let content = r#">seq1
ATGCATGC
>seq2

>seq3
>seq4
GCTAGCTA
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    write!(temp_file, "{}", content).unwrap();
    temp_file.flush().unwrap();

    let sequences = parse_fasta(temp_file.path()).unwrap();

    // Should only have 2 sequences (seq1 and seq4)
    assert_eq!(sequences.len(), 2);
    assert_eq!(sequences[0].id, "seq1");
    assert_eq!(sequences[1].id, "seq4");
}

#[test]
fn test_case_normalization() {
    // Test that sequences are normalized to uppercase
    let content = r#">seq1
atgcATGCatgc
>seq2
GCTAgctaGCTA
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    write!(temp_file, "{}", content).unwrap();
    temp_file.flush().unwrap();

    let sequences = parse_fasta(temp_file.path()).unwrap();

    assert_eq!(sequences[0].sequence, b"ATGCATGCATGC");
    assert_eq!(sequences[1].sequence, b"GCTAGCTAGCTA");
}

#[test]
fn test_large_file_performance() {
    // Create a large test file (10MB)
    let mut temp_file = NamedTempFile::new().unwrap();
    let sequence_data = "ATGC".repeat(250); // 1KB sequence

    for i in 0..10000 {
        writeln!(temp_file, ">seq_{:05} Large sequence test", i).unwrap();
        writeln!(temp_file, "{}", sequence_data).unwrap();
    }
    temp_file.flush().unwrap();

    // Time the parsing
    let start = std::time::Instant::now();
    let sequences = parse_fasta_parallel(temp_file.path(), 1024 * 1024).unwrap();
    let duration = start.elapsed();

    assert_eq!(sequences.len(), 10000);

    // Should parse 10MB file in reasonable time (< 5 seconds)
    assert!(
        duration.as_secs() < 5,
        "Parsing took too long: {:?}",
        duration
    );
}

#[test]
fn test_memory_mapped_parsing() {
    // Test memory-mapped file parsing for large files
    use memmap2::MmapOptions;
    use talaria_bio::formats::fasta::parse_fasta_from_bytes;

    let mut temp_file = NamedTempFile::new().unwrap();

    // Write test data
    writeln!(temp_file, ">seq1").unwrap();
    writeln!(temp_file, "ATGCATGC").unwrap();
    writeln!(temp_file, ">seq2").unwrap();
    writeln!(temp_file, "GCTAGCTA").unwrap();
    temp_file.flush().unwrap();

    // Memory map the file
    let file = fs::File::open(temp_file.path()).unwrap();
    let mmap = unsafe { MmapOptions::new().map(&file).unwrap() };

    // Parse from bytes
    let sequences = parse_fasta_from_bytes(&mmap).unwrap();

    assert_eq!(sequences.len(), 2);
    assert_eq!(sequences[0].id, "seq1");
    assert_eq!(sequences[1].id, "seq2");
}
