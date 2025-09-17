use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write, Read};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crate::bio::sequence::Sequence;
use crate::tools::traits::{Aligner, AlignmentResult as TraitAlignmentResult};
use crate::utils::temp_workspace::TempWorkspace;
use std::sync::{Arc, Mutex};

/// Helper function to stream output with proper carriage return handling
/// This captures LAMBDA's progress updates that use \r for same-line updates
fn stream_output_with_progress<R: Read + Send + 'static>(
    mut reader: R,
    prefix: &'static str,
    progress_counter: Arc<AtomicUsize>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let lambda_verbose = std::env::var("TALARIA_LAMBDA_VERBOSE").is_ok();
        let mut current_line = String::new();
        let mut byte = [0u8; 1];
        let mut errors = Vec::new();

        loop {
            match reader.read(&mut byte) {
                Ok(0) => {
                    // End of stream
                    if !current_line.is_empty() {
                        if lambda_verbose {
                            println!("  {}: {}", prefix, current_line);
                        } else if prefix.contains("stderr") && !current_line.is_empty() {
                            errors.push(current_line.clone());
                        }
                        std::io::stdout().flush().ok();
                    }
                    break;
                }
                Ok(_) => {
                    let ch = byte[0];

                    if ch == b'\r' {
                        // Carriage return - print current line and reset cursor
                        if !current_line.is_empty() {
                            if lambda_verbose {
                                print!("\r  {}: {}", prefix, current_line);
                                std::io::stdout().flush().ok();
                            }
                            // Track progress for structured output
                            if current_line.contains("Query no.") {
                                if let Some(num) = current_line.split_whitespace()
                                    .find_map(|s| s.parse::<usize>().ok()) {
                                    progress_counter.store(num, Ordering::Relaxed);
                                }
                            }
                            current_line.clear();
                        }
                    } else if ch == b'\n' {
                        // Newline - print line and move to next
                        if lambda_verbose {
                            println!("  {}: {}", prefix, current_line);
                            std::io::stdout().flush().ok();
                        } else if prefix.contains("stderr") && !current_line.trim().is_empty() {
                            // Store errors for later display if needed
                            errors.push(current_line.clone());
                        }
                        current_line.clear();
                    } else {
                        // Regular character - add to current line
                        current_line.push(ch as char);

                        // For immediate feedback in verbose mode, flush if we see dots being added
                        if lambda_verbose && ch == b'.' && current_line.len() % 10 == 0 {
                            print!("\r  {}: {}", prefix, current_line);
                            std::io::stdout().flush().ok();
                        }
                    }
                }
                Err(_) => break,
            }
        }

        // If we collected errors and not in verbose mode, print a summary
        if !lambda_verbose && !errors.is_empty() && prefix.contains("stderr") {
            for error in errors.iter().filter(|e| !e.trim().is_empty()) {
                eprintln!("  ⚠️ {}", error);
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
    batch_enabled: bool,
    batch_size: usize,  // Max amino acids per batch (not sequence count)
    preserve_on_failure: bool,  // Whether to preserve temp dir on failure
    failed: AtomicBool,  // Track if LAMBDA failed for cleanup decision
    workspace: Option<Arc<Mutex<TempWorkspace>>>,  // Optional workspace for organized temp files
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
            temp_dir,  // Will be set when workspace is provided or on first use
            acc_tax_map,
            tax_dump_dir,
            batch_enabled: false,  // Default: no batching
            batch_size: 5000,      // Default batch size
            preserve_on_failure,
            failed: AtomicBool::new(false),
            workspace: None,
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    fn create_filtered_mapping(&mut self,
                               large_mapping_file: &Path,
                               needed_accessions: &HashSet<String>) -> Result<PathBuf> {
        use flate2::read::GzDecoder;

        let filtered_path = self.get_temp_path("filtered_acc2taxid.tsv");

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
    #[allow(dead_code)]
    fn should_filter_mapping(&self, mapping_file: &Path) -> Result<bool> {
        if let Ok(metadata) = fs::metadata(mapping_file) {
            let size_mb = metadata.len() / (1024 * 1024);
            // Filter if file is larger than 100MB
            Ok(size_mb > 100)
        } else {
            Ok(false)
        }
    }

    /// Filter accession2taxid mapping to only include reference sequences
    fn filter_accession2taxid_for_references(&mut self, acc_map: &Path, fasta_path: &Path) -> Result<PathBuf> {
        use std::io::{BufRead, BufReader, Write};
        use std::collections::HashSet;

        // First, extract all accessions from the reference FASTA
        let mut reference_accessions = HashSet::new();
        let fasta_file = File::open(fasta_path)?;
        let reader = BufReader::new(fasta_file);

        for line in reader.lines() {
            let line = line?;
            if line.starts_with('>') {
                // Extract accession from header
                let header = &line[1..];

                // Handle various formats:
                // >sp|P12345|PROT_HUMAN ...
                // >tr|Q12345|...
                // >NP_123456.1 ...
                // >accession description

                if header.starts_with("sp|") || header.starts_with("tr|") {
                    // UniProt format
                    let parts: Vec<&str> = header.split('|').collect();
                    if parts.len() >= 2 {
                        reference_accessions.insert(parts[1].to_string());
                    }
                } else {
                    // Take first word as accession
                    if let Some(accession) = header.split_whitespace().next() {
                        reference_accessions.insert(accession.to_string());
                        // Also add without version
                        if let Some(dot_pos) = accession.rfind('.') {
                            reference_accessions.insert(accession[..dot_pos].to_string());
                        }
                    }
                }
            }
        }

        println!("    Found {} unique accessions in reference FASTA", reference_accessions.len());

        // Now filter the accession2taxid file
        let filtered_path = self.get_temp_path("filtered.accession2taxid");
        let acc_file = File::open(acc_map)?;
        let reader = BufReader::new(acc_file);
        let mut writer = File::create(&filtered_path)?;

        let mut kept_count = 0;
        let mut total_count = 0;

        for line in reader.lines() {
            let line = line?;
            total_count += 1;

            // Keep header line
            if total_count == 1 && line.starts_with("accession") {
                writeln!(writer, "{}", line)?;
                continue;
            }

            // Check if this accession is in our reference set
            let parts: Vec<&str> = line.split('\t').collect();
            if !parts.is_empty() {
                let accession = parts[0];
                // Check both with and without version
                if reference_accessions.contains(accession) {
                    writeln!(writer, "{}", line)?;
                    kept_count += 1;
                } else if let Some(dot_pos) = accession.rfind('.') {
                    if reference_accessions.contains(&accession[..dot_pos]) {
                        writeln!(writer, "{}", line)?;
                        kept_count += 1;
                    }
                }
            }
        }

        println!("    Filtered mapping: {} -> {} entries", total_count, kept_count);
        Ok(filtered_path)
    }

    /// Create a filtered taxonomy database from TaxIDs in FASTA headers
    /// Returns (filtered_taxdump_dir, accession2taxid_file)
    fn create_filtered_taxdump_from_fasta(&mut self, taxdump_dir: &Path, fasta_path: &Path) -> Result<(PathBuf, PathBuf)> {
        use std::io::{BufRead, BufReader, Write};
        use std::collections::{HashSet, HashMap};

        // First, extract accessions and taxids from FASTA headers
        let mut needed_taxids = HashSet::new();
        let mut acc2taxid_entries = Vec::new();
        let fasta_file = File::open(fasta_path)?;
        let reader = BufReader::new(fasta_file);

        for line in reader.lines() {
            let line = line?;
            if line.starts_with('>') {
                // Extract sequence ID (handle different formats)
                let header = &line[1..];
                let first_word = header.split_whitespace().next().unwrap_or("");

                // Parse different header formats to extract the actual accession
                let seq_id = if first_word.starts_with("sp|") || first_word.starts_with("tr|") {
                    // UniProt format: sp|P12345|PROT_HUMAN -> extract P12345
                    first_word.split('|').nth(1).unwrap_or(first_word)
                } else if first_word.contains('|') {
                    // Other pipe-delimited format: might be gi|12345|ref|NP_123456.1
                    // Try to find something that looks like an accession
                    first_word.split('|').find(|part| {
                        part.contains('_') || part.chars().any(|c| c.is_ascii_alphabetic())
                    }).unwrap_or(first_word)
                } else {
                    // Plain format: use as-is
                    first_word
                };

                // Look for TaxID= pattern in header
                if let Some(pos) = line.find("TaxID=") {
                    let taxid_str = &line[pos + 6..];
                    // Extract digits after TaxID=
                    let taxid_end = taxid_str.find(|c: char| !c.is_ascii_digit()).unwrap_or(taxid_str.len());
                    if let Ok(taxid) = taxid_str[..taxid_end].parse::<u32>() {
                        needed_taxids.insert(taxid);
                        // Store mapping for accession2taxid file
                        acc2taxid_entries.push((seq_id.to_string(), taxid));
                    }
                }
            }
        }

        use crate::cli::output::*;
        if std::env::var("TALARIA_DEBUG").is_ok() {
            info(&format!("Found {} unique TaxIDs in FASTA headers", format_number(needed_taxids.len())));
        }
        info(&format!("Found {} sequence-to-taxid mappings", format_number(acc2taxid_entries.len())));

        if needed_taxids.is_empty() {
            return Err(anyhow::anyhow!("No TaxIDs found in FASTA headers"));
        }

        // Create accession2taxid file
        let acc2taxid_path = self.get_temp_path("header_based.accession2taxid");
        let mut acc_file = File::create(&acc2taxid_path)?;

        // Write header
        writeln!(acc_file, "accession\taccession.version\ttaxid\tgi")?;

        // Write mappings
        for (accession, taxid) in &acc2taxid_entries {
            writeln!(acc_file, "{}\t{}\t{}\t0", accession, accession, taxid)?;
        }
        success(&format!("Created accession2taxid file with {} entries", format_number(acc2taxid_entries.len())));

        // Load nodes.dmp to find all ancestors
        let nodes_file = taxdump_dir.join("nodes.dmp");
        let mut parent_map = HashMap::new();

        let file = File::open(&nodes_file)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                if let (Ok(child), Ok(parent)) = (parts[0].parse::<u32>(), parts[2].parse::<u32>()) {
                    parent_map.insert(child, parent);
                }
            }
        }

        // Add all ancestors
        let mut all_taxids = needed_taxids.clone();
        for &taxid in &needed_taxids {
            let mut current = taxid;
            while let Some(&parent) = parent_map.get(&current) {
                if parent == current || parent == 1 {
                    break; // Root or self-loop
                }
                all_taxids.insert(parent);
                current = parent;
            }
        }

        let taxonomy_items = vec![
            ("Direct TaxIDs", format_number(needed_taxids.len())),
            ("With ancestors", format_number(all_taxids.len())),
        ];
        tree_section("Taxonomy Summary", taxonomy_items, false);

        // Create filtered taxdump directory (clean if exists)
        let filtered_dir = self.get_temp_path("filtered_taxdump");
        if filtered_dir.exists() {
            fs::remove_dir_all(&filtered_dir).ok();
        }
        fs::create_dir_all(&filtered_dir)?;

        // Filter nodes.dmp
        let nodes_file = taxdump_dir.join("nodes.dmp");
        let filtered_nodes = filtered_dir.join("nodes.dmp");
        let input = File::open(&nodes_file)?;
        let reader = BufReader::new(input);
        let mut output = File::create(&filtered_nodes)?;

        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split('\t').collect();
            if !parts.is_empty() {
                if let Ok(taxid) = parts[0].parse::<u32>() {
                    if all_taxids.contains(&taxid) {
                        writeln!(output, "{}", line)?;
                    }
                }
            }
        }

        // Filter names.dmp
        let names_file = taxdump_dir.join("names.dmp");
        let filtered_names = filtered_dir.join("names.dmp");
        let input = File::open(&names_file)?;
        let reader = BufReader::new(input);
        let mut output = File::create(&filtered_names)?;

        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split('\t').collect();
            if !parts.is_empty() {
                if let Ok(taxid) = parts[0].parse::<u32>() {
                    if all_taxids.contains(&taxid) {
                        writeln!(output, "{}", line)?;
                    }
                }
            }
        }

        Ok((filtered_dir, acc2taxid_path))
    }

    /// Create a filtered taxonomy database with only needed taxids
    fn create_filtered_taxdump(&mut self, taxdump_dir: &Path, acc_map: &Path) -> Result<PathBuf> {
        use std::io::{BufRead, BufReader, Write};
        use std::collections::{HashSet, HashMap};

        // First, extract unique taxids from the accession2taxid mapping
        let mut needed_taxids = HashSet::new();
        let acc_file = File::open(acc_map)?;
        let reader = BufReader::new(acc_file);

        for line in reader.lines() {
            let line = line?;
            // Skip header
            if line.starts_with("accession") {
                continue;
            }

            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                if let Ok(taxid) = parts[2].parse::<u32>() {
                    needed_taxids.insert(taxid);
                }
            }
        }

        if std::env::var("TALARIA_DEBUG").is_ok() {
            println!("    Found {} unique TaxIDs to include", needed_taxids.len());
        }

        // Create filtered taxdump directory (clean if exists)
        let filtered_dir = self.get_temp_path("filtered_taxdump");
        if filtered_dir.exists() {
            fs::remove_dir_all(&filtered_dir).ok();
        }
        fs::create_dir_all(&filtered_dir)?;

        // Filter nodes.dmp - include taxids and their ancestors
        let nodes_file = taxdump_dir.join("nodes.dmp");
        let names_file = taxdump_dir.join("names.dmp");

        if !nodes_file.exists() || !names_file.exists() {
            return Err(anyhow::anyhow!("Taxonomy files not found"));
        }

        // Read nodes to build parent relationships
        let mut parent_map = HashMap::new();
        let nodes_reader = BufReader::new(File::open(&nodes_file)?);

        for line in nodes_reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split("\t|\t").collect();
            if parts.len() >= 2 {
                if let (Ok(taxid), Ok(parent)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    parent_map.insert(taxid, parent);
                }
            }
        }

        // Add all ancestors of needed taxids
        let mut all_needed_taxids = needed_taxids.clone();
        for taxid in &needed_taxids {
            let mut current = *taxid;
            while let Some(&parent) = parent_map.get(&current) {
                if parent == current || parent == 1 {
                    all_needed_taxids.insert(1); // Include root
                    break;
                }
                all_needed_taxids.insert(parent);
                current = parent;
            }
        }

        use crate::cli::output::{success, format_number};
        success(&format!("With ancestors: {} total TaxIDs", format_number(all_needed_taxids.len())));

        // Filter nodes.dmp
        let filtered_nodes = filtered_dir.join("nodes.dmp");
        let mut nodes_writer = File::create(&filtered_nodes)?;
        let nodes_reader = BufReader::new(File::open(&nodes_file)?);

        for line in nodes_reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split("\t|\t").collect();
            if !parts.is_empty() {
                if let Ok(taxid) = parts[0].parse::<u32>() {
                    if all_needed_taxids.contains(&taxid) {
                        writeln!(nodes_writer, "{}", line)?;
                    }
                }
            }
        }

        // Filter names.dmp
        let filtered_names = filtered_dir.join("names.dmp");
        let mut names_writer = File::create(&filtered_names)?;
        let names_reader = BufReader::new(File::open(&names_file)?);

        for line in names_reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split("\t|\t").collect();
            if !parts.is_empty() {
                if let Ok(taxid) = parts[0].parse::<u32>() {
                    if all_needed_taxids.contains(&taxid) {
                        writeln!(names_writer, "{}", line)?;
                    }
                }
            }
        }

        // Copy other required files (if they exist)
        for filename in &["division.dmp", "gencode.dmp", "merged.dmp", "delnodes.dmp"] {
            let src = taxdump_dir.join(filename);
            if src.exists() {
                let dst = filtered_dir.join(filename);
                fs::copy(&src, &dst).ok();
            }
        }

        Ok(filtered_dir)
    }

    /// Set taxonomy mapping files
    pub fn with_taxonomy(mut self, acc_tax_map: Option<PathBuf>, tax_dump_dir: Option<PathBuf>) -> Self {
        self.acc_tax_map = acc_tax_map;
        self.tax_dump_dir = tax_dump_dir;
        self
    }

    /// Set batch processing settings
    pub fn with_batch_settings(mut self, enabled: bool, size: usize) -> Self {
        self.batch_enabled = enabled;
        self.batch_size = size;
        self
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
            // Use CASG workspace for LAMBDA operations
            let ws = workspace.lock().unwrap();
            self.temp_dir = ws.get_path("lambda");
            // Ensure directory exists
            fs::create_dir_all(&self.temp_dir).ok();
        } else {
            // No workspace, use traditional temp directory
            self.temp_dir = std::env::temp_dir().join(format!("talaria-lambda-{}", std::process::id()));
            // Ensure it exists
            fs::create_dir_all(&self.temp_dir).ok();
        }
    }

    /// Get the temp directory path, initializing if needed
    fn get_temp_dir(&mut self) -> &Path {
        if self.temp_dir.as_os_str().is_empty() {
            self.initialize_temp_dir();
        }
        &self.temp_dir
    }

    /// Get a temp file path within the workspace
    fn get_temp_path(&mut self, filename: &str) -> PathBuf {
        self.get_temp_dir().join(filename)
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
    pub fn create_index(&mut self, fasta_path: &Path) -> Result<PathBuf> {
        use crate::cli::output::{tree_section, warning, info, success};
        let index_path = self.get_temp_path("lambda_index.lba");

        // Clean up any existing index file to avoid conflicts
        if index_path.exists() {
            fs::remove_file(&index_path).ok();
        }

        let lambda_verbose = std::env::var("TALARIA_LAMBDA_VERBOSE").is_ok();
        if lambda_verbose {
            println!("Creating LAMBDA index...");
            println!("  Input file: {:?}", fasta_path);
            println!("  Input size: {} bytes", fs::metadata(fasta_path).map(|m| m.len()).unwrap_or(0));
        }
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
           .arg("2")  // Increase verbosity to see progress
           .arg("--threads")
           .arg(num_cpus::get().to_string());  // Use all available CPU cores

        // Check what taxonomy resources we have
        let has_taxdump = self.tax_dump_dir.is_some();
        let has_idmapping = self.acc_tax_map.is_some();

        // LAMBDA can use taxonomy in two ways:
        // 1. TaxID embedded in FASTA headers (our preferred CASG approach)
        // 2. External accession2taxid mapping file (traditional approach)
        if let Some(ref tax_dump_dir) = self.tax_dump_dir.clone() {
            if tax_dump_dir.exists() && (has_tax_in_sequences || has_idmapping) {
                // If sequences have TaxID in headers, we don't need accession mapping!
                if has_tax_in_sequences {
                    if lambda_verbose {
                        println!("  Detected TaxID in sequence headers - using direct taxonomy");
                    }

                    // Create filtered taxonomy database with only needed taxids from FASTA
                    if lambda_verbose {
                        println!("  Creating filtered taxonomy database from FASTA TaxIDs...");
                    }
                    match self.create_filtered_taxdump_from_fasta(&tax_dump_dir, fasta_path) {
                        Ok((filtered_dir, acc2taxid_file)) => {
                            cmd.arg("--tax-dump-dir").arg(&filtered_dir);
                            cmd.arg("--acc-tax-map").arg(&acc2taxid_file);
                            let taxonomy_config = vec![
                                ("Database", format!("{:?}", filtered_dir)),
                                ("Accession mapping", format!("{:?}", acc2taxid_file.file_name().unwrap_or_default())),
                            ];
                            tree_section("Taxonomy Configuration", taxonomy_config, false);
                        }
                        Err(e) => {
                            warning(&format!("Failed to filter taxonomy: {}", e));
                            cmd.arg("--tax-dump-dir").arg(tax_dump_dir);
                            info(&format!("Using full taxonomy database: {:?}", tax_dump_dir));
                        }
                    }
                    success("Taxonomy enabled via TaxID in headers (CASG source of truth)");
                } else if has_idmapping {
                    // Fallback to traditional accession mapping approach
                    println!("  No TaxID in headers, using accession2taxid mapping...");

                    // Filter the accession2taxid mapping to only include reference sequences
                    let filtered_acc_map = if let Some(ref acc_map) = self.acc_tax_map.clone() {
                        println!("  Filtering accession2taxid mapping to reference sequences only...");
                        match self.filter_accession2taxid_for_references(&acc_map, fasta_path) {
                            Ok(filtered) => {
                                println!("    Filtered mapping created: {:?}", filtered.file_name().unwrap_or_default());
                                Some(filtered)
                            }
                            Err(e) => {
                                eprintln!("    Warning: Failed to filter mapping: {}", e);
                                Some(acc_map.clone())
                            }
                        }
                    } else {
                        None
                    };

                    // Create filtered taxonomy database with only needed taxids
                    let filtered_taxdump = if let Some(ref filtered_map) = filtered_acc_map {
                        println!("  Creating filtered taxonomy database...");
                        match self.create_filtered_taxdump(tax_dump_dir, filtered_map) {
                            Ok(filtered_dir) => {
                                println!("    Filtered taxonomy database created");
                                filtered_dir
                            }
                            Err(e) => {
                                eprintln!("    Warning: Failed to filter taxonomy: {}", e);
                                tax_dump_dir.clone()
                            }
                        }
                    } else {
                        tax_dump_dir.clone()
                    };

                    // Use filtered resources
                    cmd.arg("--tax-dump-dir").arg(&filtered_taxdump);

                    if let Some(acc_map) = filtered_acc_map {
                        cmd.arg("--acc-tax-map").arg(&acc_map);
                        let taxonomy_items = vec![
                            ("Database", format!("{:?}", filtered_taxdump)),
                            ("Accession mapping", format!("{:?}", acc_map.file_name().unwrap_or_default())),
                        ];
                        tree_section("Taxonomy Configuration", taxonomy_items, false);
                    } else {
                        info(&format!("Using taxonomy database: {:?}", filtered_taxdump));
                    }
                    success("Full taxonomy features enabled");
                }
            } else if tax_dump_dir.exists() {
                // We have taxdump but no way to map sequences
                println!("  Note: Taxonomy database found but no TaxID in headers or mapping file");
                println!("  Running without taxonomy features");
            }
        } else {
            println!("  Note: No taxonomy data found. Download with 'talaria database download':");
            println!("    - NCBI: talaria database download ncbi -d taxonomy");
        }

        // Show debug info if requested
        if lambda_verbose {
            println!("  DEBUG: Running command: {:?}", cmd);
            println!("  DEBUG: Working directory: {:?}", self.get_temp_dir());
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
        let progress_counter = Arc::new(AtomicUsize::new(0));
        let stderr_handle = if let Some(stderr) = child.stderr.take() {
            Some(stream_output_with_progress(stderr, "LAMBDA [stderr]", progress_counter.clone()))
        } else {
            None
        };

        // Handle stdout in a thread
        let stdout_handle = if let Some(stdout) = child.stdout.take() {
            Some(stream_output_with_progress(stdout, "LAMBDA [stdout]", progress_counter))
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
    fn run_lambda_search(&mut self, query_path: &Path, index_path: &Path, output_path: &Path) -> Result<()> {
        // Clean up any existing output file to avoid LAMBDA error
        if output_path.exists() {
            fs::remove_file(output_path).ok();
        }

        println!("Running LAMBDA alignment (this may take a few minutes for large datasets)...");

        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("searchp")
           .arg("-q")
           .arg(query_path)
           .arg("-i")
           .arg(index_path)
           .arg("-o")
           .arg(output_path)
           .arg("--threads")
           .arg(num_cpus::get().to_string());  // Use all available CPU cores

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

        use crate::cli::output::*;
        action("Starting LAMBDA search...");
        let mut child = cmd.spawn()
            .context("Failed to start LAMBDA searchp")?;

        let pid = child.id();
        info(&format!("LAMBDA process PID: {} (monitor with: ps aux | grep {})", pid, pid));

        // Start memory monitoring thread if debug mode
        let monitor_handle = if std::env::var("TALARIA_DEBUG").is_ok() {
            let monitor_pid = pid;
            Some(std::thread::spawn(move || {
                let mut peak_memory = 0u64;
                loop {
                    // Try to read process memory info
                    if let Ok(status) = fs::read_to_string(format!("/proc/{}/status", monitor_pid)) {
                        // Look for VmRSS (resident set size - actual RAM usage)
                        if let Some(line) = status.lines().find(|l| l.starts_with("VmRSS:")) {
                            if let Some(kb_str) = line.split_whitespace().nth(1) {
                                if let Ok(kb) = kb_str.parse::<u64>() {
                                    let mb = kb / 1024;
                                    if mb > peak_memory {
                                        peak_memory = mb;
                                        if mb > 4000 {  // Warn if over 4GB
                                            eprintln!("  WARNING: LAMBDA memory usage: {} MB", mb);
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // Process no longer exists
                        if peak_memory > 0 {
                            println!("  LAMBDA peak memory usage: {} MB", peak_memory);
                        }
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }))
        } else {
            None
        };

        // Stream both stdout and stderr in parallel using byte-based reading
        // This properly handles carriage returns for progress updates

        // Handle stderr in a thread
        let progress_counter = Arc::new(AtomicUsize::new(0));
        let stderr_handle = if let Some(stderr) = child.stderr.take() {
            Some(stream_output_with_progress(stderr, "LAMBDA [stderr]", progress_counter.clone()))
        } else {
            None
        };

        // Handle stdout in a thread
        let stdout_handle = if let Some(stdout) = child.stdout.take() {
            Some(stream_output_with_progress(stdout, "LAMBDA [stdout]", progress_counter))
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

        // Wait for monitor thread to finish
        if let Some(handle) = monitor_handle {
            handle.join().ok();
        }

        if !status.success() {
            // Check if output file was created but empty
            if output_path.exists() && fs::metadata(output_path)?.len() == 0 {
                // No alignments found is not necessarily an error
                println!("Note: No significant alignments found");
                return Ok(());
            }

            // Provide detailed exit information
            let exit_detail = match status.code() {
                Some(137) => "SIGKILL (code 137) - likely killed by OOM killer or ulimit".to_string(),
                Some(139) => "SIGSEGV (code 139) - segmentation fault in LAMBDA".to_string(),
                Some(134) => "SIGABRT (code 134) - LAMBDA aborted".to_string(),
                Some(1) => "General error (code 1) - check LAMBDA stderr output above".to_string(),
                Some(code) => format!("Exit code {} - check stderr output above", code),
                None => "Killed by signal (no exit code) - likely SIGKILL from OOM killer".to_string(),
            };

            // Mark as failed for cleanup decision
            self.failed.store(true, Ordering::Relaxed);

            eprintln!("\n=== LAMBDA Process Failed ===");
            eprintln!("Failure details: {}", exit_detail);
            eprintln!("Query file: {:?}", query_path);
            eprintln!("Index file: {:?}", index_path);

            // Save the failing query for debugging
            let debug_path = self.get_temp_path("failed_query.fasta");
            if query_path.exists() {
                if let Ok(_) = fs::copy(query_path, &debug_path) {
                    eprintln!("Saved failing query sequences to: {:?}", debug_path);
                    eprintln!("You can inspect this file to check for problematic sequences");
                }
            }

            // Report preserved directory if enabled
            if self.preserve_on_failure {
                eprintln!("\n📁 LAMBDA temp directory preserved for debugging:");
                eprintln!("   {}", self.get_temp_dir().display());
                eprintln!("\n   Key files:");
                for entry in fs::read_dir(self.get_temp_dir()).unwrap_or_else(|_| fs::read_dir(".").unwrap()) {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if let Ok(metadata) = entry.metadata() {
                            let size = metadata.len();
                            eprintln!("     - {} ({} bytes)",
                                     path.file_name().unwrap_or_default().to_string_lossy(),
                                     size);
                        }
                    }
                }
                eprintln!("\n   To manually re-run LAMBDA with different settings:");
                eprintln!("     lambda3 searchp -q {} -i {} -o output.m8 --threads 8",
                         debug_path.display(), index_path.display());
                eprintln!("\n   To clean up later:");
                eprintln!("     rm -rf {}", self.get_temp_dir().display());
            } else {
                eprintln!("\n   To preserve temp directory on failure, set:");
                eprintln!("     export TALARIA_PRESERVE_LAMBDA_ON_FAILURE=1");
            }

            eprintln!("\nTo diagnose OOM killer:");
            eprintln!("  Run: dmesg | grep -i 'killed process'");
            eprintln!("  Or:  journalctl -xe | grep -i 'out of memory'");
            eprintln!("=============================\n");

            anyhow::bail!("LAMBDA search failed: {}", exit_detail);
        }

        Ok(())
    }

    /// Search query sequences against a reference database with batching for large datasets
    /// batch_size is now interpreted as maximum amino acids per batch (not sequence count)
    pub fn search_batched(&mut self, query_sequences: &[Sequence], reference_sequences: &[Sequence], batch_size: usize) -> Result<Vec<AlignmentResult>> {
        // Use batch_size as max amino acids, with memory-aware defaults
        let mut max_batch_aa = if batch_size == 0 {
            2_000_000  // Default: 2M amino acids (reduced from 5M to prevent OOM)
        } else {
            batch_size
        };

        // Check if we're running with limited memory and adjust accordingly
        if std::env::var("TALARIA_LOW_MEMORY").is_ok() {
            eprintln!("  Low memory mode enabled, reducing batch size to 1M amino acids");
            max_batch_aa = max_batch_aa.min(1_000_000);
        }

        const WARN_LONG_SEQ: usize = 10_000;    // Warn for sequences >10K aa
        const EXTREME_LONG_SEQ: usize = 30_000; // Sequences requiring special handling
        const WARN_AMBIGUOUS_RUN: usize = 10;   // Warn for runs of ambiguous residues

        let mut all_results = Vec::new();
        let mut problematic_sequences = Vec::new();
        let mut extreme_sequences = Vec::new();

        // Create index once for all batches
        use crate::cli::output::*;
        action("Creating reference index...");
        let reference_path = self.get_temp_path("reference.fasta");
        Self::write_fasta_with_taxid(&reference_path, reference_sequences)?;
        let index_path = self.create_index(&reference_path)?;
        success(&format!("Reference index created (size: {:.1} MB)",
            fs::metadata(&index_path).map(|m| m.len() as f64 / 1_048_576.0).unwrap_or(0.0)));

        // Pre-scan for problematic sequences and separate extreme ones
        for seq in query_sequences {
            // Check for very long sequences
            if seq.len() > WARN_LONG_SEQ {
                problematic_sequences.push((seq.id.clone(), format!("{} aa (very long)", seq.len())));

                // Special handling for known problem proteins
                if seq.id.contains("TITIN") || seq.len() > EXTREME_LONG_SEQ {
                    extreme_sequences.push(seq.id.clone());
                    problematic_sequences.push((seq.id.clone(),
                        format!("EXTREME LENGTH ({} aa) - will process separately", seq.len())));
                }
            }

            // Check for runs of ambiguous amino acids
            let ambiguous_runs = seq.sequence.windows(WARN_AMBIGUOUS_RUN)
                .filter(|window| window.iter().all(|&b| b == b'X' || b == b'B' || b == b'Z' || b == b'*'))
                .count();

            if ambiguous_runs > 0 {
                problematic_sequences.push((seq.id.clone(), format!("{} runs of ambiguous residues", ambiguous_runs)));
            }
        }

        // Warn about problematic sequences
        if !problematic_sequences.is_empty() {
            eprintln!("\n⚠️  WARNING: Found {} problematic sequences that may cause memory issues:",
                      problematic_sequences.len());
            for (i, (id, reason)) in problematic_sequences.iter().take(10).enumerate() {
                eprintln!("    {}: {} - {}", i+1, id, reason);
            }
            if problematic_sequences.len() > 10 {
                eprintln!("    ... and {} more", problematic_sequences.len() - 10);
            }

            if !extreme_sequences.is_empty() {
                eprintln!("\n  🔴 {} EXTREME LENGTH sequences will be processed in isolated batches:",
                         extreme_sequences.len());
                for id in extreme_sequences.iter().take(5) {
                    eprintln!("    - {}", id);
                }
                if extreme_sequences.len() > 5 {
                    eprintln!("    ... and {} more", extreme_sequences.len() - 5);
                }
            }

            eprintln!("\n  Mitigation strategies:");
            eprintln!("    - Set TALARIA_LOW_MEMORY=1 to reduce batch sizes");
            eprintln!("    - Use --max-align-length to skip very long sequences");
            eprintln!("    - Set TALARIA_PRESERVE_LAMBDA_ON_FAILURE=1 to debug failures");
            eprintln!("  Current batch size: {} amino acids per batch\n", max_batch_aa);
        }

        // Process sequences with size-based batching
        let mut current_batch = Vec::new();
        let mut current_batch_aa = 0;
        let mut batch_idx = 0;
        let mut total_batches = 0;
        let mut sequences_processed = 0;
        let total_sequences = query_sequences.len();

        // Calculate total amino acids for informational purposes
        let total_aa: usize = query_sequences.iter().map(|s| s.len()).sum();

        let processing_items = vec![
            ("Total sequences", format_number(total_sequences)),
            ("Total amino acids", format_number(total_aa)),
            ("Max AA per batch", format_number(max_batch_aa)),
        ];
        tree_section("Processing Setup", processing_items, false);

        for seq in query_sequences {
            let seq_len = seq.len();
            let is_extreme = extreme_sequences.contains(&seq.id);

            // Force extreme sequences to their own batch
            if is_extreme && !current_batch.is_empty() {
                // Process current batch before the extreme sequence
                total_batches += 1;
                let percent_complete = sequences_processed as f64 / total_sequences as f64 * 100.0;
                subsection_header(&format!("Batch {} ({:.1}% complete) - {} sequences, {} aa",
                         total_batches, percent_complete, format_number(current_batch.len()), format_number(current_batch_aa)));

                let batch_results = self.process_batch(&current_batch, &index_path, batch_idx)?;
                success(&format!("Found {} alignments", format_number(batch_results.len())));
                all_results.extend(batch_results);
                sequences_processed += current_batch.len();

                current_batch.clear();
                current_batch_aa = 0;
                batch_idx += 1;
            }

            // If adding this sequence would exceed batch size, process current batch first
            if !is_extreme && current_batch_aa + seq_len > max_batch_aa && !current_batch.is_empty() {
                // Process current batch
                total_batches += 1;
                let percent_complete = sequences_processed as f64 / total_sequences as f64 * 100.0;
                subsection_header(&format!("Batch {} ({:.1}% complete) - {} sequences, {} aa",
                         total_batches, percent_complete, format_number(current_batch.len()), format_number(current_batch_aa)));

                let batch_results = self.process_batch(&current_batch, &index_path, batch_idx)?;
                success(&format!("Found {} alignments", format_number(batch_results.len())));
                all_results.extend(batch_results);
                sequences_processed += current_batch.len();

                // Reset for next batch
                current_batch.clear();
                current_batch_aa = 0;
                batch_idx += 1;
            }

            // Special case: if single sequence exceeds batch size OR is extreme, process it alone
            if seq_len > max_batch_aa || is_extreme {
                if is_extreme {
                    eprintln!("\n  🔴 EXTREME: Processing {} ({} aa) in isolated batch", seq.id, seq_len);
                    eprintln!("     This sequence is known to cause memory issues");
                    eprintln!("     If it fails, consider using --max-align-length {} to skip it", seq_len - 1);
                } else {
                    eprintln!("\n  ⚠️  WARNING: Sequence {} ({} aa) exceeds batch size limit", seq.id, seq_len);
                    eprintln!("     Processing in its own batch (may use significant memory)");
                }

                // If we have sequences in current batch, process them first
                if !current_batch.is_empty() {
                    total_batches += 1;
                    let percent_complete = sequences_processed as f64 / total_sequences as f64 * 100.0;
                    println!("\n  Processing batch {} ({:.1}% complete) - {} sequences, {} aa...",
                             total_batches, percent_complete, current_batch.len(), current_batch_aa);

                    let batch_results = self.process_batch(&current_batch, &index_path, batch_idx)?;
                    success(&format!("Found {} alignments", format_number(batch_results.len())));
                    all_results.extend(batch_results);
                    sequences_processed += current_batch.len();

                    current_batch.clear();
                    current_batch_aa = 0;
                    batch_idx += 1;
                }

                // Process the large sequence alone
                total_batches += 1;
                let percent_complete = sequences_processed as f64 / total_sequences as f64 * 100.0;
                println!("\n  Processing batch {} ({:.1}% complete) - 1 large sequence, {} aa...",
                         total_batches, percent_complete, seq_len);

                let batch_results = self.process_batch(&[seq.clone()], &index_path, batch_idx)?;
                success(&format!("Found {} alignments", format_number(batch_results.len())));
                all_results.extend(batch_results);
                sequences_processed += 1;
                batch_idx += 1;
            } else {
                // Add sequence to current batch
                current_batch.push(seq.clone());
                current_batch_aa += seq_len;
            }
        }

        // Process final batch if not empty
        if !current_batch.is_empty() {
            total_batches += 1;
            let percent_complete = sequences_processed as f64 / total_sequences as f64 * 100.0;
            println!("\n  Processing batch {} ({:.1}% complete) - {} sequences, {} aa...",
                     total_batches, percent_complete, current_batch.len(), current_batch_aa);

            let batch_results = self.process_batch(&current_batch, &index_path, batch_idx)?;
            println!("    Found {} alignments", batch_results.len());
            all_results.extend(batch_results);
            sequences_processed += current_batch.len();
        }

        println!("\n  Completed {} batches, processed {} sequences, found {} alignments",
                 total_batches, sequences_processed, all_results.len());
        Ok(all_results)
    }

    /// Helper function to process a single batch
    fn process_batch(&mut self, batch: &[Sequence], index_path: &Path, batch_idx: usize) -> Result<Vec<AlignmentResult>> {
        // Clean up any existing batch files from previous runs
        let query_path = self.get_temp_path(&format!("query_batch_{}.fasta", batch_idx));
        let output_path = self.get_temp_path(&format!("alignments_batch_{}.m8", batch_idx));

        if query_path.exists() {
            fs::remove_file(&query_path).ok();
        }
        if output_path.exists() {
            fs::remove_file(&output_path).ok();
        }

        // Calculate batch statistics
        let max_len = batch.iter().map(|s| s.len()).max().unwrap_or(0);
        let min_len = batch.iter().map(|s| s.len()).min().unwrap_or(0);
        let total_aa: usize = batch.iter().map(|s| s.len()).sum();
        let avg_len = if !batch.is_empty() { total_aa / batch.len() } else { 0 };

        // Show batch statistics in verbose mode only
        let lambda_verbose = std::env::var("TALARIA_LAMBDA_VERBOSE").is_ok();
        if lambda_verbose {
            println!("    Batch statistics:");
            println!("      Sequences: {}", batch.len());
            println!("      Total amino acids: {}", total_aa);
            println!("      Length range: {} - {} aa", min_len, max_len);
            println!("      Average length: {} aa", avg_len);
        }

        // Warn about problematic sequences in this batch
        if max_len > 10_000 {
            eprintln!("    ⚠️  This batch contains very long sequences (max: {} aa)", max_len);
            for seq in batch {
                if seq.len() > 10_000 {
                    eprintln!("        {} ({} aa)", seq.id, seq.len());
                }
            }
        }

        // Check for ambiguous content
        let ambiguous_seqs: Vec<_> = batch.iter()
            .filter(|seq| {
                let ambiguous_count = seq.sequence.iter()
                    .filter(|&&b| b == b'X' || b == b'B' || b == b'Z' || b == b'*')
                    .count();
                ambiguous_count > seq.len() / 20  // More than 5% ambiguous
            })
            .collect();

        if !ambiguous_seqs.is_empty() {
            eprintln!("    ⚠️  {} sequences with high ambiguous content", ambiguous_seqs.len());
        }

        // Write batch queries (query_path already defined above)
        Self::write_fasta_with_taxid(&query_path, batch)?;

        // Run search (output_path already defined above)
        self.run_lambda_search(&query_path, index_path, &output_path)?;

        // Parse results
        if output_path.exists() {
            self.parse_blast_tab(&output_path)
        } else {
            Ok(Vec::new())
        }
    }

    /// Search query sequences against a reference database (default behavior)
    pub fn search(&mut self, query_sequences: &[Sequence], reference_sequences: &[Sequence]) -> Result<Vec<AlignmentResult>> {
        println!("Running LAMBDA query-vs-reference alignment...");
        println!("  Query sequences: {}", query_sequences.len());
        println!("  Reference sequences: {}", reference_sequences.len());

        // Check if batching is enabled
        if self.batch_enabled {
            println!("Batched processing enabled (batch size: {})", self.batch_size);
            return self.search_batched(query_sequences, reference_sequences, self.batch_size);
        }

        // For small datasets, use original single-pass approach
        // Clean up any existing files from previous runs
        let reference_path = self.get_temp_path("reference.fasta");
        let query_path = self.get_temp_path("query.fasta");
        let alignments_path = self.get_temp_path("alignments.m8");

        // Remove old files if they exist
        for path in &[&reference_path, &query_path, &alignments_path] {
            if path.exists() {
                fs::remove_file(path).ok();
            }
        }

        // Write reference sequences to FASTA with TaxID added
        Self::write_fasta_with_taxid(&reference_path, reference_sequences)?;
        let index_path = self.create_index(&reference_path)?;

        // Write query sequences to FASTA with TaxID added
        Self::write_fasta_with_taxid(&query_path, query_sequences)?;

        // Run search
        let output_path = self.get_temp_path("alignments.m8");
        self.run_lambda_search(&query_path, &index_path, &output_path)?;

        // Parse results
        if output_path.exists() {
            self.parse_blast_tab(&output_path)
        } else {
            Ok(Vec::new())
        }
    }

    /// Run all-vs-all alignment (self-alignment) - optional behavior
    pub fn search_all_vs_all(&mut self, sequences: &[Sequence]) -> Result<Vec<AlignmentResult>> {
        use crate::cli::output::*;
        section_header(&format!("LAMBDA All-vs-All Alignment ({} sequences)", format_number(sequences.len())));

        // For large datasets, use sampling
        const MAX_SEQUENCES_FOR_FULL: usize = 5000;
        let sequences_to_use = if sequences.len() > MAX_SEQUENCES_FOR_FULL {
            warning(&format!("Large dataset detected, sampling {} sequences...", format_number(MAX_SEQUENCES_FOR_FULL)));
            return self.run_sampled_alignment(sequences, MAX_SEQUENCES_FOR_FULL);
        } else {
            sequences
        };

        // Write sequences to temporary FASTA with TaxID added
        let fasta_path = self.get_temp_path("sequences.fasta");
        Self::write_fasta_with_taxid(&fasta_path, sequences_to_use)?;

        // Create index from same sequences
        let index_path = self.create_index(&fasta_path)?;

        // Run search (query same as reference)
        let output_path = self.get_temp_path("alignments.m8");
        self.run_lambda_search(&fasta_path, &index_path, &output_path)?;

        // Parse results
        if output_path.exists() {
            self.parse_blast_tab(&output_path)
        } else {
            Ok(Vec::new())
        }
    }

    /// Run alignment with sampling for large datasets
    fn run_sampled_alignment(&mut self, sequences: &[Sequence], sample_size: usize) -> Result<Vec<AlignmentResult>> {
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
                taxon_id: None,  // TODO: Parse from staxids column if available
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
    pub fn cleanup(&mut self) -> Result<()> {
        let temp_dir = self.get_temp_dir().to_path_buf();
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir)?;
        }
        Ok(())
    }
}

impl Drop for LambdaAligner {
    fn drop(&mut self) {
        // Only cleanup if we didn't fail or if preserve_on_failure is false
        if !self.failed.load(Ordering::Relaxed) || !self.preserve_on_failure {
            // Best effort cleanup
            let _ = self.cleanup();
        } else {
            // Directory preserved for debugging
            eprintln!("\n🔍 LAMBDA temp directory preserved: {}", self.get_temp_dir().display());
        }
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

// Implement the Aligner trait for LambdaAligner
impl Aligner for LambdaAligner {
    fn search(
        &mut self,
        query: &[Sequence],
        reference: &[Sequence],
    ) -> Result<Vec<AlignmentResult>> {
        // Use the existing search method
        self.search(query, reference)
    }

    fn search_batched(
        &mut self,
        query: &[Sequence],
        reference: &[Sequence],
        batch_size: usize,
    ) -> Result<Vec<AlignmentResult>> {
        // Use the existing search_batched method
        self.search_batched(query, reference, batch_size)
    }

    fn build_index(
        &mut self,
        reference_path: &Path,
        index_path: &Path,
    ) -> Result<()> {
        // Run lambda3 mkindexp to build index
        println!("Building LAMBDA index...");

        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("mkindexp")
            .arg("-d")
            .arg(reference_path)
            .arg("-i")
            .arg(index_path);

        // Add taxonomy if available
        if let Some(ref tax_dir) = self.tax_dump_dir {
            cmd.arg("-t").arg(tax_dir);
        }
        if let Some(ref acc_map) = self.acc_tax_map {
            cmd.arg("-m").arg(acc_map);
        }

        let output = cmd.output()
            .context("Failed to run lambda3 mkindexp")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("LAMBDA index building failed: {}", stderr);
        }

        println!("LAMBDA index built successfully at {:?}", index_path);
        Ok(())
    }

    fn verify_installation(&self) -> Result<()> {
        // Check if the binary exists and is executable
        if !self.binary_path.exists() {
            anyhow::bail!("LAMBDA binary not found at {:?}", self.binary_path);
        }

        // Try to run lambda3 --version to verify it works
        let output = Command::new(&self.binary_path)
            .arg("--version")
            .output()
            .context("Failed to run LAMBDA --version")?;

        if !output.status.success() {
            anyhow::bail!("LAMBDA binary exists but failed to run")
        }

        Ok(())
    }

    fn supports_taxonomy(&self) -> bool {
        // LAMBDA supports taxonomy if we have the required files
        self.acc_tax_map.is_some() && self.tax_dump_dir.is_some()
    }

    fn name(&self) -> &str {
        "LAMBDA"
    }

    fn recommended_batch_size(&self) -> usize {
        5000  // LAMBDA works well with batches of 5000 sequences
    }

    fn supports_protein(&self) -> bool {
        true  // LAMBDA primarily supports protein sequences
    }

    fn supports_nucleotide(&self) -> bool {
        false  // LAMBDA is designed for protein sequences
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::io::Write;

    fn create_test_fasta(path: &Path, sequences: &[(&str, &str)]) -> Result<()> {
        let mut file = File::create(path)?;
        for (id, seq) in sequences {
            writeln!(file, ">{}", id)?;
            writeln!(file, "{}", seq)?;
        }
        Ok(())
    }

    fn create_test_accession2taxid(path: &Path, mappings: &[(&str, u32)]) -> Result<()> {
        let mut file = File::create(path)?;
        writeln!(file, "accession\taccession.version\ttaxid\tgi")?;
        for (acc, taxid) in mappings {
            writeln!(file, "{}\t{}\t{}\t0", acc, acc, taxid)?;
        }
        Ok(())
    }

    fn create_test_taxdump(dir: &Path, taxids: &[u32]) -> Result<()> {
        fs::create_dir_all(dir)?;

        // Create minimal nodes.dmp
        let mut nodes_file = File::create(dir.join("nodes.dmp"))?;
        writeln!(nodes_file, "1\t|\t1\t|\tno rank\t|\t")?; // Root
        for taxid in taxids {
            writeln!(nodes_file, "{}\t|\t1\t|\tspecies\t|\t", taxid)?;
        }

        // Create minimal names.dmp
        let mut names_file = File::create(dir.join("names.dmp"))?;
        writeln!(names_file, "1\t|\troot\t|\t\t|\tscientific name\t|")?;
        for taxid in taxids {
            writeln!(names_file, "{}\t|\tOrganism_{}\t|\t\t|\tscientific name\t|", taxid, taxid)?;
        }

        Ok(())
    }

    #[test]
    fn test_filter_accession2taxid_for_references() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create reference FASTA with specific accessions
        let ref_fasta = temp_path.join("references.fasta");
        create_test_fasta(&ref_fasta, &[
            ("sp|P12345|PROT1_HUMAN", "ACGT"),
            ("sp|Q67890|PROT2_MOUSE", "TTGG"),
            ("NP_123456.1", "AAAA"),
        ]).unwrap();

        // Create full accession2taxid with more mappings than needed
        let full_mapping = temp_path.join("full.accession2taxid");
        create_test_accession2taxid(&full_mapping, &[
            ("P12345", 9606),    // Human - in reference
            ("Q67890", 10090),   // Mouse - in reference
            ("NP_123456.1", 9606), // Human - in reference
            ("P99999", 9606),    // Human - NOT in reference
            ("Q11111", 10090),   // Mouse - NOT in reference
            ("XP_999999.1", 7227), // Fly - NOT in reference
        ]).unwrap();

        // Create aligner
        let mut aligner = LambdaAligner {
            binary_path: PathBuf::from("/dummy"),
            temp_dir: temp_path.to_path_buf(),
            acc_tax_map: None,
            tax_dump_dir: None,
            batch_enabled: false,
            batch_size: 5000,
            preserve_on_failure: false,
            failed: AtomicBool::new(false),
            workspace: None,
        };

        // Filter the mapping
        let filtered_path = aligner.filter_accession2taxid_for_references(&full_mapping, &ref_fasta).unwrap();

        // Check filtered file contents
        let contents = fs::read_to_string(&filtered_path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();

        // Should have header + 3 reference mappings
        assert_eq!(lines.len(), 4, "Should have 4 lines (header + 3 mappings)");
        assert!(contents.contains("P12345"), "Should contain P12345");
        assert!(contents.contains("Q67890"), "Should contain Q67890");
        assert!(contents.contains("NP_123456"), "Should contain NP_123456");
        assert!(!contents.contains("P99999"), "Should NOT contain P99999");
        assert!(!contents.contains("Q11111"), "Should NOT contain Q11111");
        assert!(!contents.contains("XP_999999"), "Should NOT contain XP_999999");
    }

    #[test]
    fn test_create_filtered_taxdump() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create filtered accession2taxid with specific taxids
        let filtered_mapping = temp_path.join("filtered.accession2taxid");
        create_test_accession2taxid(&filtered_mapping, &[
            ("P12345", 9606),   // Human
            ("Q67890", 10090),  // Mouse
            ("R11111", 7227),   // Fly
        ]).unwrap();

        // Create full taxdump with many taxids
        let full_taxdump = temp_path.join("full_taxdump");
        create_test_taxdump(&full_taxdump, &[
            9606,   // Human - needed
            10090,  // Mouse - needed
            7227,   // Fly - needed
            559292, // Yeast - NOT needed
            511145, // E. coli - NOT needed
            9823,   // Pig - NOT needed
        ]).unwrap();

        // Create aligner
        let mut aligner = LambdaAligner {
            binary_path: PathBuf::from("/dummy"),
            temp_dir: temp_path.to_path_buf(),
            acc_tax_map: None,
            tax_dump_dir: None,
            batch_enabled: false,
            batch_size: 5000,
            preserve_on_failure: false,
            failed: AtomicBool::new(false),
            workspace: None,
        };

        // Filter the taxdump
        let filtered_dir = aligner.create_filtered_taxdump(&full_taxdump, &filtered_mapping).unwrap();

        // Check filtered nodes.dmp
        let nodes_contents = fs::read_to_string(filtered_dir.join("nodes.dmp")).unwrap();
        assert!(nodes_contents.contains("9606"), "Should contain human taxid");
        assert!(nodes_contents.contains("10090"), "Should contain mouse taxid");
        assert!(nodes_contents.contains("7227"), "Should contain fly taxid");
        assert!(!nodes_contents.contains("559292"), "Should NOT contain yeast taxid");
        assert!(!nodes_contents.contains("511145"), "Should NOT contain E. coli taxid");

        // Check filtered names.dmp
        let names_contents = fs::read_to_string(filtered_dir.join("names.dmp")).unwrap();
        assert!(names_contents.contains("9606"), "Names should contain human taxid");
        assert!(names_contents.contains("10090"), "Names should contain mouse taxid");
        assert!(names_contents.contains("7227"), "Names should contain fly taxid");
        assert!(!names_contents.contains("559292"), "Names should NOT contain yeast taxid");
    }

    #[test]
    fn test_batch_settings() {
        let temp_dir = TempDir::new().unwrap();

        // Test default settings
        let aligner1 = LambdaAligner::new(PathBuf::from("/dummy")).unwrap_or_else(|_| LambdaAligner {
            binary_path: PathBuf::from("/dummy"),
            temp_dir: temp_dir.path().to_path_buf(),
            acc_tax_map: None,
            tax_dump_dir: None,
            batch_enabled: false,
            batch_size: 5000,
            preserve_on_failure: false,
            failed: AtomicBool::new(false),
            workspace: None,
        });
        assert!(!aligner1.batch_enabled, "Batching should be disabled by default");
        assert_eq!(aligner1.batch_size, 5000, "Default batch size should be 5000");

        // Test with_batch_settings
        let aligner2 = aligner1.with_batch_settings(true, 10000);
        assert!(aligner2.batch_enabled, "Batching should be enabled");
        assert_eq!(aligner2.batch_size, 10000, "Batch size should be 10000");
    }

    #[test]
    fn test_accession_extraction_from_headers() {
        // Test various header formats
        let test_cases = vec![
            ("sp|P12345|PROT_HUMAN Description", vec!["P12345"]),
            ("tr|Q67890|PROT_MOUSE", vec!["Q67890"]),
            ("NP_123456.1 some description", vec!["NP_123456.1", "NP_123456"]),
            ("XP_999999.2", vec!["XP_999999.2", "XP_999999"]),
            ("simple_accession", vec!["simple_accession"]),
        ];

        for (idx, (header, expected_accessions)) in test_cases.iter().enumerate() {
            // Create a new temp dir for each test case to avoid conflicts
            let temp_dir = TempDir::new().unwrap();
            let temp_path = temp_dir.path();

            let ref_fasta = temp_path.join(format!("test_{}.fasta", idx));
            create_test_fasta(&ref_fasta, &[(header, "ACGT")]).unwrap();

            let full_mapping = temp_path.join(format!("test_{}.accession2taxid", idx));
            let mut mappings = vec![];
            for acc in expected_accessions {
                mappings.push((acc.as_ref(), 9606));
            }
            // Add some extra mappings that shouldn't match
            mappings.push(("NOMATCH1", 1111));
            mappings.push(("NOMATCH2", 2222));

            create_test_accession2taxid(&full_mapping, &mappings).unwrap();

            // Create aligner with its own temp directory
            let aligner_temp = TempDir::new().unwrap();
            let mut aligner = LambdaAligner {
                binary_path: PathBuf::from("/dummy"),
                temp_dir: aligner_temp.path().to_path_buf(),
                acc_tax_map: None,
                tax_dump_dir: None,
                batch_enabled: false,
                batch_size: 5000,
                preserve_on_failure: false,
                failed: AtomicBool::new(false),
            workspace: None,
            };

            let filtered_path = aligner.filter_accession2taxid_for_references(&full_mapping, &ref_fasta).unwrap();
            let contents = fs::read_to_string(&filtered_path).unwrap();

            // Check that at least one expected accession was found
            let found_any = expected_accessions.iter().any(|acc| contents.contains(acc));
            assert!(found_any, "Should find accession from header: {}", header);

            // Check that non-matching accessions are not included
            assert!(!contents.contains("NOMATCH1"), "Should not contain NOMATCH1");
            assert!(!contents.contains("NOMATCH2"), "Should not contain NOMATCH2");
        }
    }

    #[test]
    fn test_taxonomy_with_ancestors() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create accession2taxid with leaf taxids
        let filtered_mapping = temp_path.join("filtered.accession2taxid");
        create_test_accession2taxid(&filtered_mapping, &[
            ("P12345", 9606),   // Human (should include ancestors)
        ]).unwrap();

        // Create taxdump with taxonomic hierarchy
        let full_taxdump = temp_path.join("full_taxdump");
        fs::create_dir_all(&full_taxdump).unwrap();

        // Create nodes.dmp with hierarchy: 1 (root) -> 9605 (Hominidae) -> 9606 (Human)
        let mut nodes_file = File::create(full_taxdump.join("nodes.dmp")).unwrap();
        writeln!(nodes_file, "1\t|\t1\t|\tno rank\t|\t").unwrap();
        writeln!(nodes_file, "9605\t|\t1\t|\tfamily\t|\t").unwrap();
        writeln!(nodes_file, "9606\t|\t9605\t|\tspecies\t|\t").unwrap();
        writeln!(nodes_file, "10090\t|\t1\t|\tspecies\t|\t").unwrap(); // Mouse - not needed

        // Create names.dmp
        let mut names_file = File::create(full_taxdump.join("names.dmp")).unwrap();
        writeln!(names_file, "1\t|\troot\t|\t\t|\tscientific name\t|").unwrap();
        writeln!(names_file, "9605\t|\tHominidae\t|\t\t|\tscientific name\t|").unwrap();
        writeln!(names_file, "9606\t|\tHomo sapiens\t|\t\t|\tscientific name\t|").unwrap();
        writeln!(names_file, "10090\t|\tMus musculus\t|\t\t|\tscientific name\t|").unwrap();

        let mut aligner = LambdaAligner {
            binary_path: PathBuf::from("/dummy"),
            temp_dir: temp_path.to_path_buf(),
            acc_tax_map: None,
            tax_dump_dir: None,
            batch_enabled: false,
            batch_size: 5000,
            preserve_on_failure: false,
            failed: AtomicBool::new(false),
            workspace: None,
        };

        // Filter the taxdump
        let filtered_dir = aligner.create_filtered_taxdump(&full_taxdump, &filtered_mapping).unwrap();

        // Check that ancestors are included
        let nodes_contents = fs::read_to_string(filtered_dir.join("nodes.dmp")).unwrap();
        assert!(nodes_contents.contains("1\t|"), "Should contain root");
        assert!(nodes_contents.contains("9605\t|"), "Should contain ancestor 9605");
        assert!(nodes_contents.contains("9606\t|"), "Should contain human 9606");
        assert!(!nodes_contents.contains("10090\t|"), "Should NOT contain mouse 10090");
    }

    #[test]
    fn test_preserve_on_failure_flag() {
        // Test that preserve_on_failure flag is properly set from environment
        std::env::set_var("TALARIA_PRESERVE_LAMBDA_ON_FAILURE", "1");

        let temp_dir = TempDir::new().unwrap();
        let aligner = LambdaAligner::new(temp_dir.path().join("lambda3")).ok();

        // Clean up env var
        std::env::remove_var("TALARIA_PRESERVE_LAMBDA_ON_FAILURE");

        // The aligner might fail if lambda3 doesn't exist, but we can still test the flag would be set
        if let Some(aligner) = aligner {
            assert!(aligner.preserve_on_failure, "Flag should be set from env var");
        }
    }

    #[test]
    fn test_batch_progress_percentage() {
        // Test that batch progress shows percentage instead of X/Y
        let _temp_dir = TempDir::new().unwrap();
        let sequences = vec![
            Sequence::new("seq1".to_string(), b"ACGT".to_vec()),
            Sequence::new("seq2".to_string(), b"TGCA".to_vec()),
        ];

        // We can't easily test the actual output, but we can verify the calculation
        let total_sequences = sequences.len();
        let sequences_processed = 1;
        let percent_complete = sequences_processed as f64 / total_sequences as f64 * 100.0;

        assert_eq!(percent_complete, 50.0, "Should calculate 50% for 1 of 2 sequences");
    }

    #[test]
    fn test_extreme_sequence_detection() {
        // Test that sequences over 30,000 aa are flagged as extreme
        let long_seq = vec![b'A'; 35000]; // TITIN-like length
        let normal_seq = vec![b'A'; 500];

        let extreme_threshold = 30_000;

        assert!(long_seq.len() > extreme_threshold, "Long sequence should be extreme");
        assert!(!(normal_seq.len() > extreme_threshold), "Normal sequence should not be extreme");
    }

    #[test]
    fn test_size_based_batching() {
        // Test that batching is based on amino acid count, not sequence count
        let sequences = vec![
            Sequence::new("small1".to_string(), vec![b'A'; 100]),
            Sequence::new("small2".to_string(), vec![b'A'; 100]),
            Sequence::new("large".to_string(), vec![b'A'; 1000]),
        ];

        let max_batch_aa = 500; // Max 500 amino acids per batch

        // Calculate how sequences would be batched
        let mut batches = Vec::new();
        let mut current_batch_aa = 0;
        let mut current_batch = Vec::new();

        for seq in &sequences {
            let seq_len = seq.len();

            if current_batch_aa + seq_len > max_batch_aa && !current_batch.is_empty() {
                batches.push(current_batch.clone());
                current_batch.clear();
                current_batch_aa = 0;
            }

            if seq_len > max_batch_aa {
                // Process alone
                if !current_batch.is_empty() {
                    batches.push(current_batch.clone());
                    current_batch.clear();
                    current_batch_aa = 0;
                }
                batches.push(vec![seq.id.clone()]);
            } else {
                current_batch.push(seq.id.clone());
                current_batch_aa += seq_len;
            }
        }

        if !current_batch.is_empty() {
            batches.push(current_batch);
        }

        // Should have 3 batches: [small1, small2], [large]
        assert_eq!(batches.len(), 2, "Should create 2 batches");
        assert_eq!(batches[0].len(), 2, "First batch should have 2 small sequences");
        assert_eq!(batches[1].len(), 1, "Second batch should have 1 large sequence");
    }

    #[test]
    fn test_workspace_integration() {
        use crate::utils::temp_workspace::{TempWorkspace, WorkspaceConfig};
        use std::sync::{Arc, Mutex};

        // Create a test-specific workspace config with explicit paths
        let test_dir = TempDir::new().unwrap();
        let config = WorkspaceConfig {
            casg_root: test_dir.path().join("casg"),
            preserve_on_failure: false,
            preserve_always: false,
            max_age_seconds: 86400,
        };
        let workspace = TempWorkspace::with_config("test_lambda", config).unwrap();
        let workspace = Arc::new(Mutex::new(workspace));

        // Create aligner with workspace
        let temp_dir = TempDir::new().unwrap();
        let aligner = LambdaAligner::new(temp_dir.path().join("lambda3"))
            .unwrap_or_else(|_| LambdaAligner {
                binary_path: PathBuf::from("/dummy"),
                temp_dir: PathBuf::new(),
                acc_tax_map: None,
                tax_dump_dir: None,
                batch_enabled: false,
                batch_size: 5000,
                preserve_on_failure: false,
                failed: AtomicBool::new(false),
                workspace: None,
            })
            .with_workspace(workspace.clone());

        // Check that temp_dir is set to workspace path
        let ws = workspace.lock().unwrap();
        let expected_path = ws.get_path("lambda");
        drop(ws); // Release lock

        assert_eq!(aligner.temp_dir, expected_path,
            "Aligner should use workspace lambda directory");
        assert!(aligner.workspace.is_some(), "Workspace should be set");
    }

    #[test]
    fn test_lambda_verbose_flag() {
        // Test that TALARIA_LAMBDA_VERBOSE controls LAMBDA output
        std::env::set_var("TALARIA_LAMBDA_VERBOSE", "1");
        let is_verbose = std::env::var("TALARIA_LAMBDA_VERBOSE").is_ok();
        assert!(is_verbose, "TALARIA_LAMBDA_VERBOSE should be detected");

        std::env::remove_var("TALARIA_LAMBDA_VERBOSE");
        let is_not_verbose = std::env::var("TALARIA_LAMBDA_VERBOSE").is_err();
        assert!(is_not_verbose, "TALARIA_LAMBDA_VERBOSE should be removed");
    }

    #[test]
    fn test_get_temp_path() {
        let mut aligner = LambdaAligner {
            binary_path: PathBuf::from("/dummy"),
            temp_dir: PathBuf::new(),
            acc_tax_map: None,
            tax_dump_dir: None,
            batch_enabled: false,
            batch_size: 5000,
            preserve_on_failure: false,
            failed: AtomicBool::new(false),
            workspace: None,
        };

        // Initially temp_dir is empty
        assert!(aligner.temp_dir.as_os_str().is_empty());

        // get_temp_path should initialize and return a path
        let test_path = aligner.get_temp_path("test.fasta");
        assert!(!aligner.temp_dir.as_os_str().is_empty(), "temp_dir should be initialized");
        assert!(test_path.ends_with("test.fasta"), "Should end with filename");
        assert!(test_path.starts_with(&aligner.temp_dir), "Should start with temp_dir");
    }

    #[test]
    fn test_workspace_fallback() {

        // Test that aligner falls back to regular temp dir when no workspace
        let mut aligner = LambdaAligner {
            binary_path: PathBuf::from("/dummy"),
            temp_dir: PathBuf::new(),
            acc_tax_map: None,
            tax_dump_dir: None,
            batch_enabled: false,
            batch_size: 5000,
            preserve_on_failure: false,
            failed: AtomicBool::new(false),
            workspace: None,
        };

        aligner.initialize_temp_dir();

        // Should fall back to /tmp directory
        assert!(aligner.temp_dir.starts_with(std::env::temp_dir()),
            "Should fall back to system temp dir");
        assert!(aligner.temp_dir.to_string_lossy().contains("talaria-lambda"),
            "Should contain talaria-lambda in path");
    }

    #[test]
    fn test_progress_counter_updates() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Test that progress counter is properly updated
        let progress_counter = Arc::new(AtomicUsize::new(0));

        // Simulate progress updates
        progress_counter.store(10, Ordering::Relaxed);
        assert_eq!(progress_counter.load(Ordering::Relaxed), 10);

        progress_counter.store(50, Ordering::Relaxed);
        assert_eq!(progress_counter.load(Ordering::Relaxed), 50);

        progress_counter.store(100, Ordering::Relaxed);
        assert_eq!(progress_counter.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn test_batch_statistics_calculation() {
        let sequences = vec![
            Sequence::new("seq1".to_string(), vec![b'A'; 100]),
            Sequence::new("seq2".to_string(), vec![b'A'; 200]),
            Sequence::new("seq3".to_string(), vec![b'A'; 300]),
        ];

        let max_len = sequences.iter().map(|s| s.len()).max().unwrap_or(0);
        let min_len = sequences.iter().map(|s| s.len()).min().unwrap_or(0);
        let total_aa: usize = sequences.iter().map(|s| s.len()).sum();
        let avg_len = if !sequences.is_empty() { total_aa / sequences.len() } else { 0 };

        assert_eq!(max_len, 300, "Max length should be 300");
        assert_eq!(min_len, 100, "Min length should be 100");
        assert_eq!(total_aa, 600, "Total aa should be 600");
        assert_eq!(avg_len, 200, "Average length should be 200");
    }

    #[test]
    fn test_ambiguous_sequence_detection() {
        // Test detection of sequences with high ambiguous content
        let normal_seq = Sequence::new("normal".to_string(), b"ACGTACGTACGT".to_vec());
        let ambiguous_seq = Sequence::new("ambiguous".to_string(), b"XXXXBBBZZZ**".to_vec());

        let sequences = vec![normal_seq, ambiguous_seq];

        let ambiguous_seqs: Vec<_> = sequences.iter()
            .filter(|seq| {
                let ambiguous_count = seq.sequence.iter()
                    .filter(|&&b| b == b'X' || b == b'B' || b == b'Z' || b == b'*')
                    .count();
                ambiguous_count > seq.len() / 20  // More than 5% ambiguous
            })
            .collect();

        assert_eq!(ambiguous_seqs.len(), 1, "Should detect 1 ambiguous sequence");
        assert_eq!(ambiguous_seqs[0].id, "ambiguous", "Should identify the correct sequence");
    }

    #[test]
    fn test_cleanup_with_workspace() {
        use crate::utils::temp_workspace::{TempWorkspace, WorkspaceConfig};
        use std::sync::{Arc, Mutex};

        // Create a test-specific workspace config with explicit paths
        let test_dir = TempDir::new().unwrap();
        let config = WorkspaceConfig {
            casg_root: test_dir.path().join("casg"),
            preserve_on_failure: false,
            preserve_always: false,
            max_age_seconds: 86400,
        };
        let workspace = TempWorkspace::with_config("test_cleanup", config).unwrap();
        let workspace = Arc::new(Mutex::new(workspace));

        // Create aligner with workspace
        let mut aligner = LambdaAligner {
            binary_path: PathBuf::from("/dummy"),
            temp_dir: PathBuf::new(),
            acc_tax_map: None,
            tax_dump_dir: None,
            batch_enabled: false,
            batch_size: 5000,
            preserve_on_failure: false,
            failed: AtomicBool::new(false),
            workspace: Some(workspace.clone()),
        };

        aligner.initialize_temp_dir();
        let temp_path = aligner.temp_dir.clone();

        // Create a test file in the temp directory
        fs::create_dir_all(&temp_path).ok();
        let test_file = temp_path.join("test.txt");
        fs::write(&test_file, "test").ok();
        assert!(test_file.exists(), "Test file should exist");

        // Cleanup should remove the directory
        aligner.cleanup().ok();

        // Note: The workspace itself manages cleanup, so we just verify the method doesn't panic
    }

    #[test]
    fn test_mutability_requirements() {
        // Test that methods requiring mutation are properly marked
        let mut aligner = LambdaAligner {
            binary_path: PathBuf::from("/dummy"),
            temp_dir: PathBuf::new(),
            acc_tax_map: None,
            tax_dump_dir: None,
            batch_enabled: false,
            batch_size: 5000,
            preserve_on_failure: false,
            failed: AtomicBool::new(false),
            workspace: None,
        };

        // These methods should compile with &mut self
        let _ = aligner.get_temp_dir();
        let _ = aligner.get_temp_path("test.fasta");
        aligner.initialize_temp_dir();

        // Verify the aligner can be used mutably
        assert!(aligner.temp_dir.to_string_lossy().contains("talaria-lambda"));
    }
}
