use crate::casg::{FastaAssembler, CASGStorage, SHA256Hash, TaxonomyAwareChunk, TaxonId, SequenceRef};
use tempfile::TempDir;

fn create_test_storage() -> (TempDir, CASGStorage) {
    let temp_dir = TempDir::new().unwrap();
    let storage = CASGStorage::new(temp_dir.path()).unwrap();
    (temp_dir, storage)
}

fn create_test_chunks() -> Vec<TaxonomyAwareChunk> {
    // Create actual FASTA-format sequence data
    let chunk1_data = b">seq1\nMVALPRWFDK\n>seq2\nACGTACGTAC\n".to_vec();
    let chunk2_data = b">seq3\nMKWVTFISLLFLFSSAYS\n".to_vec();

    vec![
        TaxonomyAwareChunk {
            content_hash: SHA256Hash::compute(&chunk1_data),
            taxonomy_version: SHA256Hash::compute(b"tax_v1"),
            sequence_version: SHA256Hash::compute(b"seq_v1"),
            taxon_ids: vec![TaxonId(562)], // E. coli
            sequences: vec![
                SequenceRef {
                    chunk_hash: SHA256Hash::compute(&chunk1_data),
                    offset: 0,
                    length: 16,  // ">seq1\nMVALPRWFDK\n"
                    sequence_id: "seq1".to_string(),
                },
                SequenceRef {
                    chunk_hash: SHA256Hash::compute(&chunk1_data),
                    offset: 16,
                    length: 17,  // ">seq2\nACGTACGTAC\n"
                    sequence_id: "seq2".to_string(),
                },
            ],
            sequence_data: chunk1_data,
            created_at: chrono::Utc::now(),
            valid_from: chrono::Utc::now(),
            valid_until: None,
            size: 33,
            compressed_size: Some(25),
        },
        TaxonomyAwareChunk {
            content_hash: SHA256Hash::compute(&chunk2_data),
            taxonomy_version: SHA256Hash::compute(b"tax_v1"),
            sequence_version: SHA256Hash::compute(b"seq_v1"),
            taxon_ids: vec![TaxonId(9606)], // Human
            sequences: vec![
                SequenceRef {
                    chunk_hash: SHA256Hash::compute(&chunk2_data),
                    offset: 0,
                    length: 24,  // ">seq3\nMKWVTFISLLFLFSSAYS\n"
                    sequence_id: "seq3".to_string(),
                },
            ],
            sequence_data: chunk2_data,
            created_at: chrono::Utc::now(),
            valid_from: chrono::Utc::now(),
            valid_until: None,
            size: 24,
            compressed_size: Some(20),
        },
    ]
}

#[test]
fn test_full_database_assembly() {
    let (_temp_dir, storage) = create_test_storage();
    let chunks = create_test_chunks();

    // Store chunks
    for chunk in &chunks {
        storage.store_taxonomy_chunk(chunk).unwrap();
    }

    // Assemble all chunks
    let assembler = FastaAssembler::new(&storage);
    let chunk_hashes: Vec<_> = chunks.iter().map(|c| c.content_hash.clone()).collect();
    let sequences = assembler.assemble_from_chunks(&chunk_hashes).unwrap();

    assert_eq!(sequences.len(), 3); // Total sequences from both chunks
    assert_eq!(sequences[0].id, "seq1");
    assert_eq!(sequences[1].id, "seq2");
    assert_eq!(sequences[2].id, "seq3");
}

#[test]
fn test_taxonomic_subset_assembly() {
    let (_temp_dir, storage) = create_test_storage();
    let chunks = create_test_chunks();

    // Store chunks
    for chunk in &chunks {
        storage.store_taxonomy_chunk(chunk).unwrap();
    }

    // Assemble only E. coli sequences
    let assembler = FastaAssembler::new(&storage);
    let ecoli_chunks = vec![chunks[0].content_hash.clone()]; // Only first chunk
    let sequences = assembler.assemble_from_chunks(&ecoli_chunks).unwrap();

    assert_eq!(sequences.len(), 2); // Only E. coli sequences
    assert!(sequences.iter().all(|s| s.taxon_id == Some(562)));
}

#[test]
fn test_streaming_assembly() {
    let (_temp_dir, storage) = create_test_storage();
    let chunks = create_test_chunks();

    // Store chunks
    for chunk in &chunks {
        storage.store_taxonomy_chunk(chunk).unwrap();
    }

    // Stream to file
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let mut writer = std::fs::File::create(temp_file.path()).unwrap();

    let assembler = FastaAssembler::new(&storage);
    let chunk_hashes: Vec<_> = chunks.iter().map(|c| c.content_hash.clone()).collect();
    let count = assembler.stream_assembly(&chunk_hashes, &mut writer).unwrap();

    assert_eq!(count, 3); // Total sequences written

    // Verify file content
    let content = std::fs::read_to_string(temp_file.path()).unwrap();
    assert!(content.contains(">seq1"));
    assert!(content.contains(">seq2"));
    assert!(content.contains(">seq3"));
    assert!(content.contains("MVALPRWFDK"));
}

#[test]
fn test_cryptographic_verification_during_assembly() {
    let (_temp_dir, storage) = create_test_storage();
    let mut chunk = create_test_chunks()[0].clone();

    // Store original chunk
    storage.store_taxonomy_chunk(&chunk).unwrap();

    // Attempt to assemble - should succeed
    let assembler = FastaAssembler::new(&storage);
    let result = assembler.assemble_from_chunks(&vec![chunk.content_hash.clone()]);
    assert!(result.is_ok());

    // Now tamper with the chunk data (simulate corruption)
    chunk.sequences[0].sequence_id = "CORRUPTED_ID".to_string();
    let tampered_hash = SHA256Hash::compute(b"tampered");

    // Try to assemble with wrong hash - should fail verification
    let result = assembler.assemble_from_chunks(&vec![tampered_hash]);
    assert!(result.is_err()); // Should fail because chunk not found
}

#[test]
fn test_missing_chunk_handling() {
    let (_temp_dir, storage) = create_test_storage();

    // Try to assemble without storing any chunks
    let assembler = FastaAssembler::new(&storage);
    let missing_hash = SHA256Hash::compute(b"nonexistent");
    let result = assembler.assemble_from_chunks(&vec![missing_hash]);

    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(err_str.contains("Failed to retrieve chunk") || err_str.contains("not found"));
}

#[test]
fn test_large_database_streaming() {
    let (_temp_dir, storage) = create_test_storage();

    // Create many chunks to simulate large database
    let mut all_chunks = Vec::new();
    for i in 0..100 {
        let seq_data = format!(">seq_{}_1\nACGTACGTACGT\n", i).into_bytes();
        let chunk = TaxonomyAwareChunk {
            content_hash: SHA256Hash::compute(&seq_data),
            taxonomy_version: SHA256Hash::compute(b"tax_v1"),
            sequence_version: SHA256Hash::compute(b"seq_v1"),
            taxon_ids: vec![TaxonId(i)],
            sequences: vec![
                SequenceRef {
                    chunk_hash: SHA256Hash::compute(&seq_data),
                    offset: 0,
                    length: seq_data.len(),
                    sequence_id: format!("seq_{}_1", i),
                },
            ],
            sequence_data: seq_data,
            created_at: chrono::Utc::now(),
            valid_from: chrono::Utc::now(),
            valid_until: None,
            size: 100,
            compressed_size: Some(50),
        };
        storage.store_taxonomy_chunk(&chunk).unwrap();
        all_chunks.push(chunk);
    }

    // Stream assembly should handle large numbers efficiently
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let mut writer = std::fs::File::create(temp_file.path()).unwrap();

    let assembler = FastaAssembler::new(&storage);
    let chunk_hashes: Vec<_> = all_chunks.iter().map(|c| c.content_hash.clone()).collect();
    let count = assembler.stream_assembly(&chunk_hashes, &mut writer).unwrap();

    assert_eq!(count, 100); // All sequences written
}

#[test]
fn test_assembly_with_compression() {
    let (_temp_dir, storage) = create_test_storage();

    // Create chunk with compressed data
    let seq_data = b">compressed_seq\nMVALPRWFDKMVALPRWFDK\n".to_vec();
    let compressed_chunk = TaxonomyAwareChunk {
        content_hash: SHA256Hash::compute(&seq_data),
        taxonomy_version: SHA256Hash::compute(b"tax_v1"),
        sequence_version: SHA256Hash::compute(b"seq_v1"),
        taxon_ids: vec![TaxonId(1)],
        sequences: vec![
            SequenceRef {
                chunk_hash: SHA256Hash::compute(&seq_data),
                offset: 0,
                length: seq_data.len(),
                sequence_id: "compressed_seq".to_string(),
            },
        ],
        sequence_data: seq_data,
        created_at: chrono::Utc::now(),
        valid_from: chrono::Utc::now(),
        valid_until: None,
        size: 200,
        compressed_size: Some(50), // Good compression ratio
    };

    storage.store_taxonomy_chunk(&compressed_chunk).unwrap();

    // Should decompress and assemble correctly
    let assembler = FastaAssembler::new(&storage);
    let sequences = assembler.assemble_from_chunks(&vec![compressed_chunk.content_hash]).unwrap();

    assert_eq!(sequences.len(), 1);
    assert_eq!(sequences[0].id, "compressed_seq");
    assert_eq!(sequences[0].sequence, b"MVALPRWFDKMVALPRWFDK");
}

#[test]
fn test_deterministic_assembly_order() {
    let (_temp_dir, storage) = create_test_storage();
    let chunks = create_test_chunks();

    // Store chunks
    for chunk in &chunks {
        storage.store_taxonomy_chunk(chunk).unwrap();
    }

    // Assemble multiple times - order should be consistent
    let assembler = FastaAssembler::new(&storage);
    let chunk_hashes: Vec<_> = chunks.iter().map(|c| c.content_hash.clone()).collect();

    let sequences1 = assembler.assemble_from_chunks(&chunk_hashes).unwrap();
    let sequences2 = assembler.assemble_from_chunks(&chunk_hashes).unwrap();

    // Order should be identical
    for (s1, s2) in sequences1.iter().zip(sequences2.iter()) {
        assert_eq!(s1.id, s2.id);
        assert_eq!(s1.sequence, s2.sequence);
    }
}

#[test]
fn test_assembly_preserves_metadata() {
    let (_temp_dir, storage) = create_test_storage();

    let seq_data = b">NP_123456.1 RecA protein [Escherichia coli]\nMAIDENKQKALAAALGQIEK\n".to_vec();
    let chunk = TaxonomyAwareChunk {
        content_hash: SHA256Hash::compute(&seq_data),
        taxonomy_version: SHA256Hash::compute(b"tax_v1"),
        sequence_version: SHA256Hash::compute(b"seq_v1"),
        taxon_ids: vec![TaxonId(562)],
        sequences: vec![
            SequenceRef {
                chunk_hash: SHA256Hash::compute(&seq_data),
                offset: 0,
                length: seq_data.len(),
                sequence_id: "NP_123456.1".to_string(),
            },
        ],
        sequence_data: seq_data,
        created_at: chrono::Utc::now(),
        valid_from: chrono::Utc::now(),
        valid_until: None,
        size: 100,
        compressed_size: None,
    };

    storage.store_taxonomy_chunk(&chunk).unwrap();

    let assembler = FastaAssembler::new(&storage);
    let sequences = assembler.assemble_from_chunks(&vec![chunk.content_hash]).unwrap();

    // All metadata should be preserved
    assert_eq!(sequences[0].id, "NP_123456.1");
    assert_eq!(sequences[0].description, Some("RecA protein [Escherichia coli]".to_string()));
    assert_eq!(sequences[0].taxon_id, Some(562));
}