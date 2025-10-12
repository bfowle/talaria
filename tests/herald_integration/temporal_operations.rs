/// Tests for bi-temporal versioning and retroactive analysis functionality
use anyhow::Result;
use chrono::{TimeZone, Utc};
use std::path::PathBuf;
use talaria_bio::sequence::Sequence;
use talaria_herald::retroactive::RetroactiveAnalyzer;
use talaria_herald::traits::temporal::*;
use talaria_herald::types::{BiTemporalCoordinate, TaxonId};
use talaria_herald::{HeraldRepository, TaxonomyEvolutionTracker};
use tempfile::TempDir;

/// Create test sequences with taxonomic assignments
fn create_test_sequences() -> Vec<Sequence> {
    vec![
        Sequence {
            id: "NP_123456".to_string(),
            description: Some("E. coli protein".to_string()),
            sequence: b"MKVLFVTSAL".to_vec(),
            taxon_id: Some(562), // E. coli
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "YP_789012".to_string(),
            description: Some("Lactobacillus protein".to_string()),
            sequence: b"MSKLVFTGAR".to_vec(),
            taxon_id: Some(1578), // Lactobacillus
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "WP_345678".to_string(),
            description: Some("Salmonella protein".to_string()),
            sequence: b"MTNKVFTSAL".to_vec(),
            taxon_id: Some(590), // Salmonella
            taxonomy_sources: Default::default(),
        },
    ]
}

/// Create test sequences with updated taxonomy (simulating reclassification)
fn create_reclassified_sequences() -> Vec<Sequence> {
    vec![
        Sequence {
            id: "NP_123456".to_string(),
            description: Some("E. coli protein".to_string()),
            sequence: b"MKVLFVTSAL".to_vec(),
            taxon_id: Some(562), // Still E. coli
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "YP_789012".to_string(),
            description: Some("Lactobacillus protein".to_string()),
            sequence: b"MSKLVFTGAR".to_vec(),
            taxon_id: Some(33958), // Reclassified to Lactobacillaceae
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "WP_345678".to_string(),
            description: Some("Salmonella protein".to_string()),
            sequence: b"MTNKVFTSAL".to_vec(),
            taxon_id: Some(590), // Still Salmonella
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "ZP_456789".to_string(),
            description: Some("New protein".to_string()),
            sequence: b"MAQKVFTGAL".to_vec(),
            taxon_id: Some(562), // New E. coli sequence
            taxonomy_sources: Default::default(),
        },
    ]
}

/// Setup test repository with temporal data
async fn setup_test_repository(base_path: PathBuf) -> Result<HeraldRepository> {
    let mut repo = HeraldRepository::init(&base_path)?;

    // Create initial snapshot (March 2023)
    let march_2023 = Utc.with_ymd_and_hms(2023, 3, 15, 0, 0, 0).unwrap();
    let initial_sequences = create_test_sequences();

    // Store initial snapshot with temporal coordinate
    let initial_coordinate = BiTemporalCoordinate::at(march_2023);
    store_temporal_snapshot(&mut repo, initial_sequences, initial_coordinate).await?;

    // Create updated snapshot (September 2024)
    let sep_2024 = Utc.with_ymd_and_hms(2024, 9, 15, 0, 0, 0).unwrap();
    let updated_sequences = create_reclassified_sequences();

    // Store updated snapshot
    let updated_coordinate = BiTemporalCoordinate::at(sep_2024);
    store_temporal_snapshot(&mut repo, updated_sequences, updated_coordinate).await?;

    Ok(repo)
}

async fn store_temporal_snapshot(
    repo: &mut HeraldRepository,
    sequences: Vec<Sequence>,
    coordinate: BiTemporalCoordinate,
) -> Result<()> {
    use talaria_herald::chunker::TaxonomicChunker;
    use talaria_herald::ChunkingStrategy;

    // Create chunks with temporal metadata
    use talaria_herald::Chunker;

    // Create a simple chunking strategy
    let strategy = ChunkingStrategy {
        target_chunk_size: 1024 * 1024,  // 1MB
        max_chunk_size: 5 * 1024 * 1024, // 5MB
        min_sequences_per_chunk: 1,
        taxonomic_coherence: 0.8,
        special_taxa: vec![],
    };

    let mut chunker = TaxonomicChunker::new(strategy);

    // Chunk the sequences (trait method only takes sequences)
    let chunks = chunker.chunk_sequences(&sequences)?;
    let chunk_count = chunks.len();

    // Store chunks - convert metadata to bytes for storage
    for chunk_data in chunks {
        // Serialize the chunk metadata and store it
        let serialized = serde_json::to_vec(&chunk_data)?;
        repo.storage.store_chunk(&serialized, true)?;
    }

    // Add to temporal index
    let version = coordinate.sequence_time.format("%Y%m%d").to_string();
    let root_hash = talaria::herald::types::SHA256Hash::zero();
    let sequence_count = sequences.len();

    repo.temporal
        .add_sequence_version(version, root_hash, chunk_count, sequence_count)?;

    Ok(())
}

#[tokio::test]
async fn test_historical_reproduction() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo = setup_test_repository(temp_dir.path().to_path_buf()).await?;
    let analyzer = RetroactiveAnalyzer::from_repository(repo);

    // Query March 2023 state
    let march_2023 = Utc.with_ymd_and_hms(2023, 3, 15, 0, 0, 0).unwrap();
    let query = SnapshotQuery {
        coordinate: BiTemporalCoordinate::at(march_2023),
        taxon_filter: None,
    };

    let snapshot = analyzer.query_snapshot(query)?;

    // Should have 3 sequences from March 2023
    assert_eq!(snapshot.sequences.len(), 3);

    // Check specific sequence
    let ecoli_seq = snapshot
        .sequences
        .iter()
        .find(|s| s.id == "NP_123456")
        .expect("Should find E. coli sequence");

    assert_eq!(ecoli_seq.taxon_id, Some(562));

    Ok(())
}

#[tokio::test]
async fn test_retroactive_analysis() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo = setup_test_repository(temp_dir.path().to_path_buf()).await?;
    let analyzer = RetroactiveAnalyzer::from_repository(repo);

    // Apply 2024 taxonomy to 2023 sequences
    let march_2023 = Utc.with_ymd_and_hms(2023, 3, 15, 0, 0, 0).unwrap();
    let sep_2024 = Utc.with_ymd_and_hms(2024, 9, 15, 0, 0, 0).unwrap();

    let query = SnapshotQuery {
        coordinate: BiTemporalCoordinate::new(march_2023, sep_2024),
        taxon_filter: None,
    };

    let retroactive = analyzer.query_snapshot(query)?;

    // Should have 2023 sequences with 2024 taxonomy
    let lacto_seq = retroactive
        .sequences
        .iter()
        .find(|s| s.id == "YP_789012")
        .expect("Should find Lactobacillus sequence");

    // Should have updated taxonomy
    assert_eq!(lacto_seq.taxon_id, Some(33958));

    Ok(())
}

#[tokio::test]
async fn test_temporal_join() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo = setup_test_repository(temp_dir.path().to_path_buf()).await?;
    let analyzer = RetroactiveAnalyzer::from_repository(repo);

    // Find reclassified sequences between March 2023 and Sep 2024
    let march_2023 = Utc.with_ymd_and_hms(2023, 3, 15, 0, 0, 0).unwrap();
    let sep_2024 = Utc.with_ymd_and_hms(2024, 9, 15, 0, 0, 0).unwrap();

    let query = JoinQuery {
        reference_date: march_2023,
        comparison_date: Some(sep_2024),
        taxon_filter: None,
        find_reclassified: true,
    };

    let join_result = analyzer.query_join(query)?;

    // Should find the Lactobacillus reclassification
    assert!(join_result.taxonomies_changed > 0);
    assert!(!join_result.reclassified.is_empty());

    let reclassified = &join_result.reclassified[0];
    assert_eq!(reclassified.old_taxon, Some(TaxonId(1578)));
    assert_eq!(reclassified.new_taxon, Some(TaxonId(33958)));

    Ok(())
}

#[tokio::test]
async fn test_evolution_tracking() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo = setup_test_repository(temp_dir.path().to_path_buf()).await?;
    let mut tracker = TaxonomyEvolutionTracker::new(repo);

    let march_2023 = Utc.with_ymd_and_hms(2023, 3, 15, 0, 0, 0).unwrap();
    let sep_2024 = Utc.with_ymd_and_hms(2024, 9, 15, 0, 0, 0).unwrap();

    // Track evolution of Lactobacillus sequence
    let history = tracker.track_entity("YP_789012", march_2023, sep_2024)?;

    assert!(!history.events.is_empty());

    // Should have at least a reclassification event
    let has_reclassification = history
        .events
        .iter()
        .any(|e| matches!(e.event_type, EventType::Reclassified));

    assert!(has_reclassification, "Should detect reclassification");

    Ok(())
}

#[tokio::test]
async fn test_temporal_diff() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo = setup_test_repository(temp_dir.path().to_path_buf()).await?;
    let analyzer = RetroactiveAnalyzer::from_repository(repo);

    let march_2023 = Utc.with_ymd_and_hms(2023, 3, 15, 0, 0, 0).unwrap();
    let sep_2024 = Utc.with_ymd_and_hms(2024, 9, 15, 0, 0, 0).unwrap();

    let query = DiffQuery {
        from: BiTemporalCoordinate::at(march_2023),
        to: BiTemporalCoordinate::at(sep_2024),
        taxon_filter: None,
    };

    let diff = analyzer.query_diff(query)?;

    // Should detect changes
    assert!(
        !diff.sequence_changes.added.is_empty(),
        "Should find added sequences"
    );
    assert!(
        !diff.reclassifications.is_empty(),
        "Should find reclassifications"
    );

    // New sequence ZP_456789 should be in added
    assert!(diff
        .sequence_changes
        .added
        .contains(&"ZP_456789".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_taxon_evolution_report() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo = setup_test_repository(temp_dir.path().to_path_buf()).await?;
    let mut tracker = TaxonomyEvolutionTracker::new(repo);

    let march_2023 = Utc.with_ymd_and_hms(2023, 3, 15, 0, 0, 0).unwrap();
    let sep_2024 = Utc.with_ymd_and_hms(2024, 9, 15, 0, 0, 0).unwrap();

    // Generate report for E. coli
    let report = tracker.generate_taxon_report(TaxonId(562), march_2023, sep_2024)?;

    // E. coli should have gained ZP_456789
    assert!(
        !report.sequences_added.is_empty(),
        "E. coli should gain sequences"
    );
    assert!(report.sequences_added.contains(&"ZP_456789".to_string()));

    // NP_123456 should be stable
    assert!(report.sequences_stable.contains(&"NP_123456".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_mass_reclassification_detection() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo = setup_test_repository(temp_dir.path().to_path_buf()).await?;
    let mut tracker = TaxonomyEvolutionTracker::new(repo);

    let march_2023 = Utc.with_ymd_and_hms(2023, 3, 15, 0, 0, 0).unwrap();
    let sep_2024 = Utc.with_ymd_and_hms(2024, 9, 15, 0, 0, 0).unwrap();

    // Find mass reclassifications (threshold = 1 for test)
    let mass_events = tracker.find_mass_reclassifications(1, march_2023, sep_2024)?;

    // Should detect Lactobacillus reclassification
    let lacto_reclass = mass_events
        .iter()
        .find(|e| e.old_taxon == Some(TaxonId(1578)));

    assert!(
        lacto_reclass.is_some(),
        "Should detect Lactobacillus mass reclassification"
    );

    Ok(())
}

#[test]
fn test_bitemporal_coordinate() {
    let now = Utc::now();
    let coord = BiTemporalCoordinate::at(now);

    assert_eq!(coord.sequence_time, now);
    assert_eq!(coord.taxonomy_time, now);

    // Test separate times
    let seq_time = Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap();
    let tax_time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let separate_coord = BiTemporalCoordinate::new(seq_time, tax_time);

    assert_eq!(separate_coord.sequence_time, seq_time);
    assert_eq!(separate_coord.taxonomy_time, tax_time);
}

#[test]
fn test_taxon_id_display() {
    let taxon = TaxonId(562);
    assert_eq!(format!("{}", taxon), "taxid:562");
}

#[test]
fn test_wrapped_fasta_header_parsing() {
    use talaria_bio::fasta::parse_fasta_from_bytes;

    // Test case from the cholera database with wrapped header
    let input = b">UniRef100_A0A0B2VFC3 Flagellin B n=4 Tax=Vibrio cholerae
 TaxID=666 RepID=A0A0B2VFC3_VIBCH
MAQVINTNSL";

    let sequences = parse_fasta_from_bytes(input).unwrap();
    assert!(!sequences.is_empty(), "Should parse wrapped header");

    let seq = &sequences[0];
    assert_eq!(seq.id, "UniRef100_A0A0B2VFC3");
    assert!(seq.description.as_ref().unwrap().contains("TaxID=666"));
    assert_eq!(seq.taxon_id, Some(666));
}

#[test]
fn test_ox_field_fallback() {
    use talaria_bio::fasta::parse_fasta_from_bytes;

    // Test OX= field extraction when TaxID=0
    let fasta_with_ox = b">test_seq Some protein OX=12345 GN=gene
MTEST";
    let sequences = parse_fasta_from_bytes(fasta_with_ox).unwrap();
    assert_eq!(sequences[0].taxon_id, Some(12345));

    // Test TaxID precedence over OX
    let fasta_with_both = b">test_seq Protein TaxID=562 OX=12345
MTEST";
    let sequences2 = parse_fasta_from_bytes(fasta_with_both).unwrap();
    assert_eq!(sequences2[0].taxon_id, Some(562));

    // Test TaxID=0 triggers OX fallback
    let fasta_with_zero = b">test_seq Protein TaxID=0 OX=9606
MTEST";
    let sequences3 = parse_fasta_from_bytes(fasta_with_zero).unwrap();
    assert_eq!(sequences3[0].taxon_id, Some(9606));
}
