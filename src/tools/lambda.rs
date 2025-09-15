use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use std::io::{BufRead, BufReader};
use crate::bio::sequence::Sequence;

/// LAMBDA aligner integration
pub struct LambdaAligner {
    binary_path: PathBuf,
    temp_dir: PathBuf,
    acc_tax_map: Option<PathBuf>,  // Accession to taxonomy mapping file
    tax_dump_dir: Option<PathBuf>, // NCBI taxonomy dump directory
}

/// Alignment result from LAMBDA
#[derive(Debug, Clone)]
pub struct AlignmentResult {
    pub query_id: String,
    pub subject_id: String,
    pub identity: f64,
    pub alignment_length: usize,
    pub mismatches: usize,
    pub gap_opens: usize,
    pub query_start: usize,
    pub query_end: usize,
    pub subject_start: usize,
    pub subject_end: usize,
    pub evalue: f64,
    pub bit_score: f64,
}

impl LambdaAligner {
    /// Create a new LAMBDA aligner instance
    pub fn new(binary_path: PathBuf) -> Result<Self> {
        if !binary_path.exists() {
            anyhow::bail!("LAMBDA binary not found at {:?}", binary_path);
        }

        // Create temp directory for LAMBDA operations
        let temp_dir = std::env::temp_dir().join(format!("talaria-lambda-{}", std::process::id()));
        fs::create_dir_all(&temp_dir)?;

        // Auto-detect taxonomy files
        let (acc_tax_map, tax_dump_dir) = Self::find_taxonomy_files();

        Ok(Self {
            binary_path,
            temp_dir,
            acc_tax_map,
            tax_dump_dir,
        })
    }

    /// Find taxonomy files in the database directory
    fn find_taxonomy_files() -> (Option<PathBuf>, Option<PathBuf>) {
        use crate::core::paths;

        // First check CASG location for taxonomy
        let casg_base = paths::talaria_casg_dir();
        let casg_taxonomy_dir = casg_base.join("taxonomy/taxdump");

        // Check if CASG has taxonomy files
        if casg_taxonomy_dir.join("nodes.dmp").exists() &&
           casg_taxonomy_dir.join("names.dmp").exists() {
            // Check for idmapping support in CASG
            let casg_taxonomy_base = casg_base.join("taxonomy");

            // Look for UniProt idmapping in CASG
            let uniprot_idmap = casg_taxonomy_base.join("uniprot_idmapping.dat.gz");
            let ncbi_idmap = casg_taxonomy_base.join("prot.accession2taxid.gz");

            let idmap_path = if uniprot_idmap.exists() {
                Some(uniprot_idmap)
            } else if ncbi_idmap.exists() {
                Some(ncbi_idmap)
            } else {
                None
            };

            return (idmap_path, Some(casg_taxonomy_dir));
        }

        // Fall back to old database location
        let db_base = paths::talaria_databases_dir();

        // Look for UniProt idmapping
        let acc_tax_map = Self::find_latest_file(&db_base.join("uniprot/idmapping"), "idmapping.dat.gz");

        // Look for NCBI taxonomy dump directory
        let tax_dump_dir = Self::find_latest_dir(&db_base.join("ncbi/taxonomy"));

        // Only use taxonomy if we have BOTH the taxonomy dump AND the accession mapping
        // Lambda requires both to work properly
        match (acc_tax_map, tax_dump_dir) {
            (Some(map), Some(dir)) => (Some(map), Some(dir)),
            _ => (None, None), // Need both or neither
        }
    }

    /// Find the latest version of a file in a versioned directory structure
    fn find_latest_file(base_dir: &Path, filename: &str) -> Option<PathBuf> {
        if !base_dir.exists() {
            return None;
        }

        // Find version directories (YYYY-MM-DD format)
        let mut versions: Vec<_> = fs::read_dir(base_dir).ok()?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.file_type().ok().map(|t| t.is_dir()).unwrap_or(false) &&
                !entry.file_name().to_string_lossy().starts_with('.')
            })
            .collect();

        // Sort by name (which is date) to get the latest
        versions.sort_by_key(|e| e.file_name());

        // Check the latest version for the file
        if let Some(latest) = versions.last() {
            let file_path = latest.path().join(filename);
            if file_path.exists() {
                return Some(file_path);
            }
        }

        None
    }

    /// Find the latest version directory
    fn find_latest_dir(base_dir: &Path) -> Option<PathBuf> {
        if !base_dir.exists() {
            return None;
        }

        // Find version directories (YYYY-MM-DD format)
        let mut versions: Vec<_> = fs::read_dir(base_dir).ok()?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.file_type().ok().map(|t| t.is_dir()).unwrap_or(false) &&
                !entry.file_name().to_string_lossy().starts_with('.')
            })
            .collect();

        // Sort by name (which is date) to get the latest
        versions.sort_by_key(|e| e.file_name());

        // Return the latest directory that contains taxonomy files
        versions.iter().rev().find_map(|entry| {
            let path = entry.path();
            // Check if it contains names.dmp and nodes.dmp (core taxonomy files)
            if path.join("names.dmp").exists() && path.join("nodes.dmp").exists() {
                Some(path)
            } else {
                None
            }
        })
    }

    /// Set taxonomy mapping files
    pub fn with_taxonomy(mut self, acc_tax_map: Option<PathBuf>, tax_dump_dir: Option<PathBuf>) -> Self {
        self.acc_tax_map = acc_tax_map;
        self.tax_dump_dir = tax_dump_dir;
        self
    }
    
    /// Check if LAMBDA is working
    pub fn check_version(&self) -> Result<String> {
        let output = Command::new(&self.binary_path)
            .arg("--version")
            .output()
            .context("Failed to run LAMBDA")?;
        
        if !output.status.success() {
            anyhow::bail!("LAMBDA returned error");
        }
        
        let version = String::from_utf8_lossy(&output.stdout);
        Ok(version.trim().to_string())
    }
    
    /// Create a LAMBDA index from a FASTA file
    pub fn create_index(&self, fasta_path: &Path) -> Result<PathBuf> {
        let index_path = self.temp_dir.join("lambda_index.lba");

        println!("Creating LAMBDA index...");

        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("mkindexp")
           .arg("-d")
           .arg(fasta_path)
           .arg("-i")
           .arg(&index_path)
           .arg("--verbosity")
           .arg("1");

        // Add taxonomy mapping if available
        let has_taxonomy = self.acc_tax_map.is_some() || self.tax_dump_dir.is_some();

        if let Some(acc_tax_map) = &self.acc_tax_map {
            if acc_tax_map.exists() {
                cmd.arg("--acc-tax-map").arg(acc_tax_map);
                println!("  Using UniProt ID mapping: {:?}", acc_tax_map.file_name().unwrap_or_default());
            }
        }

        if let Some(tax_dump_dir) = &self.tax_dump_dir {
            if tax_dump_dir.exists() {
                cmd.arg("--tax-dump-dir").arg(tax_dump_dir);
                println!("  Using NCBI taxonomy: {:?}", tax_dump_dir.file_name().unwrap_or_default());
            }
        }

        if !has_taxonomy {
            println!("  Note: No taxonomy data found. Download with 'talaria database download':");
            println!("    - UniProt: talaria database download uniprot -d idmapping");
            println!("    - NCBI: talaria database download ncbi -d taxonomy");
        }

        let output = cmd.output()
            .context("Failed to create LAMBDA index")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            anyhow::bail!("LAMBDA indexing failed:\nSTDERR: {}\nSTDOUT: {}", stderr, stdout);
        }

        // Verify index was created
        if !index_path.exists() {
            anyhow::bail!("LAMBDA index file was not created at {:?}", index_path);
        }

        Ok(index_path)
    }
    
    /// Run a LAMBDA search with given query and index
    fn run_lambda_search(&self, query_path: &Path, index_path: &Path, output_path: &Path) -> Result<()> {
        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("searchp")
           .arg("-q")
           .arg(query_path)
           .arg("-i")
           .arg(index_path)
           .arg("-o")
           .arg(output_path);

        // Only request taxonomy columns if we have taxonomy data
        if self.acc_tax_map.is_some() && self.tax_dump_dir.is_some() {
            cmd.arg("--output-columns")
               .arg("std slen qframe staxids");  // Standard columns with taxonomy
        } else {
            cmd.arg("--output-columns")
               .arg("std");  // Standard columns without taxonomy
        }

        cmd.arg("--verbosity")
           .arg("1");

        let output = cmd.output()
            .context("Failed to run LAMBDA search")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            // Check if output file was created but empty
            if output_path.exists() && fs::metadata(output_path)?.len() == 0 {
                // No alignments found is not necessarily an error
                println!("Note: No significant alignments found");
                return Ok(());
            }

            anyhow::bail!("LAMBDA search failed:\nSTDERR: {}\nSTDOUT: {}", stderr, stdout);
        }

        Ok(())
    }

    /// Search query sequences against a reference database (default behavior)
    pub fn search(&self, query_sequences: &[Sequence], reference_sequences: &[Sequence]) -> Result<Vec<AlignmentResult>> {
        println!("Running LAMBDA query-vs-reference alignment...");
        println!("  Query sequences: {}", query_sequences.len());
        println!("  Reference sequences: {}", reference_sequences.len());

        // Write reference sequences to FASTA and create index
        let reference_path = self.temp_dir.join("reference.fasta");
        crate::bio::fasta::write_fasta(&reference_path, reference_sequences)?;
        let index_path = self.create_index(&reference_path)?;

        // Write query sequences to FASTA
        let query_path = self.temp_dir.join("query.fasta");
        crate::bio::fasta::write_fasta(&query_path, query_sequences)?;

        // Run search
        let output_path = self.temp_dir.join("alignments.m8");
        self.run_lambda_search(&query_path, &index_path, &output_path)?;

        // Parse results
        if output_path.exists() {
            self.parse_blast_tab(&output_path)
        } else {
            Ok(Vec::new())
        }
    }

    /// Run all-vs-all alignment (self-alignment) - optional behavior
    pub fn search_all_vs_all(&self, sequences: &[Sequence]) -> Result<Vec<AlignmentResult>> {
        println!("Running LAMBDA all-vs-all alignment on {} sequences...", sequences.len());

        // For large datasets, use sampling
        const MAX_SEQUENCES_FOR_FULL: usize = 5000;
        let sequences_to_use = if sequences.len() > MAX_SEQUENCES_FOR_FULL {
            println!("Large dataset detected, sampling {} sequences...", MAX_SEQUENCES_FOR_FULL);
            return self.run_sampled_alignment(sequences, MAX_SEQUENCES_FOR_FULL);
        } else {
            sequences
        };

        // Write sequences to temporary FASTA
        let fasta_path = self.temp_dir.join("sequences.fasta");
        crate::bio::fasta::write_fasta(&fasta_path, sequences_to_use)?;

        // Create index from same sequences
        let index_path = self.create_index(&fasta_path)?;

        // Run search (query same as reference)
        let output_path = self.temp_dir.join("alignments.m8");
        self.run_lambda_search(&fasta_path, &index_path, &output_path)?;

        // Parse results
        if output_path.exists() {
            self.parse_blast_tab(&output_path)
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Run alignment with sampling for large datasets
    fn run_sampled_alignment(&self, sequences: &[Sequence], sample_size: usize) -> Result<Vec<AlignmentResult>> {
        use rand::seq::SliceRandom;

        // Sample sequences
        let mut rng = rand::thread_rng();
        let sampled: Vec<_> = sequences.choose_multiple(&mut rng, sample_size)
            .cloned()
            .collect();

        // Use all-vs-all on the sample
        self.search_all_vs_all(&sampled)
    }
    
    /// Parse BLAST tabular format output
    fn parse_blast_tab(&self, output_path: &Path) -> Result<Vec<AlignmentResult>> {
        let file = fs::File::open(output_path)?;
        let reader = BufReader::new(file);
        let mut results = Vec::new();
        
        for line in reader.lines() {
            let line = line?;
            if line.starts_with('#') {
                continue;
            }
            
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 12 {
                continue;
            }
            
            let result = AlignmentResult {
                query_id: parts[0].to_string(),
                subject_id: parts[1].to_string(),
                identity: parts[2].parse().unwrap_or(0.0),
                alignment_length: parts[3].parse().unwrap_or(0),
                mismatches: parts[4].parse().unwrap_or(0),
                gap_opens: parts[5].parse().unwrap_or(0),
                query_start: parts[6].parse().unwrap_or(0),
                query_end: parts[7].parse().unwrap_or(0),
                subject_start: parts[8].parse().unwrap_or(0),
                subject_end: parts[9].parse().unwrap_or(0),
                evalue: parts[10].parse().unwrap_or(1.0),
                bit_score: parts[11].parse().unwrap_or(0.0),
            };
            
            // Skip self-alignments
            if result.query_id != result.subject_id {
                results.push(result);
            }
        }
        
        Ok(results)
    }
    
    /// Clean up temporary files
    pub fn cleanup(&self) -> Result<()> {
        if self.temp_dir.exists() {
            fs::remove_dir_all(&self.temp_dir)?;
        }
        Ok(())
    }
}

impl Drop for LambdaAligner {
    fn drop(&mut self) {
        // Best effort cleanup
        let _ = self.cleanup();
    }
}

/// Process LAMBDA alignment results for reference selection
pub fn process_alignment_results(alignments: Vec<AlignmentResult>) -> AlignmentGraph {
    let mut graph = AlignmentGraph::new();
    
    for alignment in alignments {
        graph.add_edge(
            alignment.query_id.clone(),
            alignment.subject_id.clone(),
            alignment.identity,
            alignment.alignment_length,
        );
    }
    
    graph
}

/// Graph structure for alignment results
pub struct AlignmentGraph {
    pub nodes: std::collections::HashSet<String>,
    pub edges: std::collections::HashMap<String, Vec<AlignmentEdge>>,
}

#[derive(Debug, Clone)]
pub struct AlignmentEdge {
    pub target: String,
    pub identity: f64,
    pub length: usize,
}

impl AlignmentGraph {
    pub fn new() -> Self {
        Self {
            nodes: std::collections::HashSet::new(),
            edges: std::collections::HashMap::new(),
        }
    }
    
    pub fn add_edge(&mut self, from: String, to: String, identity: f64, length: usize) {
        self.nodes.insert(from.clone());
        self.nodes.insert(to.clone());
        
        self.edges.entry(from).or_insert_with(Vec::new).push(AlignmentEdge {
            target: to,
            identity,
            length,
        });
    }
    
    /// Get sequences that align to a given sequence
    pub fn get_aligned_sequences(&self, seq_id: &str) -> Vec<&AlignmentEdge> {
        self.edges.get(seq_id)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }
    
    /// Calculate coverage score for a sequence (how many others it can represent)
    pub fn coverage_score(&self, seq_id: &str) -> usize {
        self.edges.get(seq_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }
}