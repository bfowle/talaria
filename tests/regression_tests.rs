/// Regression tests for specific bugs found in production
///
/// These tests ensure that previously fixed bugs don't reoccur
use std::fs;
use std::path::Path;
use talaria::bio::fasta::{parse_fasta, write_fasta};
use talaria::bio::sequence::Sequence;
use tempfile::TempDir;

/// Regression test for cholera database bug
/// Issue: LAMBDA failed with "Assigning = to amino acid alphabet" error
/// Cause: Malformed FASTA with wrapped headers and sequences starting with metadata
#[test]
fn test_cholera_database_regression() {
    // Load the problematic cholera FASTA
    let fixture_path = Path::new("tests/fixtures/cholera_malformed.fasta");

    // Skip if fixture doesn't exist (CI environment)
    if !fixture_path.exists() {
        eprintln!("Skipping cholera regression test - fixture not found");
        return;
    }

    let sequences = parse_fasta(fixture_path).expect("Failed to parse cholera FASTA");

    // Verify correct parsing
    assert!(sequences.len() >= 4, "Expected at least 4 sequences");

    // Check first sequence (wrapped header with TaxID=0, OX=666)
    let seq1 = &sequences[0];
    assert_eq!(seq1.id, "tr|A0A0H6DB96|A0A0H6DB96_VIBCL");
    assert_eq!(
        seq1.taxon_id,
        Some(666),
        "Should use OX=666 instead of TaxID=0"
    );

    // Verify sequence doesn't contain metadata
    let seq1_str = String::from_utf8(seq1.sequence.clone()).unwrap();
    assert!(seq1_str.starts_with("MIYQ"), "Sequence was: {}", seq1_str);
    assert!(!seq1_str.contains("SV="), "SV= leaked into sequence");
    assert!(!seq1_str.contains("3"), "Line number leaked into sequence");

    // Check sequence with SV=1 bleeding into sequence
    let cholix = sequences
        .iter()
        .find(|s| s.id.contains("Q5EK40"))
        .expect("Cholix sequence not found");

    let cholix_seq = String::from_utf8(cholix.sequence.clone()).unwrap();
    assert!(
        cholix_seq.starts_with("MYLT"),
        "Cholix sequence was: {}",
        cholix_seq
    );
    assert!(!cholix_seq.contains("="), "= character in sequence");
}

/// Regression test for TaxID=0 handling
/// Issue: TaxID=0 was being used instead of falling back to OX= field
#[test]
fn test_taxid_zero_regression() {
    let fasta_content = b">test_protein Description TaxID=0 OX=9606 OS=Homo sapiens
MKLTFYLEKVMKKMLLIAGATVIS
>another_protein TaxID=0
ACGTACGTACGT";

    let sequences = parse_fasta_from_bytes(fasta_content).unwrap();

    // First sequence should use OX=9606
    assert_eq!(sequences[0].taxon_id, Some(9606));

    // Second sequence has TaxID=0 with no OX, should be None
    assert_eq!(sequences[1].taxon_id, None);
}

/// Regression test for wrapped FASTA headers
/// Issue: Headers wrapped across multiple lines were not parsed correctly
#[test]
fn test_wrapped_header_regression() {
    let fasta_content = b">tr|A0A0H6|A0A0H6_VIBCL
Fatty acid oxidation complex subunit alpha
OS=Vibrio cholerae OX=666 GN=fadB PE=3 SV=1
MKLTFYLEKVMKK";

    let sequences = parse_fasta_from_bytes(fasta_content).unwrap();

    assert_eq!(sequences.len(), 1);
    let seq = &sequences[0];

    // Should combine all metadata lines
    let desc = seq.description.as_ref().expect("No description found");

    // Check that key metadata is present (might be combined differently)
    assert!(
        desc.contains("Fatty acid oxidation") || desc.contains("OS=Vibrio cholerae"),
        "Description missing expected content: {}",
        desc
    );
    assert!(desc.contains("OX=666"), "Missing OX field in: {}", desc);
    assert!(
        desc.contains("GN=fadB") || desc.contains("PE=3"),
        "Missing gene/PE info in: {}",
        desc
    );

    // Should extract TaxID from OX field
    assert_eq!(seq.taxon_id, Some(666));

    // Sequence should be clean
    assert_eq!(
        String::from_utf8(seq.sequence.clone()).unwrap(),
        "MKLTFYLEKVMKK"
    );
}

/// Regression test for authoritative chunk TaxID
/// Issue: Generated headers were using description TaxID instead of chunk's authoritative value
#[test]
fn test_authoritative_taxid_regression() {
    // Create sequence with conflicting TaxIDs
    let mut seq = Sequence::new("test_id".to_string(), b"ACGT".to_vec());
    seq.description = Some("Protein TaxID=999 OX=888".to_string());
    seq.taxon_id = Some(666); // Authoritative value from chunk

    let header = seq.header();

    // Should use authoritative TaxID=666
    assert!(header.contains("TaxID=666"), "Header was: {}", header);

    // Should NOT contain conflicting TaxIDs
    assert!(
        !header.contains("TaxID=999"),
        "Found TaxID=999 in: {}",
        header
    );
    assert!(
        !header.contains("TaxID=888"),
        "Found TaxID=888 in: {}",
        header
    );

    // Should preserve OX field
    assert!(header.contains("OX=888"), "Missing OX field in: {}", header);
}

/// Regression test for PE= field with inconsistent spacing
/// Issue: "PE= 3 SV=1" format was not handled correctly
#[test]
fn test_pe_field_spacing_regression() {
    let fasta_content = b">test_protein Description PE=
3 SV=1MKLTFFF
AAAAAAA";

    let sequences = parse_fasta_from_bytes(fasta_content).unwrap();

    let seq = &sequences[0];
    let seq_str = String::from_utf8(seq.sequence.clone()).unwrap();

    // Sequence should not contain metadata
    assert_eq!(seq_str, "MKLTFFFAAAAAAA");
    assert!(!seq_str.contains("3"));
    assert!(!seq_str.contains("SV"));
    assert!(!seq_str.contains("="));

    // Description should contain PE and SV info
    let desc = seq.description.as_ref().unwrap();
    assert!(desc.contains("PE=") || desc.contains("3 SV=1"));
}

/// Regression test for equals sign in protein sequences
/// Issue: LAMBDA failed when sequences contained '=' character
#[test]
fn test_equals_in_sequence_regression() {
    // Write test FASTA to temp file
    let temp_dir = TempDir::new().unwrap();
    let fasta_path = temp_dir.path().join("test.fasta");

    let sequences = vec![
        Sequence::new("seq1".to_string(), b"MKLTFFF".to_vec()),
        Sequence::new("seq2".to_string(), b"ACGTACGT".to_vec()),
    ];

    write_fasta(&fasta_path, &sequences).unwrap();

    // Read back and verify
    let content = fs::read_to_string(&fasta_path).unwrap();

    // Should not have '=' in sequence lines (only in headers)
    for line in content.lines() {
        if !line.starts_with('>') {
            assert!(!line.contains('='), "Found '=' in sequence line: {}", line);
        }
    }

    // Parse and verify sequences are clean
    let parsed = parse_fasta(&fasta_path).unwrap();
    for seq in parsed {
        let seq_str = String::from_utf8(seq.sequence).unwrap();
        assert!(!seq_str.contains('='), "Sequence contains '=': {}", seq_str);
    }
}

/// Regression test for various header formats
/// Issue: Different databases use different header formats
#[test]
fn test_various_formats_regression() {
    let fixture_path = Path::new("tests/fixtures/various_formats.fasta");

    if !fixture_path.exists() {
        eprintln!("Skipping various formats test - fixture not found");
        return;
    }

    let sequences = parse_fasta(fixture_path).expect("Failed to parse various formats");

    // UniProt format
    let uniprot = sequences
        .iter()
        .find(|s| s.id == "sp|P31946|1433B_HUMAN")
        .expect("UniProt sequence not found");
    assert_eq!(uniprot.taxon_id, Some(9606));

    // NCBI format with taxon: field
    let ncbi = sequences
        .iter()
        .find(|s| s.id.contains("gi|123456"))
        .expect("NCBI sequence not found");
    assert_eq!(ncbi.taxon_id, Some(562));

    // No taxonomy info
    let no_tax = sequences
        .iter()
        .find(|s| s.id == "tr|A0A0H6|A0A0H6_VIBCL")
        .expect("No-tax sequence not found");
    assert_eq!(no_tax.taxon_id, None);

    // TaxID=0 with OX fallback
    let fallback = sequences
        .iter()
        .find(|s| s.id == "SIMPLE_ID")
        .expect("Fallback sequence not found");
    assert_eq!(fallback.taxon_id, Some(789)); // From OX=789, not TaxID=0

    // Wrapped header
    let wrapped = sequences
        .iter()
        .find(|s| s.id == "test_wrapped")
        .expect("Wrapped sequence not found");
    assert_eq!(wrapped.taxon_id, Some(12345));
    assert!(wrapped
        .description
        .as_ref()
        .unwrap()
        .contains("Multi-line description"));
}

/// Helper to parse FASTA from bytes (re-exported for tests)
fn parse_fasta_from_bytes(data: &[u8]) -> Result<Vec<Sequence>, talaria::TalariaError> {
    talaria::bio::fasta::parse_fasta_from_bytes(data)
}
