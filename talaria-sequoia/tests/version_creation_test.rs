/// Test to ensure version creation doesn't create multiple versions during a single operation
use anyhow::Result;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;
use talaria_sequoia::database::DatabaseManager;
use talaria_core::{DatabaseSource, UniProtDatabase};
use talaria_bio::sequence::Sequence;
use talaria_bio::taxonomy::TaxonomySources;

#[test]
#[serial]
fn test_single_version_creation_for_batch_processing() -> Result<()> {
    // Setup test environment
    let temp_dir = TempDir::new()?;
    std::env::set_var("TALARIA_DATA_DIR", temp_dir.path());

    // Create database manager
    let mut manager = DatabaseManager::new(None)?;

    // Create test sequences (simulating multiple batches)
    let mut all_sequences = Vec::new();
    for batch_num in 0..3 {
        for seq_num in 0..100 {
            all_sequences.push(Sequence {
                id: format!("SEQ_{}_{}|Test sequence", batch_num, seq_num),
                description: None,
                sequence: vec![b'A'; 100], // Simple test sequence
                taxon_id: Some(9606), // Human
                taxonomy_sources: TaxonomySources::default(), // Empty taxonomy sources
            });
        }
    }

    let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);

    // Process all sequences at once (should create only ONE version)
    manager.chunk_sequences_direct(all_sequences, &source)?;

    // Check versions directory
    let versions_dir = temp_dir.path()
        .join("databases")
        .join("versions")
        .join("uniprot")
        .join("swissprot");

    // Count version directories (should be exactly 1)
    let entries: Vec<_> = fs::read_dir(&versions_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            // Filter out symlinks like 'current' or temporal aliases
            let path = entry.path();
            if path.is_symlink() {
                return false;
            }
            // Check if it looks like a timestamp version (YYYYMMDD_HHMMSS)
            let name = entry.file_name().to_string_lossy().to_string();
            name.len() == 15 && name.chars().nth(8) == Some('_')
        })
        .collect();

    assert_eq!(entries.len(), 1, "Expected exactly 1 version directory, found {}", entries.len());

    // Clean up
    std::env::remove_var("TALARIA_DATA_DIR");
    Ok(())
}

#[test]
#[serial]
fn test_streaming_mode_single_version() -> Result<()> {
    use std::io::Write;

    // Setup test environment
    let temp_dir = TempDir::new()?;
    std::env::set_var("TALARIA_DATA_DIR", temp_dir.path());

    // Create a test FASTA file
    let fasta_path = temp_dir.path().join("test.fasta");
    let mut fasta_file = fs::File::create(&fasta_path)?;

    // Write enough sequences to trigger streaming mode (>100MB)
    for i in 0..100_000 {
        writeln!(fasta_file, ">SEQ_{} Test sequence", i)?;
        writeln!(fasta_file, "ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT")?;
    }
    fasta_file.sync_all()?;

    // Create database manager
    let mut manager = DatabaseManager::new(None)?;
    let source = DatabaseSource::UniProt(UniProtDatabase::UniRef50);

    // Process file in streaming mode (should still create only ONE version)
    manager.chunk_database(&fasta_path, &source)?;

    // Check versions directory
    let versions_dir = temp_dir.path()
        .join("databases")
        .join("versions")
        .join("uniprot")
        .join("uniref50");

    // Count version directories
    let entries: Vec<_> = fs::read_dir(&versions_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            if path.is_symlink() {
                return false;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            name.len() == 15 && name.chars().nth(8) == Some('_')
        })
        .collect();

    assert_eq!(entries.len(), 1,
        "Expected exactly 1 version directory in streaming mode, found {}", entries.len());

    // Clean up
    std::env::remove_var("TALARIA_DATA_DIR");
    Ok(())
}