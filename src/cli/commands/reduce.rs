use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use crate::cli::TargetAligner;
use crate::cli::formatter::{self, TaskList, TaskStatus, info_box, print_success, print_error, print_tip, format_bytes};
use crate::utils::casg_workspace::CasgWorkspaceManager;
use std::sync::{Arc, Mutex};

#[derive(Args, Debug)]
pub struct ReduceArgs {
    /// Database to reduce (e.g., "uniprot/swissprot", "ncbi/nr@2024-01-01")
    /// When specified, automatically stores result in database structure
    #[arg(value_name = "DATABASE")]
    pub database: Option<String>,
    
    /// Input FASTA file (required if database not specified)
    #[arg(short, long, value_name = "FILE")]
    pub input: Option<PathBuf>,
    
    /// Output reduced FASTA file (required if database not specified and --store not used)
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
    
    // Validate arguments: either database or input must be specified
    if args.database.is_none() && args.input.is_none() {
        anyhow::bail!("Must specify either a database reference or input file (-i)");
    }
    
    if args.database.is_some() && args.input.is_some() {
        anyhow::bail!("Cannot specify both database reference and input file (-i). Use one or the other.");
    }
    
    // Handle database reference and optional manifest-based taxonomy
    let (actual_input, manifest_acc2taxid) = if let Some(db_ref_str) = &args.database {
            // Assemble FASTA from chunks on-demand
            use crate::core::database_manager::DatabaseManager;
            use crate::download::{DatabaseSource, NCBIDatabase, UniProtDatabase};

            let manager = DatabaseManager::new(None)?;

            // Parse database reference to determine source
            let database_source = match db_ref_str.as_str() {
                s if s.starts_with("uniprot/swissprot") => DatabaseSource::UniProt(UniProtDatabase::SwissProt),
                s if s.starts_with("uniprot/trembl") => DatabaseSource::UniProt(UniProtDatabase::TrEMBL),
                s if s.starts_with("ncbi/nr") => DatabaseSource::NCBI(NCBIDatabase::NR),
                s if s.starts_with("ncbi/nt") => DatabaseSource::NCBI(NCBIDatabase::NT),
                _ => anyhow::bail!("Unknown database: {}", db_ref_str),
            };

            // Create temporary file in workspace for assembled FASTA
            let temp_file = workspace.lock().unwrap().get_file_path("input_fasta", "fasta");

            // Use spinner for assembly
            let spinner = ProgressBar::new_spinner();
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.cyan} {msg}")
                    .unwrap()
                    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            );
            spinner.set_message("Assembling database from chunks...");
            spinner.enable_steady_tick(std::time::Duration::from_millis(100));

            manager.assemble_database(&database_source, &temp_file)?;

            spinner.finish_with_message("Database assembled successfully");

            // Create accession2taxid mapping from manifest if using LAMBDA
            let acc2taxid_file = if args.target_aligner == TargetAligner::Lambda {
                spinner.set_message("Creating taxonomy mapping from manifest...");
                spinner.enable_steady_tick(std::time::Duration::from_millis(100));

                match manager.create_accession2taxid_from_manifest(&database_source) {
                    Ok(path) => {
                        spinner.finish_with_message("Taxonomy mapping created from manifest");
                        Some(path)
                    }
                    Err(e) => {
                        spinner.finish_with_message("Warning: Could not create taxonomy mapping from manifest");
                        println!("  {}", e);
                        None
                    }
                }
            } else {
                None
            };

            (temp_file, acc2taxid_file)
    } else {
        // Traditional file-based usage
        let input = args.input.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Input file (-i) is required when not using database reference"))?;

        if !input.exists() {
            anyhow::bail!("Input file does not exist: {:?}", input);
        }

        if !args.store && args.output.is_none() {
            anyhow::bail!("Output file (-o) is required when not using database reference or --store");
        }

        // CASG doesn't support --store yet
        if args.store {
            anyhow::bail!("--store option not yet implemented for CASG. Please specify an output file with -o instead.");
        }

        (input.clone(), None)  // No manifest-based taxonomy for file input
    };

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
    let header = if let Some(db) = &args.database {
        format!("Reduction Pipeline: {}", db)
    } else if let Some(input) = &args.input {
        format!("Reduction Pipeline: {}", input.display())
    } else {
        "Reduction Pipeline".to_string()
    };
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
    
    println!("  Using {} threads", threads);
    task_list.update_task(init_task, TaskStatus::Complete);
    
    // Load configuration if provided
    let mut config = if let Some(config_path) = &args.config {
        println!("  Loading configuration from {:?}...", config_path);
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
    println!("  Reading FASTA file...");
    let sequences = crate::bio::fasta::parse_fasta(&actual_input)?;

    // Update workspace stats
    workspace.lock().unwrap().update_stats(|s| {
        s.input_sequences = sequences.len();
    })?;

    task_list.update_task(load_task, TaskStatus::Complete);
    println!("  Loaded {} sequences ({})",
        sequences.len(),
        format_bytes(input_size));
    
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
            task_list.update_task(select_task, TaskStatus::Complete);
            println!("  Selected {} reference sequences", result.0.len());
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
                print_tip("Try one of these solutions:");
                println!("  1. Use a fixed reduction ratio: -r 0.3");
                println!("  2. Skip auto-detection and use simple selection");
                println!("  3. Ensure your FASTA headers include TaxID tags");
            }

            // Workspace preserved for debugging
            let ws_id = workspace.lock().unwrap().id.clone();
            eprintln!("\nWorkspace preserved for debugging: {}", ws_id);
            eprintln!("To inspect: talaria tools workspace inspect {}", ws_id);

            return Err(e.into());
        }
    };

    // Update delta encoding status and workspace stats
    if args.no_deltas {
        task_list.update_task(encode_task, TaskStatus::Skipped);
    } else if !deltas.is_empty() {
        task_list.update_task(encode_task, TaskStatus::Complete);
        println!("  Encoded {} child sequences as deltas", deltas.len());
    } else {
        task_list.update_task(encode_task, TaskStatus::Skipped);
    }

    // Update workspace stats
    workspace.lock().unwrap().update_stats(|s| {
        s.selected_references = references.len();
        s.final_output_sequences = references.len() + deltas.len();
    })?;
    
    // Determine output paths - should_store is always false now (CASG doesn't support it yet)
    let (output_path, metadata_path) = {
        // Use specified output paths
        let output = args.output.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Output file (-o) is required"))?;
        
        let metadata_path = if let Some(path) = &args.metadata {
            path.clone()
        } else {
            // Auto-generate based on output filename
            let mut delta_path = output.clone();
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
        
        (output.clone(), metadata_path)
    };
    
    // Choose output method: CASG or traditional files
    let output_size = if args.casg_output {
        // Output to CASG repository
        println!("  Storing reduction in CASG repository...");

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
        )?
    } else {
        // Traditional file output
        task_list.update_task(write_task, TaskStatus::InProgress);
        println!("  Writing output files...");

        crate::bio::fasta::write_fasta(&output_path, &references)?;

        // Get output file size
        let output_size = get_file_size(&output_path).unwrap_or(0);

        // Write deltas if they were computed
        if !args.no_deltas && !deltas.is_empty() {
            crate::storage::metadata::write_metadata(&metadata_path, &deltas)?;
            println!("  Saved deltas to {:?}", metadata_path);
        }

        task_list.update_task(write_task, TaskStatus::Complete);

        output_size
    };
    
    // Print statistics using the new stats display
    use crate::cli::stats_display::create_reduction_stats;
    
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
    
    // Show completion message with nice formatting
    let file_size_reduction = if input_size > 0 && output_size > 0 {
        (1.0 - (output_size as f64 / input_size as f64)) * 100.0
    } else {
        0.0
    };
    let sequence_coverage = (references.len() + deltas.len()) as f64 / original_count as f64 * 100.0;

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

    // Get source database info (if available)
    let source_database = input_path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

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
        source_database,
        parameters,
    );

    // Chunk and store reference sequences
    println!("📦 Chunking reference sequences...");
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
        println!("📦 Storing delta chunks...");

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

    // Store the manifest
    let manifest_hash = casg.storage.store_reduction_manifest(&manifest)?;

    // Calculate total size
    let total_size = manifest.statistics.total_size_with_deltas;

    println!("✓ Reduction stored in CASG repository");
    println!("   Profile: {}", profile_name);
    println!("   Manifest: {}", manifest_hash);
    println!("   References: {} chunks", manifest.reference_chunks.len());
    if !args.no_deltas {
        println!("   Deltas: {} chunks", manifest.delta_chunks.len());
    }
    println!("   Merkle root: {}", manifest.reduction_merkle_root);
    println!("   Deduplication ratio: {:.1}%", manifest.statistics.deduplication_ratio * 100.0);

    Ok(total_size)
}


