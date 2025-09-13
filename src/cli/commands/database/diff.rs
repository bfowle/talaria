use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct DiffArgs {
    /// First database reference (older version)
    /// Can be: path, "source/dataset", "source/dataset@version", "source/dataset:reduction"
    /// Examples: "uniprot/swissprot", "uniprot/swissprot@2024-01-01", "uniprot/swissprot:30-percent"
    #[arg(value_name = "OLD")]
    pub old: String,
    
    /// Second database reference (newer version, optional)
    /// If not provided, compares OLD with its previous version
    #[arg(value_name = "NEW")]
    pub new: Option<String>,
    
    /// Output report file
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    
    /// Report format
    #[arg(short, long, value_enum, default_value = "text")]
    pub format: ReportFormat,
    
    /// Include taxonomic analysis
    #[arg(long)]
    pub taxonomy: bool,
    
    /// Show detailed sequence-level changes
    #[arg(long)]
    pub detailed: bool,
    
    /// Similarity threshold for modified sequences (0.0-1.0)
    #[arg(long, default_value = "0.95")]
    pub similarity_threshold: f64,
    
    /// Compare only headers (fast mode)
    #[arg(long)]
    pub headers_only: bool,
    
    /// Generate visual charts (HTML format only)
    #[arg(long)]
    pub visual: bool,
    
    /// Number of threads to use
    #[arg(short = 'j', long)]
    pub threads: Option<usize>,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum ReportFormat {
    Text,
    Html,
    Json,
    Csv,
}

pub fn run(args: DiffArgs) -> anyhow::Result<()> {
    use crate::core::database_diff::DatabaseDiffer;
    use crate::core::database_manager::DatabaseManager;
    use crate::core::config::load_config;
    use indicatif::{ProgressBar, ProgressStyle};
    use std::path::Path;
    
    // Load config to get database settings
    let config = load_config("talaria.toml").unwrap_or_default();
    
    // Initialize database manager
    let db_manager = DatabaseManager::new(config.database.database_dir)?;
    
    // Resolve the old reference
    let old_path = if Path::new(&args.old).exists() {
        // It's a direct file path
        PathBuf::from(&args.old)
    } else {
        // Try to parse as database reference with potential reduction
        let (reference, reduction) = parse_reference_with_reduction(&args.old)?;
        let reference = db_manager.parse_reference(&reference)?;
        
        // Find the actual file in the resolved directory
        let dir = db_manager.resolve_reference(&reference)?;
        
        // If a reduction is specified, look in the reduced subdirectory
        if let Some(reduction_profile) = reduction {
            let reduced_dir = dir.join("reduced").join(&reduction_profile);
            if !reduced_dir.exists() {
                anyhow::bail!("Reduction profile '{}' not found for {}/{}", 
                              reduction_profile, reference.source, reference.dataset);
            }
            find_fasta_in_dir(&reduced_dir)?
        } else {
            find_fasta_in_dir(&dir)?
        }
    };
    
    // Resolve the new reference
    let new_path = if let Some(new_ref) = args.new.clone() {
        if Path::new(&new_ref).exists() {
            // It's a direct file path
            PathBuf::from(&new_ref)
        } else {
            // Try to parse as database reference with potential reduction
            let (reference, reduction) = parse_reference_with_reduction(&new_ref)?;
            let reference = db_manager.parse_reference(&reference)?;
            
            // Find the actual file in the resolved directory
            let dir = db_manager.resolve_reference(&reference)?;
            
            // If a reduction is specified, look in the reduced subdirectory
            if let Some(reduction_profile) = reduction {
                let reduced_dir = dir.join("reduced").join(&reduction_profile);
                if !reduced_dir.exists() {
                    anyhow::bail!("Reduction profile '{}' not found for {}/{}", 
                                  reduction_profile, reference.source, reference.dataset);
                }
                find_fasta_in_dir(&reduced_dir)?
            } else {
                find_fasta_in_dir(&dir)?
            }
        }
    } else {
        // No new specified, compare with previous version
        let reference = db_manager.parse_reference(&args.old)?;
        let versions = db_manager.list_versions(&reference.source, &reference.dataset)?;
        
        if versions.len() < 2 {
            anyhow::bail!("Need at least 2 versions to compare. Only {} version(s) found.", 
                          versions.len());
        }
        
        // Find current and previous
        let current_idx = versions.iter()
            .position(|v| v.is_current || v.version == reference.version.as_deref().unwrap_or("current"))
            .unwrap_or(0);
        
        if current_idx + 1 >= versions.len() {
            anyhow::bail!("No previous version available for comparison");
        }
        
        let previous = &versions[current_idx + 1];
        find_fasta_in_dir(&previous.path)?
    };
    
    if !old_path.exists() {
        anyhow::bail!("Old database file does not exist: {}", old_path.display());
    }
    
    if !new_path.exists() {
        anyhow::bail!("New database file does not exist: {}", new_path.display());
    }
    
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
    );
    
    pb.set_message("Comparing databases...");
    
    let mut differ = DatabaseDiffer::new()
        .with_similarity_threshold(args.similarity_threshold)
        .with_headers_only(args.headers_only);
    
    if let Some(threads) = args.threads {
        differ = differ.with_threads(threads);
    }
    
    let comparison_result = differ.compare(&old_path, &new_path)?;
    
    pb.finish_with_message("Comparison complete!");
    
    generate_report(&comparison_result, args)?;
    
    Ok(())
}

fn generate_report(
    result: &crate::core::database_diff::ComparisonResult,
    args: DiffArgs,
) -> anyhow::Result<()> {
    use crate::report::{ReportGenerator, ReportOptions};
    
    let options = ReportOptions {
        format: match args.format {
            ReportFormat::Text => crate::report::Format::Text,
            ReportFormat::Html => crate::report::Format::Html,
            ReportFormat::Json => crate::report::Format::Json,
            ReportFormat::Csv => crate::report::Format::Csv,
        },
        include_taxonomy: args.taxonomy,
        include_details: args.detailed,
        include_visuals: args.visual,
    };
    
    let generator = ReportGenerator::new(options);
    let report = generator.generate(result)?;
    
    if let Some(output_path) = &args.output {
        std::fs::write(output_path, report)?;
        println!("Report written to: {}", output_path.display());
    } else {
        println!("{}", report);
    }
    
    Ok(())
}

/// Parse a reference string that may include a reduction profile
/// Format: "source/dataset[:reduction][@version]"
/// Returns: (base_reference, Option<reduction_profile>)
fn parse_reference_with_reduction(reference: &str) -> anyhow::Result<(String, Option<String>)> {
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

/// Find a FASTA file in a directory
fn find_fasta_in_dir(dir: &std::path::Path) -> anyhow::Result<PathBuf> {
    use std::fs;
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if matches!(ext, "fasta" | "fa" | "fna" | "faa" | "ffn" | "frn") {
                    return Ok(path);
                }
            }
        }
    }
    
    anyhow::bail!("No FASTA file found in directory: {}", dir.display())
}