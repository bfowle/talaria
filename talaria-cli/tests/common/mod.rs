#![allow(dead_code)]

use anyhow::Result;
use assert_cmd::Command;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Create a test FASTA file with the given content
pub fn create_test_fasta(dir: &Path, name: &str, content: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    fs::write(&path, content)?;
    Ok(path)
}

/// Create a simple test FASTA with n sequences
pub fn create_simple_fasta(n: usize) -> String {
    let mut content = String::new();
    for i in 0..n {
        content.push_str(&format!(">seq_{} Test sequence {}\n", i, i));
        content.push_str("ATGATGATGATGATGATGATGATGATGATGATGATGATGATGATG\n");
    }
    content
}

/// Create a FASTA with diverse sequences for reduction testing
pub fn create_diverse_fasta(n: usize) -> String {
    let mut content = String::new();
    let bases = ['A', 'T', 'G', 'C'];

    for i in 0..n {
        content.push_str(&format!(">seq_{} Test sequence {}\n", i, i));

        // Generate truly different sequences - use different lengths and patterns
        // to avoid deduplication issues
        let seq_length = 60 + (i % 20); // Vary length from 60 to 79
        let mut seq = String::new();

        // Use a more complex pattern that creates unique sequences
        for j in 0..seq_length {
            // Create different patterns for each sequence
            let base_idx = ((i * 7 + j * 3) ^ (i * 11)) % 4;
            seq.push(bases[base_idx]);
        }
        content.push_str(&seq);
        content.push('\n');
    }
    content
}

/// Create a FASTA with taxonomy information
pub fn create_taxonomic_fasta() -> String {
    r#">seq1 Escherichia coli OX=562
ATGATGATGATGATGATGATGATGATGATGATGATGATGATGATG
>seq2 Salmonella enterica OX=28901
CGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCG
>seq3 Bacillus subtilis OX=1423
TAGTAGTAGTAGTAGTAGTAGTAGTAGTAGTAGTAGTAGTAGTAG
>seq4 Homo sapiens OX=9606
CATCATCATCATCATCATCATCATCATCATCATCATCATCATCAT
"#
    .to_string()
}

/// Create a FASTA with redundant sequences
pub fn create_redundant_fasta() -> String {
    r#">seq1 Original
ATGATGATGATGATGATGATGATGATGATGATGATGATGATGATG
>seq2 Duplicate
ATGATGATGATGATGATGATGATGATGATGATGATGATGATGATG
>seq3 Similar
ATGATGATGATGATGATGATGATGATGATGATGATGATGATGACG
>seq4 Different
CGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCG
>seq5 Another duplicate
ATGATGATGATGATGATGATGATGATGATGATGATGATGATGATG
"#
    .to_string()
}

/// Helper to run talaria CLI command
pub fn talaria_cmd() -> Command {
    Command::cargo_bin("talaria").unwrap()
}

/// Helper to add a test database from a FASTA file
pub fn add_test_database(fasta_path: &Path, db_name: &str, temp_dir: &Path) -> Result<()> {
    let mut cmd = talaria_cmd();
    cmd.arg("database")
        .arg("add")
        .arg("--input")
        .arg(fasta_path)
        .arg("--name")
        .arg(db_name)
        .arg("--source")
        .arg("local")
        .env("TALARIA_HOME", temp_dir);

    let output = cmd.output().expect("Failed to execute command");

    if !output.status.success() {
        anyhow::bail!("Database add command failed");
    }
    Ok(())
}

/// Helper to run a reduce command with basic options
pub fn run_reduce(db_name: &str, output: &Path, temp_dir: &Path) -> Result<()> {
    // Create delta file path by adding .deltas.fasta extension
    let delta_path = output.with_extension("deltas.fasta");

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg(db_name)
        .arg("-o")
        .arg(output)
        .arg("-m")
        .arg(&delta_path)
        .arg("--target-aligner")
        .arg("generic")
        .arg("--reduction-ratio")
        .arg("0.5")
        .env("TALARIA_HOME", temp_dir);

    let assert = cmd.assert();
    assert.success();
    Ok(())
}

/// Helper to run reconstruct command
pub fn run_reconstruct(reference: &Path, deltas: &Path, output: &Path) -> Result<()> {
    let mut cmd = talaria_cmd();
    cmd.arg("reconstruct")
        .arg("--references")
        .arg(reference)
        .arg("--deltas")
        .arg(deltas)
        .arg("-o")
        .arg(output);

    let assert = cmd.assert();
    assert.success();
    Ok(())
}

/// Helper to validate reduction results
pub fn run_validate(original: &Path, reduced: &Path) -> Result<()> {
    let mut cmd = talaria_cmd();
    cmd.arg("validate")
        .arg("-o")
        .arg(original)
        .arg("-r")
        .arg(reduced);

    let assert = cmd.assert();
    assert.success();
    Ok(())
}

/// Setup test environment with temporary directory
pub struct TestEnvironment {
    pub temp_dir: TempDir,
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
}

impl TestEnvironment {
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let input_dir = temp_dir.path().join("input");
        let output_dir = temp_dir.path().join("output");

        fs::create_dir_all(&input_dir)?;
        fs::create_dir_all(&output_dir)?;

        Ok(Self {
            temp_dir,
            input_dir,
            output_dir,
        })
    }

    pub fn create_input_file(&self, name: &str, content: &str) -> Result<PathBuf> {
        create_test_fasta(&self.input_dir, name, content)
    }

    pub fn output_path(&self, name: &str) -> PathBuf {
        self.output_dir.join(name)
    }
}

/// Count sequences in a FASTA file
pub fn count_sequences(path: &Path) -> Result<usize> {
    let content = fs::read_to_string(path)?;
    Ok(content.lines().filter(|l| l.starts_with('>')).count())
}

/// Check if a FASTA file contains a specific sequence ID
pub fn contains_sequence(path: &Path, seq_id: &str) -> Result<bool> {
    let content = fs::read_to_string(path)?;
    Ok(content
        .lines()
        .any(|l| l.starts_with('>') && l.contains(seq_id)))
}

/// Create a corrupted FASTA for error testing
pub fn create_corrupted_fasta() -> String {
    r#">seq1 Missing sequence data
>seq2 Invalid characters
ATGXYZ123!@#
>seq3 Valid sequence
ATGATGATGATGATGATGATGATGATGATGATGATGATGATGATG
seq4 Missing header
CGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCG
"#
    .to_string()
}

/// Create a large FASTA for performance testing
pub fn create_large_fasta(num_sequences: usize, seq_length: usize) -> String {
    let mut content = String::new();
    let bases = ['A', 'T', 'G', 'C'];

    for i in 0..num_sequences {
        content.push_str(&format!(">seq_{} Large sequence {}\n", i, i));

        // Generate pseudo-random sequence
        let mut seq = String::with_capacity(seq_length);
        for j in 0..seq_length {
            seq.push(bases[(i + j) % 4]);
        }
        content.push_str(&seq);
        content.push('\n');
    }

    content
}

/// Mock LAMBDA aligner for testing
pub struct MockLambda;

impl MockLambda {
    pub fn setup(dir: &Path) -> Result<PathBuf> {
        let mock_lambda = dir.join("lambda");

        // Create a mock executable
        #[cfg(unix)]
        {
            fs::write(&mock_lambda, "#!/bin/sh\necho 'Mock LAMBDA output'\nexit 0")?;
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&mock_lambda)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&mock_lambda, perms)?;
        }

        #[cfg(windows)]
        {
            fs::write(
                &mock_lambda.with_extension("bat"),
                "@echo off\necho Mock LAMBDA output\nexit /b 0",
            )?;
        }

        Ok(mock_lambda)
    }
}

/// Assert that two FASTA files have the same sequences (ignoring order)
pub fn assert_fasta_equivalent(path1: &Path, path2: &Path) -> Result<()> {
    use std::collections::HashSet;

    let content1 = fs::read_to_string(path1)?;
    let content2 = fs::read_to_string(path2)?;

    let seqs1: HashSet<String> = extract_sequences(&content1);
    let seqs2: HashSet<String> = extract_sequences(&content2);

    assert_eq!(seqs1, seqs2, "FASTA files have different sequences");
    Ok(())
}

fn extract_sequences(content: &str) -> HashSet<String> {
    let mut sequences = HashSet::new();
    let mut current_seq = String::new();
    let mut in_sequence = false;

    for line in content.lines() {
        if line.starts_with('>') {
            if in_sequence && !current_seq.is_empty() {
                sequences.insert(current_seq.clone());
                current_seq.clear();
            }
            in_sequence = true;
        } else if in_sequence {
            current_seq.push_str(line.trim());
        }
    }

    if !current_seq.is_empty() {
        sequences.insert(current_seq);
    }

    sequences
}
