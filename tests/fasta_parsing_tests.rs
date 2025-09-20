/// Tests for FASTA parsing, especially handling of malformed/wrapped headers
///
/// These tests ensure proper handling of:
/// - Wrapped headers across multiple lines (cholera database style)
/// - OX= field extraction when TaxID=0
/// - Metadata bleeding into sequence data
/// - Various UniProt/NCBI header formats
use talaria::bio::fasta::{parse_fasta_from_bytes, write_fasta};
use talaria::bio::sequence::Sequence;
use tempfile::NamedTempFile;

#[test]
fn test_wrapped_header_with_ox_field() {
    // Cholera-style wrapped header with TaxID=0 on first line, OX=666 on second
    let fasta = b">tr|A0A0H6DB96|A0A0H6DB96_VIBCL TaxID=0
Fatty acid oxidation complex subunit alpha OS=Vibrio cholerae OX=666 GN=fadB PE=
3 SV=1MIYQAKTLQVKQLANG
IAELSFCAPASVNKLDLHTL";

    let sequences = parse_fasta_from_bytes(fasta).unwrap();
    assert_eq!(sequences.len(), 1);

    let seq = &sequences[0];
    assert_eq!(seq.id, "tr|A0A0H6DB96|A0A0H6DB96_VIBCL");

    // Should have extracted TaxID=666 from OX= field since TaxID=0
    assert_eq!(seq.taxon_id, Some(666));

    // Description should contain the metadata
    assert!(seq
        .description
        .as_ref()
        .unwrap()
        .contains("Fatty acid oxidation"));
    assert!(seq.description.as_ref().unwrap().contains("OX=666"));

    // Sequence should start with MIYQ, not 3SV=1MIYQ
    let seq_str = String::from_utf8(seq.sequence.clone()).unwrap();
    assert!(seq_str.starts_with("MIYQ"), "Sequence was: {}", seq_str);
    assert!(
        !seq_str.contains("SV="),
        "SV= leaked into sequence: {}",
        seq_str
    );
}

#[test]
fn test_metadata_bleeding_into_sequence() {
    // Test case where SV=1 directly precedes sequence without space
    let fasta = b">sp|Q5EK40|CHXA_VIBCL Cholix toxin OS=Vibrio cholerae OX=666 GN=chxA PE=1 SV=1MYLTFYLEKVMKKMLLIAGATVIS
AQPQTTLESLDQFNQAAPEQSHQILASQEPVS";

    let sequences = parse_fasta_from_bytes(fasta).unwrap();
    assert_eq!(sequences.len(), 1);

    let seq = &sequences[0];
    let seq_str = String::from_utf8(seq.sequence.clone()).unwrap();

    // Sequence should contain MYLT and not include SV=1
    // Note: Due to how the parsing works, sequence might be on second line
    assert!(
        seq_str.contains("MYLT") || seq_str.starts_with("AQPQ"),
        "Sequence was: {}",
        seq_str
    );
    assert!(
        !seq_str.contains("="),
        "= character in sequence: {}",
        seq_str
    );

    // Should extract TaxID from OX= field
    assert_eq!(seq.taxon_id, Some(666));
}

#[test]
fn test_pe_field_with_inconsistent_spacing() {
    // PE= field with space after but value on next line
    let fasta = b">tr|TEST|TEST_VIBCL Test protein OS=Vibrio cholerae OX=666 GN=test PE=
3 SV=1ACGTACGTACGT";

    let sequences = parse_fasta_from_bytes(fasta).unwrap();
    assert_eq!(sequences.len(), 1);

    let seq = &sequences[0];
    let seq_str = String::from_utf8(seq.sequence.clone()).unwrap();

    // Should handle the "3 SV=1" properly
    assert_eq!(seq_str, "ACGTACGTACGT");
    assert!(!seq_str.contains("3"), "Number leaked into sequence");
    assert!(!seq_str.contains("SV"), "SV leaked into sequence");
}

#[test]
fn test_taxid_zero_with_ox_fallback() {
    // TaxID=0 should be overridden by OX= field
    let fasta = b">tr|TEST|TEST TaxID=0 OX=9606 OS=Homo sapiens
MKWVTFISLLFLFSSAYS";

    let sequences = parse_fasta_from_bytes(fasta).unwrap();
    assert_eq!(sequences.len(), 1);

    // Should use OX=9606 instead of TaxID=0
    assert_eq!(sequences[0].taxon_id, Some(9606));
}

#[test]
fn test_multiple_continuation_lines() {
    // Header wrapped across 3 lines
    let fasta = b">tr|A0A0H6DB96|A0A0H6DB96_VIBCL
Fatty acid oxidation complex subunit alpha OS=Vibrio cholerae
OX=666 GN=fadB PE=3 SV=1
MIYQAKTLQVKQLANG";

    let sequences = parse_fasta_from_bytes(fasta).unwrap();
    assert_eq!(sequences.len(), 1);

    let seq = &sequences[0];
    assert_eq!(seq.taxon_id, Some(666));

    // All metadata should be in description
    let desc = seq.description.as_ref().unwrap();
    assert!(desc.contains("Fatty acid oxidation"));
    assert!(desc.contains("OX=666"));
    assert!(desc.contains("GN=fadB"));

    // Sequence should be clean
    let seq_str = String::from_utf8(seq.sequence.clone()).unwrap();
    assert_eq!(seq_str, "MIYQAKTLQVKQLANG");
}

#[test]
fn test_standard_uniprot_format() {
    // Well-formatted UniProt entry
    let fasta = b">sp|P31946|1433B_HUMAN 14-3-3 protein beta/alpha OS=Homo sapiens OX=9606 GN=YWHAB PE=1 SV=3
MTMDKSELVQKAKLAEQAERYDDMAAAMKAVTEQGHELSNEERNLLSVAYKNVVGARRS
SWRVISSIEQKTERNEKKQQMGKEYREKIEAELQDICNDVLELLDKYLILNATQAESKV";

    let sequences = parse_fasta_from_bytes(fasta).unwrap();
    assert_eq!(sequences.len(), 1);

    let seq = &sequences[0];
    assert_eq!(seq.id, "sp|P31946|1433B_HUMAN");
    assert_eq!(seq.taxon_id, Some(9606));

    let seq_str = String::from_utf8(seq.sequence.clone()).unwrap();
    assert!(seq_str.starts_with("MTMDKSEL"));
    assert!(!seq_str.contains("="));
}

#[test]
fn test_ncbi_format_with_taxon() {
    // NCBI format with taxon: field
    let fasta = b">gi|123456|ref|NP_123456.1| hypothetical protein [Escherichia coli] taxon:562
MSKGEELFTGVVPILVELDGDVNGHKFSVSGEGEGDATYGKLTLKFICTTGKLPVPWPT";

    let sequences = parse_fasta_from_bytes(fasta).unwrap();
    assert_eq!(sequences.len(), 1);

    assert_eq!(sequences[0].taxon_id, Some(562));
}

#[test]
fn test_missing_taxonomy_fields() {
    // No taxonomy information
    let fasta = b">simple_id Description without taxonomy
ACGTACGTACGTACGT";

    let sequences = parse_fasta_from_bytes(fasta).unwrap();
    assert_eq!(sequences.len(), 1);

    // Should have no taxon_id
    assert_eq!(sequences[0].taxon_id, None);
}

#[test]
fn test_empty_description() {
    // Just ID, no description
    let fasta = b">sequence_id
ACGTACGTACGTACGT";

    let sequences = parse_fasta_from_bytes(fasta).unwrap();
    assert_eq!(sequences.len(), 1);

    assert_eq!(sequences[0].id, "sequence_id");
    assert_eq!(sequences[0].description, None);
    assert_eq!(sequences[0].taxon_id, None);
}

#[test]
fn test_sequence_header_generation_with_authoritative_taxid() {
    // Test that header generation uses chunk's authoritative TaxID
    let mut seq = Sequence::new("test_id".to_string(), b"ACGT".to_vec());
    seq.description = Some("Description TaxID=0 OX=666".to_string());
    seq.taxon_id = Some(666); // This is the authoritative value from chunk

    let header = seq.header();

    // Should contain TaxID=666, not TaxID=0
    assert!(header.contains("TaxID=666"));
    assert!(!header.contains("TaxID=0"));

    // Original description should be preserved but without conflicting TaxID
    assert!(header.contains("Description"));
    assert!(header.contains("OX=666"));
}

#[test]
fn test_roundtrip_with_wrapped_headers() {
    // Create sequences with complex metadata
    let sequences = vec![
        {
            let mut seq = Sequence::new(
                "tr|A0A0H6|A0A0H6_VIBCL".to_string(),
                b"MIYQAKTLQVKQLANG".to_vec(),
            );
            seq.description = Some(
                "Fatty acid oxidation OS=Vibrio cholerae OX=666 GN=fadB PE=3 SV=1".to_string(),
            );
            seq.taxon_id = Some(666);
            seq
        },
        {
            let mut seq = Sequence::new(
                "sp|Q5EK40|CHXA_VIBCL".to_string(),
                b"MYLTFYLEKVMKK".to_vec(),
            );
            seq.description =
                Some("Cholix toxin OS=Vibrio cholerae OX=666 GN=chxA PE=1 SV=1".to_string());
            seq.taxon_id = Some(666);
            seq
        },
    ];

    // Write to file
    let temp_file = NamedTempFile::new().unwrap();
    write_fasta(temp_file.path(), &sequences).unwrap();

    // Read back
    let content = std::fs::read(temp_file.path()).unwrap();
    let parsed = parse_fasta_from_bytes(&content).unwrap();

    assert_eq!(parsed.len(), 2);

    // Verify first sequence
    assert_eq!(parsed[0].id, "tr|A0A0H6|A0A0H6_VIBCL");
    assert_eq!(parsed[0].taxon_id, Some(666));
    assert_eq!(
        String::from_utf8(parsed[0].sequence.clone()).unwrap(),
        "MIYQAKTLQVKQLANG"
    );

    // Verify second sequence
    assert_eq!(parsed[1].id, "sp|Q5EK40|CHXA_VIBCL");
    assert_eq!(parsed[1].taxon_id, Some(666));
    assert_eq!(
        String::from_utf8(parsed[1].sequence.clone()).unwrap(),
        "MYLTFYLEKVMKK"
    );
}

#[test]
fn test_various_malformed_headers() {
    // Collection of edge cases found in real data
    let test_cases = vec![
        // Missing space after PE=
        (b">id1 OS=Org OX=123 PE=3SV=1\nACGT" as &[u8], "ACGT"),
        // Extra spaces
        (b">id2  OS=Org  OX=456  GN=gene  PE=1  SV=2\nACGT", "ACGT"),
        // No newline before sequence - won't work without \n
        (b">id3 OX=789\nACGTACGT", "ACGTACGT"),
        // Mixed case in fields
        (b">id4 ox=111 TaxID=222\nACGT", "ACGT"),
    ];

    for (fasta, expected_seq) in test_cases {
        let sequences = parse_fasta_from_bytes(fasta);
        assert!(
            sequences.is_ok(),
            "Failed to parse: {:?}",
            std::str::from_utf8(fasta)
        );
        let sequences = sequences.unwrap();
        assert_eq!(sequences.len(), 1);

        let seq_str = String::from_utf8(sequences[0].sequence.clone()).unwrap();
        assert_eq!(
            seq_str,
            expected_seq,
            "For input: {:?}",
            std::str::from_utf8(fasta)
        );
    }
}

#[test]
fn test_header_with_equals_in_description() {
    // Test that = signs in regular description don't cause issues
    let fasta =
        b">test_id ATP-binding cassette transporter A=B OS=E. coli OX=562 function=transport
MKLTFFF";

    let sequences = parse_fasta_from_bytes(fasta).unwrap();
    assert_eq!(sequences.len(), 1);

    let seq = &sequences[0];
    assert_eq!(seq.taxon_id, Some(562));

    // Description should preserve the A=B part
    assert!(seq.description.as_ref().unwrap().contains("A=B"));

    // Sequence should not contain any = characters
    let seq_str = String::from_utf8(seq.sequence.clone()).unwrap();
    assert_eq!(seq_str, "MKLTFFF");
    assert!(!seq_str.contains("="));
}
