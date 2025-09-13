use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct ValidateArgs {
    /// Database reduction to validate (e.g., "uniprot/swissprot:blast-30")
    /// When specified, automatically finds original, reduced, and delta files
    #[arg(value_name = "DATABASE:PROFILE")]
    pub database: Option<String>,
    
    /// Original FASTA file (required if database not specified)
    #[arg(short = 'o', long, value_name = "FILE")]
    pub original: Option<PathBuf>,
    
    /// Reduced FASTA file (required if database not specified)
    #[arg(short = 'r', long, value_name = "FILE")]
    pub reduced: Option<PathBuf>,
    
    /// Delta metadata file (required if database not specified)
    #[arg(short = 'd', long, value_name = "FILE")]
    pub deltas: Option<PathBuf>,
    
    /// Alignment results from original (optional)
    #[arg(long)]
    pub original_results: Option<PathBuf>,
    
    /// Alignment results from reduced (optional)
    #[arg(long)]
    pub reduced_results: Option<PathBuf>,
    
    /// Output validation report
    #[arg(long)]
    pub report: Option<PathBuf>,
}

pub fn run(args: ValidateArgs) -> anyhow::Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};
    use crate::utils::format::{format_bytes, get_file_size};
    use crate::core::database_manager::DatabaseManager;
    use crate::core::config::load_config;
    
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
    );
    
    // Validate arguments: either database or all file paths must be specified
    if args.database.is_none() && (args.original.is_none() || args.reduced.is_none() || args.deltas.is_none()) {
        anyhow::bail!("Must specify either a database reference or all three files (-o, -r, -d)");
    }
    
    if args.database.is_some() && (args.original.is_some() || args.reduced.is_some() || args.deltas.is_some()) {
        anyhow::bail!("Cannot specify both database reference and file paths. Use one or the other.");
    }
    
    // Resolve file paths based on input method
    let (original_path, reduced_path, deltas_path) = if let Some(db_ref_str) = &args.database {
        // Parse database reference with reduction profile
        let (base_ref, profile) = parse_database_with_profile(db_ref_str)?;
        
        // Profile is required for validation
        let profile = profile.ok_or_else(|| anyhow::anyhow!(
            "Reduction profile required for validation. Use format: 'database:profile' (e.g., 'uniprot/swissprot:blast-30')"
        ))?;
        
        // Load config and database manager
        let config = load_config("talaria.toml").unwrap_or_default();
        let db_manager = DatabaseManager::new(config.database.database_dir)?;
        
        // Parse and resolve the database reference
        let db_ref = db_manager.parse_reference(&base_ref)?;
        let db_dir = db_manager.resolve_reference(&db_ref)?;
        
        // Find original FASTA in main directory
        let original = db_manager.find_fasta_in_dir(&db_dir)?;
        
        // Find reduced FASTA and deltas in reduced subdirectory
        let reduced_dir = db_dir.join("reduced").join(&profile);
        if !reduced_dir.exists() {
            anyhow::bail!("Reduction profile '{}' not found for {}/{}", 
                          profile, db_ref.source, db_ref.dataset);
        }
        
        let reduced = db_manager.find_fasta_in_dir(&reduced_dir)?;
        
        // Find delta file (look for .deltas.tal or .deltas extension)
        let deltas = find_delta_file(&reduced_dir)?;
        
        pb.set_message(format!("Validating {}/{}:{}", db_ref.source, db_ref.dataset, profile));
        
        (original, reduced, deltas)
    } else {
        // Traditional file-based usage
        let original = args.original.ok_or_else(|| anyhow::anyhow!("Original file (-o) is required"))?;
        let reduced = args.reduced.ok_or_else(|| anyhow::anyhow!("Reduced file (-r) is required"))?;
        let deltas = args.deltas.ok_or_else(|| anyhow::anyhow!("Delta file (-d) is required"))?;
        
        if !original.exists() {
            anyhow::bail!("Original file does not exist: {:?}", original);
        }
        if !reduced.exists() {
            anyhow::bail!("Reduced file does not exist: {:?}", reduced);
        }
        if !deltas.exists() {
            anyhow::bail!("Delta file does not exist: {:?}", deltas);
        }
        
        (original, reduced, deltas)
    };
    
    // Get file sizes
    let original_size = get_file_size(&original_path).unwrap_or(0);
    let reduced_size = get_file_size(&reduced_path).unwrap_or(0);
    
    // Load sequences
    pb.set_message("Loading original FASTA file...");
    let original_seqs = crate::bio::fasta::parse_fasta(&original_path)?;
    pb.set_message(format!("Loaded {} original sequences ({})", original_seqs.len(), format_bytes(original_size)));
    
    pb.set_message("Loading reduced FASTA file...");
    let reduced_seqs = crate::bio::fasta::parse_fasta(&reduced_path)?;
    pb.set_message(format!("Loaded {} reference sequences ({})", reduced_seqs.len(), format_bytes(reduced_size)));
    
    pb.set_message("Loading delta metadata...");
    let deltas = crate::storage::metadata::load_metadata(&deltas_path)?;
    pb.set_message(format!("Loaded {} delta records", deltas.len()));
    
    // Calculate coverage metrics
    pb.set_message("Calculating validation metrics...");
    let validator = crate::core::validator::Validator::new();
    let metrics = validator.calculate_metrics(&original_seqs, &reduced_seqs, &deltas, original_size, reduced_size)?;
    
    // Compare alignment results if provided
    if let (Some(orig_results), Some(red_results)) = (args.original_results, args.reduced_results) {
        pb.set_message("Comparing alignment results...");
        let alignment_metrics = validator.compare_alignments(&orig_results, &red_results)?;
        println!("\nAlignment similarity: {:.2}%", alignment_metrics.similarity * 100.0);
    }
    
    pb.finish_and_clear();
    
    // Print results
    use crate::cli::stats_display::create_validation_stats;
    
    let stats = create_validation_stats(
        metrics.total_sequences,
        metrics.reference_count,
        metrics.child_count,
        metrics.covered_sequences,
        metrics.sequence_coverage,
        metrics.covered_taxa,
        metrics.total_taxa,
        metrics.taxonomic_coverage,
        metrics.original_file_size,
        metrics.reduced_file_size,
        metrics.avg_delta_size,
    );
    
    println!("\n{}", stats);
    
    if let Some(report_path) = args.report {
        let report = serde_json::to_string_pretty(&metrics)?;
        std::fs::write(&report_path, report)?;
        println!("\nDetailed report saved to {:?}", report_path);
    }
    
    Ok(())
}

/// Parse a database reference that must include a reduction profile
/// Format: "source/dataset[:profile][@version]"
/// Returns: (base_reference, Option<profile>)
fn parse_database_with_profile(reference: &str) -> anyhow::Result<(String, Option<String>)> {
    // Check for reduction profile (colon separator)
    if let Some(colon_idx) = reference.find(':') {
        // Split at colon
        let base = &reference[..colon_idx];
        let remainder = &reference[colon_idx + 1..];
        
        // Check if remainder has version (@) - not expected for validate but handle it
        if let Some(at_idx) = remainder.find('@') {
            // Format: source/dataset:profile@version (unusual for validate)
            let profile = &remainder[..at_idx];
            let version = &remainder[at_idx..];
            Ok((format!("{}{}", base, version), Some(profile.to_string())))
        } else {
            // Format: source/dataset:profile
            Ok((base.to_string(), Some(remainder.to_string())))
        }
    } else {
        // No reduction specified - return None for profile
        Ok((reference.to_string(), None))
    }
}

/// Find a delta file in a directory
fn find_delta_file(dir: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    use std::fs;
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                // Look for .deltas.tal or .deltas or .delta extensions
                if name.contains(".deltas.") || name.ends_with(".deltas") || name.ends_with(".delta") {
                    return Ok(path);
                }
            }
        }
    }
    
    anyhow::bail!("No delta file found in directory: {}", dir.display())
}