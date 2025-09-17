use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use crate::cli::TargetAligner;
use crate::cli::formatter::{self, TaskList, TaskStatus, info_box, print_success, print_error, print_tip, format_bytes};
use crate::cli::output::*;
use crate::utils::casg_workspace::CasgWorkspaceManager;
use std::sync::{Arc, Mutex};

#[derive(Args, Debug)]
pub struct ReduceArgs {
    /// Database to reduce (e.g., "uniprot/swissprot", "custom/taxids_9606")
    /// Must be a database that exists in the CASG repository
    #[arg(value_name = "DATABASE")]
    pub database: String,

    /// Output reduced FASTA file (optional - stores in CASG by default)
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

    /// Use taxonomy data to weight alignment scores (requires taxonomy data in FASTA or CASG)
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

    /// Use all-vs-all alignment mode for Lambda (default: query-vs-reference)
    #[arg(long)]
    pub all_vs_all: bool,

    /// Selection algorithm to use for choosing reference sequences
    /// Options: single-pass (default, O(n)), similarity-matrix (O(n²) but potentially more optimal)
    #[arg(long, default_value = "single-pass", value_name = "ALGORITHM")]
    pub selection_algorithm: String,

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

    /// Output to CASG repository instead of files
    #[arg(long)]
    pub casg_output: bool,

    /// CASG repository path (default: ${TALARIA_HOME}/databases)
    #[arg(long, value_name = "PATH")]
    pub casg_path: Option<PathBuf>,

    /// Skip visualization charts in output
    #[arg(long)]
    pub no_visualize: bool,


    /// Generate HTML report with visualization
    #[arg(long)]
    pub html_report: Option<PathBuf>,
}

/// Parse the selection algorithm string into the enum
fn parse_selection_algorithm(algorithm: &str) -> anyhow::Result<crate::core::reference_selector::SelectionAlgorithm> {
    use crate::core::reference_selector::SelectionAlgorithm;

    match algorithm.to_lowercase().as_str() {
        "single-pass" | "singlepass" | "single_pass" => Ok(SelectionAlgorithm::SinglePass),
        "similarity-matrix" | "similarity_matrix" | "matrix" => Ok(SelectionAlgorithm::SimilarityMatrix),
        "hybrid" => Ok(SelectionAlgorithm::Hybrid),
        _ => anyhow::bail!("Invalid selection algorithm: '{}'. Options: single-pass, similarity-matrix", algorithm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reference_selector::SelectionAlgorithm;

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
}

pub fn run(mut args: ReduceArgs) -> anyhow::Result<()> {
    use crate::utils::format::get_file_size;

    // Initialize formatter
    formatter::init();

    // Initialize CASG workspace manager
    let mut casg_manager = CasgWorkspaceManager::new()?;

    // Create workspace for this reduction operation
    let command = format!("reduce {:?}", &args);
    let workspace = Arc::new(Mutex::new(casg_manager.create_workspace(&command)?));

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
    use crate::core::database_manager::DatabaseManager;
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

    // Check if database exists
    let databases = db_manager.list_databases()?;
    let db_full_name = format!("{}/{}", source, dataset);
    if !databases.iter().any(|db| db.name == db_full_name) {
        anyhow::bail!("Database '{}' not found. Use 'talaria database list' to see available databases.", db_full_name);
    }
    // Assemble FASTA from CASG chunks
    let temp_file = workspace.lock().unwrap().get_file_path("input_fasta", "fasta");

    // Map database to internal source enum if it's a standard database
    use crate::download::{DatabaseSource, NCBIDatabase, UniProtDatabase};
    let database_source = match db_full_name.as_str() {
        "uniprot/swissprot" => Some(DatabaseSource::UniProt(UniProtDatabase::SwissProt)),
        "uniprot/trembl" => Some(DatabaseSource::UniProt(UniProtDatabase::TrEMBL)),
        "ncbi/nr" => Some(DatabaseSource::NCBI(NCBIDatabase::NR)),
        "ncbi/nt" => Some(DatabaseSource::NCBI(NCBIDatabase::NT)),
        _ => None,  // Custom database
    };

    // Use spinner for assembly
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
    );
    spinner.set_message("Assembling database from CASG chunks...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    // Assemble the database from CASG
    if let Some(db_source) = &database_source {
        db_manager.assemble_database(db_source, &temp_file)?;
    } else {
        // For custom databases, assemble from chunks referenced in manifest
        use crate::casg::assembler::FastaAssembler;
        use crate::casg::types::TemporalManifest;
        use crate::core::paths;
        use std::fs;

        // Find the manifest
        let db_path = paths::talaria_databases_dir().join(&source).join(&dataset);
        let mut manifest_path = db_path.join("manifest.json");

        if !manifest_path.exists() {
            // Try the manifests directory
            manifest_path = paths::talaria_databases_dir()
                .join("manifests")
                .join(format!("{}-{}.json", source, dataset));
            if !manifest_path.exists() {
                anyhow::bail!("Cannot find manifest for database: {}", db_full_name);
            }
        }

        // Load the manifest
        let manifest_content = fs::read_to_string(&manifest_path)?;
        let manifest: TemporalManifest = serde_json::from_str(&manifest_content)?;

        // Assemble from the chunks referenced in the manifest
        let assembler = FastaAssembler::new(db_manager.get_storage());
        let chunk_hashes: Vec<_> = manifest.chunk_index.iter().map(|c| c.hash.clone()).collect();
        let sequences = assembler.assemble_from_chunks(&chunk_hashes)?;

        // Write to temp file
        crate::bio::fasta::write_fasta(&temp_file, &sequences)?;
    }

    spinner.finish_with_message("Database assembled successfully");

    // Create accession2taxid mapping from manifest if using LAMBDA
    let manifest_acc2taxid = if args.target_aligner == TargetAligner::Lambda && database_source.is_some() {
        spinner.set_message("Creating taxonomy mapping from manifest...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        match db_manager.create_accession2taxid_from_manifest(database_source.as_ref().unwrap()) {
            Ok(path) => {
                spinner.finish_with_message("Taxonomy mapping created from manifest");
                Some(path)
            }
            Err(e) => {
                spinner.finish_with_message("Warning: Could not create taxonomy mapping from manifest");
                warning(&e.to_string());
                None
            }
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
        0.0  // Sentinel value for auto-detection
    };

    // Create task list for tracking reduction pipeline
    let mut task_list = TaskList::new();

    // Print header
    let header = format!("Reduction Pipeline: {}", db_full_name);
    task_list.print_header(&header);

    // Show reduction mode info
    if reduction_ratio == 0.0 {
        info_box("Using LAMBDA for intelligent auto-detection", &[
            "Alignment-based selection",
            "Taxonomy-aware clustering",
            "Dynamic coverage optimization"
        ]);
    } else {
        info_box(&format!("Fixed reduction to {:.0}% of original", reduction_ratio * 100.0), &[
            "Greedy selection by sequence length",
            "Predictable output size"
        ]);
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
    if let Err(_) = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global() {
        // Thread pool already initialized, that's fine
    }

    task_list.set_task_message(init_task, &format!("Using {} threads", threads));
    task_list.update_task(init_task, TaskStatus::Complete);
    
    // Load configuration if provided
    let mut config = if let Some(config_path) = &args.config {
        task_list.set_task_message(init_task, &format!("Loading configuration from {:?}...", config_path));
        crate::core::config::load_config(config_path)?
    } else {
        crate::core::config::default_config()
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
    let sequences = crate::bio::fasta::parse_fasta(&actual_input)?;

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

    task_list.set_task_message(load_task, &format!("Loaded {} sequences ({})",
        sequences.len(),
        format_bytes(input_size)));
    task_list.update_task(load_task, TaskStatus::Complete);
    
    // Run reduction pipeline with workspace
    task_list.update_task(select_task, TaskStatus::InProgress);
    let mut reducer = crate::core::reducer::Reducer::new(config)
        .with_selection_mode(
            args.similarity_threshold.is_some() || args.align_select,
            args.align_select
        )
        .with_no_deltas(args.no_deltas)
        .with_max_align_length(args.max_align_length)
        .with_all_vs_all(args.all_vs_all)
        .with_taxonomy_weights(args.use_taxonomy_weights)
        .with_manifest_acc2taxid(manifest_acc2taxid)
        .with_batch_settings(args.batch, args.batch_size)
        .with_selection_algorithm(parse_selection_algorithm(&args.selection_algorithm)?)
        .with_file_sizes(input_size, 0)
        .with_workspace(workspace.clone());  // Pass workspace to reducer

    // Run reduction with better error handling
    let reduction_result = reducer.reduce(
        sequences,
        reduction_ratio,
        args.target_aligner.clone(),
    );

    let (references, deltas, original_count) = match reduction_result {
        Ok(result) => {
            task_list.set_task_message(select_task, &format!("Selected {} reference sequences", result.0.len()));
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
            info(&format!("To inspect: talaria tools workspace inspect {}", ws_id));

            return Err(e.into());
        }
    };

    // Update delta encoding status and workspace stats
    if args.no_deltas {
        task_list.update_task(encode_task, TaskStatus::Skipped);
    } else if !deltas.is_empty() {
        task_list.set_task_message(encode_task, &format!("Encoded {} child sequences as deltas", deltas.len()));
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
    let use_casg_storage = args.output.is_none();

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
        // CASG storage mode - these are placeholder paths as actual storage goes to CASG repository
        (PathBuf::from("casg_storage"), PathBuf::from("casg_storage.deltas"))
    };
    
    // Choose output method: CASG storage (default) or traditional files
    let output_size = if use_casg_storage || args.casg_output {
        // Output to CASG repository
        task_list.set_task_message(write_task, "Storing reduction in CASG repository...");

        let casg_path = args.casg_path.clone().unwrap_or_else(|| {
            use crate::core::paths;
            paths::talaria_databases_dir()
        });

        store_reduction_in_casg(
            &casg_path,
            &actual_input,
            &references,
            &deltas,
            &args,
            reduction_ratio,
            original_count,
            input_size,
            Some(&db_full_name),  // Use original database name, not a new one
        )?
    } else {
        // Traditional file output
        task_list.update_task(write_task, TaskStatus::InProgress);
        task_list.set_task_message(write_task, "Writing output files...");

        crate::bio::fasta::write_fasta(&output_path, &references)?;

        // Get output file size
        let output_size = get_file_size(&output_path).unwrap_or(0);

        // Write deltas if they were computed
        if !args.no_deltas && !deltas.is_empty() {
            crate::storage::metadata::write_metadata(&metadata_path, &deltas)?;
            task_list.set_task_message(write_task, &format!("Saved deltas to {:?}", metadata_path));
        }

        task_list.update_task(write_task, TaskStatus::Complete);

        output_size
    };
    
    // Print statistics using the new stats display
    use crate::cli::stats_display::create_reduction_stats;
    use crate::cli::charts::{create_reduction_summary_chart, create_length_histogram};

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
            coverage
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
    let sequence_coverage = (references.len() + deltas.len()) as f64 / original_count as f64 * 100.0;

    // Generate HTML report if requested
    if let Some(html_path) = &args.html_report {
        task_list.set_task_message(write_task, "Generating HTML report...");

        // Create selection result for report
        let selection_result = crate::core::reference_selector::SelectionResult {
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
        let html_content = crate::report::reduction_html::generate_reduction_html_report(
            &actual_input,
            &output_path,
            &original_sequences,
            &selection_result,
            sequence_coverage,
            None, // No taxonomic stats for now - could be added later
        )?;

        // Write HTML report to file
        std::fs::write(&html_path, html_content)?;
        task_list.set_task_message(write_task, &format!("✓ HTML report saved to: {}", html_path.display()));
    }

    print_success(&format!("Reduction complete: {:.1}% file size reduction, {:.1}% sequence coverage",
        file_size_reduction,
        sequence_coverage
    ));

    if !args.no_deltas && !deltas.is_empty() {
        print_tip("Use 'talaria reconstruct' to recover original sequences from the reduced set and deltas");
    }

    // Mark workspace as completed successfully
    workspace.lock().unwrap().mark_completed()?;

    // Log operation to CASG
    casg_manager.log_operation("reduce", &format!("Completed: {} sequences -> {} references", original_count, references.len()))?;

    Ok(())
}

/// Store reduction results in CASG repository
fn store_reduction_in_casg(
    casg_path: &PathBuf,
    input_path: &PathBuf,
    references: &[crate::bio::sequence::Sequence],
    deltas: &[crate::core::delta_encoder::DeltaRecord],
    args: &ReduceArgs,
    reduction_ratio: f64,
    original_count: usize,
    input_size: u64,
    database_name: Option<&str>,
) -> anyhow::Result<u64> {
    use crate::casg::{CASGRepository, delta_generator::{DeltaGenerator, DeltaGeneratorConfig}, reduction::{ReductionManifest, ReductionParameters, ReferenceChunk, DeltaChunkRef}};
    use crate::casg::chunker::TaxonomicChunker;
    use crate::casg::types::SHA256Hash;
    use std::collections::HashMap;
    use std::time::Instant;

    let start = Instant::now();

    // Initialize or open CASG repository
    let casg = if casg_path.exists() {
        CASGRepository::open(casg_path)?
    } else {
        CASGRepository::init(casg_path)?
    };

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
        input_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    };

    // Create reduction parameters
    let parameters = ReductionParameters {
        reduction_ratio,
        target_aligner: Some(args.target_aligner.clone()),
        min_length: args.min_length,
        similarity_threshold: args.similarity_threshold.unwrap_or(0.9),
        taxonomy_aware: args.taxonomy_aware,
        align_select: args.align_select,
        max_align_length: args.max_align_length,
        no_deltas: args.no_deltas,
    };

    // Get actual source manifest if input was from CASG
    // Check if the path is within a Talaria databases directory
    let databases_dir = crate::core::paths::talaria_databases_dir();
    let source_manifest_hash = if input_path.starts_with(&databases_dir) {
        // Try to find manifest.json in the CASG structure
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

    // Chunk and store reference sequences
    action("Chunking reference sequences...");
    use crate::casg::types::ChunkingStrategy;
    let strategy = ChunkingStrategy {
        target_chunk_size: 1024 * 1024,  // 1MB
        max_chunk_size: 10 * 1024 * 1024,  // 10MB
        min_sequences_per_chunk: 1,
        taxonomic_coherence: 0.8,
        special_taxa: Vec::new(),
    };
    let chunker = TaxonomicChunker::new(strategy);
    let ref_chunks = chunker.chunk_sequences(references.to_vec())?;

    let mut reference_chunk_refs = Vec::new();
    let mut ref_chunk_map = HashMap::new();

    for chunk in ref_chunks {
        let chunk_hash = casg.storage.store_taxonomy_chunk(&chunk)?;

        // Create reference chunk metadata
        let ref_chunk = ReferenceChunk {
            chunk_hash: chunk_hash.clone(),
            sequence_ids: chunk.sequences.iter().map(|s| s.sequence_id.clone()).collect(),
            sequence_count: chunk.sequences.len(),
            size: chunk.size,
            compressed_size: chunk.compressed_size,
            taxon_ids: chunk.taxon_ids.clone(),
        };

        reference_chunk_refs.push(ref_chunk);

        // Map sequence IDs to chunk hash for delta processing
        for seq_ref in &chunk.sequences {
            ref_chunk_map.insert(seq_ref.sequence_id.clone(), chunk_hash.clone());
        }
    }

    manifest.add_reference_chunks(reference_chunk_refs);

    // Process and store delta chunks if present
    if !deltas.is_empty() && !args.no_deltas {
        action("Storing delta chunks...");

        // Group deltas by reference sequence
        let mut deltas_by_ref: HashMap<String, Vec<crate::core::delta_encoder::DeltaRecord>> = HashMap::new();
        for delta in deltas {
            deltas_by_ref
                .entry(delta.reference_id.clone())
                .or_insert_with(Vec::new)
                .push(delta.clone());
        }

        let mut delta_chunk_refs = Vec::new();

        // Create delta generator
        let delta_config = DeltaGeneratorConfig {
            max_chunk_size: 16 * 1024 * 1024,
            min_similarity_threshold: 0.85,
            enable_compression: true,
            target_sequences_per_chunk: 1000,
            max_delta_ops_threshold: 100,
        };
        let mut delta_generator = DeltaGenerator::new(delta_config);

        // Convert delta records to sequences for delta generation
        let all_child_sequences: Vec<crate::bio::sequence::Sequence> =
            deltas.iter().map(|d| crate::bio::sequence::Sequence {
                id: d.child_id.clone(),
                description: None,
                sequence: Vec::new(), // Will be filled by delta generator
                taxon_id: d.taxon_id,
            }).collect();

        let all_ref_sequences: Vec<crate::bio::sequence::Sequence> =
            references.iter().cloned().collect();

        // Generate delta chunks using the new system
        if !all_child_sequences.is_empty() && !all_ref_sequences.is_empty() {
            // Get the first reference chunk hash as the base
            let base_ref_hash = ref_chunk_map.values().next()
                .ok_or_else(|| anyhow::anyhow!("No reference chunks available"))?;

            let delta_chunks = delta_generator.generate_delta_chunks(
                &all_child_sequences,
                &all_ref_sequences,
                base_ref_hash.clone(),
            )?;

            // Store delta chunks and create references
            for delta_chunk in delta_chunks {
                let delta_hash = casg.storage.store_delta_chunk(&delta_chunk)?;

                let delta_ref = DeltaChunkRef {
                    chunk_hash: delta_hash,
                    reference_chunk_hash: delta_chunk.reference_hash.clone(),
                    child_count: delta_chunk.sequences.len(),
                    child_ids: delta_chunk.sequences.iter().map(|s| s.sequence_id.clone()).collect(),
                    size: delta_chunk.compressed_size,
                    avg_delta_ops: delta_chunk.deltas.len() as f32 / delta_chunk.sequences.len().max(1) as f32,
                };

                delta_chunk_refs.push(delta_ref);
            }
        }

        manifest.add_delta_chunks(delta_chunk_refs);
    }

    // Compute Merkle roots
    manifest.compute_merkle_roots()?;

    // Calculate statistics
    let elapsed = start.elapsed().as_secs();
    manifest.calculate_statistics(original_count, input_size, elapsed);

    // Store the reduction manifest as a profile
    // This will automatically create the profile reference in /profiles/ directory
    let manifest_hash = casg.storage.store_reduction_manifest(&manifest)?;

    // Note: We do NOT create a new database manifest here
    // The reduction is stored as a profile associated with the original database

    // Calculate total size
    let total_size = manifest.statistics.total_size_with_deltas;

    success("Reduction stored in CASG repository");

    let mut details = vec![
        ("Database", source_database.clone()),
        ("Profile", profile_name.clone()),
        ("Manifest", manifest_hash.to_string()),
        ("References", format!("{} chunks", format_number(manifest.reference_chunks.len()))),
    ];
    if !args.no_deltas {
        details.push(("Deltas", format!("{} chunks", format_number(manifest.delta_chunks.len()))));
    }
    details.push(("Merkle root", manifest.reduction_merkle_root.to_string()));
    details.push(("Deduplication", format!("{:.1}%", manifest.statistics.deduplication_ratio * 100.0)));

    tree_section("Storage Summary", details, false);

    subsection_header("Next Steps");
    info(&format!("View with: talaria database list"));
    info(&format!("Info: talaria database info {}", source_database));

    Ok(total_size)
}


