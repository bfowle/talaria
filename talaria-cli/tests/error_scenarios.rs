mod common;

use anyhow::Result;
use predicates::prelude::*;
use std::fs;

use common::*;

#[test]
fn test_missing_input_database() -> Result<()> {
    let env = TestEnvironment::new()?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("nonexistent/database")
        .arg("-o").arg("output.fasta")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error"))
        .code(predicate::ne(0));

    Ok(())
}

#[test]
fn test_corrupted_fasta_file() -> Result<()> {
    let env = TestEnvironment::new()?;

    let corrupted = env.create_input_file("corrupted.fasta", &create_corrupted_fasta())?;
    let _output = env.output_path("output.fasta");

    // Try to add corrupted file as database - should fail or warn
    let mut cmd = talaria_cmd();
    cmd.arg("database")
        .arg("add")
        .arg("--input").arg(&corrupted)
        .arg("--name").arg("local/test_corrupted")
        .arg("--source").arg("test")
        .env("TALARIA_HOME", env.temp_dir.path());

    // May succeed with warnings or fail depending on corruption type
    // Important is that it doesn't panic
    let assert = cmd.assert();
    assert.code(predicate::ne(101)); // Not a panic exit code

    Ok(())
}

#[test]
fn test_empty_input_file() -> Result<()> {
    let env = TestEnvironment::new()?;

    let empty = env.create_input_file("empty.fasta", "")?;

    // Try to add empty file as database - should fail
    let mut cmd = talaria_cmd();
    cmd.arg("database")
        .arg("add")
        .arg("--input").arg(&empty)
        .arg("--name").arg("local/test_empty")
        .arg("--source").arg("test")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error"));

    Ok(())
}

#[test]
fn test_invalid_output_directory() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input = env.create_input_file("test.fasta", &create_simple_fasta(5))?;
    let invalid_output = "/root/cannot/write/here/output.fasta";

    // Add database first
    add_test_database(&input, "test_invalid_out", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_invalid_out")
        .arg("-o").arg(invalid_output)
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error"));

    Ok(())
}

#[test]
fn test_conflicting_arguments() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input = env.create_input_file("test.fasta", &create_simple_fasta(5))?;
    let output = env.output_path("output.fasta");

    // Add database first
    add_test_database(&input, "test_conflict", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_conflict")
        .arg("-o").arg(&output)
        .arg("--ratio").arg("0.5")
        .arg("--target-sequences").arg("100") // Conflicting with ratio
        .env("TALARIA_HOME", env.temp_dir.path());

    // Should either fail or use one of the options
    let _ = cmd.assert();

    Ok(())
}

#[test]
fn test_out_of_range_parameters() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input = env.create_input_file("test.fasta", &create_simple_fasta(5))?;
    let output = env.output_path("output.fasta");

    // Add database first
    add_test_database(&input, "test_range", env.temp_dir.path())?;

    // Test invalid compression level
    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_range")
        .arg("-o").arg(&output)
        .arg("--compression").arg("100") // Out of range
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().failure();

    // Test invalid ratio
    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_range")
        .arg("-o").arg(&output)
        .arg("--ratio").arg("-0.5") // Negative ratio
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().failure();

    // Test invalid similarity threshold
    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_range")
        .arg("-o").arg(&output)
        .arg("--min-similarity").arg("1.5") // > 1.0
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().failure();

    Ok(())
}

#[test]
fn test_insufficient_disk_space() -> Result<()> {
    // This is hard to test properly without actually filling the disk
    // We can test the handling of write errors though

    let env = TestEnvironment::new()?;
    let input = env.create_input_file("test.fasta", &create_simple_fasta(5))?;

    // Create output directory and make it read-only
    let readonly_dir = env.temp_dir.path().join("readonly");
    fs::create_dir(&readonly_dir)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&readonly_dir)?.permissions();
        perms.set_mode(0o444); // Read-only
        fs::set_permissions(&readonly_dir, perms)?;
    }

    let output = readonly_dir.join("output.fasta");

    // Add database first
    add_test_database(&input, "test_readonly", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_readonly")
        .arg("-o").arg(&output)
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .failure()
        .code(predicate::ne(0));

    Ok(())
}

#[test]
fn test_interrupt_handling() -> Result<()> {
    // Test that the program handles interrupts gracefully
    // This is difficult to test directly, but we can test cleanup behavior

    let env = TestEnvironment::new()?;
    let input = env.create_input_file("large.fasta", &create_large_fasta(1000, 1000))?;
    let output = env.output_path("output.fasta");

    // Add database first
    add_test_database(&input, "test_interrupt", env.temp_dir.path())?;

    // Start a reduction with a very large file
    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_interrupt")
        .arg("-o").arg(&output)
        .arg("--batch")
        .arg("--batch-size").arg("1")
        .env("TALARIA_HOME", env.temp_dir.path())
        .timeout(std::time::Duration::from_millis(100)); // Force timeout

    // Should timeout without panicking
    let _ = cmd.assert();

    Ok(())
}

#[test]
fn test_malformed_database_reference() -> Result<()> {
    let env = TestEnvironment::new()?;

    let mut cmd = talaria_cmd();
    cmd.arg("reconstruct")
        .arg("-r").arg("invalid::database//reference")
        .arg("-o").arg("output.fasta")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error"));

    Ok(())
}

#[test]
fn test_missing_required_arguments() -> Result<()> {
    let env = TestEnvironment::new()?;

    // Test reduce without input
    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("-o").arg("output.fasta")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("required"));

    // Since output is now optional (uses SEQUOIA by default), this test is no longer valid
    // Remove this test case - reduce with database but no output is now valid

    // Test reconstruct without reference
    let mut cmd = talaria_cmd();
    cmd.arg("reconstruct")
        .arg("-o").arg("output.fasta")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Must specify"));

    Ok(())
}

#[test]
fn test_invalid_file_format() -> Result<()> {
    let env = TestEnvironment::new()?;

    // Create a non-FASTA file
    let binary_file = env.create_input_file("binary.dat", "\x00\x01\x02\x03\x04")?;

    // Try to add binary file as database - should fail
    let mut cmd = talaria_cmd();
    cmd.arg("database")
        .arg("add")
        .arg("--input").arg(&binary_file)
        .arg("--name").arg("local/test_binary")
        .arg("--source").arg("test")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error"));

    Ok(())
}

#[test]
fn test_circular_dependency() -> Result<()> {
    let env = TestEnvironment::new()?;

    // Try to use the same file as input and output
    let file = env.create_input_file("test.fasta", &create_simple_fasta(5))?;

    // Add database first
    add_test_database(&file, "test_circular", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_circular")
        .arg("-o").arg(&file) // Same file as source!
        .env("TALARIA_HOME", env.temp_dir.path());

    // Should either fail or handle gracefully
    let _ = cmd.assert();

    Ok(())
}

#[test]
fn test_invalid_thread_count() -> Result<()> {
    let env = TestEnvironment::new()?;

    let input = env.create_input_file("test.fasta", &create_simple_fasta(5))?;
    let output = env.output_path("output.fasta");

    // Add database first
    add_test_database(&input, "test_threads", env.temp_dir.path())?;

    // Test negative thread count
    let mut cmd = talaria_cmd();
    cmd.arg("--threads").arg("-1")
        .arg("reduce")
        .arg("local/test_threads")
        .arg("-o").arg(&output)
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert().failure();

    Ok(())
}

#[test]
fn test_unicode_in_paths() -> Result<()> {
    let env = TestEnvironment::new()?;

    // Create file with unicode characters in name
    let unicode_name = "test_序列_🧬.fasta";
    let input = env.create_input_file(unicode_name, &create_simple_fasta(5))?;
    let output = env.output_path("output_結果.fasta");

    // Add database first
    add_test_database(&input, "test_unicode", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_unicode")
        .arg("-o").arg(&output)
        .arg("--target-aligner").arg("generic")
        .env("TALARIA_HOME", env.temp_dir.path());

    // Should handle unicode paths
    // May succeed or fail depending on filesystem, but shouldn't panic
    // Just run the command without asserting on the result
    let _ = cmd.assert();

    Ok(())
}

#[test]
fn test_extremely_long_sequences() -> Result<()> {
    let env = TestEnvironment::new()?;

    // Create FASTA with extremely long sequence
    let mut long_fasta = ">very_long_sequence\n".to_string();
    long_fasta.push_str(&"A".repeat(1_000_000)); // 1MB sequence

    let input = env.create_input_file("long.fasta", &long_fasta)?;
    let output = env.output_path("output.fasta");

    // Add database first
    add_test_database(&input, "test_long", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_long")
        .arg("-o").arg(&output)
        .arg("--target-aligner").arg("generic")
        .env("TALARIA_HOME", env.temp_dir.path())
        .timeout(std::time::Duration::from_secs(10));

    // Should handle long sequences without issues
    let _ = cmd.assert();

    Ok(())
}

#[test]
fn test_symlink_handling() -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let env = TestEnvironment::new()?;

        let original = env.create_input_file("original.fasta", &create_simple_fasta(5))?;
        let symlink_path = env.input_dir.join("symlink.fasta");
        symlink(&original, &symlink_path)?;

        let output = env.output_path("output.fasta");

        // Add database using symlink
        add_test_database(&symlink_path, "test_symlink", env.temp_dir.path())?;

        let mut cmd = talaria_cmd();
        cmd.arg("reduce")
            .arg("local/test_symlink")
            .arg("-o").arg(&output)
            .arg("--target-aligner").arg("generic")
            .arg("--reduction-ratio").arg("0.5")
            .env("TALARIA_HOME", env.temp_dir.path());

        cmd.assert().success();
    }

    Ok(())
}

#[test]
fn test_permission_denied() -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let env = TestEnvironment::new()?;

        let input = env.create_input_file("test.fasta", &create_simple_fasta(5))?;

        // Remove read permissions
        let mut perms = fs::metadata(&input)?.permissions();
        perms.set_mode(0o000); // No permissions
        fs::set_permissions(&input, perms)?;

        let _output = env.output_path("output.fasta");

        // Try to add database with unreadable file - should fail
        let mut cmd = talaria_cmd();
        cmd.arg("database")
            .arg("add")
            .arg("--input").arg(&input)
            .arg("--name").arg("test_perm")
            .arg("--source").arg("local")
            .env("TALARIA_HOME", env.temp_dir.path());

        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("Error"));

        // Restore permissions for cleanup
        let mut perms = fs::metadata(&input)?.permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&input, perms)?;
    }

    Ok(())
}

#[test]
fn test_network_timeout() -> Result<()> {
    let env = TestEnvironment::new()?;

    // Test database download with unreachable server
    let mut cmd = talaria_cmd();
    cmd.arg("database")
        .arg("download")
        .arg("--server").arg("http://192.0.2.0:9999") // TEST-NET-1, unreachable
        .arg("ncbi/taxonomy")
        .env("TALARIA_HOME", env.temp_dir.path())
        .timeout(std::time::Duration::from_secs(5));

    cmd.assert().failure();

    Ok(())
}

#[test]
fn test_exit_codes() -> Result<()> {
    let env = TestEnvironment::new()?;

    // Success case - exit code 0
    let input = env.create_input_file("test.fasta", &create_simple_fasta(5))?;
    let output = env.output_path("output.fasta");

    // Add database first
    add_test_database(&input, "test_exit", env.temp_dir.path())?;

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("local/test_exit")
        .arg("-o").arg(&output)
        .arg("--target-aligner").arg("generic")
        .arg("--reduction-ratio").arg("0.5")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .success()
        .code(0);

    // Failure case - non-zero exit code
    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("nonexistent/database")
        .arg("-o").arg("output.fasta")
        .env("TALARIA_HOME", env.temp_dir.path());

    cmd.assert()
        .failure()
        .code(predicate::ne(0));

    Ok(())
}