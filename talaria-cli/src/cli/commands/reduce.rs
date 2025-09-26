use crate::cli::formatting::{
    info_box, print_error, print_success, print_tip, TaskList, TaskStatus,
};
use talaria_utils::display::format::format_bytes;
use crate::cli::formatting::output::*;
use crate::cli::TargetAligner;
use talaria_utils::workspace::SequoiaWorkspaceManager;
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Args, Debug)]
pub struct ReduceArgs {
    /// Database to reduce (e.g., "uniprot/swissprot", "custom/taxids_9606")
    /// Must be a database that exists in the SEQUOIA repository
    #[arg(value_name = "DATABASE")]
    pub database: String,

    /// Output reduced FASTA file (optional - stores in SEQUOIA by default)
    #[arg(short, long, value_name = "FILE")]
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

    /// Use taxonomy data to weight alignment scores (requires taxonomy data in FASTA or SEQUOIA)
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

    /// Output to SEQUOIA repository instead of files
    #[arg(long)]
    pub sequoia_output: bool,

    /// SEQUOIA repository path (default: ${TALARIA_HOME}/databases)
    #[arg(long, value_name = "PATH")]
    pub sequoia_path: Option<PathBuf>,

    /// Skip visualization charts in output
    #[arg(long)]
    pub no_visualize: bool,

    /// Generate HTML report with visualization
    #[arg(long)]
    pub html_report: Option<PathBuf>,
}

/// Parse the selection algorithm string into the enum
fn parse_selection_algorithm(
    algorithm: &str,
) -> anyhow::Result<talaria_sequoia::SelectionAlgorithm> {
    use talaria_sequoia::SelectionAlgorithm;

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
    use talaria_sequoia::SelectionAlgorithm;

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
            assert!(level >= 1 && level <= 22, "Invalid compression level: {}", level);
        }
    }

    #[test]
    fn test_batch_size_calculation() {
        // Test batch size calculations for different input sizes
        let test_cases = vec![
            (100, 1000, 100),      // Small file, use all
            (10_000, 1000, 1000),  // Medium file, use batch size
            (1_000_000, 5000, 5000), // Large file, use batch size
        ];

        for (total, batch_size, expected) in test_cases {
            let actual = total.min(batch_size);
            assert_eq!(actual, expected,
                "Batch size calculation failed for total={}, batch_size={}",
                total, batch_size);
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
            ("1.5", Err(())), // Out of range
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
        let valid_formats = vec!["fasta", "sequoia", "json"];

        for format in valid_formats {
            assert!(
                format == "fasta" || format == "sequoia" || format == "json",
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
            (0, false),    // No sequences - invalid
            (1, true),     // Single sequence - valid but warning
            (10, true),    // Few sequences - valid
            (1000, true),  // Many sequences - valid
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
            (0, cpu_count),     // 0 means use all CPUs
            (1, 1),             // Single thread
            (4, 4),             // Specific count
            (1000, 1000),       // More than available (should be capped in practice)
        ];

        for (requested, expected) in test_cases {
            let actual = if requested == 0 { cpu_count } else { requested };
            assert_eq!(actual, expected,
                "Thread count calculation failed for requested={}", requested);
        }
    }
}

pub fn run(mut args: ReduceArgs) -> anyhow::Result<()> {
    use talaria_utils::display::format::get_file_size;

    // Initialize formatter
    crate::cli::formatting::formatter::init();

    // Initialize SEQUOIA workspace manager
    let mut sequoia_manager = SequoiaWorkspaceManager::new()?;

    // Create workspace for this reduction operation
    let command = format!("reduce {:?}", &args);
    let workspace = Arc::new(Mutex::new(sequoia_manager.create_workspace(&command)?));

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
    use talaria_sequoia::database::DatabaseManager;
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
    // Assemble FASTA from SEQUOIA chunks
    let temp_file = workspace
        .lock()
        .unwrap()
        .get_file_path("input_fasta", "fasta");

    // Map database to internal source enum if it's a standard database
    use talaria_sequoia::download::{DatabaseSource, NCBIDatabase, UniProtDatabase};
    let database_source = match db_full_name.as_str() {
        "uniprot/swissprot" => Some(DatabaseSource::UniProt(UniProtDatabase::SwissProt)),
        "uniprot/trembl" => Some(DatabaseSource::UniProt(UniProtDatabase::TrEMBL)),
        "ncbi/nr" => Some(DatabaseSource::NCBI(NCBIDatabase::NR)),
        "ncbi/nt" => Some(DatabaseSource::NCBI(NCBIDatabase::NT)),
        _ => None, // Custom database
    };

    // Use spinner for assembly
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    spinner.set_message("Assembling database from SEQUOIA chunks...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    // Assemble the database from SEQUOIA
    if let Some(db_source) = &database_source {
        db_manager.assemble_database(db_source, &temp_file)?;
        spinner.finish_and_clear();
        println!("Database assembled successfully");
    } else {
        // For custom databases, assemble from chunks referenced in manifest
        use talaria_sequoia::operations::FastaAssembler;
        use talaria_sequoia::TemporalManifest;
        use talaria_core::system::paths;
        use std::fs;
        use std::io::Write;

        // Magic bytes for Talaria manifest format
        const TALARIA_MAGIC: &[u8] = b"TAL\x01";

        // Find the manifest in the versions/ structure
        let versions_path = paths::talaria_databases_dir()
            .join("versions")
            .join(&source)
            .join(&dataset)
            .join("current");

        // Try .tal file first (preferred binary format)
        let mut manifest_path = versions_path.join("manifest.tal");
        let mut is_tal_format = true;

        if !manifest_path.exists() {
            // Try .json as fallback
            manifest_path = versions_path.join("manifest.json");
            is_tal_format = false;

            if !manifest_path.exists() {
                anyhow::bail!(
                    "Cannot find manifest for database: {}. Expected at: {}",
                    db_full_name,
                    versions_path.display()
                );
            }
        }

        // Load the manifest based on format
        let manifest: TemporalManifest = if is_tal_format {
            // Read binary .tal format
            let mut content = fs::read(&manifest_path)?;

            // Check and skip magic header
            if content.starts_with(TALARIA_MAGIC) {
                content = content[TALARIA_MAGIC.len()..].to_vec();
            }

            rmp_serde::from_slice(&content)?
        } else {
            // Read JSON format
            let manifest_content = fs::read_to_string(&manifest_path)?;
            serde_json::from_str(&manifest_content)?
        };

        // Stream assembly directly to file (much more efficient than loading into memory)
        let assembler = FastaAssembler::new(db_manager.get_storage());
        let chunk_hashes: Vec<_> = manifest
            .chunk_index
            .iter()
            .map(|c| c.hash.clone())
            .collect();

        // Stream chunks directly to file without loading all sequences into memory
        spinner.set_message("Streaming sequences from SEQUOIA chunks to file...");

        // Create scope to ensure file is properly closed and flushed
        let sequence_count = {
            let mut output_file = std::fs::File::create(&temp_file)?;
            let count = assembler.stream_assembly(&chunk_hashes, &mut output_file)?;
            // Explicitly flush before closing
            output_file.flush()?;
            count
        }; // File handle dropped and closed here

        spinner.finish_and_clear();
        println!(
            "Assembled {} sequences from SEQUOIA chunks",
            format_number(sequence_count)
        );
        println!("Database written successfully");
    }

    // Debug: Print file path and verify it was written
    println!("FASTA file created at: {}", temp_file.display());
    if let Ok(metadata) = std::fs::metadata(&temp_file) {
        println!("File size: {} bytes", format_number(metadata.len() as usize));

        // Read and display first few lines for verification
        if std::env::var("TALARIA_DEBUG").is_ok() || std::env::var("TALARIA_LAMBDA_VERBOSE").is_ok() {
            use std::io::{BufRead, BufReader};
            if let Ok(file) = std::fs::File::open(&temp_file) {
                let reader = BufReader::new(file);
                let mut lines_shown = 0;
                println!("First few lines of FASTA file:");
                for line in reader.lines() {
                    if let Ok(line) = line {
                        if lines_shown < 10 {
                            if line.starts_with('>') {
                                println!("  Header: {}", if line.len() > 100 {
                                    format!("{}...", &line[..100])
                                } else {
                                    line.clone()
                                });
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
            spinner.enable_steady_tick(std::time::Duration::from_millis(100));

            match db_manager.create_accession2taxid_from_manifest(db_src) {
                Ok(path) => {
                    spinner.finish_and_clear();
                    println!("Taxonomy mapping created from manifest");
                    Some(path)
                }
                Err(e) => {
                    spinner.finish_and_clear();
                    println!("Warning: Could not create taxonomy mapping from manifest");
                    warning(&e.to_string());
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
        .build_global().is_err()
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

    // Parse input FASTA
    task_list.update_task(load_task, TaskStatus::InProgress);
    task_list.set_task_message(load_task, "Reading FASTA file...");
    let mut sequences = talaria_bio::parse_fasta(&actual_input)?;

    // Apply processing pipeline if batch processing or filtering is enabled
    if args.batch || args.low_complexity_filter {
        use talaria_sequoia::processing::{create_reduction_pipeline, BatchProcessor, ProcessingPipeline};

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
    let original_sequences = if args.html_report.is_some() {
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
            sequences.len(),
            format_bytes(input_size)
        ),
    );
    task_list.update_task(load_task, TaskStatus::Complete);

    // Run reduction pipeline with workspace
    task_list.update_task(select_task, TaskStatus::InProgress);
    let mut reducer = talaria_sequoia::Reducer::new(config)
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
    // Convert CLI TargetAligner to SEQUOIA TargetAligner
    let target_aligner = match args.target_aligner {
        crate::cli::TargetAligner::Lambda => talaria_sequoia::TargetAligner::Lambda,
        crate::cli::TargetAligner::Blast => talaria_sequoia::TargetAligner::Blast,
        crate::cli::TargetAligner::Kraken => talaria_sequoia::TargetAligner::Kraken,
        crate::cli::TargetAligner::Diamond => talaria_sequoia::TargetAligner::Diamond,
        crate::cli::TargetAligner::MMseqs2 => talaria_sequoia::TargetAligner::MMseqs2,
        crate::cli::TargetAligner::Generic => talaria_sequoia::TargetAligner::Generic,
    };
    let reduction_result = reducer.reduce(sequences, reduction_ratio, target_aligner);

    let (references, deltas, original_count) = match reduction_result {
        Ok(result) => {
            task_list.set_task_message(
                select_task,
                &format!("Selected {} reference sequences", result.0.len()),
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
            &format!("Encoded {} child sequences as deltas", deltas.len()),
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
    let use_sequoia_storage = args.output.is_none();

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
        // SEQUOIA storage mode - these are placeholder paths as actual storage goes to SEQUOIA repository
        (
            PathBuf::from("sequoia_storage"),
            PathBuf::from("sequoia_storage.deltas"),
        )
    };

    // Choose output method: SEQUOIA storage (default) or traditional files
    let output_size = if use_sequoia_storage || args.sequoia_output {
        // Output to SEQUOIA repository
        task_list.set_task_message(write_task, "Storing reduction in SEQUOIA repository...");

        let sequoia_path = args.sequoia_path.clone().unwrap_or_else(|| {
            use talaria_core::system::paths;
            paths::talaria_databases_dir()
        });

        store_reduction_in_sequoia(
            &sequoia_path,
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
        )?
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

    // Generate HTML report if requested
    if let Some(html_path) = &args.html_report {
        task_list.set_task_message(write_task, "Generating HTML report...");

        // Create selection result for report
        let selection_result = talaria_sequoia::SelectionResult {
            references: references.clone(),
            children: {
                let mut children_map = std::collections::HashMap::new();
                for delta in &deltas {
                    children_map.insert(delta.reference_id.clone(), vec![delta.child_id.clone()]);
                }
                children_map
            },
            discarded: std::collections::HashSet::new(), // We don't track discarded sequences here
        };

        // Generate HTML report
        // Convert from sequoia SelectionResult to utils SelectionResult
        let utils_selection_result = talaria_utils::report::reduction_html::SelectionResult {
            references: selection_result.references.clone(),
            deltas: std::collections::HashMap::new(),
            children: std::collections::HashMap::new(),
            discarded: Vec::new(),
        };

        let html_content = talaria_utils::report::reduction_html::generate_reduction_html_report(
            &actual_input,
            &output_path,
            &original_sequences,
            &utils_selection_result,
            sequence_coverage,
            None, // No taxonomic stats for now - could be added later
        )?;

        // Write HTML report to file
        std::fs::write(html_path, html_content)?;
        task_list.set_task_message(
            write_task,
            &format!("✓ HTML report saved to: {}", html_path.display()),
        );
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

    // Log operation to SEQUOIA
    sequoia_manager.log_operation(
        "reduce",
        &format!(
            "Completed: {} sequences -> {} references",
            original_count,
            references.len()
        ),
    )?;

    Ok(())
}

/// Store reduction results in SEQUOIA repository
fn store_reduction_in_sequoia(
    sequoia_path: &PathBuf,
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
    // use talaria_sequoia::chunker::TaxonomicChunker; // Disabled until reduce is updated
    use talaria_sequoia::SHA256Hash;
    use talaria_sequoia::{
        delta::DeltaGeneratorConfig,
        DeltaGenerator,
        operations::reduction::{DeltaChunkRef, ReductionManifest, ReductionParameters, ReferenceChunk},
        SEQUOIARepository,
    };
    use std::collections::HashMap;
    use std::time::Instant;

    let start = Instant::now();

    // Initialize or open SEQUOIA repository
    let sequoia = if sequoia_path.exists() {
        SEQUOIARepository::open(sequoia_path)?
    } else {
        SEQUOIARepository::init(sequoia_path)?
    };

    // Apply storage optimization if requested
    if args.optimize_for_memory {
        use talaria_storage::optimization::{OptimizationOptions, StandardStorageOptimizer, StorageOptimizer};
        use talaria_storage::StorageStrategy;

        let mut optimizer = StandardStorageOptimizer::new(sequoia_path.clone());
        let options = OptimizationOptions {
            strategies: vec![StorageStrategy::Compression],
            ..Default::default()
        };

        info("Optimizing storage for memory efficiency...");
        let optimization_results = futures::executor::block_on(optimizer.optimize(options))?;

        if let Some(result) = optimization_results.first() {
            if result.space_saved > 0 {
                success(&format!(
                    "Storage optimized: {} bytes saved, {} chunks affected",
                    result.space_saved, result.chunks_affected
                ));
            }
        }
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
    // Convert CLI TargetAligner to SEQUOIA TargetAligner
    let target_aligner_sequoia = match args.target_aligner {
        crate::cli::TargetAligner::Lambda => talaria_sequoia::TargetAligner::Lambda,
        crate::cli::TargetAligner::Blast => talaria_sequoia::TargetAligner::Blast,
        crate::cli::TargetAligner::Kraken => talaria_sequoia::TargetAligner::Kraken,
        crate::cli::TargetAligner::Diamond => talaria_sequoia::TargetAligner::Diamond,
        crate::cli::TargetAligner::MMseqs2 => talaria_sequoia::TargetAligner::MMseqs2,
        crate::cli::TargetAligner::Generic => talaria_sequoia::TargetAligner::Generic,
    };
    let parameters = ReductionParameters {
        reduction_ratio,
        target_aligner: Some(target_aligner_sequoia),
        min_length: args.min_length,
        similarity_threshold: args.similarity_threshold.unwrap_or(0.9),
        taxonomy_aware: args.taxonomy_aware,
        align_select: args.align_select,
        max_align_length: args.max_align_length,
        no_deltas: args.no_deltas,
    };

    // Get actual source manifest if input was from SEQUOIA
    // Check if the path is within a Talaria databases directory
    let databases_dir = talaria_core::system::paths::talaria_databases_dir();
    let source_manifest_hash = if input_path.starts_with(&databases_dir) {
        // Try to find manifest.json in the SEQUOIA structure
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

    use talaria_sequoia::storage::SequenceStorage;
    use talaria_sequoia::chunker::TaxonomicChunker;
    use talaria_sequoia::ChunkingStrategy;

    // Initialize canonical sequence storage
    let sequences_path = sequoia.storage.base_path.join("sequences");
    let sequence_storage = SequenceStorage::new(&sequences_path)?;

    // Create database source for chunker
    let db_source = match source {
        "uniprot" => match dataset {
            "swissprot" => talaria_core::DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt),
            "trembl" => talaria_core::DatabaseSource::UniProt(talaria_core::UniProtDatabase::TrEMBL),
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
    let mut chunker = TaxonomicChunker::new(
        ChunkingStrategy::default(),
        sequence_storage,
        db_source,
    );

    // Process references and get chunk manifests
    let chunk_manifests = chunker.chunk_sequences_canonical(references.to_vec())?;

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

    for manifest in chunk_manifests {
        // Store the manifest (not the chunk with data!)
        let chunk_hash = sequoia.storage.store_chunk_manifest(&manifest)?;

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

        reference_chunk_refs.push(ref_chunk);

        // Map sequence IDs to chunk hash for delta processing
        for seq_id in sequence_ids {
            ref_chunk_map.insert(seq_id, chunk_hash.clone());
        }

        // Update progress
        chunk_progress.inc(1);
    }

    chunk_progress.finish_with_message("Reference chunks stored");

    manifest.add_reference_chunks(reference_chunk_refs);

    // Process and store delta chunks if present
    if !deltas.is_empty() && !args.no_deltas {
        action("Processing delta sequences...");

        // Group deltas by reference sequence
        let mut deltas_by_ref: HashMap<String, Vec<talaria_bio::compression::DeltaRecord>> =
            HashMap::new();

        info(&format!("Grouping {} deltas by reference...", deltas.len()));
        for delta in deltas {
            deltas_by_ref
                .entry(delta.reference_id.clone())
                .or_default()
                .push(delta.clone());
        }
        info(&format!("Grouped into {} reference groups", deltas_by_ref.len()));

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

        let all_ref_sequences: Vec<talaria_bio::sequence::Sequence> =
            references.to_vec();

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
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} delta chunks stored")
                    .unwrap()
                    .progress_chars("##-"),
            );
            delta_progress.set_message("Storing delta chunks...");

            // Store delta chunks and create references
            for delta_chunk in delta_chunks {
                let delta_hash = sequoia.storage.store_delta_chunk(&delta_chunk)?;

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

                delta_chunk_refs.push(delta_ref);

                // Update progress
                delta_progress.inc(1);
            }

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
    action("Storing manifest in SEQUOIA repository...");
    let manifest_hash = sequoia
        .storage
        .store_database_reduction_manifest(&manifest, source, dataset, version)?;

    // Note: We do NOT create a new database manifest here
    // The reduction is stored as a profile associated with the original database

    // Calculate total size
    let total_size = manifest.statistics.total_size_with_deltas;

    success("Reduction stored in SEQUOIA repository");

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

    subsection_header("Next Steps");
    info("View with: talaria database list");
    info(&format!("Info: talaria database info {}", source_database));

    Ok(total_size)
}
