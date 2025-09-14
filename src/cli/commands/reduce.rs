use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use crate::cli::TargetAligner;

#[derive(Args)]
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

    /// Skip delta encoding (much faster, but no reconstruction possible)
    #[arg(long)]
    pub no_deltas: bool,

    /// Use all-vs-all alignment mode for Lambda (default: query-vs-reference)
    #[arg(long)]
    pub all_vs_all: bool,
    
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
}

pub fn run(mut args: ReduceArgs) -> anyhow::Result<()> {
    use crate::utils::format::get_file_size;
    use crate::core::database_manager::DatabaseManager;
    use crate::core::config::load_config;
    
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
    
    // Handle database reference
    let (actual_input, should_store, db_reference) = if let Some(db_ref_str) = &args.database {
        // Parse database reference with potential reduction profile
        let (base_ref, reduction_profile) = parse_database_with_reduction(db_ref_str)?;
        
        // Load config and database manager
        let config = load_config("talaria.toml").unwrap_or_default();
        let db_manager = DatabaseManager::new(config.database.database_dir)?;
        
        // Parse the base reference
        let db_ref = db_manager.parse_reference(&base_ref)?;
        
        // Resolve to directory
        let db_dir = db_manager.resolve_reference(&db_ref)?;
        
        // If reduction profile specified, look in reduced subdirectory
        let search_dir = if let Some(profile) = reduction_profile {
            let reduced_dir = db_dir.join("reduced").join(&profile);
            if !reduced_dir.exists() {
                anyhow::bail!("Reduction profile '{}' not found for {}/{}", 
                              profile, db_ref.source, db_ref.dataset);
            }
            reduced_dir
        } else {
            db_dir
        };
        
        // Find FASTA file in directory
        let fasta_path = db_manager.find_fasta_in_dir(&search_dir)?;
        
        (fasta_path, true, Some(db_ref))
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
        
        // Try to infer database reference if --store is used
        let db_ref = if args.store {
            infer_database_from_path(input)?
        } else {
            None
        };
        
        (input.clone(), args.store, db_ref)
    };
    
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
    );
    pb.set_message("Initializing reduction pipeline...");
    
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
    
    pb.set_message(format!("Using {} threads", threads));
    
    // Load configuration if provided
    let mut config = if let Some(config_path) = args.config {
        pb.set_message("Loading configuration...");
        crate::core::config::load_config(&config_path)?
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
    
    // Parse input FASTA
    pb.set_message("Parsing input FASTA...");
    let sequences = crate::bio::fasta::parse_fasta(&actual_input)?;
    pb.finish_with_message(format!("Loaded {} sequences", sequences.len()));
    
    // Run reduction pipeline
    let reducer = crate::core::reducer::Reducer::new(config)
        .with_selection_mode(
            args.similarity_threshold.is_some() || args.align_select,
            args.align_select
        )
        .with_no_deltas(args.no_deltas)
        .with_max_align_length(args.max_align_length)
        .with_all_vs_all(args.all_vs_all)
        .with_file_sizes(input_size, 0);  // Output size will be set later
    let (references, deltas, original_count) = reducer.reduce(
        sequences,
        reduction_ratio,
        args.target_aligner.clone(),
    )?;
    
    // Determine output paths based on whether we're storing
    let (output_path, metadata_path) = if should_store {
        // Store in database structure
        let db_ref = db_reference.as_ref().ok_or_else(|| anyhow::anyhow!("Database reference required for storage"))?;
        
        // Load config and database manager if not already loaded
        let config = load_config("talaria.toml").unwrap_or_default();
        let db_manager = DatabaseManager::new(config.database.database_dir)?;
        
        // Determine profile name
        let profile_name = args.profile.clone().unwrap_or_else(|| {
            if reduction_ratio == 0.0 {
                // Auto-detection mode
                "auto-detect".to_string()
            } else {
                format!("{}-percent", (reduction_ratio * 100.0) as u32)
            }
        });
        
        // Get the version directory for this database
        let db_versions = db_manager.list_versions(&db_ref.source, &db_ref.dataset)?;
        let current_version = db_versions.iter()
            .find(|v| v.is_current)
            .or_else(|| db_versions.first())
            .ok_or_else(|| anyhow::anyhow!("No versions found for {}/{}", db_ref.source, db_ref.dataset))?;
        
        // Create reduced subdirectory
        let reduced_dir = current_version.path.join("reduced").join(&profile_name);
        std::fs::create_dir_all(&reduced_dir)?;
        
        let output_file = reduced_dir.join(format!("{}.fasta", db_ref.dataset));
        let delta_file = reduced_dir.join(format!("{}.deltas.tal", db_ref.dataset));
        
        pb.set_message(format!("Storing as {}/{}/reduced/{}", db_ref.source, db_ref.dataset, profile_name));
        
        (output_file, delta_file)
    } else {
        // Use specified output paths
        let output = args.output.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Output file (-o) is required when not storing in database"))?;
        
        let metadata_path = if let Some(path) = args.metadata {
            path
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
    
    // Write output
    pb.set_message("Writing reduced FASTA...");
    crate::bio::fasta::write_fasta(&output_path, &references)?;
    
    // Get output file size
    let output_size = get_file_size(&output_path).unwrap_or(0);
    
    // Write deltas if they were computed
    if !args.no_deltas && !deltas.is_empty() {
        pb.set_message("Writing delta metadata...");
        crate::storage::metadata::write_metadata(&metadata_path, &deltas)?;
        pb.set_message(format!("Saved deltas to {:?}", metadata_path));
    } else if args.no_deltas {
        pb.set_message("Skipped delta encoding (--no-deltas flag)");
    }
    
    // If storing in database structure, also save reduction metadata
    if should_store {
        use chrono::Utc;
        use serde_json::json;
        
        let source_db = db_reference.as_ref()
            .map(|r| format!("{}/{}", r.source, r.dataset))
            .unwrap_or_else(|| "unknown".to_string());
        
        let reduction_metadata = json!({
            "source_database": source_db,
            "reduction_ratio": if reduction_ratio == 0.0 {
                serde_json::Value::String("auto-detect".to_string())
            } else {
                serde_json::Value::Number(serde_json::Number::from_f64(reduction_ratio).unwrap())
            },
            "auto_detected": reduction_ratio == 0.0,
            "target_aligner": format!("{:?}", &args.target_aligner),
            "original_sequences": original_count,
            "reference_sequences": references.len(),
            "child_sequences": deltas.len(),
            "input_size": input_size,
            "output_size": output_size,
            "reduction_date": Utc::now().to_rfc3339(),
            "parameters": {
                "min_length": args.min_length,
                "similarity_threshold": args.similarity_threshold,
                "taxonomy_aware": args.taxonomy_aware,
                "align_select": args.align_select,
                "no_deltas": args.no_deltas,
                "max_align_length": args.max_align_length,
            }
        });
        
        let metadata_json_path = output_path.parent().unwrap().join("metadata.json");
        std::fs::write(metadata_json_path, serde_json::to_string_pretty(&reduction_metadata)?)?;
    }
    
    pb.finish_and_clear();
    
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
    
    // Show simple completion message
    let file_size_reduction = if input_size > 0 && output_size > 0 {
        (1.0 - (output_size as f64 / input_size as f64)) * 100.0
    } else {
        0.0
    };
    let sequence_coverage = (references.len() + deltas.len()) as f64 / original_count as f64 * 100.0;
    
    println!("\nReduction complete: {:.1}% file size reduction, {:.1}% sequence coverage",
        file_size_reduction,
        sequence_coverage
    );
    
    Ok(())
}

/// Parse a database reference that may include a reduction profile
/// Format: "source/dataset[:reduction][@version]"
/// Returns: (base_reference, Option<reduction_profile>)
fn parse_database_with_reduction(reference: &str) -> anyhow::Result<(String, Option<String>)> {
    // Check for reduction profile (colon separator)
    if let Some(colon_idx) = reference.find(':') {
        // Split at colon
        let base = &reference[..colon_idx];
        let remainder = &reference[colon_idx + 1..];
        
        // Check if remainder has version (@)
        if let Some(at_idx) = remainder.find('@') {
            // Format: source/dataset:reduction@version
            let reduction = &remainder[..at_idx];
            let version = &remainder[at_idx..];
            Ok((format!("{}{}", base, version), Some(reduction.to_string())))
        } else {
            // Format: source/dataset:reduction
            Ok((base.to_string(), Some(remainder.to_string())))
        }
    } else {
        // No reduction specified
        Ok((reference.to_string(), None))
    }
}

/// Try to infer database reference from a file path
fn infer_database_from_path(path: &PathBuf) -> anyhow::Result<Option<crate::core::database_manager::DatabaseReference>> {
    let path_str = path.to_string_lossy();
    
    if path_str.contains("/databases/data/") {
        // Extract source/dataset from path
        let parts: Vec<&str> = path_str.split('/').collect();
        if let Some(idx) = parts.iter().position(|&x| x == "data") {
            if idx + 2 < parts.len() {
                let source = parts[idx + 1].to_string();
                let dataset = parts[idx + 2].to_string();
                return Ok(Some(crate::core::database_manager::DatabaseReference {
                    source,
                    dataset,
                    version: None,
                }));
            }
        }
    }
    
    // Cannot infer - user must specify explicitly
    anyhow::bail!("Cannot infer database from input path. For external files, specify source/dataset explicitly.");
}