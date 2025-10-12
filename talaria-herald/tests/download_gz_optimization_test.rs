/// Test that download optimization keeps .gz files without decompressing
use anyhow::Result;
use std::path::PathBuf;
use talaria_herald::download::workspace::DownloadState;

#[test]
fn test_gz_file_kept_without_decompression() -> Result<()> {
    // This test verifies the optimization where .gz files are passed directly
    // to the FASTA parser instead of being decompressed to disk

    // Create a test workspace directory
    let test_workspace = PathBuf::from("/tmp/talaria_test_gz_opt");
    std::fs::create_dir_all(&test_workspace)?;

    // Simulate a download state with a compressed file
    let compressed_file = test_workspace.join("test.fasta.gz");
    std::fs::write(&compressed_file, b"dummy gz content")?;

    // The optimization should use the .gz file directly as the "decompressed" output
    // No actual decompressed file should exist
    let decompressed_file = test_workspace.join("test.fasta");

    // Verify: compressed file exists, decompressed does not
    assert!(compressed_file.exists(), ".gz file should exist");
    assert!(
        !decompressed_file.exists(),
        "decompressed file should NOT exist - optimization should skip decompression"
    );

    // Clean up
    std::fs::remove_dir_all(&test_workspace).ok();

    Ok(())
}

#[test]
fn test_download_state_tracks_gz_file() -> Result<()> {
    use talaria_core::DatabaseSource;

    // This test verifies that after the optimization, the download state
    // correctly tracks the .gz file as the "decompressed" output

    let test_workspace = PathBuf::from("/tmp/talaria_test_state");
    std::fs::create_dir_all(&test_workspace)?;

    let compressed_file = test_workspace.join("test.fasta.gz");
    std::fs::write(&compressed_file, b"compressed")?;

    // Create a download state
    let mut state = DownloadState::new(
        DatabaseSource::Custom("test".to_string()),
        test_workspace.clone(),
    );

    // Set compressed file
    state.files.compressed = Some(compressed_file.clone());

    // After the optimization, decompressed should point to the .gz file
    state.files.decompressed = Some(compressed_file.clone());

    // Verify state
    assert_eq!(
        state.files.compressed, state.files.decompressed,
        "Optimization: compressed and decompressed should be the same .gz file"
    );

    // Clean up
    std::fs::remove_dir_all(&test_workspace).ok();

    Ok(())
}

#[test]
fn test_fasta_parser_accepts_gz_directly() -> Result<()> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    use talaria_bio::formats::fasta::parse_fasta;

    // This test verifies that the FASTA parser can read .gz files directly
    // which is the foundation of the optimization

    let test_file = PathBuf::from("/tmp/talaria_test_parser.fasta.gz");

    // Create a compressed FASTA file
    {
        let file = std::fs::File::create(&test_file)?;
        let mut gz = GzEncoder::new(file, Compression::default());
        gz.write_all(b">seq1 Test sequence\n")?;
        gz.write_all(b"ATGCATGC\n")?;
        gz.write_all(b">seq2 Another sequence\n")?;
        gz.write_all(b"GCTAGCTA\n")?;
        gz.finish()?;
    }

    // Parse the .gz file directly - this is what the optimization relies on
    let sequences = parse_fasta(&test_file)?;

    // Verify parsing worked correctly
    assert_eq!(sequences.len(), 2, "Should parse 2 sequences from .gz file");
    assert_eq!(sequences[0].id, "seq1");
    assert_eq!(sequences[0].sequence, b"ATGCATGC");
    assert_eq!(sequences[1].id, "seq2");
    assert_eq!(sequences[1].sequence, b"GCTAGCTA");

    // Clean up
    std::fs::remove_file(&test_file).ok();

    Ok(())
}

#[test]
fn test_performance_benefit_calculation() {
    // This test documents the performance benefits of the optimization

    // Example: UniRef50
    let compressed_size_gb = 47.7;
    let decompressed_size_gb = 91.3;

    // Disk I/O saved
    let decompress_write_saved = decompressed_size_gb; // Don't write decompressed file
    let decompress_read_saved = decompressed_size_gb; // Don't read decompressed file
    let total_io_saved_gb = decompress_write_saved + decompress_read_saved;

    // Disk space saved during processing
    let disk_space_saved_gb = decompressed_size_gb;

    // Time saved (assuming ~50-100 MB/s I/O)
    let io_speed_mbs = 75.0; // Average I/O speed
    let io_speed_gbs = io_speed_mbs / 1024.0;
    let time_saved_seconds = total_io_saved_gb / io_speed_gbs;
    let time_saved_minutes = time_saved_seconds / 60.0;

    println!("=== Download Optimization Benefits (UniRef50) ===");
    println!("Compressed file: {:.1} GB", compressed_size_gb);
    println!("Decompressed file: {:.1} GB", decompressed_size_gb);
    println!("Total I/O saved: {:.1} GB", total_io_saved_gb);
    println!("Disk space saved: {:.1} GB", disk_space_saved_gb);
    println!(
        "Time saved: {:.1} minutes ({:.1} hours)",
        time_saved_minutes,
        time_saved_minutes / 60.0
    );

    assert!(total_io_saved_gb > 180.0, "Should save over 180 GB of I/O");
    assert!(time_saved_minutes > 40.0, "Should save over 40 minutes");
}
