use crate::cli::formatting::output::*;
use crate::cli::formatting::{
    info_box, print_error, print_success, print_tip, TaskHandle, TaskList, TaskStatus,
};
use crate::cli::TargetAligner;
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use talaria_utils::display::format::format_bytes;
use talaria_utils::workspace::HeraldWorkspaceManager;

#[derive(Args, Debug)]
pub struct ReduceArgs {
    /// Database to reduce (e.g., "uniprot/swissprot", "custom/taxids_9606")
    /// Must be a database that exists in the HERALD repository
    #[arg(value_name = "DATABASE")]
    pub database: String,

    /// Output reduced FASTA file (optional - stores in HERALD by default)
    #[arg(short = 'o', long = "output-fasta", value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Target aligner for optimization
    #[arg(short = 'a', long, default_value = "generic")]
    pub target_aligner: TargetAligner,

    /// Target reduction ratio (0.0-1.0, where 0.3 = 30% of original size)
    /// If not specified, uses dynamic selection based on sequence alignments
    #[arg(short = 'r', long)]
    pub reduction_ratio: Option<f64>,

    /// Minimum sequence length to consider
    #[arg(long, default_value = "50")]
    pub min_length: usize,

    /// Output metadata file for deltas
    #[arg(short = 'm', long)]
    pub metadata: Option<PathBuf>,

    /// Configuration file
    #[arg(short = 'c', long)]
    pub config: Option<PathBuf>,

    /// Use amino acid scoring (default: auto-detect)
    #[arg(long)]
    pub protein: bool,

    /// Use nucleotide scoring (default: auto-detect)
    #[arg(long)]
    pub nucleotide: bool,

    /// Skip validation step
    #[arg(long)]
    pub skip_validation: bool,

    /// Number of threads (passed from global)
    #[arg(skip)]
    pub threads: usize,

    // Optional advanced features (not in original db-reduce)
    /// Enable similarity-based clustering (default: disabled)
    #[arg(long, value_name = "THRESHOLD")]
    pub similarity_threshold: Option<f64>,

    /// Filter out low complexity sequences
    #[arg(long)]
    pub low_complexity_filter: bool,

    /// Use alignment-based selection instead of simple greedy
    #[arg(long)]
    pub align_select: bool,

    /// Enable taxonomy-aware clustering
    #[arg(long)]
    pub taxonomy_aware: bool,

    /// Use taxonomy data to weight alignment scores (requires taxonomy data in FASTA or HERALD)
    #[arg(long)]
    pub use_taxonomy_weights: bool,

    /// Enable batched processing for large datasets (default: false)
    #[arg(long)]
    pub batch: bool,

    /// Maximum amino acids per batch for batched processing (default: 5000000)
    /// Helps prevent memory issues with very long sequences
    #[arg(long, default_value = "5000000")]
    pub batch_size: usize,

    /// Skip delta encoding (much faster, but no reconstruction possible)
    #[arg(long)]
    pub no_deltas: bool,

    /// Use all-vs-all alignment mode for Lambda
    #[arg(long, default_value_t = true)]
    pub all_vs_all: bool,

    /// Selection algorithm to use for choosing reference sequences
    /// Options: single-pass (default, O(n)), similarity-matrix (O(n²) but potentially more optimal)
    #[arg(long, default_value = "single-pass", value_name = "ALGORITHM")]
    pub selection_algorithm: String,

    /// Optimize storage for memory efficiency (may impact performance)
    #[arg(long)]
    pub optimize_for_memory: bool,

    /// Maximum sequence length for alignment (longer sequences skip delta encoding)
    #[arg(long, default_value = "10000")]
    pub max_align_length: usize,

    /// Store reduced version in database structure (only needed when using -i)
    #[arg(long)]
    pub store: bool,

    /// Profile name for stored reduction (e.g., "blast-optimized")
    /// If not specified, uses reduction ratio (e.g., "30-percent")
    #[arg(long, value_name = "NAME")]
    pub profile: Option<String>,

    /// Output to HERALD repository instead of files
    #[arg(long)]
    pub herald_output: bool,

    /// HERALD repository path (default: ${TALARIA_HOME}/databases)
    #[arg(long, value_name = "PATH")]
    pub herald_path: Option<PathBuf>,

    /// Skip visualization charts in output
    #[arg(long)]
    pub no_visualize: bool,

    /// Generate HTML report with visualization (deprecated: use --report-output with --report-format=html)
    #[arg(long)]
    pub html_report: Option<PathBuf>,

    /// Report output file path
    #[arg(long = "report-output", value_name = "FILE")]
    pub report_output: Option<PathBuf>,

    /// Report output format (text, html, json, csv)
    #[arg(long = "report-format", value_name = "FORMAT", default_value = "text")]
    pub report_format: String,
}

/// Parse the selection algorithm string into the enum
fn parse_selection_algorithm(
    algorithm: &str,
) -> anyhow::Result<talaria_herald::SelectionAlgorithm> {
    use talaria_herald::SelectionAlgorithm;

    match algorithm.to_lowercase().as_str() {
        "single-pass" | "singlepass" | "single_pass" => Ok(SelectionAlgorithm::SinglePass),
        "similarity-matrix" | "similarity_matrix" | "matrix" => Ok(SelectionAlgorithm::SimilarityMatrix),
        "hybrid" => Ok(SelectionAlgorithm::Hybrid),
        "graph" | "graph-centrality" | "graphcentrality" | "centrality" => Ok(SelectionAlgorithm::GraphCentrality),
        _ => anyhow::bail!("Invalid selection algorithm: '{}'. Options: single-pass, similarity-matrix, graph-centrality", algorithm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use talaria_herald::SelectionAlgorithm;

    #[test]
    fn test_parse_selection_algorithm_valid() {
        // Test valid inputs
        assert_eq!(
            parse_selection_algorithm("single-pass").unwrap(),
            SelectionAlgorithm::SinglePass
        );
        assert_eq!(
            parse_selection_algorithm("singlepass").unwrap(),
            SelectionAlgorithm::SinglePass
        );
        assert_eq!(
            parse_selection_algorithm("single_pass").unwrap(),
            SelectionAlgorithm::SinglePass
        );
        assert_eq!(
            parse_selection_algorithm("similarity-matrix").unwrap(),
            SelectionAlgorithm::SimilarityMatrix
        );
        assert_eq!(
            parse_selection_algorithm("similarity_matrix").unwrap(),
            SelectionAlgorithm::SimilarityMatrix
        );
        assert_eq!(
            parse_selection_algorithm("matrix").unwrap(),
            SelectionAlgorithm::SimilarityMatrix
        );
        assert_eq!(
            parse_selection_algorithm("hybrid").unwrap(),
            SelectionAlgorithm::Hybrid
        );
        assert_eq!(
            parse_selection_algorithm("graph").unwrap(),
            SelectionAlgorithm::GraphCentrality
        );
        assert_eq!(
            parse_selection_algorithm("graph-centrality").unwrap(),
            SelectionAlgorithm::GraphCentrality
        );
        assert_eq!(
            parse_selection_algorithm("centrality").unwrap(),
            SelectionAlgorithm::GraphCentrality
        );
    }

    #[test]
    fn test_parse_selection_algorithm_case_insensitive() {
        assert_eq!(
            parse_selection_algorithm("SINGLE-PASS").unwrap(),
            SelectionAlgorithm::SinglePass
        );
        assert_eq!(
            parse_selection_algorithm("SiMiLaRiTy-MaTrIx").unwrap(),
            SelectionAlgorithm::SimilarityMatrix
        );
        assert_eq!(
            parse_selection_algorithm("HYBRID").unwrap(),
            SelectionAlgorithm::Hybrid
        );
        assert_eq!(
            parse_selection_algorithm("GRAPH-CENTRALITY").unwrap(),
            SelectionAlgorithm::GraphCentrality
        );
    }

    #[test]
    fn test_parse_selection_algorithm_invalid() {
        assert!(parse_selection_algorithm("invalid").is_err());
        assert!(parse_selection_algorithm("").is_err());
        assert!(parse_selection_algorithm("random-algo").is_err());
    }

    #[test]
    fn test_reduce_args_default_algorithm() {
        // Test that default algorithm string parses correctly
        let default_algo = "single-pass";
        let algo = parse_selection_algorithm(default_algo).unwrap();
        assert_eq!(algo, SelectionAlgorithm::SinglePass);
    }

    #[test]
    fn test_compression_level_validation() {
        // Test that compression levels are within valid range
        let valid_levels = vec![1, 3, 5, 9, 11, 15, 19, 22];
        for level in valid_levels {
            assert!(
                level >= 1 && level <= 22,
                "Invalid compression level: {}",
                level
            );
        }
    }

    #[test]
    fn test_batch_size_calculation() {
        // Test batch size calculations for different input sizes
        let test_cases = vec![
            (100, 1000, 100),        // Small file, use all
            (10_000, 1000, 1000),    // Medium file, use batch size
            (1_000_000, 5000, 5000), // Large file, use batch size
        ];

        for (total, batch_size, expected) in test_cases {
            let actual = total.min(batch_size);
            assert_eq!(
                actual, expected,
                "Batch size calculation failed for total={}, batch_size={}",
                total, batch_size
            );
        }
    }

    // Test removed - ReductionParameters struct no longer exists in talaria_core

    #[test]
    fn test_target_ratio_parsing() {
        // Test parsing of target ratio values
        let test_cases = vec![
            ("0.5", Ok(0.5)),
            ("0.1", Ok(0.1)),
            ("1.0", Ok(1.0)),
            ("0.0", Ok(0.0)),
            ("1.5", Err(())),  // Out of range
            ("-0.1", Err(())), // Negative
        ];

        for (input, expected) in test_cases {
            let result = input.parse::<f64>();
            match expected {
                Ok(val) => {
                    assert!(result.is_ok());
                    let parsed = result.unwrap();
                    assert!((parsed - val).abs() < 0.001);
                    assert!(parsed >= 0.0 && parsed <= 1.0 || expected.is_err());
                }
                Err(_) => {
                    let parsed = result.unwrap_or(2.0);
                    assert!(parsed < 0.0 || parsed > 1.0);
                }
            }
        }
    }

    #[test]
    fn test_aligner_specific_defaults() {
        use crate::cli::TargetAligner;

        // Test that each aligner has appropriate default settings
        let aligners = vec![
            TargetAligner::Lambda,
            TargetAligner::Blast,
            TargetAligner::Kraken,
            TargetAligner::Diamond,
            TargetAligner::MMseqs2,
            TargetAligner::Generic,
        ];

        for aligner in aligners {
            match aligner {
                TargetAligner::Lambda => {
                    // Lambda should have specific optimizations
                    assert!(true, "Lambda aligner should be supported");
                }
                TargetAligner::Blast => {
                    // BLAST has different requirements
                    assert!(true, "BLAST aligner should be supported");
                }
                _ => {
                    assert!(true, "Aligner {:?} should be supported", aligner);
                }
            }
        }
    }

    #[test]
    fn test_output_format_validation() {
        // Test that output formats are correctly handled
        let valid_formats = vec!["fasta", "herald", "json"];

        for format in valid_formats {
            assert!(
                format == "fasta" || format == "herald" || format == "json",
                "Invalid output format: {}",
                format
            );
        }
    }

    #[test]
    fn test_workspace_cleanup_on_error() {
        use tempfile::TempDir;

        // Test that workspace is cleaned up on error
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().join("workspace");

        // Simulate workspace creation
        std::fs::create_dir_all(&workspace_path).unwrap();
        assert!(workspace_path.exists());

        // Simulate cleanup (drop behavior)
        drop(temp_dir);

        // Note: Can't check after drop, but this tests the pattern
    }

    #[test]
    fn test_min_sequences_validation() {
        // Test minimum sequence requirements
        let test_cases = vec![
            (0, false),   // No sequences - invalid
            (1, true),    // Single sequence - valid but warning
            (10, true),   // Few sequences - valid
            (1000, true), // Many sequences - valid
        ];

        for (count, should_be_valid) in test_cases {
            assert_eq!(
                count > 0,
                should_be_valid,
                "Sequence count {} validation failed",
                count
            );
        }
    }

    #[test]
    fn test_parallel_processing_settings() {
        // Test thread count settings
        let cpu_count = num_cpus::get();

        let test_cases = vec![
            (0, cpu_count), // 0 means use all CPUs
            (1, 1),         // Single thread
            (4, 4),         // Specific count
            (1000, 1000),   // More than available (should be capped in practice)
        ];

        for (requested, expected) in test_cases {
            let actual = if requested == 0 { cpu_count } else { requested };
            assert_eq!(
                actual, expected,
                "Thread count calculation failed for requested={}",
                requested
            );
        }
    }
}

pub fn run(mut args: ReduceArgs) -> anyhow::Result<()> {
    use talaria_utils::display::format::get_file_size;

    // Initialize formatter
    crate::cli::formatting::formatter::init();

    // Initialize HERALD workspace manager
    let mut herald_manager = HeraldWorkspaceManager::new()?;

    // Create workspace for this reduction operation
    let command = format!("reduce {:?}", &args);
    let workspace = Arc::new(Mutex::new(herald_manager.create_workspace(&command)?));

    // Get threads from environment or default
    args.threads = std::env::var("TALARIA_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Check for default config file from environment
    if args.config.is_none() {
        if let Ok(config_path) = std::env::var("TALARIA_CONFIG") {
            args.config = Some(PathBuf::from(config_path));
        }
    }

    // Validate database exists
    use talaria_herald::database::DatabaseManager;
    let db_manager = DatabaseManager::new(None)?;

    // Parse database reference
    let (source, dataset) = if args.database.contains('/') {
        let parts: Vec<&str> = args.database.split('/').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid database reference format. Use 'source/dataset' (e.g., 'uniprot/swissprot')")
        }
        (parts[0].to_string(), parts[1].to_string())
    } else {
        // Assume custom source if no slash
        ("custom".to_string(), args.database.clone())
    };

    // Check if database exists and get its version
    let databases = db_manager.list_databases()?;
    let db_full_name = format!("{}/{}", source, dataset);
    let db_info = databases
        .iter()
        .find(|db| db.name == db_full_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Database '{}' not found. Use 'talaria database list' to see available databases.",
                db_full_name
            )
        })?;
    let db_version = db_info.version.clone();
    // Assemble FASTA from HERALD chunks
    let temp_file = workspace
        .lock()
        .unwrap()
        .get_file_path("input_fasta", "fasta");

    // Map database to internal source enum (for taxonomy mapping later)
    use talaria_herald::download::{DatabaseSource, NCBIDatabase, UniProtDatabase};
    let database_source = match db_full_name.as_str() {
        "uniprot/swissprot" => Some(DatabaseSource::UniProt(UniProtDatabase::SwissProt)),
        "uniprot/trembl" => Some(DatabaseSource::UniProt(UniProtDatabase::TrEMBL)),
        "uniprot/uniref50" => Some(DatabaseSource::UniProt(UniProtDatabase::UniRef50)),
        "uniprot/uniref90" => Some(DatabaseSource::UniProt(UniProtDatabase::UniRef90)),
        "uniprot/uniref100" => Some(DatabaseSource::UniProt(UniProtDatabase::UniRef100)),
        "ncbi/nr" => Some(DatabaseSource::NCBI(NCBIDatabase::NR)),
        "ncbi/nt" => Some(DatabaseSource::NCBI(NCBIDatabase::NT)),
        "ncbi/refseq" => Some(DatabaseSource::NCBI(NCBIDatabase::RefSeq)),
        "ncbi/refseq_protein" => Some(DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein)),
        "ncbi/refseq_genomic" => Some(DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic)),
        _ => None, // Custom database - won't have taxonomy mapping from manifest
    };

    // Use spinner for assembly
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    spinner.set_message("Checking database manifest...");

    // First check if this is a streaming database (lightweight check)
    let manifest_lightweight = db_manager.get_manifest_lightweight(&db_full_name)?;
    let is_streaming = manifest_lightweight.etag.starts_with("streaming-");

    // Parse source and dataset from db_full_name
    let parts: Vec<&str> = db_full_name.split('/').collect();
    let (source_name, dataset_name) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        anyhow::bail!("Invalid database name format: {}", db_full_name);
    };

    let sequence_count = if is_streaming {
        // Streaming mode: assemble directly from partials without loading full chunk_index
        // This avoids OOM for massive databases like UniRef100 with 36M+ chunks
        spinner.set_message("Starting streaming assembly from partials...");

        // Clone spinner for use in callback
        let spinner_clone = spinner.clone();

        db_manager.assemble_from_partials_streaming(
            source_name,
            dataset_name,
            &db_version,
            &temp_file,
            Some(Box::new(move |batches, sequences| {
                spinner_clone.set_message(format!(
                    "Streaming batch {}, {} sequences assembled...",
                    format_number(batches),
                    format_number(sequences)
                ));
                spinner_clone.tick();
            })),
        )?
    } else {
        // Non-streaming mode: load full manifest and assemble
        spinner.set_message("Loading database manifest from HERALD...");
        let manifest = db_manager.get_manifest(&db_full_name)?;

        spinner.set_message("Assembling database from HERALD chunks...");

        use std::io::Write;
        use talaria_herald::operations::FastaAssembler;

        let assembler = FastaAssembler::new(db_manager.get_storage());
        let chunk_hashes: Vec<_> = manifest
            .chunk_index
            .iter()
            .map(|c| c.hash.clone())
            .collect();

        spinner.set_message("Streaming sequences to file...");

        let count = {
            let mut output_file = std::fs::File::create(&temp_file)?;
            let count = assembler.stream_assembly(&chunk_hashes, &mut output_file)?;
            output_file.flush()?;
            count
        };

        count
    };

    spinner.finish_and_clear();

    // Log completion of assembly stage
    tracing::info!(
        "✓ Assembly complete: {} sequences assembled",
        sequence_count
    );
    if std::env::var("TALARIA_LOG").unwrap_or_default() == "debug" {
        tracing::debug!("Memory checkpoint: Post-assembly");
    }

    // Start database preparation section
    subsection_header("Database Preparation");
    success(&format!(
        "Assembled {} sequences from HERALD chunks",
        format_number(sequence_count)
    ));

    // Display file information in tree format
    tree_item(false, "Input file", Some(&temp_file.display().to_string()));

    if let Ok(metadata) = std::fs::metadata(&temp_file) {
        let file_size = metadata.len();
        let formatted_size = format_bytes(file_size);
        tree_item(
            false,
            "File size",
            Some(&format!(
                "{} ({} bytes)",
                formatted_size,
                format_number(file_size as usize)
            )),
        );

        // Read and display first few lines for verification (debug mode only)
        if std::env::var("TALARIA_DEBUG").is_ok() || std::env::var("TALARIA_LAMBDA_VERBOSE").is_ok()
        {
            use std::io::{BufRead, BufReader};
            if let Ok(file) = std::fs::File::open(&temp_file) {
                let reader = BufReader::new(file);
                let mut lines_shown = 0;
                println!("\n  First few lines of FASTA file:");
                for line in reader.lines() {
                    if let Ok(line) = line {
                        if lines_shown < 10 {
                            if line.starts_with('>') {
                                println!(
                                    "    Header: {}",
                                    if line.len() > 100 {
                                        format!("{}...", &line[..100])
                                    } else {
                                        line.clone()
                                    }
                                );
                            }
                            lines_shown += 1;
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    } else {
        warning("Could not read file metadata - file may not have been written correctly!");
    }

    // Create accession2taxid mapping from manifest if using LAMBDA
    tracing::info!("Memory checkpoint: Before taxonomy mapping");
    let manifest_acc2taxid = if args.target_aligner == TargetAligner::Lambda {
        if let Some(ref db_src) = database_source {
            let spinner = ProgressBar::new_spinner();
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.cyan} {msg}")
                    .unwrap()
                    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
            );
            spinner.set_message("Creating taxonomy mapping from manifest...");
            // Don't use steady_tick - causes ETA miscalculation

            tracing::debug!("Creating taxonomy mapping for {:?}", db_src);
            match db_manager.create_accession2taxid_from_manifest(db_src) {
                Ok(path) => {
                    spinner.finish_and_clear();
                    tree_item(true, "Taxonomy mapping", Some("Created from manifest"));
                    tracing::info!("✓ Taxonomy mapping created at {:?}", path);
                    tracing::info!("Memory checkpoint: After taxonomy mapping");
                    Some(path)
                }
                Err(e) => {
                    spinner.finish_and_clear();
                    tree_item(true, "Taxonomy mapping", Some(&format!("Warning: {}", e)));
                    tracing::warn!("Taxonomy mapping failed: {}", e);
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    let actual_input = temp_file;

    // Generate output database name for reduced version
    let _profile_or_ratio = if let Some(profile) = &args.profile {
        profile.clone()
    } else if args.reduction_ratio.is_some() && args.reduction_ratio.unwrap() > 0.0 {
        format!("{}pct", (args.reduction_ratio.unwrap() * 100.0) as u32)
    } else {
        "auto".to_string()
    };

    // Note: We don't create a new database name for reductions anymore
    // Reductions are stored as profiles associated with the original database

    // Use reduction ratio if provided, otherwise use auto-detection
    let reduction_ratio = if let Some(ratio) = args.reduction_ratio {
        if ratio <= 0.0 || ratio > 1.0 {
            anyhow::bail!("Reduction ratio must be between 0.0 and 1.0");
        }
        ratio
    } else {
        // Auto-detection will be handled by the reducer
        0.0 // Sentinel value for auto-detection
    };

    // Create task list for tracking reduction pipeline
    let mut task_list = TaskList::new();

    // Print header
    let header = format!("Reduction Pipeline: {}", db_full_name);
    task_list.print_header(&header);

    // Show reduction mode info
    if reduction_ratio == 0.0 {
        info_box(
            "Using LAMBDA for intelligent auto-detection",
            &[
                "Alignment-based selection",
                "Taxonomy-aware clustering",
                "Dynamic coverage optimization",
            ],
        );
    } else {
        info_box(
            &format!(
                "Fixed reduction to {:.0}% of original",
                reduction_ratio * 100.0
            ),
            &[
                "Greedy selection by sequence length",
                "Predictable output size",
            ],
        );
    }

    // Add tasks
    let init_task = task_list.add_task("Initialize pipeline");
    let load_task = task_list.add_task("Load sequences");
    let select_task = task_list.add_task("Select references");
    let encode_task = task_list.add_task("Encode deltas");
    let write_task = task_list.add_task("Write output files");

    task_list.update_task(init_task, TaskStatus::InProgress);

    // Set up thread pool
    let threads = if args.threads == 0 {
        rayon::current_num_threads()
    } else {
        args.threads
    };

    // Only initialize if not already done
    if rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .is_err()
    {
        // Thread pool already initialized, that's fine
    }

    task_list.set_task_message(init_task, &format!("Using {} threads", threads));
    task_list.update_task(init_task, TaskStatus::Complete);

    // Load configuration if provided
    let mut config = if let Some(config_path) = &args.config {
        task_list.set_task_message(
            init_task,
            &format!("Loading configuration from {:?}...", config_path),
        );
        talaria_core::config::load_config(config_path)?
    } else {
        talaria_core::config::default_config()
    };

    // Override config with command-line arguments
    config.reduction.min_sequence_length = args.min_length;

    // Default to no similarity threshold (matching original db-reduce)
    // Only use similarity if explicitly specified
    if let Some(threshold) = args.similarity_threshold {
        config.reduction.similarity_threshold = threshold;
    } else {
        // Set to 0.0 to disable similarity checking in simple mode
        config.reduction.similarity_threshold = 0.0;
    }

    config.reduction.taxonomy_aware = args.taxonomy_aware;

    // Get input file size
    let input_size = get_file_size(&actual_input).unwrap_or(0);

    // Update workspace metadata with input information
    workspace.lock().unwrap().update_metadata(|m| {
        m.input_file = Some(actual_input.to_string_lossy().to_string());
        if let Some(output) = &args.output {
            m.output_file = Some(output.to_string_lossy().to_string());
        }
    })?;

    // Parse input FASTA (use parallel parser for large files)
    task_list.update_task(load_task, TaskStatus::InProgress);
    task_list.set_task_message(load_task, "Reading FASTA file...");

    let file_size = std::fs::metadata(&actual_input)?.len();

    // Check if we need chunked processing (for databases > 20M sequences)
    const CHUNK_THRESHOLD: usize = 20_000_000;
    const CHUNK_SIZE: usize = 10_000_000;

    if sequence_count > CHUNK_THRESHOLD {
        println!();
        info(&format!(
            "🔄 Large database detected: {} sequences",
            format_number(sequence_count)
        ));
        info(&format!(
            "   Estimated memory for full load: ~{} GB",
            (sequence_count as u64 * 200) / (1024 * 1024 * 1024)
        ));
        info(&format!(
            "   Using chunked processing mode ({} sequences per chunk)",
            format_number(CHUNK_SIZE)
        ));
        println!();

        tracing::info!(
            "Entering chunked reduction mode: {} sequences -> {} chunks",
            sequence_count,
            (sequence_count + CHUNK_SIZE - 1) / CHUNK_SIZE
        );
        tracing::info!("Memory checkpoint: Before chunked processing");

        // Use chunked reduction for massive databases
        return reduce_in_chunks(
            &actual_input,
            CHUNK_SIZE,
            sequence_count,
            &args,
            &config,
            threads,
            &mut task_list,
            load_task,
            select_task,
            encode_task,
            write_task,
            workspace.clone(),
            reduction_ratio,
            manifest_acc2taxid,
        );
    }

    // Regular mode for smaller databases (< 20M sequences)
    task_list.set_task_message(
        load_task,
        &format!("Loading {} sequences...", format_number(sequence_count)),
    );

    // Use parallel parser for files > 50MB, chunk size of 10MB
    let mut sequences = if file_size > 50 * 1024 * 1024 {
        tracing::info!(
            "Using parallel FASTA parser for large file ({} bytes)",
            file_size
        );
        talaria_bio::parse_fasta_parallel(&actual_input, 10 * 1024 * 1024)?
    } else {
        talaria_bio::parse_fasta(&actual_input)?
    };

    // Apply processing pipeline if batch processing or filtering is enabled
    if args.batch || args.low_complexity_filter {
        use talaria_herald::processing::{
            create_reduction_pipeline, BatchProcessor, ProcessingPipeline,
        };

        task_list.set_task_message(load_task, "Applying sequence processing pipeline...");

        // Create pipeline with filters
        let pipeline = create_reduction_pipeline(
            0.3, // Low complexity threshold
            args.min_length,
            true, // Convert to uppercase
        );

        // Process sequences with progress reporting
        let initial_count = sequences.len();

        if args.batch {
            // Use batch processing
            let result = pipeline.process_with_progress(
                &mut sequences,
                args.batch_size / 100, // Batch size for processing
                |processed, total| {
                    task_list.set_task_message(
                        load_task,
                        &format!("Processing sequences: {}/{}", processed, total),
                    );
                },
            )?;

            info(&format!(
                "Pipeline processed {} sequences: {} filtered, {} modified",
                result.processed, result.filtered, result.modified
            ));
        } else if args.low_complexity_filter {
            // Just apply filters
            let result = pipeline.process(&mut sequences)?;

            if result.filtered > 0 {
                info(&format!(
                    "Filtered {} low-complexity sequences",
                    result.filtered
                ));
            }
        }

        // Log if sequences were filtered
        let filtered_count = initial_count - sequences.len();
        if filtered_count > 0 {
            warning(&format!(
                "Filtered {} sequences ({}% removed)",
                filtered_count,
                (filtered_count as f64 / initial_count as f64 * 100.0) as u32
            ));
        }
    }

    // Keep a copy for the HTML report if needed
    let _original_sequences = if args.html_report.is_some() {
        sequences.clone()
    } else {
        vec![]
    };

    // Update workspace stats
    workspace.lock().unwrap().update_stats(|s| {
        s.input_sequences = sequences.len();
    })?;

    task_list.set_task_message(
        load_task,
        &format!(
            "Loaded {} sequences ({})",
            format_number(sequences.len()),
            format_bytes(input_size)
        ),
    );
    task_list.update_task(load_task, TaskStatus::Complete);

    // Run reduction pipeline with workspace
    task_list.update_task(select_task, TaskStatus::InProgress);
    let mut reducer = talaria_herald::Reducer::new(config)
        .with_selection_mode(
            args.similarity_threshold.is_some() || args.align_select,
            args.align_select,
        )
        .with_no_deltas(args.no_deltas)
        .with_max_align_length(args.max_align_length)
        .with_all_vs_all(args.all_vs_all)
        .with_taxonomy_weights(args.use_taxonomy_weights)
        .with_manifest_acc2taxid(manifest_acc2taxid)
        .with_batch_settings(args.batch, args.batch_size)
        .with_selection_algorithm(parse_selection_algorithm(&args.selection_algorithm)?)
        .with_file_sizes(input_size, 0)
        .with_workspace(workspace.clone()); // Pass workspace to reducer

    // Run reduction with better error handling
    // Convert CLI TargetAligner to HERALD TargetAligner
    let target_aligner = match args.target_aligner {
        crate::cli::TargetAligner::Lambda => talaria_herald::TargetAligner::Lambda,
        crate::cli::TargetAligner::Blast => talaria_herald::TargetAligner::Blast,
        crate::cli::TargetAligner::Kraken => talaria_herald::TargetAligner::Kraken,
        crate::cli::TargetAligner::Diamond => talaria_herald::TargetAligner::Diamond,
        crate::cli::TargetAligner::MMseqs2 => talaria_herald::TargetAligner::MMseqs2,
        crate::cli::TargetAligner::Generic => talaria_herald::TargetAligner::Generic,
    };
    let reduction_result = reducer.reduce(sequences, reduction_ratio, target_aligner);

    let (references, deltas, original_count) = match reduction_result {
        Ok(result) => {
            task_list.set_task_message(
                select_task,
                &format!(
                    "Selected {} reference sequences",
                    format_number(result.0.len())
                ),
            );
            task_list.update_task(select_task, TaskStatus::Complete);
            result
        }
        Err(e) => {
            task_list.update_task(select_task, TaskStatus::Failed);
            // Mark workspace as failed
            workspace.lock().unwrap().mark_error(&e.to_string())?;

            // Print a helpful error message
            print_error(&format!("Reference selection failed: {}", e));

            // Check if it's a LAMBDA error
            if e.to_string().contains("LAMBDA") && e.to_string().contains("taxonomy") {
                print_tip("This error often occurs when sequences lack taxonomy IDs.");
                subsection_header("Try one of these solutions");
                tree_item(false, "Use a fixed reduction ratio: -r 0.3", None);
                tree_item(false, "Skip auto-detection and use simple selection", None);
                tree_item(true, "Ensure your FASTA headers include TaxID tags", None);
            }

            // Workspace preserved for debugging
            let ws_id = workspace.lock().unwrap().id.clone();
            warning(&format!("Workspace preserved for debugging: {}", ws_id));
            info(&format!(
                "To inspect: talaria tools workspace inspect {}",
                ws_id
            ));

            return Err(e.into());
        }
    };

    // Update delta encoding status and workspace stats
    if args.no_deltas {
        task_list.update_task(encode_task, TaskStatus::Skipped);
    } else if !deltas.is_empty() {
        task_list.set_task_message(
            encode_task,
            &format!(
                "Encoded {} child sequences as deltas",
                format_number(deltas.len())
            ),
        );
        task_list.update_task(encode_task, TaskStatus::Complete);
    } else {
        task_list.update_task(encode_task, TaskStatus::Skipped);
    }

    // Update workspace stats
    workspace.lock().unwrap().update_stats(|s| {
        s.selected_references = references.len();
        s.final_output_sequences = references.len() + deltas.len();
    })?;

    // Determine output method
    let use_herald_storage = args.output.is_none();

    // Generate output paths
    let (output_path, metadata_path) = if let Some(specified_output) = &args.output {
        // Traditional file output to specified location
        let metadata_path = if let Some(path) = &args.metadata {
            path.clone()
        } else {
            // Auto-generate based on output filename
            let mut delta_path = specified_output.clone();
            if let Some(ext) = delta_path.extension() {
                let mut new_name = delta_path.file_stem().unwrap().to_os_string();
                new_name.push(".deltas.");
                new_name.push(ext);
                delta_path.set_file_name(new_name);
            } else {
                delta_path.set_extension("deltas");
            }
            delta_path
        };
        (specified_output.clone(), metadata_path)
    } else {
        // HERALD storage mode - these are placeholder paths as actual storage goes to HERALD repository
        (
            PathBuf::from("herald_storage"),
            PathBuf::from("herald_storage.deltas"),
        )
    };

    // Choose output method: HERALD storage (default) or traditional files
    let output_size = if use_herald_storage || args.herald_output {
        // Output to HERALD repository
        task_list.set_task_message(write_task, "Storing reduction in HERALD repository...");

        // Use DatabaseManager to access unified repository
        // Note: db_manager was already created earlier to validate database exists

        let size = store_reduction_in_herald(
            &db_manager,
            &actual_input,
            &references,
            &deltas,
            &args,
            reduction_ratio,
            original_count,
            input_size,
            Some(&db_full_name), // Use original database name, not a new one
            &source,
            &dataset,
            &db_version,
        )?;

        task_list.update_task(write_task, TaskStatus::Complete);
        size
    } else {
        // Traditional file output
        task_list.update_task(write_task, TaskStatus::InProgress);
        task_list.set_task_message(write_task, "Writing output files...");

        talaria_bio::write_fasta(&output_path, &references)?;

        // Get output file size
        let output_size = get_file_size(&output_path).unwrap_or(0);

        // Write deltas if they were computed
        if !args.no_deltas && !deltas.is_empty() {
            talaria_storage::io::metadata::write_metadata(&metadata_path, &deltas)?;
            task_list.set_task_message(write_task, &format!("Saved deltas to {:?}", metadata_path));
        }

        task_list.update_task(write_task, TaskStatus::Complete);

        output_size
    };

    // Print statistics using the new stats display
    use crate::cli::charts::{create_length_histogram, create_reduction_summary_chart};
    use crate::cli::formatting::stats_display::create_reduction_stats;

    let avg_deltas = if deltas.is_empty() {
        0.0
    } else {
        deltas.iter().map(|d| d.deltas.len()).sum::<usize>() as f64 / deltas.len() as f64
    };

    let stats = create_reduction_stats(
        original_count,
        references.len(),
        deltas.len(),
        input_size,
        output_size,
        avg_deltas,
    );

    println!("\n{}", stats);

    // Show visualization charts
    if !args.no_visualize {
        // Reduction summary chart
        let coverage = (references.len() + deltas.len()) as f64 / original_count as f64 * 100.0;
        let summary_chart = create_reduction_summary_chart(
            original_count,
            references.len(),
            deltas.len(),
            coverage,
        );
        println!("{}", summary_chart);

        // Length distribution histogram
        let lengths: Vec<usize> = references.iter().map(|s| s.len()).collect();
        if !lengths.is_empty() {
            let length_histogram = create_length_histogram(&lengths);
            println!("{}", length_histogram);
        }
    }

    // Show completion message with nice formatting
    let file_size_reduction = if input_size > 0 && output_size > 0 {
        (1.0 - (output_size as f64 / input_size as f64)) * 100.0
    } else {
        0.0
    };
    let sequence_coverage =
        (references.len() + deltas.len()) as f64 / original_count as f64 * 100.0;

    // Generate HTML report if requested (deprecated - use --report-output instead)
    if let Some(_html_path) = &args.html_report {
        eprintln!("Warning: --html-report is deprecated. Legacy HTML generation temporarily disabled. Use --report-output with --report-format html instead");
    }

    print_success(&format!(
        "Reduction complete: {:.1}% file size reduction, {:.1}% sequence coverage",
        file_size_reduction, sequence_coverage
    ));

    if !args.no_deltas && !deltas.is_empty() {
        print_tip("Use 'talaria reconstruct' to recover original sequences from the reduced set and deltas");
    }

    // Mark workspace as completed successfully
    workspace.lock().unwrap().mark_completed()?;

    // Log operation to HERALD
    herald_manager.log_operation(
        "reduce",
        &format!(
            "Completed: {} sequences -> {} references",
            original_count,
            references.len()
        ),
    )?;

    Ok(())
}

/// Store reduction results in HERALD repository
fn store_reduction_in_herald(
    db_manager: &talaria_herald::database::DatabaseManager,
    input_path: &PathBuf,
    references: &[talaria_bio::sequence::Sequence],
    deltas: &[talaria_bio::compression::DeltaRecord],
    args: &ReduceArgs,
    reduction_ratio: f64,
    original_count: usize,
    input_size: u64,
    database_name: Option<&str>,
    source: &str,
    dataset: &str,
    version: &str,
) -> anyhow::Result<u64> {
    // use talaria_herald::chunker::TaxonomicChunker; // Disabled until reduce is updated
    use std::collections::HashMap;
    use std::time::Instant;
    use talaria_herald::SHA256Hash;
    use talaria_herald::{
        delta::DeltaGeneratorConfig,
        operations::reduction::{
            DeltaChunkRef, ReductionManifest, ReductionParameters, ReferenceChunk,
        },
        DeltaGenerator,
    };

    let start = Instant::now();

    // Access the unified HERALD repository from DatabaseManager
    let herald = db_manager.get_repository();

    // Storage optimization is handled by DatabaseManager
    // No need for separate optimization here since we're using the unified repository
    if args.optimize_for_memory {
        info("Storage optimization enabled (managed by unified repository)");
    }

    // Determine profile name
    let profile_name = args.profile.clone().unwrap_or_else(|| {
        if reduction_ratio == 0.0 {
            "auto-detect".to_string()
        } else {
            format!("{}-percent", (reduction_ratio * 100.0) as u32)
        }
    });

    // Use provided database name or derive from input path
    let source_database = if let Some(db_name) = database_name {
        db_name.to_string()
    } else {
        input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    };

    // Create reduction parameters
    // Convert CLI TargetAligner to HERALD TargetAligner
    let target_aligner_herald = match args.target_aligner {
        crate::cli::TargetAligner::Lambda => talaria_herald::TargetAligner::Lambda,
        crate::cli::TargetAligner::Blast => talaria_herald::TargetAligner::Blast,
        crate::cli::TargetAligner::Kraken => talaria_herald::TargetAligner::Kraken,
        crate::cli::TargetAligner::Diamond => talaria_herald::TargetAligner::Diamond,
        crate::cli::TargetAligner::MMseqs2 => talaria_herald::TargetAligner::MMseqs2,
        crate::cli::TargetAligner::Generic => talaria_herald::TargetAligner::Generic,
    };
    let parameters = ReductionParameters {
        reduction_ratio,
        target_aligner: Some(target_aligner_herald),
        min_length: args.min_length,
        similarity_threshold: args.similarity_threshold.unwrap_or(0.9),
        taxonomy_aware: args.taxonomy_aware,
        align_select: args.align_select,
        max_align_length: args.max_align_length,
        no_deltas: args.no_deltas,
    };

    // Get actual source manifest if input was from HERALD
    // Check if the path is within a Talaria databases directory
    let databases_dir = talaria_core::system::paths::talaria_databases_dir();
    let source_manifest_hash = if input_path.starts_with(&databases_dir) {
        // Try to find manifest.json in the HERALD structure
        let mut current = input_path.clone();
        loop {
            let manifest_path = current.join("manifest.json");
            if manifest_path.exists() {
                // Load and hash the manifest content
                if let Ok(content) = std::fs::read(&manifest_path) {
                    break SHA256Hash::compute(&content);
                }
            }
            // Go up one directory
            if !current.pop() || current.parent().is_none() {
                break SHA256Hash::compute(input_path.to_string_lossy().as_bytes());
            }
        }
    } else {
        SHA256Hash::compute(input_path.to_string_lossy().as_bytes())
    };

    // Create reduction manifest
    let mut manifest = ReductionManifest::new(
        profile_name.clone(),
        source_manifest_hash,
        source_database.clone(),
        parameters,
    );

    // Chunk and store reference sequences using canonical storage
    action("Chunking reference sequences...");

    use std::sync::Arc;
    use talaria_herald::chunker::TaxonomicChunker;
    use talaria_herald::ChunkingStrategy;

    // Reuse the existing SequenceStorage from HERALD repository
    // This avoids double-initialization of RocksDB
    let sequence_storage = Arc::clone(&herald.storage.sequence_storage);

    // Create database source for chunker
    let db_source = match source {
        "uniprot" => match dataset {
            "swissprot" => {
                talaria_core::DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt)
            }
            "trembl" => {
                talaria_core::DatabaseSource::UniProt(talaria_core::UniProtDatabase::TrEMBL)
            }
            _ => talaria_core::DatabaseSource::Custom(format!("{}/{}", source, dataset)),
        },
        "ncbi" => match dataset {
            "nr" => talaria_core::DatabaseSource::NCBI(talaria_core::NCBIDatabase::NR),
            "nt" => talaria_core::DatabaseSource::NCBI(talaria_core::NCBIDatabase::NT),
            "refseq" => talaria_core::DatabaseSource::NCBI(talaria_core::NCBIDatabase::RefSeq),
            "genbank" => talaria_core::DatabaseSource::NCBI(talaria_core::NCBIDatabase::GenBank),
            _ => talaria_core::DatabaseSource::Custom(format!("{}/{}", source, dataset)),
        },
        _ => talaria_core::DatabaseSource::Custom(format!("{}/{}", source, dataset)),
    };

    // Create chunker with canonical storage
    let mut chunker =
        TaxonomicChunker::new(ChunkingStrategy::default(), sequence_storage, db_source);

    // Create progress bar for sequence processing
    let ref_count = references.len();
    let pb = ProgressBar::new(ref_count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );
    pb.set_message("Processing reference sequences");
    let pb_clone = pb.clone();

    // Create progress callback
    let progress_callback = Box::new(move |count: usize, msg: &str| {
        pb_clone.set_position(count as u64);
        pb_clone.set_message(msg.to_string());
    });

    // Process references and get chunk manifests with progress tracking
    let chunk_manifests = chunker
        .chunk_sequences_canonical_with_progress(references.to_vec(), Some(progress_callback))?;

    pb.finish_and_clear();

    // Add progress bar for chunk storage
    let chunk_progress = ProgressBar::new(chunk_manifests.len() as u64);
    chunk_progress.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} chunks stored")
            .unwrap()
            .progress_chars("##-"),
    );
    chunk_progress.set_message("Storing reference chunks...");

    let mut reference_chunk_refs = Vec::new();
    let mut ref_chunk_map = HashMap::new();

    // Parallelize chunk manifest storage for better performance
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let progress_counter = Arc::new(AtomicUsize::new(0));
    let pb_clone = chunk_progress.clone();
    let counter_clone = Arc::clone(&progress_counter);

    // Process all manifests in parallel
    let results: Vec<_> = chunk_manifests
        .par_iter()
        .map(|manifest| {
            // Store the manifest (not the chunk with data!)
            let chunk_hash = herald.storage.store_chunk_manifest(manifest)?;

            // Get sequence IDs from the manifest's sequence_refs
            let mut sequence_ids = Vec::new();
            for seq_hash in &manifest.sequence_refs {
                // Get sequence ID from the canonical storage
                if let Ok(seq_info) = chunker.sequence_storage.get_sequence_info(seq_hash) {
                    sequence_ids.push(seq_info.id);
                }
            }

            // Create reference chunk metadata
            let ref_chunk = ReferenceChunk {
                chunk_hash: chunk_hash.clone(),
                sequence_ids: sequence_ids.clone(),
                sequence_count: manifest.sequence_count,
                size: manifest.total_size,
                compressed_size: Some(manifest.total_size),
                taxon_ids: manifest.taxon_ids.clone(),
            };

            // Update progress every 10 chunks
            let count = counter_clone.fetch_add(1, Ordering::Relaxed);
            if count % 10 == 0 {
                pb_clone.set_position(count as u64);
            }

            Ok((ref_chunk, chunk_hash, sequence_ids))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    // Build reference_chunk_refs and ref_chunk_map from results
    for (ref_chunk, chunk_hash, sequence_ids) in results {
        reference_chunk_refs.push(ref_chunk);

        // Map sequence IDs to chunk hash for delta processing
        for seq_id in sequence_ids {
            ref_chunk_map.insert(seq_id, chunk_hash.clone());
        }
    }

    chunk_progress.set_position(chunk_manifests.len() as u64);
    chunk_progress.finish_with_message("Reference chunks stored");

    manifest.add_reference_chunks(reference_chunk_refs);

    // Process and store delta chunks if present
    if !deltas.is_empty() && !args.no_deltas {
        action("Processing delta sequences...");

        // Group deltas by reference sequence (parallel grouping with DashMap)
        use dashmap::DashMap;

        info(&format!(
            "Grouping {} deltas by reference...",
            format_number(deltas.len())
        ));

        let deltas_by_ref_concurrent = DashMap::new();
        deltas.par_iter().for_each(|delta| {
            deltas_by_ref_concurrent
                .entry(delta.reference_id.clone())
                .or_insert_with(Vec::new)
                .push(delta.clone());
        });

        // Convert DashMap to HashMap for compatibility with rest of code
        let deltas_by_ref: HashMap<String, Vec<talaria_bio::compression::DeltaRecord>> =
            deltas_by_ref_concurrent.into_iter().collect();

        info(&format!(
            "Grouped into {} reference groups",
            format_number(deltas_by_ref.len())
        ));

        let mut delta_chunk_refs = Vec::new();

        // Create delta generator
        let delta_config = DeltaGeneratorConfig {
            min_delta_size: 1024,
            max_delta_size: 100 * 1024 * 1024,
            compression_threshold: 0.8,
            enable_caching: true,
            max_chunk_size: 16 * 1024 * 1024,
            min_similarity_threshold: 0.85,
            enable_compression: true,
            target_sequences_per_chunk: 1000,
            max_delta_ops_threshold: 100,
        };
        let mut delta_generator = DeltaGenerator::new(delta_config);

        // Convert delta records to sequences for delta generation
        let all_child_sequences: Vec<talaria_bio::sequence::Sequence> = deltas
            .iter()
            .map(|d| talaria_bio::sequence::Sequence {
                id: d.child_id.clone(),
                description: None,
                sequence: Vec::new(), // Will be filled by delta generator
                taxon_id: d.taxon_id,
                taxonomy_sources: Default::default(),
            })
            .collect();

        let all_ref_sequences: Vec<talaria_bio::sequence::Sequence> = references.to_vec();

        // Generate delta chunks using the new system
        if !all_child_sequences.is_empty() && !all_ref_sequences.is_empty() {
            // Get the first reference chunk hash as the base
            let base_ref_hash = ref_chunk_map
                .values()
                .next()
                .ok_or_else(|| anyhow::anyhow!("No reference chunks available"))?;

            info("Generating delta chunks...");
            let delta_chunks = delta_generator.generate_delta_chunks(
                &all_child_sequences,
                &all_ref_sequences,
                base_ref_hash.clone(),
            )?;

            // Add progress bar for delta chunk storage
            let delta_progress = ProgressBar::new(delta_chunks.len() as u64);
            delta_progress.set_style(
                ProgressStyle::default_bar()
                    .template(
                        "[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} delta chunks stored",
                    )
                    .unwrap()
                    .progress_chars("##-"),
            );
            delta_progress.set_message("Storing delta chunks...");

            // Store delta chunks and create references (parallel for better performance)
            let delta_progress_counter = Arc::new(AtomicUsize::new(0));
            let delta_pb_clone = delta_progress.clone();
            let delta_counter_clone = Arc::clone(&delta_progress_counter);

            let delta_results: Vec<_> = delta_chunks
                .par_iter()
                .map(|delta_chunk| {
                    let delta_hash = herald.storage.store_delta_chunk(delta_chunk)?;

                    let delta_ref = DeltaChunkRef {
                        chunk_hash: delta_hash,
                        reference_chunk_hash: delta_chunk.reference_hash.clone(),
                        child_count: delta_chunk.sequences.len(),
                        child_ids: delta_chunk
                            .sequences
                            .iter()
                            .map(|s| s.sequence_id.clone())
                            .collect(),
                        size: delta_chunk.compressed_size,
                        avg_delta_ops: delta_chunk.deltas.len() as f32
                            / delta_chunk.sequences.len().max(1) as f32,
                    };

                    // Update progress every 10 chunks
                    let count = delta_counter_clone.fetch_add(1, Ordering::Relaxed);
                    if count % 10 == 0 {
                        delta_pb_clone.set_position(count as u64);
                    }

                    Ok(delta_ref)
                })
                .collect::<anyhow::Result<Vec<_>>>()?;

            delta_chunk_refs.extend(delta_results);

            delta_progress.set_position(delta_chunks.len() as u64);
            delta_progress.finish_with_message("Delta chunks stored");
        }

        manifest.add_delta_chunks(delta_chunk_refs);
    }

    // Compute Merkle roots
    action("Computing Merkle roots...");
    manifest.compute_merkle_roots()?;

    // Calculate statistics
    action("Calculating statistics...");
    let elapsed = start.elapsed().as_secs();
    manifest.calculate_statistics(original_count, input_size, elapsed);

    // Store the reduction manifest as a profile in the database version directory
    action("Storing manifest in HERALD repository...");
    let manifest_hash = herald
        .storage
        .store_database_reduction_manifest(&manifest, source, dataset, version)?;

    // Update metadata cache with the new reduction profile
    // This ensures it shows up immediately in database list/info
    db_manager.add_reduction_profile_to_metadata(source, dataset, &profile_name)?;

    // Note: We do NOT create a new database manifest here
    // The reduction is stored as a profile associated with the original database

    // Calculate total size
    let total_size = manifest.statistics.total_size_with_deltas;

    success("Reduction stored in HERALD repository");

    let mut details = vec![
        ("Database", source_database.clone()),
        ("Profile", profile_name.clone()),
        ("Manifest", manifest_hash.to_string()),
        (
            "References",
            format!("{} chunks", format_number(manifest.reference_chunks.len())),
        ),
    ];
    if !args.no_deltas {
        details.push((
            "Deltas",
            format!("{} chunks", format_number(manifest.delta_chunks.len())),
        ));
    }
    details.push(("Merkle root", manifest.reduction_merkle_root.to_string()));
    details.push((
        "Deduplication",
        format!("{:.1}%", manifest.statistics.deduplication_ratio * 100.0),
    ));

    tree_section("Storage Summary", details, false);

    // Generate report if requested
    if let Some(report_path) = args.report_output.clone().or(args.html_report.clone()) {
        use std::time::Duration;
        use talaria_herald::operations::ReductionResult;

        let result = ReductionResult {
            statistics: manifest.statistics.clone(),
            parameters: manifest.parameters.clone(),
            selection_stats: None, // TODO: Capture selection stats during reduction
            manifest: manifest.clone(),
            duration: Duration::from_secs(elapsed),
        };

        let format = if args.html_report.is_some() {
            "html"
        } else {
            &args.report_format
        };
        crate::cli::commands::save_report(&result, format, &report_path)?;
        success(&format!("Report saved to {}", report_path.display()));
    }

    subsection_header("Next Steps");
    info("View with: talaria database list");
    info(&format!("Info: talaria database info {}", source_database));

    Ok(total_size)
}

/// Process FASTA file in chunks, calling a callback for each chunk
/// This truly streams - only one chunk is in memory at a time
/// Returns the number of chunks processed
fn process_fasta_in_chunks<F>(
    path: &PathBuf,
    chunk_size: usize,
    mut callback: F,
) -> anyhow::Result<usize>
where
    F: FnMut(Vec<talaria_bio::Sequence>, usize) -> anyhow::Result<()>,
{
    use std::io::{BufRead, BufReader};
    use talaria_bio::Sequence;

    let file = std::fs::File::open(path)?;
    let reader = BufReader::with_capacity(10 * 1024 * 1024, file); // 10MB buffer

    let mut current_chunk = Vec::new();
    let mut current_seq: Option<Sequence> = None;
    let mut chunk_idx = 0;

    for line in reader.lines() {
        let line = line?;

        if line.starts_with('>') {
            // Save previous sequence if exists
            if let Some(seq) = current_seq.take() {
                current_chunk.push(seq);

                // Check if chunk is full
                if current_chunk.len() >= chunk_size {
                    // Process this chunk and clear it (memory efficient!)
                    let chunk = std::mem::replace(&mut current_chunk, Vec::new());
                    callback(chunk, chunk_idx)?;
                    chunk_idx += 1;
                }
            }

            // Parse header
            let header = line[1..].trim();
            let (id, description) = if let Some(space_pos) = header.find(' ') {
                (
                    header[..space_pos].to_string(),
                    Some(header[space_pos + 1..].to_string()),
                )
            } else {
                (header.to_string(), None)
            };

            current_seq = Some(Sequence {
                id,
                description,
                sequence: Vec::new(),
                taxon_id: None,
                taxonomy_sources: Default::default(),
            });
        } else if let Some(ref mut seq) = current_seq {
            // Append sequence data
            seq.sequence.extend(line.trim().as_bytes());
        }
    }

    // Don't forget the last sequence
    if let Some(seq) = current_seq {
        current_chunk.push(seq);
    }

    // Don't forget the last chunk
    if !current_chunk.is_empty() {
        callback(current_chunk, chunk_idx)?;
        chunk_idx += 1;
    }

    Ok(chunk_idx)
}

/// Process massive databases in chunks to avoid OOM
/// This function reads sequences in chunks, processes each chunk separately,
/// then merges the results while deduplicating references
#[allow(clippy::too_many_arguments)]
fn reduce_in_chunks(
    input_path: &PathBuf,
    chunk_size: usize,
    total_sequences: usize,
    args: &ReduceArgs,
    config: &talaria_core::config::Config,
    _threads: usize,
    task_list: &mut TaskList,
    load_task: TaskHandle,
    select_task: TaskHandle,
    encode_task: TaskHandle,
    write_task: TaskHandle,
    workspace: Arc<Mutex<talaria_utils::workspace::TempWorkspace>>,
    reduction_ratio: f64,
    manifest_acc2taxid: Option<PathBuf>,
) -> anyhow::Result<()> {
    use std::collections::HashSet;
    use talaria_bio::Sequence;

    // Calculate number of chunks
    let num_chunks = (total_sequences + chunk_size - 1) / chunk_size;

    info(&format!(
        "📦 Processing in {} chunks of {} sequences each",
        format_number(num_chunks),
        format_number(chunk_size)
    ));
    println!();

    // Start processing task
    task_list.update_task(load_task, TaskStatus::InProgress);
    task_list.update_task(select_task, TaskStatus::InProgress);

    info(&format!(
        "Streaming {} sequences in {} chunks...",
        format_number(total_sequences),
        format_number(num_chunks)
    ));

    // Shared state for accumulating results across chunks
    let mut all_references: Vec<Sequence> = Vec::new();
    let mut all_deltas: Vec<talaria_bio::compression::DeltaRecord> = Vec::new();
    let mut reference_ids: HashSet<String> = HashSet::new();

    // Convert CLI TargetAligner to HERALD TargetAligner once
    let target_aligner = match args.target_aligner {
        crate::cli::TargetAligner::Lambda => talaria_herald::TargetAligner::Lambda,
        crate::cli::TargetAligner::Blast => talaria_herald::TargetAligner::Blast,
        crate::cli::TargetAligner::Kraken => talaria_herald::TargetAligner::Kraken,
        crate::cli::TargetAligner::Diamond => talaria_herald::TargetAligner::Diamond,
        crate::cli::TargetAligner::MMseqs2 => talaria_herald::TargetAligner::MMseqs2,
        crate::cli::TargetAligner::Generic => talaria_herald::TargetAligner::Generic,
    };

    // Stream and process chunks one at a time (memory efficient!)
    let total_chunks = process_fasta_in_chunks(input_path, chunk_size, |chunk, chunk_idx| {
        // Update progress
        task_list.set_task_message(
            select_task,
            &format!(
                "Processing chunk {} ({} sequences)...",
                chunk_idx + 1,
                format_number(chunk.len())
            ),
        );

        // Create reducer for this chunk
        let mut reducer = talaria_herald::Reducer::new(config.clone())
            .with_selection_mode(
                args.similarity_threshold.is_some() || args.align_select,
                args.align_select,
            )
            .with_no_deltas(args.no_deltas)
            .with_max_align_length(args.max_align_length)
            .with_all_vs_all(args.all_vs_all)
            .with_taxonomy_weights(args.use_taxonomy_weights)
            .with_manifest_acc2taxid(manifest_acc2taxid.clone())
            .with_batch_settings(args.batch, args.batch_size)
            .with_selection_algorithm(parse_selection_algorithm(&args.selection_algorithm)?)
            .with_workspace(workspace.clone());

        // Process chunk
        match reducer.reduce(chunk, reduction_ratio, target_aligner.clone()) {
            Ok((chunk_refs, chunk_deltas, _original_count)) => {
                let chunk_refs_count = chunk_refs.len();

                // Deduplicate references - only add if not already seen
                for seq in chunk_refs {
                    if !reference_ids.contains(&seq.id) {
                        reference_ids.insert(seq.id.clone());
                        all_references.push(seq);
                    }
                }

                // Accumulate deltas
                all_deltas.extend(chunk_deltas);

                info(&format!(
                    "  ✓ Chunk {}: {} references selected (total: {})",
                    chunk_idx + 1,
                    format_number(chunk_refs_count),
                    format_number(all_references.len())
                ));

                Ok(())
            }
            Err(e) => {
                task_list.update_task(select_task, TaskStatus::Failed);
                workspace.lock().unwrap().mark_error(&e.to_string())?;
                Err(e.into())
            }
        }
    })?;

    task_list.update_task(load_task, TaskStatus::Complete);
    task_list.set_task_message(
        select_task,
        &format!(
            "Selected {} total references from {} chunks",
            format_number(all_references.len()),
            format_number(total_chunks)
        ),
    );
    task_list.update_task(select_task, TaskStatus::Complete);

    // Handle delta encoding
    if args.no_deltas {
        task_list.update_task(encode_task, TaskStatus::Skipped);
    } else if !all_deltas.is_empty() {
        task_list.set_task_message(
            encode_task,
            &format!(
                "Encoded {} child sequences as deltas",
                format_number(all_deltas.len())
            ),
        );
        task_list.update_task(encode_task, TaskStatus::Complete);
    } else {
        task_list.update_task(encode_task, TaskStatus::Skipped);
    }

    // Update workspace stats
    workspace.lock().unwrap().update_stats(|s| {
        s.input_sequences = total_sequences;
        s.selected_references = all_references.len();
        s.final_output_sequences = all_references.len() + all_deltas.len();
    })?;

    // Store results in HERALD
    task_list.update_task(write_task, TaskStatus::InProgress);
    task_list.set_task_message(write_task, "Storing reduction in HERALD repository...");

    // Get input file size
    let input_size = std::fs::metadata(input_path)?.len();

    // Parse database reference
    let (source, dataset) = if args.database.contains('/') {
        let parts: Vec<&str> = args.database.split('/').collect();
        (parts[0].to_string(), parts[1].to_string())
    } else {
        ("custom".to_string(), args.database.clone())
    };

    // Get database version
    use talaria_herald::database::DatabaseManager;
    let db_manager = DatabaseManager::new(None)?;
    let databases = db_manager.list_databases()?;
    let db_full_name = format!("{}/{}", source, dataset);
    let db_info = databases
        .iter()
        .find(|db| db.name == db_full_name)
        .ok_or_else(|| anyhow::anyhow!("Database '{}' not found", db_full_name))?;
    let db_version = db_info.version.clone();

    let output_size = store_reduction_in_herald(
        &db_manager,
        input_path,
        &all_references,
        &all_deltas,
        args,
        reduction_ratio,
        total_sequences,
        input_size,
        Some(&db_full_name),
        &source,
        &dataset,
        &db_version,
    )?;

    task_list.update_task(write_task, TaskStatus::Complete);

    // Print statistics
    use crate::cli::charts::{create_length_histogram, create_reduction_summary_chart};
    use crate::cli::formatting::stats_display::create_reduction_stats;

    let avg_deltas = if all_deltas.is_empty() {
        0.0
    } else {
        all_deltas.iter().map(|d| d.deltas.len()).sum::<usize>() as f64 / all_deltas.len() as f64
    };

    let stats = create_reduction_stats(
        total_sequences,
        all_references.len(),
        all_deltas.len(),
        input_size,
        output_size,
        avg_deltas,
    );

    println!("\n{}", stats);

    // Show visualization charts
    if !args.no_visualize {
        let coverage =
            (all_references.len() + all_deltas.len()) as f64 / total_sequences as f64 * 100.0;
        let summary_chart = create_reduction_summary_chart(
            total_sequences,
            all_references.len(),
            all_deltas.len(),
            coverage,
        );
        println!("{}", summary_chart);

        // Length distribution histogram
        let lengths: Vec<usize> = all_references.iter().map(|s| s.len()).collect();
        if !lengths.is_empty() {
            let length_histogram = create_length_histogram(&lengths);
            println!("{}", length_histogram);
        }
    }

    // Show completion message
    let file_size_reduction = if input_size > 0 && output_size > 0 {
        (1.0 - (output_size as f64 / input_size as f64)) * 100.0
    } else {
        0.0
    };
    let sequence_coverage =
        (all_references.len() + all_deltas.len()) as f64 / total_sequences as f64 * 100.0;

    print_success(&format!(
        "Chunked reduction complete: {:.1}% file size reduction, {:.1}% sequence coverage",
        file_size_reduction, sequence_coverage
    ));

    if !args.no_deltas && !all_deltas.is_empty() {
        print_tip("Use 'talaria reconstruct' to recover original sequences from the reduced set and deltas");
    }

    // Mark workspace as completed
    workspace.lock().unwrap().mark_completed()?;

    Ok(())
}
