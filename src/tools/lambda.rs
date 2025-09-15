use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::fs;
use std::io::{BufRead, BufReader, Write, Read};
use std::collections::HashSet;
use crate::bio::sequence::Sequence;

/// Helper function to stream output with proper carriage return handling
/// This captures LAMBDA's progress updates that use \r for same-line updates
fn stream_output_with_progress<R: Read + Send + 'static>(
    mut reader: R,
    prefix: &'static str,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut current_line = String::new();
        let mut byte = [0u8; 1];

        loop {
            match reader.read(&mut byte) {
                Ok(0) => {
                    // End of stream
                    if !current_line.is_empty() {
                        println!("  {}: {}", prefix, current_line);
                        std::io::stdout().flush().ok();
                    }
                    break;
                }
                Ok(_) => {
                    let ch = byte[0];

                    if ch == b'\r' {
                        // Carriage return - print current line and reset cursor
                        if !current_line.is_empty() {
                            print!("\r  {}: {}", prefix, current_line);
                            std::io::stdout().flush().ok();
                            current_line.clear();
                        }
                    } else if ch == b'\n' {
                        // Newline - print line and move to next
                        println!("  {}: {}", prefix, current_line);
                        std::io::stdout().flush().ok();
                        current_line.clear();
                    } else {
                        // Regular character - add to current line
                        current_line.push(ch as char);

                        // For immediate feedback, flush if we see dots being added
                        if ch == b'.' && current_line.len() % 10 == 0 {
                            print!("\r  {}: {}", prefix, current_line);
                            std::io::stdout().flush().ok();
                        }
                    }
                }
                Err(_) => break,
            }
        }
    })
}

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

        // First check TALARIA_TAXONOMY_DIR or default taxonomy location
        let taxonomy_dir = paths::talaria_taxonomy_dir();
        let tax_dump_dir = taxonomy_dir.join("taxdump");

        // Check if we have the required taxonomy dump files
        if tax_dump_dir.join("nodes.dmp").exists() &&
           tax_dump_dir.join("names.dmp").exists() {

            // Look for idmapping files - PRIORITIZE NCBI format which LAMBDA expects
            // Skip huge UniProt idmapping files unless explicitly enabled
            let use_large_idmapping = std::env::var("TALARIA_USE_LARGE_IDMAPPING").is_ok();

            let mut idmap_candidates = vec![
                // PRIORITIZE NCBI format (what LAMBDA expects)
                // Check in ncbi subdirectory (consistent with uniprot/ structure)
                taxonomy_dir.join("ncbi/prot.accession2taxid.gz"),
                taxonomy_dir.join("ncbi/prot.accession2taxid"),
                taxonomy_dir.join("ncbi/nucl.accession2taxid.gz"),
                taxonomy_dir.join("ncbi/nucl.accession2taxid"),
            ];

            // Only check for huge UniProt files if explicitly enabled
            // The 24GB idmapping.dat.gz causes LAMBDA to hang when loading
            if use_large_idmapping {
                println!("  Warning: Large UniProt idmapping enabled (may be slow)");
                idmap_candidates.extend(vec![
                    taxonomy_dir.join("uniprot/idmapping.dat.gz"),
                    taxonomy_dir.join("uniprot/idmapping.dat"),
                ]);
            };

            let idmap_path = idmap_candidates.into_iter().find(|p| p.exists());

            // Return taxdump even if no idmapping found (we add TaxID to headers)
            if let Some(ref idmap) = idmap_path {
                println!("  Found accession mapping: {:?}", idmap);
            } else {
                if !use_large_idmapping && taxonomy_dir.join("uniprot/idmapping.dat.gz").exists() {
                    println!("  Note: Large UniProt idmapping.dat.gz found but skipped (24GB file causes LAMBDA to hang)");
                    println!("  To use it anyway, set TALARIA_USE_LARGE_IDMAPPING=1 (not recommended)");
                    println!("  Using NCBI prot.accession2taxid.gz is recommended");
                } else {
                    println!("  Note: No accession2taxid mapping file found");
                    println!("  Expected location: {:?}", taxonomy_dir.join("ncbi/prot.accession2taxid.gz"));
                    println!("  (Consistent with uniprot/ subdirectory structure)");
                }
            }

            return (idmap_path, Some(tax_dump_dir));
        }

        // Fall back to check databases directory structure
        let db_base = paths::talaria_databases_dir();

        // Check alternative locations
        let alt_tax_dump = db_base.join("ncbi/taxonomy/taxdump");
        if alt_tax_dump.join("nodes.dmp").exists() &&
           alt_tax_dump.join("names.dmp").exists() {

            // Look for idmapping in various locations
            let acc_tax_map = Self::find_latest_file(&db_base.join("uniprot"), "idmapping.dat.gz")
                .or_else(|| Self::find_latest_file(&db_base.join("ncbi"), "prot.accession2taxid.gz"));

            return (acc_tax_map, Some(alt_tax_dump));
        }

        // No taxonomy found
        (None, None)
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


    /// Extract accession numbers from a FASTA file
    fn extract_accessions_from_fasta(&self, fasta_path: &Path) -> Result<HashSet<String>> {
        use std::collections::HashSet;
        let mut accessions = HashSet::new();
        let file = fs::File::open(fasta_path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if line.starts_with('>') {
                // Parse different header formats
                if let Some(acc) = Self::parse_accession_from_header(&line) {
                    accessions.insert(acc);
                }
            }
        }

        println!("  Extracted {} unique accessions from FASTA", accessions.len());
        Ok(accessions)
    }

    /// Parse accession from various FASTA header formats
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
                    && i + 1 < parts.len() {
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

    /// Create a filtered accession2taxid mapping with only needed entries
    fn create_filtered_mapping(&self,
                               large_mapping_file: &Path,
                               needed_accessions: &HashSet<String>) -> Result<PathBuf> {
        use flate2::read::GzDecoder;

        let filtered_path = self.temp_dir.join("filtered_acc2taxid.tsv");

        // Check if we already created it
        if filtered_path.exists() {
            println!("  Using existing filtered mapping file");
            return Ok(filtered_path);
        }

        println!("  Creating filtered accession2taxid mapping...");
        println!("    Source: {:?}", large_mapping_file);
        println!("    Filtering for {} accessions", needed_accessions.len());

        let file = fs::File::open(large_mapping_file)?;
        let reader: Box<dyn BufRead> = if large_mapping_file.extension()
            .and_then(|s| s.to_str()) == Some("gz") {
            Box::new(BufReader::new(GzDecoder::new(file)))
        } else {
            Box::new(BufReader::new(file))
        };

        let mut output = fs::File::create(&filtered_path)?;
        let mut found_count = 0;
        let mut line_count = 0;

        // Process the file line by line
        for line in reader.lines() {
            let line = line?;
            line_count += 1;

            if line_count % 1000000 == 0 {
                print!("\r    Processed {} million lines, found {} matches...",
                       line_count / 1000000, found_count);
                std::io::stdout().flush().ok();
            }

            // Skip header if present
            if line_count == 1 && (line.starts_with("accession") || line.starts_with('#')) {
                writeln!(output, "{}", line)?;
                continue;
            }

            // Parse the accession (first column)
            if let Some(accession) = line.split('\t').next() {
                // Remove version if present
                let acc_no_version = accession.split('.').next().unwrap_or(accession);

                if needed_accessions.contains(acc_no_version) {
                    writeln!(output, "{}", line)?;
                    found_count += 1;

                    // If we found all needed accessions, we can stop early
                    if found_count >= needed_accessions.len() {
                        println!("\n    Found all {} needed mappings!", found_count);
                        break;
                    }
                }
            }
        }

        println!("\n    Created filtered mapping with {} entries", found_count);

        if found_count == 0 {
            println!("    WARNING: No matching accessions found in mapping file!");
            println!("    This might indicate incompatible accession formats.");
        } else if found_count < needed_accessions.len() / 2 {
            println!("    Note: Only found {}/{} accessions. Some sequences may lack taxonomy.",
                     found_count, needed_accessions.len());
        }

        Ok(filtered_path)
    }

    /// Check if a mapping file should be filtered
    fn should_filter_mapping(&self, mapping_file: &Path) -> Result<bool> {
        if let Ok(metadata) = fs::metadata(mapping_file) {
            let size_mb = metadata.len() / (1024 * 1024);
            // Filter if file is larger than 100MB
            Ok(size_mb > 100)
        } else {
            Ok(false)
        }
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
        println!("  Input file: {:?}", fasta_path);
        println!("  Input size: {} bytes", fs::metadata(fasta_path).map(|m| m.len()).unwrap_or(0));
                std::io::stdout().flush().ok();

        // Check if sequences have taxonomic IDs in headers
        let has_tax_in_sequences = self.check_sequences_have_taxonomy(fasta_path)?;

        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("mkindexp")
           .arg("-d")
           .arg(fasta_path)
           .arg("-i")
           .arg(&index_path)
           .arg("--verbosity")
           .arg("2");  // Increase verbosity to see progress

        // Check what taxonomy resources we have
        let has_taxdump = self.tax_dump_dir.is_some();
        let has_idmapping = self.acc_tax_map.is_some();

        // Only use taxonomy if we have BOTH taxdump AND a way to map sequences
        // LAMBDA requires either an idmapping file OR sequences with embedded TaxIDs it can parse
        if let Some(tax_dump_dir) = &self.tax_dump_dir {
            if tax_dump_dir.exists() && has_idmapping {
                // We have both taxonomy and mapping - use full taxonomy features
                cmd.arg("--tax-dump-dir").arg(tax_dump_dir);
                println!("  Using taxonomy database: {:?}", tax_dump_dir);

                if let Some(acc_map) = &self.acc_tax_map {
                    cmd.arg("--acc-tax-map").arg(acc_map);
                    println!("  Using accession-to-taxid mapping: {:?}", acc_map.file_name().unwrap_or_default());
                }
                println!("  Full taxonomy features enabled");
            } else if tax_dump_dir.exists() {
                // We have taxdump but no idmapping - can't use taxonomy
                println!("  Note: Taxonomy database found but no idmapping file available");
                println!("  Running without taxonomy features (LAMBDA requires accession mapping)");
                println!("  To enable taxonomy, download idmapping files to: {:?}",
                         crate::core::paths::talaria_taxonomy_dir());
            }
        } else {
            println!("  Note: No taxonomy data found. Download with 'talaria database download':");
            println!("    - NCBI: talaria database download ncbi -d taxonomy");
        }

        // Show debug info if requested
        if std::env::var("TALARIA_DEBUG").is_ok() {
            println!("  DEBUG: Running command: {:?}", cmd);
            println!("  DEBUG: Working directory: {:?}", self.temp_dir);
        }

        // Use spawn() to stream output in real-time
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        println!("  Starting LAMBDA index creation...");
        println!("  Executing: {:?}", cmd);
        std::io::stdout().flush().ok();

        let mut child = cmd.spawn()
            .context("Failed to start LAMBDA mkindexp")?;

        // Stream both stdout and stderr in parallel using byte-based reading
        // This properly handles carriage returns for progress updates

        // Handle stderr in a thread
        let stderr_handle = if let Some(stderr) = child.stderr.take() {
            Some(stream_output_with_progress(stderr, "LAMBDA [stderr]"))
        } else {
            None
        };

        // Handle stdout in a thread
        let stdout_handle = if let Some(stdout) = child.stdout.take() {
            Some(stream_output_with_progress(stdout, "LAMBDA [stdout]"))
        } else {
            None
        };

        // Wait for threads to finish
        if let Some(handle) = stderr_handle {
            handle.join().ok();
        }
        if let Some(handle) = stdout_handle {
            handle.join().ok();
        }

        let status = child.wait()
            .context("Failed to wait for LAMBDA index creation")?;

        if !status.success() {
            // Try to provide more helpful error message
            let mut error_msg = format!("LAMBDA indexing failed with exit code: {:?}", status.code());

            if !has_tax_in_sequences && has_taxdump {
                error_msg.push_str("\n\nThis may be because sequences lack TaxID tags but taxonomy was requested.");
                error_msg.push_str("\nConsider downloading idmapping files or using sequences with TaxID tags.");
            }

            anyhow::bail!(error_msg);
        }

        // Verify index was created
        if !index_path.exists() {
            anyhow::bail!("LAMBDA index file was not created at {:?}", index_path);
        }

        Ok(index_path)
    }

    /// Run a LAMBDA search with given query and index
    fn run_lambda_search(&self, query_path: &Path, index_path: &Path, output_path: &Path) -> Result<()> {
        println!("Running LAMBDA alignment (this may take a few minutes for large datasets)...");

        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("searchp")
           .arg("-q")
           .arg(query_path)
           .arg("-i")
           .arg(index_path)
           .arg("-o")
           .arg(output_path);

        // Set output columns based on whether we have taxonomy
        // std = standard BLAST columns (12 columns)
        // slen = subject sequence length (important for coverage calculations)
        // qframe = query frame (for translated searches)
        // staxids = subject taxonomy IDs (only when taxonomy is available)
        if self.tax_dump_dir.is_some() && self.acc_tax_map.is_some() {
            cmd.arg("--output-columns").arg("std slen qframe staxids");
        } else {
            cmd.arg("--output-columns").arg("std slen qframe");
        }

        cmd.arg("--verbosity")
           .arg("2");  // Increase verbosity to see progress

        // Show debug info if requested
        if std::env::var("TALARIA_DEBUG").is_ok() {
            println!("  DEBUG: Running command: {:?}", cmd);
            println!("  DEBUG: Query file: {:?} ({} bytes)", query_path, fs::metadata(query_path).map(|m| m.len()).unwrap_or(0));
            println!("  DEBUG: Index file: {:?} ({} bytes)", index_path, fs::metadata(index_path).map(|m| m.len()).unwrap_or(0));
        }

        // Use spawn() to stream output in real-time
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        println!("  Starting LAMBDA search...");
        let mut child = cmd.spawn()
            .context("Failed to start LAMBDA searchp")?;

        // Stream both stdout and stderr in parallel using byte-based reading
        // This properly handles carriage returns for progress updates

        // Handle stderr in a thread
        let stderr_handle = if let Some(stderr) = child.stderr.take() {
            Some(stream_output_with_progress(stderr, "LAMBDA [stderr]"))
        } else {
            None
        };

        // Handle stdout in a thread
        let stdout_handle = if let Some(stdout) = child.stdout.take() {
            Some(stream_output_with_progress(stdout, "LAMBDA [stdout]"))
        } else {
            None
        };

        // Wait for threads to finish
        if let Some(handle) = stderr_handle {
            handle.join().ok();
        }
        if let Some(handle) = stdout_handle {
            handle.join().ok();
        }

        let status = child.wait()
            .context("Failed to wait for LAMBDA search")?;

        if !status.success() {
            // Check if output file was created but empty
            if output_path.exists() && fs::metadata(output_path)?.len() == 0 {
                // No alignments found is not necessarily an error
                println!("Note: No significant alignments found");
                return Ok(());
            }

            anyhow::bail!("LAMBDA search failed with exit code: {:?}", status.code());
        }

        Ok(())
    }

    /// Search query sequences against a reference database with batching for large datasets
    pub fn search_batched(&self, query_sequences: &[Sequence], reference_sequences: &[Sequence], batch_size: usize) -> Result<Vec<AlignmentResult>> {
        let mut all_results = Vec::new();

        // Create index once for all batches
        println!("Creating reference index (once for all batches)...");
        let reference_path = self.temp_dir.join("reference.fasta");
        Self::write_fasta_with_taxid(&reference_path, reference_sequences)?;
        let index_path = self.create_index(&reference_path)?;

        // Process queries in batches
        let num_batches = (query_sequences.len() + batch_size - 1) / batch_size;
        for (batch_idx, batch) in query_sequences.chunks(batch_size).enumerate() {
            println!("  Processing batch {}/{} ({} sequences)...",
                     batch_idx + 1, num_batches, batch.len());

            // Write batch queries
            let query_path = self.temp_dir.join(format!("query_batch_{}.fasta", batch_idx));
            Self::write_fasta_with_taxid(&query_path, batch)?;

            // Run search for this batch
            let output_path = self.temp_dir.join(format!("alignments_batch_{}.m8", batch_idx));
            self.run_lambda_search(&query_path, &index_path, &output_path)?;

            // Parse and collect results
            if output_path.exists() {
                let batch_results = self.parse_blast_tab(&output_path)?;
                println!("    Found {} alignments in batch {}", batch_results.len(), batch_idx + 1);
                all_results.extend(batch_results);
            }
        }

        println!("  Total alignments from all batches: {}", all_results.len());
        Ok(all_results)
    }

    /// Search query sequences against a reference database (default behavior)
    pub fn search(&self, query_sequences: &[Sequence], reference_sequences: &[Sequence]) -> Result<Vec<AlignmentResult>> {
        println!("Running LAMBDA query-vs-reference alignment...");
        println!("  Query sequences: {}", query_sequences.len());
        println!("  Reference sequences: {}", reference_sequences.len());

        // Use batching for large query sets to prevent memory issues
        const BATCH_SIZE: usize = 5000;

        if query_sequences.len() > BATCH_SIZE {
            println!("Large query set detected (>{} sequences), using batched search...", BATCH_SIZE);
            return self.search_batched(query_sequences, reference_sequences, BATCH_SIZE);
        }

        // For small datasets, use original single-pass approach
        // Write reference sequences to FASTA with TaxID added
        let reference_path = self.temp_dir.join("reference.fasta");
        Self::write_fasta_with_taxid(&reference_path, reference_sequences)?;
        let index_path = self.create_index(&reference_path)?;

        // Write query sequences to FASTA with TaxID added
        let query_path = self.temp_dir.join("query.fasta");
        Self::write_fasta_with_taxid(&query_path, query_sequences)?;

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

        // Write sequences to temporary FASTA with TaxID added
        let fasta_path = self.temp_dir.join("sequences.fasta");
        Self::write_fasta_with_taxid(&fasta_path, sequences_to_use)?;

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

    /// Add TaxID to sequences for LAMBDA compatibility
    /// Extracts organism from description and maps to common TaxIDs
    fn add_taxid_to_sequences(sequences: &[Sequence]) -> Vec<Sequence> {
        sequences.iter().map(|seq| {
            let mut modified_seq = seq.clone();

            // Check if sequence already has TaxID
            if let Some(ref desc) = seq.description {
                if desc.contains("TaxID=") || desc.contains("Tax=") {
                    return modified_seq;  // Already has TaxID
                }

                // Try to extract organism from OS= tag or description
                let desc_upper = desc.to_uppercase();

                // First try to extract from OS= tag
                let organism = if let Some(os_start) = desc.find("OS=") {
                    let os_text = &desc[os_start + 3..];
                    os_text.split_whitespace()
                        .take(2)  // Take first two words (genus species)
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_uppercase()
                } else {
                    desc_upper.clone()
                };

                // Map organism to TaxID
                let taxid = if organism.contains("HOMO SAPIENS") || desc_upper.contains("HUMAN") {
                    "9606"  // Human
                } else if organism.contains("MUS MUSCULUS") || desc_upper.contains("MOUSE") {
                    "10090"  // Mouse
                } else if organism.contains("RATTUS NORVEGICUS") || desc_upper.contains("RAT") {
                    "10116"  // Rat
                } else if organism.contains("DROSOPHILA MELANOGASTER") || desc_upper.contains("DROME") {
                    "7227"  // Fruit fly
                } else if organism.contains("CAENORHABDITIS ELEGANS") || desc_upper.contains("CAEEL") {
                    "6239"  // C. elegans
                } else if organism.contains("SACCHAROMYCES CEREVISIAE") || desc_upper.contains("YEAST") {
                    "4932"  // Baker's yeast
                } else if organism.contains("ESCHERICHIA COLI") || desc_upper.contains("ECOLI") {
                    "562"  // E. coli
                } else if organism.contains("ARABIDOPSIS THALIANA") || desc_upper.contains("ARATH") {
                    "3702"  // Arabidopsis
                } else if organism.contains("DANIO RERIO") || desc_upper.contains("ZEBRAFISH") {
                    "7955"  // Zebrafish
                } else if organism.contains("BOS TAURUS") || desc_upper.contains("BOVIN") {
                    "9913"  // Cow
                } else if organism.contains("SUS SCROFA") || desc_upper.contains("PIG") {
                    "9823"  // Pig
                } else if organism.contains("GALLUS GALLUS") || desc_upper.contains("CHICK") {
                    "9031"  // Chicken
                } else if organism.contains("XENOPUS") {
                    "8355"  // Xenopus
                } else if organism.contains("BACILLUS SUBTILIS") {
                    "1423"  // B. subtilis
                } else if organism.contains("STAPHYLOCOCCUS AUREUS") {
                    "1280"  // S. aureus
                } else if organism.contains("MYCOBACTERIUM TUBERCULOSIS") {
                    "1773"  // M. tuberculosis
                } else if organism.contains("PLASMODIUM FALCIPARUM") {
                    "5833"  // P. falciparum (malaria)
                } else {
                    "32644"  // Default: unclassified
                };

                // Append TaxID to description
                modified_seq.description = Some(format!("{} TaxID={}", desc, taxid));
            } else {
                // No description, add a minimal one with TaxID
                modified_seq.description = Some("TaxID=32644".to_string());
            }

            modified_seq
        }).collect()
    }

    /// Write sequences to FASTA with TaxID added for LAMBDA
    fn write_fasta_with_taxid(path: &Path, sequences: &[Sequence]) -> Result<()> {
        let sequences_with_taxid = Self::add_taxid_to_sequences(sequences);
        crate::bio::fasta::write_fasta(path, &sequences_with_taxid)
            .map_err(|e| anyhow::anyhow!("Failed to write FASTA: {}", e))
    }

    /// Check if sequences in FASTA have taxonomic IDs
    fn check_sequences_have_taxonomy(&self, fasta_path: &Path) -> Result<bool> {
        use std::io::{BufRead, BufReader};
        use std::fs::File;

        let file = File::open(fasta_path)?;
        let reader = BufReader::new(file);
        let mut checked_headers = 0;
        let mut headers_with_tax = 0;

        // Check first 100 headers
        for line in reader.lines() {
            let line = line?;
            if line.starts_with('>') {
                checked_headers += 1;
                // Check for various TaxID patterns
                if line.contains("TaxID=") || line.contains("OX=") ||
                   line.contains("taxon:") || line.contains("tax_id=") {
                    headers_with_tax += 1;
                }
                if checked_headers >= 100 {
                    break;
                }
            }
        }

        // Consider sequences to have taxonomy if >50% of checked headers have tax IDs
        Ok(checked_headers > 0 && headers_with_tax as f64 / checked_headers as f64 > 0.5)
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
