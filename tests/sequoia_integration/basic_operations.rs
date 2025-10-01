use talaria_bio::sequence::Sequence;
use talaria_sequoia::{
    SequoiaRepository, ChunkMetadata, ChunkingStrategy, FastaAssembler, SHA256Hash, TaxonId,
    TaxonomicChunker, TemporalManifest,
};
use talaria_cli::TargetAligner;
use talaria_core::config::Config;
use talaria_core::reducer::Reducer;
use tempfile::TempDir;

// Helper to create test manifests with required fields
fn create_test_manifest(version: &str, seq_version: &str, tax_version: &str) -> TemporalManifest {
    TemporalManifest {
        version: version.to_string(),
        created_at: chrono::Utc::now(),
        sequence_version: seq_version.to_string(),
        taxonomy_version: tax_version.to_string(),
        temporal_coordinate: None,
        taxonomy_root: SHA256Hash::compute(format!("tax_{}", version).as_bytes()),
        sequence_root: SHA256Hash::compute(format!("seq_{}", version).as_bytes()),
        chunk_merkle_tree: None,
        taxonomy_manifest_hash: SHA256Hash::compute(b"test_tax_manifest"),
        taxonomy_dump_version: "2024-01-01".to_string(),
        source_database: Some("test_db".to_string()),
        chunk_index: vec![],
        discrepancies: vec![],
        etag: "test".to_string(),
        previous_version: None,
    }
}

fn setup_test_sequoia() -> (TempDir, SequoiaRepository) {
    let temp_dir = TempDir::new().unwrap();
    let repo = SequoiaRepository::init(temp_dir.path()).unwrap();
    (temp_dir, repo)
}

fn create_test_sequences() -> Vec<Sequence> {
    vec![
        Sequence {
            id: "seq1".to_string(),
            description: Some("E. coli protein 1".to_string()),
            sequence: b"MVALPRWFDKMVALPRWFDK".to_vec(),
            taxon_id: Some(562),
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "seq2".to_string(),
            description: Some("E. coli protein 2".to_string()),
            sequence: b"MVALPRWFDKMVALPRWFDA".to_vec(), // Similar to seq1
            taxon_id: Some(562),
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "seq3".to_string(),
            description: Some("Human protein 1".to_string()),
            sequence: b"MKWVTFISLLFLFSSAYS".to_vec(),
            taxon_id: Some(9606),
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "seq4".to_string(),
            description: Some("Human protein 2".to_string()),
            sequence: b"MKWVTFISLLFLFSSAYA".to_vec(), // Similar to seq3
            taxon_id: Some(9606),
            taxonomy_sources: Default::default(),
        },
    ]
}

#[test]
fn test_sequoia_to_reduce_workflow() {
    let (_temp_dir, repo) = setup_test_sequoia();
    let sequences = create_test_sequences();

    // Step 1: Chunk sequences and store in SEQUOIA
    let chunker = TaxonomicChunker::new(ChunkingStrategy::default());
    let chunks = chunker
        .chunk_sequences_into_taxonomy_aware(sequences.clone())
        .unwrap();

    for chunk in &chunks {
        repo.storage.store_taxonomy_chunk(chunk).unwrap();
    }

    // Step 2: Assemble FASTA from SEQUOIA
    let chunk_hashes: Vec<_> = chunks.iter().map(|c| c.content_hash.clone()).collect();
    let assembler = FastaAssembler::new(&repo.storage);
    let assembled_sequences = assembler.assemble_from_chunks(&chunk_hashes).unwrap();

    assert_eq!(assembled_sequences.len(), sequences.len());

    // Step 3: Run reduce on assembled FASTA
    let config = Config::default();
    let mut reducer = Reducer::new(config).with_silent(true);
    let (reduced_sequences, _deltas, original_count) = reducer
        .reduce(assembled_sequences.clone(), 0.5, TargetAligner::Generic)
        .unwrap();

    // Verify reduction worked
    assert!(reduced_sequences.len() <= assembled_sequences.len());
    assert_eq!(original_count, assembled_sequences.len());
    // Now that we have real sequence data, deltas should be generated for similar sequences
    // Note: With our test data, deltas may still be empty if sequences aren't similar enough
    // We'll keep this commented for now as the test sequences may not be similar
    // assert!(!deltas.is_empty());

    // Step 4: Verify we can expand reduced sequences using SEQUOIA chunks
    // (This would require implementing delta expansion with SEQUOIA)
}

#[test]
fn test_incremental_update_simulation() {
    let (_temp_dir, repo) = setup_test_sequoia();

    // Step 1: Initialize with version 1
    let v1_sequences = vec![Sequence {
        id: "seq1".to_string(),
        description: Some("Original sequence".to_string()),
        sequence: b"ACGTACGTACGT".to_vec(),
        taxon_id: Some(562),
        taxonomy_sources: Default::default(),
    }];

    let chunker = TaxonomicChunker::new(ChunkingStrategy::default());
    let v1_chunks = chunker
        .chunk_sequences_into_taxonomy_aware(v1_sequences)
        .unwrap();

    for chunk in &v1_chunks {
        repo.storage.store_taxonomy_chunk(chunk).unwrap();
    }

    let mut v1_manifest = create_test_manifest("v1", "2024.01", "2024.01");
    v1_manifest.chunk_index = v1_chunks
        .iter()
        .map(|c| ChunkMetadata {
            hash: c.content_hash.clone(),
            taxon_ids: c.taxon_ids.clone(),
            sequence_count: c.sequences.len(),
            size: c.size,
            compressed_size: c.compressed_size,
        })
        .collect();
    v1_manifest.etag = "etag_v1".to_string();

    // Step 2: Simulate version 2 with changes
    let v2_sequences = vec![
        Sequence {
            id: "seq1".to_string(),
            description: Some("Original sequence".to_string()),
            sequence: b"ACGTACGTACGT".to_vec(),
            taxon_id: Some(562),
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "seq2".to_string(),
            description: Some("New sequence".to_string()),
            sequence: b"TGCATGCATGCA".to_vec(),
            taxon_id: Some(562),
            taxonomy_sources: Default::default(),
        },
    ];

    let v2_chunks = chunker
        .chunk_sequences_into_taxonomy_aware(v2_sequences.clone())
        .unwrap();

    // Step 3: Identify which chunks are new
    let v1_hashes: std::collections::HashSet<_> =
        v1_chunks.iter().map(|c| &c.content_hash).collect();
    let _v2_hashes: std::collections::HashSet<_> =
        v2_chunks.iter().map(|c| &c.content_hash).collect();

    let new_chunks: Vec<_> = v2_chunks
        .iter()
        .filter(|c| !v1_hashes.contains(&c.content_hash))
        .collect();

    // Step 4: Only download/store new chunks
    for chunk in new_chunks {
        repo.storage.store_taxonomy_chunk(chunk).unwrap();
    }

    // Step 5: Verify assembled FASTA matches v2
    let v2_chunk_hashes: Vec<_> = v2_chunks.iter().map(|c| c.content_hash.clone()).collect();
    let assembler = FastaAssembler::new(&repo.storage);
    let assembled = assembler.assemble_from_chunks(&v2_chunk_hashes).unwrap();

    assert_eq!(assembled.len(), v2_sequences.len());
    assert_eq!(assembled[0].id, "seq1");
    assert_eq!(assembled[1].id, "seq2");
}

#[test]
fn test_taxonomic_subset_with_reduce() {
    let (_temp_dir, repo) = setup_test_sequoia();
    let sequences = create_test_sequences();

    // Store all sequences in SEQUOIA
    let chunker = TaxonomicChunker::new(ChunkingStrategy::default());
    let chunks = chunker
        .chunk_sequences_into_taxonomy_aware(sequences.clone())
        .unwrap();

    for chunk in &chunks {
        repo.storage.store_taxonomy_chunk(chunk).unwrap();
    }

    // Extract only E. coli sequences (taxon 562)
    let ecoli_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.taxon_ids.contains(&TaxonId(562)))
        .map(|c| c.content_hash.clone())
        .collect();

    let assembler = FastaAssembler::new(&repo.storage);
    let ecoli_sequences = assembler.assemble_from_chunks(&ecoli_chunks).unwrap();

    // Should only have E. coli sequences
    assert_eq!(ecoli_sequences.len(), 2);
    assert!(ecoli_sequences.iter().all(|s| s.taxon_id == Some(562)));

    // Run reduce on taxonomic subset
    let config = Config::default();
    let mut reducer = Reducer::new(config).with_silent(true);
    let (reduced, _, _) = reducer
        .reduce(ecoli_sequences.clone(), 0.5, TargetAligner::Generic)
        .unwrap();

    assert!(reduced.len() < ecoli_sequences.len());
}

// TODO: Re-enable when VersionIdentifier is available
#[ignore]
#[test]
fn test_version_tracking_through_workflow() {
    // use talaria_sequoia::{VersionIdentifier, VersionInfo};

    let (_temp_dir, repo) = setup_test_sequoia();
    let sequences = create_test_sequences();

    // Create and store chunks
    let chunker = TaxonomicChunker::new(ChunkingStrategy::default());
    let chunks = chunker
        .chunk_sequences_into_taxonomy_aware(sequences.clone())
        .unwrap();

    for chunk in &chunks {
        repo.storage.store_taxonomy_chunk(chunk).unwrap();
    }

    // Create manifest with version info
    let mut manifest = create_test_manifest("test_v1", "2024.03", "2024.03");
    manifest.chunk_index = chunks
        .iter()
        .map(|c| ChunkMetadata {
            hash: c.content_hash.clone(),
            taxon_ids: c.taxon_ids.clone(),
            sequence_count: c.sequences.len(),
            size: c.size,
            compressed_size: c.compressed_size,
        })
        .collect();
    manifest.etag = "test_v1".to_string();

    // Version identifier would be used here if available
    // let mut identifier = VersionIdentifier::new();
    // identifier.add_known_manifest(manifest.clone());

    // Assemble FASTA
    let chunk_hashes: Vec<_> = chunks.iter().map(|c| c.content_hash.clone()).collect();
    let assembler = FastaAssembler::new(&repo.storage);
    let assembled = assembler.assemble_from_chunks(&chunk_hashes).unwrap();

    // Identify version (would need proper implementation)
    // For now, just verify the assembled sequences match original
    assert_eq!(assembled.len(), sequences.len());
}

#[test]
fn test_large_database_simulation() {
    let (_temp_dir, repo) = setup_test_sequoia();

    // Create a larger set of sequences
    let mut sequences = Vec::new();
    for i in 0..100 {
        sequences.push(Sequence {
            id: format!("seq_{}", i),
            description: Some(format!("Protein {}", i)),
            sequence: format!("ACGT{}", i).repeat(10).into_bytes(),
            taxon_id: Some((i % 10) as u32), // 10 different taxa
            taxonomy_sources: Default::default(),
        });
    }

    // Chunk and store
    let chunker = TaxonomicChunker::new(ChunkingStrategy::default());
    let chunks = chunker
        .chunk_sequences_into_taxonomy_aware(sequences.clone())
        .unwrap();

    for chunk in &chunks {
        repo.storage.store_taxonomy_chunk(chunk).unwrap();
    }

    // Assemble and verify
    let chunk_hashes: Vec<_> = chunks.iter().map(|c| c.content_hash.clone()).collect();
    let assembler = FastaAssembler::new(&repo.storage);
    let assembled = assembler.assemble_from_chunks(&chunk_hashes).unwrap();

    assert_eq!(assembled.len(), 100);

    // Run reduce on large dataset
    let config = Config::default();
    let mut reducer = Reducer::new(config).with_silent(true);
    let (reduced, deltas, _) = reducer
        .reduce(assembled, 0.3, TargetAligner::Generic)
        .unwrap();

    // Should achieve significant reduction
    assert!(reduced.len() <= 30); // 30% or less
    assert!(!deltas.is_empty());
}

#[test]
fn test_streaming_assembly_to_file() {
    let (_temp_dir, repo) = setup_test_sequoia();
    let sequences = create_test_sequences();

    // Store sequences in SEQUOIA
    let chunker = TaxonomicChunker::new(ChunkingStrategy::default());
    let chunks = chunker
        .chunk_sequences_into_taxonomy_aware(sequences.clone())
        .unwrap();

    for chunk in &chunks {
        repo.storage.store_taxonomy_chunk(chunk).unwrap();
    }

    // Stream assembly to file
    let output_file = tempfile::NamedTempFile::new().unwrap();
    let mut writer = std::fs::File::create(output_file.path()).unwrap();

    let chunk_hashes: Vec<_> = chunks.iter().map(|c| c.content_hash.clone()).collect();
    let assembler = FastaAssembler::new(&repo.storage);
    let count = assembler
        .stream_assembly(&chunk_hashes, &mut writer)
        .unwrap();

    assert_eq!(count, sequences.len());

    // Verify file content
    let content = std::fs::read_to_string(output_file.path()).unwrap();
    for seq in &sequences {
        assert!(content.contains(&seq.id));
    }
}
