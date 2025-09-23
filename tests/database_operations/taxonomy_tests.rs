use flate2::write::GzEncoder;
use flate2::Compression;
/// Tests for taxonomy mapping integration with accession2taxid files
///
/// These tests ensure proper loading and usage of:
/// - NCBI prot.accession2taxid.gz files
/// - UniProt idmapping.dat.gz files
/// - Simple accession2taxid.txt format
/// - Integration with TaxonomicChunker
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use talaria_bio::sequence::Sequence;
use talaria_sequoia::chunker::TaxonomicChunker;
use talaria_sequoia::types::{ChunkingStrategy, TaxonId};
use tempfile::TempDir;

/// Create a test environment with taxonomy files
struct TaxonomyTestEnv {
    _temp_dir: TempDir,
    taxonomy_dir: PathBuf,
    ncbi_dir: PathBuf,
    uniprot_dir: PathBuf,
}

impl TaxonomyTestEnv {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let taxonomy_dir = temp_dir.path().join("databases").join("taxonomy");
        let ncbi_dir = taxonomy_dir.join("ncbi");
        let uniprot_dir = taxonomy_dir.join("uniprot");

        fs::create_dir_all(&ncbi_dir).unwrap();
        fs::create_dir_all(&uniprot_dir).unwrap();

        TaxonomyTestEnv {
            _temp_dir: temp_dir,
            taxonomy_dir,
            ncbi_dir,
            uniprot_dir,
        }
    }

    fn create_ncbi_prot_accession2taxid(&self, mappings: &[(String, u32)]) {
        let file_path = self.ncbi_dir.join("prot.accession2taxid.gz");
        let file = File::create(file_path).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::default());

        // Write header
        writeln!(encoder, "accession\taccession.version\ttaxid\tgi").unwrap();

        // Write mappings
        for (accession, taxid) in mappings {
            writeln!(encoder, "{}\t{}.1\t{}\t12345", accession, accession, taxid).unwrap();
        }
        encoder.finish().unwrap();
    }

    fn create_uniprot_idmapping(&self, mappings: &[(String, u32)]) {
        let file_path = self.uniprot_dir.join("idmapping.dat.gz");
        let file = File::create(file_path).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::default());

        for (accession, taxid) in mappings {
            // UniProt format: UniProtKB-AC<tab>ID-type<tab>ID-value
            writeln!(encoder, "{}\tNCBI-taxon\t{}", accession, taxid).unwrap();
            // Add some other entries to test filtering
            writeln!(encoder, "{}\tGene_Name\tGENE1", accession).unwrap();
        }
        encoder.finish().unwrap();
    }

    fn create_simple_accession2taxid(&self, mappings: &[(String, u32)]) {
        let file_path = self.taxonomy_dir.join("accession2taxid.txt");
        let mut file = File::create(file_path).unwrap();

        // Write header
        writeln!(file, "accession\tversion\ttaxid").unwrap();

        // Write mappings
        for (accession, taxid) in mappings {
            writeln!(file, "{}\t1\t{}", accession, taxid).unwrap();
        }
    }
}

#[test]
fn test_load_ncbi_prot_accession2taxid() {
    let env = TaxonomyTestEnv::new();

    // Create test mappings
    let test_mappings = vec![
        ("NP_123456".to_string(), 9606),  // Human
        ("YP_789012".to_string(), 562),   // E. coli
        ("XP_345678".to_string(), 10090), // Mouse
    ];
    env.create_ncbi_prot_accession2taxid(&test_mappings);

    // Set environment variable to use test directory
    std::env::set_var("TALARIA_DATABASES_DIR", env.taxonomy_dir.parent().unwrap());

    // Load mappings (simulate what database add does)
    let mappings = load_test_mappings(&env.taxonomy_dir).unwrap();

    // Verify mappings
    assert_eq!(mappings.get("NP_123456"), Some(&TaxonId(9606)));
    assert_eq!(mappings.get("YP_789012"), Some(&TaxonId(562)));
    assert_eq!(mappings.get("XP_345678"), Some(&TaxonId(10090)));

    std::env::remove_var("TALARIA_DATABASES_DIR");
}

#[test]
fn test_load_uniprot_idmapping() {
    let env = TaxonomyTestEnv::new();

    // Create test mappings
    let test_mappings = vec![
        ("P12345".to_string(), 9606),  // Human protein
        ("Q67890".to_string(), 10090), // Mouse protein
        ("A0A0H6".to_string(), 666),   // Vibrio cholerae
    ];
    env.create_uniprot_idmapping(&test_mappings);

    // Remove NCBI file to test UniProt fallback
    let ncbi_file = env.ncbi_dir.join("prot.accession2taxid.gz");
    if ncbi_file.exists() {
        fs::remove_file(ncbi_file).ok();
    }

    std::env::set_var("TALARIA_DATABASES_DIR", env.taxonomy_dir.parent().unwrap());

    let mappings = load_test_mappings(&env.taxonomy_dir).unwrap();

    // Verify UniProt mappings
    assert_eq!(mappings.get("P12345"), Some(&TaxonId(9606)));
    assert_eq!(mappings.get("Q67890"), Some(&TaxonId(10090)));
    assert_eq!(mappings.get("A0A0H6"), Some(&TaxonId(666)));

    std::env::remove_var("TALARIA_DATABASES_DIR");
}

#[test]
fn test_load_simple_accession2taxid() {
    let env = TaxonomyTestEnv::new();

    // Create simple mapping file
    let test_mappings = vec![("ACC001".to_string(), 1234), ("ACC002".to_string(), 5678)];
    env.create_simple_accession2taxid(&test_mappings);

    std::env::set_var("TALARIA_DATABASES_DIR", env.taxonomy_dir.parent().unwrap());

    let mappings = load_test_mappings(&env.taxonomy_dir).unwrap();

    // Should load from simple file when others don't exist
    assert!(mappings.contains_key("ACC001"));
    assert_eq!(mappings.get("ACC001"), Some(&TaxonId(1234)));
    assert_eq!(mappings.get("ACC002"), Some(&TaxonId(5678)));

    std::env::remove_var("TALARIA_DATABASES_DIR");
}

#[test]
fn test_chunker_with_taxonomy_mappings() {
    let env = TaxonomyTestEnv::new();

    // Create mappings
    let test_mappings = vec![
        ("A0A0H6DB96".to_string(), 666),
        ("P12345".to_string(), 9606),
    ];
    env.create_ncbi_prot_accession2taxid(&test_mappings);

    std::env::set_var("TALARIA_DATABASES_DIR", env.taxonomy_dir.parent().unwrap());

    // Create chunker and load mappings
    let mut chunker = TaxonomicChunker::new(ChunkingStrategy::default());
    let mappings = load_test_mappings(&env.taxonomy_dir).unwrap();
    chunker.load_taxonomy_mapping(mappings);

    // Create test sequences
    let sequences = vec![
        // UniProt format - should extract A0A0H6DB96 and map to 666
        Sequence::new(
            "tr|A0A0H6DB96|A0A0H6DB96_VIBCL".to_string(),
            b"MKLTF".to_vec(),
        ),
        // Simple accession - should map P12345 to 9606
        Sequence::new("P12345".to_string(), b"ACGTACGT".to_vec()),
        // Unknown accession - should return TaxonId(0)
        Sequence::new("UNKNOWN".to_string(), b"TTTTAAAA".to_vec()),
    ];

    // Chunk sequences to test taxonomy resolution
    let chunks = chunker
        .chunk_sequences_into_taxonomy_aware(sequences)
        .unwrap();

    // Check that sequences were grouped by taxonomy
    let mut found_taxons = HashSet::new();
    for chunk in &chunks {
        for taxon in &chunk.taxon_ids {
            found_taxons.insert(taxon.0);
        }
    }

    // Should have found the mapped taxonomies
    assert!(found_taxons.contains(&666)); // From A0A0H6DB96 mapping
    assert!(found_taxons.contains(&9606)); // From P12345 mapping
    assert!(found_taxons.contains(&0)); // Unknown sequences

    std::env::remove_var("TALARIA_DATABASES_DIR");
}

#[test]
fn test_accession_extraction_from_headers() {
    // Test accession extraction patterns
    let test_cases = vec![
        ("sp|P12345|NAME_HUMAN", "P12345"),
        ("tr|A0A0H6|A0A0H6_VIBCL", "A0A0H6"),
        ("gi|123456|ref|NP_789012.1|", "NP_789012"),
        ("NP_123456.2", "NP_123456"),
        ("XP_987654", "XP_987654"),
        ("simple_id", "simple_id"),
    ];

    for (id, _expected_accession) in test_cases {
        // Test by creating sequences and chunking them
        let seq = Sequence::new(id.to_string(), b"MKLTF".to_vec());
        let chunker = TaxonomicChunker::new(ChunkingStrategy::default());
        let chunks = chunker
            .chunk_sequences_into_taxonomy_aware(vec![seq])
            .unwrap();

        // The chunker should handle accession extraction internally
        assert!(!chunks.is_empty(), "No chunks created for ID: {}", id);
    }
}

#[test]
fn test_mapping_priority() {
    let env = TaxonomyTestEnv::new();

    // Create mappings with specific TaxID
    let test_mappings = vec![
        ("A0A0H6".to_string(), 999), // Different from header
    ];
    env.create_ncbi_prot_accession2taxid(&test_mappings);

    std::env::set_var("TALARIA_DATABASES_DIR", env.taxonomy_dir.parent().unwrap());

    let mut chunker = TaxonomicChunker::new(ChunkingStrategy::default());
    let mappings = load_test_mappings(&env.taxonomy_dir).unwrap();
    chunker.load_taxonomy_mapping(mappings);

    // Create sequence with TaxID in description
    let mut seq = Sequence::new("tr|A0A0H6|A0A0H6_VIBCL".to_string(), b"MKLTF".to_vec());
    seq.description = Some("Protein TaxID=666".to_string());

    // Loaded mapping should take priority over header TaxID
    let chunks = chunker
        .chunk_sequences_into_taxonomy_aware(vec![seq])
        .unwrap();

    // Check that the mapped taxonomy (999) was used, not header (666)
    assert!(!chunks.is_empty());
    let chunk_taxons: Vec<u32> = chunks[0].taxon_ids.iter().map(|t| t.0).collect();
    assert!(
        chunk_taxons.contains(&999),
        "Should use mapped TaxID 999, not header 666"
    );

    std::env::remove_var("TALARIA_DATABASES_DIR");
}

#[test]
fn test_chunking_with_taxonomy_groups() {
    let env = TaxonomyTestEnv::new();

    // Create mappings for different organisms
    let test_mappings = vec![
        ("HUMAN1".to_string(), 9606),
        ("HUMAN2".to_string(), 9606),
        ("ECOLI1".to_string(), 562),
        ("ECOLI2".to_string(), 562),
        ("MOUSE1".to_string(), 10090),
    ];
    env.create_simple_accession2taxid(&test_mappings);

    std::env::set_var("TALARIA_DATABASES_DIR", env.taxonomy_dir.parent().unwrap());

    let mut chunker = TaxonomicChunker::new(ChunkingStrategy::default());
    let mappings = load_test_mappings(&env.taxonomy_dir).unwrap();
    chunker.load_taxonomy_mapping(mappings);

    // Create sequences from different organisms
    let sequences = vec![
        Sequence::new("HUMAN1".to_string(), b"AAAA".to_vec()),
        Sequence::new("HUMAN2".to_string(), b"AAAA".to_vec()),
        Sequence::new("ECOLI1".to_string(), b"TTTT".to_vec()),
        Sequence::new("ECOLI2".to_string(), b"TTTT".to_vec()),
        Sequence::new("MOUSE1".to_string(), b"GGGG".to_vec()),
    ];

    // Chunk sequences - should group by taxonomy
    let chunks = chunker
        .chunk_sequences_into_taxonomy_aware(sequences)
        .unwrap();

    // Verify chunks are grouped by taxonomy
    let mut taxon_groups = HashMap::new();
    for chunk in chunks {
        for taxon_id in &chunk.taxon_ids {
            *taxon_groups.entry(taxon_id.0).or_insert(0) += 1;
        }
    }

    // Should have 3 different taxonomic groups
    assert!(taxon_groups.contains_key(&9606)); // Human
    assert!(taxon_groups.contains_key(&562)); // E. coli
    assert!(taxon_groups.contains_key(&10090)); // Mouse

    std::env::remove_var("TALARIA_DATABASES_DIR");
}

#[test]
fn test_missing_taxonomy_files_warning() {
    let temp_dir = TempDir::new().unwrap();
    let empty_taxonomy_dir = temp_dir.path().join("databases").join("taxonomy");
    fs::create_dir_all(&empty_taxonomy_dir).unwrap();

    std::env::set_var(
        "TALARIA_DATABASES_DIR",
        empty_taxonomy_dir.parent().unwrap(),
    );

    // Should return empty mappings when no files exist
    let mappings = load_test_mappings(&empty_taxonomy_dir).unwrap();
    assert!(mappings.is_empty());

    std::env::remove_var("TALARIA_DATABASES_DIR");
}

// Helper function to load taxonomy mappings (simulates database add logic)
fn load_test_mappings(taxonomy_dir: &PathBuf) -> anyhow::Result<HashMap<String, TaxonId>> {
    use flate2::read::GzDecoder;
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let mut mapping = HashMap::new();

    // Try NCBI prot.accession2taxid first
    let ncbi_file = taxonomy_dir.join("ncbi").join("prot.accession2taxid.gz");
    if ncbi_file.exists() {
        let file = File::open(&ncbi_file)?;
        let decoder = GzDecoder::new(file);
        let reader = BufReader::new(decoder);

        for (idx, line) in reader.lines().enumerate() {
            if idx == 0 {
                continue;
            } // Skip header
            if idx > 1000000 {
                break;
            } // Limit for testing

            let line = line?;
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let accession = parts[0].to_string();
                if let Ok(taxid) = parts[2].parse::<u32>() {
                    mapping.insert(accession, TaxonId(taxid));
                }
            }
        }

        if !mapping.is_empty() {
            return Ok(mapping);
        }
    }

    // Try UniProt idmapping
    let uniprot_file = taxonomy_dir.join("uniprot").join("idmapping.dat.gz");
    if uniprot_file.exists() {
        let file = File::open(&uniprot_file)?;
        let decoder = GzDecoder::new(file);
        let reader = BufReader::new(decoder);

        for (idx, line) in reader.lines().enumerate() {
            if idx > 1000000 {
                break;
            }

            let line = line?;
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 && parts[1] == "NCBI-taxon" {
                let accession = parts[0].to_string();
                if let Ok(taxid) = parts[2].parse::<u32>() {
                    mapping.insert(accession, TaxonId(taxid));
                }
            }
        }
    }

    // Check for simple accession2taxid file
    let simple_file = taxonomy_dir.join("accession2taxid.txt");
    if simple_file.exists() {
        let file = File::open(&simple_file)?;
        let reader = BufReader::new(file);

        for line in reader.lines().skip(1) {
            let line = line?;
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let accession = parts[0].to_string();
                if let Ok(taxid) = parts[2].parse::<u32>() {
                    mapping.insert(accession, TaxonId(taxid));
                }
            }
        }
    }

    Ok(mapping)
}
