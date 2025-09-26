//! LAMBDA aligner implementation

mod parser;
mod utils;

use talaria_bio::sequence::Sequence;
use talaria_bio::formats::fasta::{FastaReadable, FastaFile};
use crate::traits::{Aligner, AlignmentSummary as TraitAlignmentResult};
use talaria_utils::workspace::TempWorkspace;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use utils::read_lines_lossy;

/// LAMBDA aligner integration
pub struct LambdaAligner {
    binary_path: PathBuf,
    temp_dir: PathBuf,
    acc_tax_map: Option<PathBuf>,  // Accession to taxonomy mapping file
    tax_dump_dir: Option<PathBuf>, // NCBI taxonomy dump directory
    batch_enabled: bool,
    batch_size: usize,         // Max amino acids per batch (not sequence count)
    _preserve_on_failure: bool, // Whether to preserve temp dir on failure
    _failed: AtomicBool,        // Track if LAMBDA failed for cleanup decision
    workspace: Option<Arc<Mutex<TempWorkspace>>>, // Optional workspace for organized temp files
}

/// Alignment result from LAMBDA (type alias for compatibility)
pub type AlignmentResult = TraitAlignmentResult;

impl LambdaAligner {
    /// Create a new LAMBDA aligner instance
    pub fn new(binary_path: PathBuf) -> Result<Self> {
        if !binary_path.exists() {
            anyhow::bail!("LAMBDA binary not found at {:?}", binary_path);
        }

        // Note: temp_dir will be determined later based on workspace availability
        let temp_dir = PathBuf::new();

        // Auto-detect taxonomy files
        let (acc_tax_map, tax_dump_dir) = Self::find_taxonomy_files();

        // Check if we should preserve temp directory on failure
        let preserve_on_failure = std::env::var("TALARIA_PRESERVE_LAMBDA_ON_FAILURE").is_ok();
        if preserve_on_failure {
            println!("  LAMBDA temp directory will be preserved on failure for debugging");
        }

        Ok(Self {
            binary_path,
            temp_dir, // Will be set when workspace is provided or on first use
            acc_tax_map,
            tax_dump_dir,
            batch_enabled: false, // Default: no batching
            batch_size: 50_000_000,  // Default: 50M amino acids (matching db-reduce approach)
            _preserve_on_failure: preserve_on_failure,
            _failed: AtomicBool::new(false),
            workspace: None,
        })
    }

    /// Find taxonomy files in the database directory
    fn find_taxonomy_files() -> (Option<PathBuf>, Option<PathBuf>) {
        use talaria_core::system::paths;

        // Check unified taxonomy location
        let taxonomy_dir = paths::talaria_taxonomy_current_dir();

        // Resolve symlink to actual directory to ensure we find files
        let taxonomy_dir = taxonomy_dir.canonicalize().unwrap_or(taxonomy_dir.clone());

        let tax_dump_dir = taxonomy_dir.join("tree"); // Changed from "taxdump" to "tree"
        let _mappings_dir = taxonomy_dir.join("mappings");

        // Debug output if verbose mode or debug taxonomy
        let lambda_verbose = std::env::var("TALARIA_LAMBDA_VERBOSE").is_ok();
        let debug_taxonomy = std::env::var("TALARIA_DEBUG_TAXONOMY").is_ok();
        if lambda_verbose || debug_taxonomy {
            eprintln!("LAMBDA taxonomy search:");
            eprintln!("  Base dir: {:?}", taxonomy_dir);
            eprintln!("  Tree dir: {:?}", tax_dump_dir);
            eprintln!("  Tree dir exists: {}", tax_dump_dir.exists());
            eprintln!("  nodes.dmp: {}", tax_dump_dir.join("nodes.dmp").exists());
            eprintln!("  names.dmp: {}", tax_dump_dir.join("names.dmp").exists());
        }

        // Check if we have the required taxonomy dump files
        if tax_dump_dir.join("nodes.dmp").exists() && tax_dump_dir.join("names.dmp").exists() {
            // Return with simplified logic for now
            return (None, Some(tax_dump_dir));
        }

        // No taxonomy found
        (None, None)
    }

    /// Extract accession numbers from a FASTA file
    #[allow(dead_code)]
    fn extract_accessions_from_fasta(&self, fasta_path: &Path) -> Result<HashSet<String>> {
        use std::collections::HashSet;
        let mut accessions = HashSet::new();
        let reader = FastaFile::open_for_reading(fasta_path)?;

        for line in read_lines_lossy(reader) {
            let line = line?;
            if line.starts_with('>') {
                // Parse different header formats
                if let Some(acc) = Self::parse_accession_from_header(&line) {
                    accessions.insert(acc);
                }
            }
        }

        println!(
            "  Extracted {} unique accessions from FASTA",
            accessions.len()
        );
        Ok(accessions)
    }

    /// Parse accession from various FASTA header formats
    #[allow(dead_code)]
    fn parse_accession_from_header(header: &str) -> Option<String> {
        // Remove the '>' prefix
        let header = header.trim_start_matches('>');

        // UniProt format: sp|P12345|PROT1_HUMAN or tr|Q12345|...
        if header.starts_with("sp|") || header.starts_with("tr|") {
            let parts: Vec<&str> = header.split('|').collect();
            if parts.len() >= 2 {
                return Some(parts[1].to_string());
            }
        }

        // NCBI format: might be just the accession or gi|12345|ref|NP_123456.1|
        if header.contains('|') {
            let parts: Vec<&str> = header.split('|').collect();
            // Look for ref| or gb| or similar
            for (i, part) in parts.iter().enumerate() {
                if (*part == "ref" || *part == "gb" || *part == "emb" || *part == "dbj")
                    && i + 1 < parts.len()
                {
                    // Take the next part, removing version if present
                    let acc = parts[i + 1].split('.').next().unwrap_or(parts[i + 1]);
                    return Some(acc.to_string());
                }
            }
        }

        // Simple format: just accession (possibly with version)
        let first_part = header.split_whitespace().next()?;
        let acc = first_part.split('.').next().unwrap_or(first_part);

        // Only return if it looks like an accession (alphanumeric with possible underscore)
        if acc.chars().any(|c| c.is_alphanumeric()) {
            Some(acc.to_string())
        } else {
            None
        }
    }

    /// Set workspace for organized temp file management
    pub fn with_workspace(mut self, workspace: Arc<Mutex<TempWorkspace>>) -> Self {
        self.workspace = Some(workspace);
        // Initialize temp_dir from workspace
        self.initialize_temp_dir();
        self
    }

    /// Initialize or get the temp directory path
    fn initialize_temp_dir(&mut self) {
        if let Some(ref workspace) = self.workspace {
            // Use SEQUOIA workspace for LAMBDA operations
            let ws = workspace.lock().unwrap();
            self.temp_dir = ws.root.join("lambda");
            // Ensure directory exists
            fs::create_dir_all(&self.temp_dir).ok();
        } else {
            // No workspace, use traditional temp directory
            self.temp_dir =
                std::env::temp_dir().join(format!("talaria-lambda-{}", std::process::id()));
            // Ensure it exists
            fs::create_dir_all(&self.temp_dir).ok();
        }
    }

    /// Check LAMBDA version
    pub fn check_version(&self) -> Result<String> {
        self.version()
    }

    /// Configure batch settings
    pub fn with_batch_settings(mut self, enabled: bool, size: usize) -> Self {
        self.batch_enabled = enabled;
        self.batch_size = size;
        self
    }

    /// Configure taxonomy settings
    pub fn with_taxonomy(mut self, acc_tax_map: Option<PathBuf>, tax_dump_dir: Option<PathBuf>) -> Self {
        self.acc_tax_map = acc_tax_map;
        self.tax_dump_dir = tax_dump_dir;
        self
    }

    /// Perform actual search (non-trait implementation)
    pub fn search(&mut self, _query: &[Sequence], _reference: &[Sequence]) -> Result<Vec<AlignmentResult>> {
        // TODO: Implement actual LAMBDA search
        Ok(Vec::new())
    }

    /// Search all vs all
    pub fn search_all_vs_all(&mut self, sequences: &[Sequence]) -> Result<Vec<AlignmentResult>> {
        self.search(sequences, sequences)
    }

    /// Create index for sequences
    pub fn create_index_for_sequences(&mut self, _sequences: &[Sequence]) -> Result<PathBuf> {
        // TODO: Implement LAMBDA index creation
        // For now, return a temporary path
        let index_path = std::env::temp_dir().join("lambda_index");
        Ok(index_path)
    }

    /// Search groups in parallel
    pub fn search_groups_parallel(&mut self, _groups: Vec<Vec<Sequence>>, _index_path: &Path, _parallel_processes: usize) -> Result<Vec<Vec<AlignmentResult>>> {
        // TODO: Implement parallel group search
        Ok(Vec::new())
    }

    /// Search with index (silent mode)
    pub fn search_with_index_silent(&mut self, _query: &[Sequence], _index_path: &Path) -> Result<Vec<AlignmentResult>> {
        // TODO: Implement search with pre-built index
        Ok(Vec::new())
    }
}

impl Aligner for LambdaAligner {
    fn search(
        &mut self,
        query: &[Sequence],
        reference: &[Sequence],
    ) -> Result<Vec<AlignmentResult>> {
        // Call the non-trait search method
        LambdaAligner::search(self, query, reference)
    }

    fn version(&self) -> Result<String> {
        // Get LAMBDA version
        let output = Command::new(&self.binary_path)
            .arg("--version")
            .output()
            .context("Failed to run LAMBDA --version")?;

        let version_str = String::from_utf8_lossy(&output.stdout);
        Ok(version_str.trim().to_string())
    }

    fn is_available(&self) -> bool {
        self.binary_path.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs::File;
    use std::io::Write;
    use std::sync::atomic::Ordering;

    fn create_test_sequence(id: &str, seq: &str) -> Sequence {
        Sequence {
            id: id.to_string(),
            description: Some(format!("{} Test sequence", id)),
            sequence: seq.as_bytes().to_vec(),
            taxon_id: None,
            taxonomy_sources: Default::default(),
        }
    }

    fn create_mock_binary() -> (PathBuf, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let binary_path = temp_dir.path().join("lambda");

        // Create a mock executable file
        File::create(&binary_path).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&binary_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&binary_path, perms).unwrap();
        }

        (binary_path, temp_dir)
    }

    #[test]
    fn test_new_aligner_creation() {
        let (binary_path, _temp_dir) = create_mock_binary();

        let aligner = LambdaAligner::new(binary_path.clone());
        assert!(aligner.is_ok());

        let aligner = aligner.unwrap();
        assert_eq!(aligner.binary_path, binary_path);
        assert_eq!(aligner.batch_size, 50_000_000);
        assert!(!aligner.batch_enabled);
    }

    #[test]
    fn test_new_aligner_missing_binary() {
        let result = LambdaAligner::new(PathBuf::from("/nonexistent/lambda"));
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("LAMBDA binary not found"));
        }
    }

    #[test]
    fn test_taxonomy_file_detection() {
        // This tests the static method
        let (acc_tax_map, tax_dump_dir) = LambdaAligner::find_taxonomy_files();

        // The actual result depends on the environment
        // We just verify the function doesn't panic
        if let Some(tax_dir) = tax_dump_dir {
            // If a directory is returned, it should exist
            assert!(tax_dir.exists() || tax_dir.parent().map_or(false, |p| p.exists()));
        }

        // acc_tax_map is currently always None in the implementation
        assert!(acc_tax_map.is_none());
    }

    #[test]
    fn test_batch_settings_configuration() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let aligner = LambdaAligner::new(binary_path).unwrap();

        let aligner = aligner.with_batch_settings(true, 100_000_000);
        assert!(aligner.batch_enabled);
        assert_eq!(aligner.batch_size, 100_000_000);
    }

    #[test]
    fn test_taxonomy_configuration() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let aligner = LambdaAligner::new(binary_path).unwrap();

        let acc_tax_path = PathBuf::from("/path/to/acc_tax.tsv");
        let tax_dump_path = PathBuf::from("/path/to/taxdump");

        let aligner = aligner.with_taxonomy(Some(acc_tax_path.clone()), Some(tax_dump_path.clone()));
        assert_eq!(aligner.acc_tax_map, Some(acc_tax_path));
        assert_eq!(aligner.tax_dump_dir, Some(tax_dump_path));
    }

    #[test]
    fn test_workspace_integration() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let mut aligner = LambdaAligner::new(binary_path).unwrap();

        // Create a mock workspace
        let temp_workspace = TempWorkspace::new("test_lambda").unwrap();
        let workspace = Arc::new(Mutex::new(temp_workspace));

        aligner = aligner.with_workspace(workspace.clone());

        // Verify temp_dir is set correctly
        let ws = workspace.lock().unwrap();
        let expected_dir = ws.root.join("lambda");
        drop(ws); // Release lock

        assert_eq!(aligner.temp_dir, expected_dir);
        assert!(aligner.workspace.is_some());
    }

    #[test]
    fn test_initialize_temp_dir_without_workspace() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let mut aligner = LambdaAligner::new(binary_path).unwrap();

        aligner.initialize_temp_dir();

        // Should create a temp dir with process ID
        assert!(aligner.temp_dir.to_string_lossy().contains("talaria-lambda"));
        assert!(aligner.temp_dir.to_string_lossy().contains(&std::process::id().to_string()));
    }

    #[test]
    fn test_parse_accession_from_header() {
        // UniProt format
        let header = ">sp|P12345|PROT1_HUMAN Protein description";
        let acc = LambdaAligner::parse_accession_from_header(header);
        assert_eq!(acc, Some("P12345".to_string()));

        // TrEMBL format
        let header = ">tr|Q12345|PROT2_MOUSE Description";
        let acc = LambdaAligner::parse_accession_from_header(header);
        assert_eq!(acc, Some("Q12345".to_string()));

        // NCBI ref format
        let header = ">gi|123456|ref|NP_123456.1| protein";
        let acc = LambdaAligner::parse_accession_from_header(header);
        assert_eq!(acc, Some("NP_123456".to_string()));

        // Simple format
        let header = ">NP_987654 some protein";
        let acc = LambdaAligner::parse_accession_from_header(header);
        assert_eq!(acc, Some("NP_987654".to_string()));

        // Simple format with version
        let header = ">XP_123456.2 hypothetical protein";
        let acc = LambdaAligner::parse_accession_from_header(header);
        assert_eq!(acc, Some("XP_123456".to_string()));

        // Invalid format
        let header = ">|||";
        let acc = LambdaAligner::parse_accession_from_header(header);
        assert_eq!(acc, None);
    }

    #[test]
    fn test_extract_accessions_from_fasta() {
        let temp_dir = TempDir::new().unwrap();
        let fasta_path = temp_dir.path().join("test.fasta");

        // Write test FASTA file
        let mut file = File::create(&fasta_path).unwrap();
        writeln!(file, ">sp|P12345|PROT1 Description").unwrap();
        writeln!(file, "ACDEFGHIKLMNPQRSTVWY").unwrap();
        writeln!(file, ">tr|Q67890|PROT2 Another protein").unwrap();
        writeln!(file, "MKLMNPQRSTVWYACDEFGH").unwrap();
        writeln!(file, ">NP_123456 RefSeq protein").unwrap();
        writeln!(file, "STVWYACDEFGHIKLMNPQR").unwrap();

        let (binary_path, _temp_dir) = create_mock_binary();
        let aligner = LambdaAligner::new(binary_path).unwrap();

        let accessions = aligner.extract_accessions_from_fasta(&fasta_path).unwrap();

        assert_eq!(accessions.len(), 3);
        assert!(accessions.contains("P12345"));
        assert!(accessions.contains("Q67890"));
        assert!(accessions.contains("NP_123456"));
    }

    #[test]
    fn test_search_returns_empty_results() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let mut aligner = LambdaAligner::new(binary_path).unwrap();

        let query = vec![create_test_sequence("Q1", "ACDEFG")];
        let reference = vec![create_test_sequence("R1", "ACDEFG")];

        let results = aligner.search(&query, &reference).unwrap();
        assert_eq!(results.len(), 0); // Currently returns empty as it's not implemented
    }

    #[test]
    fn test_search_all_vs_all() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let mut aligner = LambdaAligner::new(binary_path).unwrap();

        let sequences = vec![
            create_test_sequence("S1", "ACDEFG"),
            create_test_sequence("S2", "GHIJKL"),
        ];

        let results = aligner.search_all_vs_all(&sequences).unwrap();
        assert_eq!(results.len(), 0); // Currently returns empty as it's not implemented
    }

    #[test]
    fn test_create_index_for_sequences() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let mut aligner = LambdaAligner::new(binary_path).unwrap();

        let sequences = vec![create_test_sequence("S1", "ACDEFG")];

        let index_path = aligner.create_index_for_sequences(&sequences).unwrap();
        assert!(index_path.to_string_lossy().contains("lambda_index"));
    }

    #[test]
    fn test_search_groups_parallel() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let mut aligner = LambdaAligner::new(binary_path).unwrap();

        let groups = vec![
            vec![create_test_sequence("G1S1", "ACDEFG")],
            vec![create_test_sequence("G2S1", "GHIJKL")],
        ];

        let index_path = PathBuf::from("/tmp/index");
        let results = aligner.search_groups_parallel(groups, &index_path, 2).unwrap();
        assert_eq!(results.len(), 0); // Currently returns empty as it's not implemented
    }

    #[test]
    fn test_search_with_index_silent() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let mut aligner = LambdaAligner::new(binary_path).unwrap();

        let query = vec![create_test_sequence("Q1", "ACDEFG")];
        let index_path = PathBuf::from("/tmp/index");

        let results = aligner.search_with_index_silent(&query, &index_path).unwrap();
        assert_eq!(results.len(), 0); // Currently returns empty as it's not implemented
    }

    #[test]
    fn test_is_available() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let aligner = LambdaAligner::new(binary_path).unwrap();

        assert!(aligner.is_available());

        // Test with non-existent binary
        let mut aligner = aligner;
        aligner.binary_path = PathBuf::from("/nonexistent/lambda");
        assert!(!aligner.is_available());
    }

    #[test]
    fn test_preserve_on_failure_flag() {
        // Set environment variable
        std::env::set_var("TALARIA_PRESERVE_LAMBDA_ON_FAILURE", "1");

        let (binary_path, _temp_dir) = create_mock_binary();
        let aligner = LambdaAligner::new(binary_path).unwrap();

        assert!(aligner._preserve_on_failure);

        // Clean up
        std::env::remove_var("TALARIA_PRESERVE_LAMBDA_ON_FAILURE");
    }

    #[test]
    fn test_failed_flag_atomic() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let aligner = LambdaAligner::new(binary_path).unwrap();

        // Initially should be false
        assert!(!aligner._failed.load(Ordering::Relaxed));

        // Set to true
        aligner._failed.store(true, Ordering::Relaxed);
        assert!(aligner._failed.load(Ordering::Relaxed));
    }

    #[test]
    fn test_recommended_batch_size() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let aligner = LambdaAligner::new(binary_path).unwrap();

        // Test default implementation from trait
        assert_eq!(aligner.recommended_batch_size(), 1000);
    }

    #[test]
    fn test_supports_protein_and_nucleotide() {
        let (binary_path, _temp_dir) = create_mock_binary();
        let aligner = LambdaAligner::new(binary_path).unwrap();

        // Test default implementations from trait
        assert!(aligner.supports_protein());
        assert!(aligner.supports_nucleotide());
    }
}
