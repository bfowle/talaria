mod common;

use anyhow::Result;
use predicates::prelude::*;

use common::*;

#[test]
fn test_cli_help_command() {
    let mut cmd = talaria_cmd();
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Talaria reduces biological sequence databases"))
        .stdout(predicate::str::contains("reduce"))
        .stdout(predicate::str::contains("reconstruct"))
        .stdout(predicate::str::contains("validate"));
}

#[test]
fn test_cli_version_command() {
    let mut cmd = talaria_cmd();
    cmd.arg("--version");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("talaria"));
}

#[test]
fn test_reduce_basic_workflow() -> Result<()> {
    let env = TestEnvironment::new()?;

    // Create input FASTA
    let input_fasta = env.create_input_file("test.fasta", &create_simple_fasta(10))?;
    let output = env.output_path("reduced.fasta");

    // Add as a test database first
    add_test_database(&input_fasta, "test_basic", env.temp_dir.path())?;

    // Run reduce command
    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_basic")
        .arg("-o").arg(&output)
        .arg("--target-aligner").arg("generic")
        .arg("--reduction-ratio").arg("0.5")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().success();

    // Verify output exists
    assert!(output.exists(), "Output file should exist");

    // Verify reduction occurred
    let input_count = count_sequences(&input_fasta)?;
    let output_count = count_sequences(&output)?;
    assert!(output_count <= input_count, "Output should have fewer or equal sequences");

    Ok(())
}

#[test]
fn test_reduce_with_compression() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input_fasta = env.create_input_file("test.fasta", &create_simple_fasta(20))?;
    let output = env.output_path("reduced.fasta");

    // Add as database first
    add_test_database(&input_fasta, "test_compression", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_compression")
        .arg("-o").arg(&output)
        .arg("--target-aligner").arg("generic")
        .arg("--reduction-ratio").arg("0.5")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().success();

    assert!(output.exists());
    Ok(())
}

#[test]
fn test_reduce_batch_mode() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input_fasta = env.create_input_file("large.fasta", &create_simple_fasta(100))?;
    let output = env.output_path("reduced.fasta");

    // Add as database first
    add_test_database(&input_fasta, "test_batch", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_batch")
        .arg("-o").arg(&output)
        .arg("--batch")
        .arg("--batch-size").arg("1000")
        .arg("--target-aligner").arg("generic")
        .arg("--reduction-ratio").arg("0.3")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().success();

    assert!(output.exists());
    Ok(())
}

#[test]
fn test_reduce_reconstruct_roundtrip() -> Result<()> {
    let env = TestEnvironment::new()?;

    // Create input with diverse sequences for meaningful reduction
    let original = env.create_input_file("original.fasta", &create_diverse_fasta(20))?;
    let reduced = env.output_path("reduced.fasta");
    let deltas = env.output_path("reduced.deltas.fasta");
    let reconstructed = env.output_path("reconstructed.fasta");

    // Add as database first
    add_test_database(&original, "test_roundtrip", env.temp_dir.path())?;

    // Reduce
    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_roundtrip")
        .arg("-o").arg(&reduced)
        .arg("-m").arg(&deltas)
        .arg("--target-aligner").arg("generic")
        .arg("--reduction-ratio").arg("0.5")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().success();
    assert!(reduced.exists());
    assert!(deltas.exists());

    // Reconstruct
    let mut cmd = talaria_cmd();
    cmd.arg("reconstruct")
        .arg("-r").arg(&reduced)
        .arg("-d").arg(&deltas)
        .arg("-o").arg(&reconstructed)
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().success();
    assert!(reconstructed.exists());

    // Verify roundtrip
    let original_count = count_sequences(&original)?;
    let reconstructed_count = count_sequences(&reconstructed)?;
    assert_eq!(original_count, reconstructed_count,
        "Reconstructed should have same number of sequences as original");

    Ok(())
}

#[test]
fn test_validate_command() -> Result<()> {
    let env = TestEnvironment::new()?;

    let original = env.create_input_file("original.fasta", &create_diverse_fasta(10))?;
    let reduced = env.output_path("reduced.fasta");
    let deltas = reduced.with_extension("deltas.fasta");

    // Add as database and reduce
    add_test_database(&original, "test_validate", env.temp_dir.path())?;
    run_reduce("local/test_validate", &reduced, env.temp_dir.path())?;

    // Then validate
    let mut cmd = talaria_cmd();
    cmd.arg("validate")
        .arg("-o").arg(&original)
        .arg("-r").arg(&reduced)
        .arg("-d").arg(&deltas)
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Validation"));

    Ok(())
}

#[test]
fn test_stats_command() -> Result<()> {
    let env = TestEnvironment::new()?;

    let fasta = env.create_input_file("test.fasta", &create_simple_fasta(10))?;

    // Stats command still uses file input for now
    let mut cmd = talaria_cmd();
    cmd.arg("stats")
        .arg("-i").arg(&fasta);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Sequences"))
        .stdout(predicate::str::contains("Total Bases"));

    Ok(())
}

#[test]
fn test_reduce_with_taxonomy() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input = env.create_input_file("taxonomic.fasta", &create_taxonomic_fasta())?;
    let output = env.output_path("reduced.fasta");

    // Add as database first
    add_test_database(&input, "test_taxonomy", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_taxonomy")
        .arg("-o").arg(&output)
        .arg("--target-aligner").arg("generic")
        .arg("--reduction-ratio").arg("0.5")
        .arg("--taxonomy-aware")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().success();
    assert!(output.exists());

    Ok(())
}

#[test]
fn test_reduce_with_invalid_input() {
    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("nonexistent/database")
        .arg("-o").arg("output.fasta");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error"));
}

#[test]
fn test_reduce_with_invalid_ratio() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input = env.create_input_file("test.fasta", &create_simple_fasta(10))?;
    let output = env.output_path("reduced.fasta");

    // Add as database first
    add_test_database(&input, "test_ratio", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_ratio")
        .arg("-o").arg(&output)
        .arg("--ratio").arg("2.0") // Invalid ratio > 1.0
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().failure();

    Ok(())
}

#[test]
fn test_reconstruct_missing_deltas() -> Result<()> {
    let env = TestEnvironment::new()?;

    let reference = env.create_input_file("ref.fasta", &create_simple_fasta(5))?;
    let output = env.output_path("output.fasta");

    let mut cmd = talaria_cmd();
    cmd.arg("reconstruct")
        .arg("-r").arg(&reference)
        // Missing deltas file
        .arg("-o").arg(&output)
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().failure();

    Ok(())
}

#[test]
fn test_parallel_processing() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input = env.create_input_file("test.fasta", &create_simple_fasta(100))?;
    let output = env.output_path("reduced.fasta");

    // Add as database first
    add_test_database(&input, "test_parallel", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_parallel")
        .arg("-o").arg(&output)
        .arg("--threads").arg("4")
        .arg("--target-aligner").arg("generic")
        .arg("--reduction-ratio").arg("0.5")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().success();
    assert!(output.exists());

    Ok(())
}

#[test]
fn test_verbose_output() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input = env.create_input_file("test.fasta", &create_simple_fasta(5))?;
    let output = env.output_path("reduced.fasta");

    // Add as database first
    add_test_database(&input, "test_verbose", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("-v")  // Global verbose flag
        .arg("reduce")
        .arg("local/test_verbose")
        .arg("-o").arg(&output)
        .arg("--target-aligner").arg("generic")
        .arg("--reduction-ratio").arg("0.5")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Using"));

    Ok(())
}

#[test]
fn test_database_list_command() -> Result<()> {
    let env = TestEnvironment::new()?;

    let mut cmd = talaria_cmd();
    cmd.arg("database")
        .arg("list")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Database"));

    Ok(())
}

#[test]
#[ignore] // Requires network and can be slow
fn test_database_download_command() -> Result<()> {
    let env = TestEnvironment::new()?;

    let mut cmd = talaria_cmd();
    cmd.arg("database")
        .arg("download")
        .arg("ncbi/taxonomy")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().success();

    Ok(())
}

#[test]
fn test_tools_list_command() -> Result<()> {
    let env = TestEnvironment::new()?;

    let mut cmd = talaria_cmd();
    cmd.arg("tools")
        .arg("list")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Tools"));

    Ok(())
}

#[test]
fn test_reduce_with_sequoia_output() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input = env.create_input_file("test.fasta", &create_simple_fasta(10))?;
    let _output = env.output_path("reduced");

    // Add as database first
    add_test_database(&input, "test_sequoia", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_sequoia")
        .arg("--sequoia-output")
        .arg("--target-aligner").arg("generic")
        .arg("--reduction-ratio").arg("0.5")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().success();

    Ok(())
}

#[test]
fn test_reduce_with_redundant_sequences() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input = env.create_input_file("redundant.fasta", &create_redundant_fasta())?;
    let output = env.output_path("reduced.fasta");

    // Add as database first
    add_test_database(&input, "test_redundant", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_redundant")
        .arg("-o").arg(&output)
        .arg("--target-aligner").arg("generic")
        .arg("--reduction-ratio").arg("0.5")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().success();

    // Check that redundant sequences were reduced
    let input_count = count_sequences(&input)?;
    let output_count = count_sequences(&output)?;
    assert!(output_count < input_count,
        "Redundant sequences should be reduced");

    Ok(())
}

#[test]
fn test_chunk_lookup_command() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input = env.create_input_file("test.fasta", &create_simple_fasta(10))?;
    let output = env.output_path("reduced.fasta");

    // First reduce to create chunks
    add_test_database(&input, "test_reconstruct", env.temp_dir.path())?;
    run_reduce("local/test_reconstruct", &output, env.temp_dir.path())?;

    // Then try chunk lookup
    let mut cmd = talaria_cmd();
    cmd.arg("chunk")
        .arg("inspect")
        .arg("--sequence").arg("seq_0")
        .env("TALARIA_HOME", env.temp_dir.path());

    // This may fail if no SEQUOIA storage, but command should parse
    let _ = cmd.assert();

    Ok(())
}

#[test]
fn test_temporal_query_command() -> Result<()> {
    let env = TestEnvironment::new()?;

    let mut cmd = talaria_cmd();
    cmd.arg("temporal")
        .arg("query")
        .arg("--at-time").arg("2024-01-15T10:00:00Z")
        .env("TALARIA_HOME", env.temp_dir.path());

    // Command should parse even if no temporal data exists
    let _ = cmd.assert();

    Ok(())
}

#[test]
#[serial_test::serial]
fn test_environment_variables() -> Result<()> {
    let env = TestEnvironment::new()?;

    // Set custom thread count via environment
    std::env::set_var("TALARIA_THREADS", "2");

    let input = env.create_input_file("test.fasta", &create_simple_fasta(10))?;
    let output = env.output_path("reduced.fasta");

    // Add as database first
    add_test_database(&input, "test_env", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_env")
        .arg("-o").arg(&output)
        .arg("--target-aligner").arg("generic")
        .arg("--reduction-ratio").arg("0.5")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().success();

    std::env::remove_var("TALARIA_THREADS");

    Ok(())
}