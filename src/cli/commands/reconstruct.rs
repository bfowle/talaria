use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct ReconstructArgs {
    /// Database reduction to reconstruct (e.g., "uniprot/swissprot:blast-30")
    /// When specified, automatically finds reference and delta files
    #[arg(value_name = "DATABASE:PROFILE")]
    pub database: Option<String>,
    
    /// Reference FASTA file (required if database not specified)
    #[arg(short = 'r', long, value_name = "FILE")]
    pub references: Option<PathBuf>,
    
    /// Delta metadata file (required if database not specified)
    #[arg(short = 'd', long, value_name = "FILE")]
    pub deltas: Option<PathBuf>,
    
    /// Output reconstructed FASTA file (auto-generated if not specified)
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
    
    /// Only reconstruct specific sequences (by ID)
    #[arg(long)]
    pub sequences: Vec<String>,
    
    /// List available sequences without reconstructing (dry run)
    #[arg(long)]
    pub list_only: bool,
}

pub fn run(args: ReconstructArgs) -> anyhow::Result<()> {
    use crate::core::database_manager::DatabaseManager;
    use crate::core::config::load_config;
    use crate::utils::format::{format_bytes, get_file_size};
    use indicatif::{ProgressBar, ProgressStyle};

    // Create progress bar
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
    );
    pb.set_message("Initializing reconstruction...");

    // Validate arguments: either database or both file paths must be specified
    if args.database.is_none() && (args.references.is_none() || args.deltas.is_none()) {
        anyhow::bail!("Must specify either a database reference or both files (-r and -d)");
    }

    if args.database.is_some() && (args.references.is_some() || args.deltas.is_some()) {
        anyhow::bail!("Cannot specify both database reference and file paths. Use one or the other.");
    }
    
    // Resolve file paths and output name based on input method
    let (references_path, deltas_path, output_path, db_info) = if let Some(db_ref_str) = &args.database {
        // Parse database reference with reduction profile
        let (base_ref, profile) = parse_database_with_profile(db_ref_str)?;
        
        // Profile is required for reconstruction
        let profile = profile.ok_or_else(|| anyhow::anyhow!(
            "Reduction profile required for reconstruction. Use format: 'database:profile' (e.g., 'uniprot/swissprot:blast-30')"
        ))?;
        
        // Load config and database manager
        let config = load_config("talaria.toml").unwrap_or_default();
        let db_manager = DatabaseManager::new(config.database.database_dir)?;
        
        // Parse and resolve the database reference
        let db_ref = db_manager.parse_reference(&base_ref)?;
        let db_dir = db_manager.resolve_reference(&db_ref)?;
        
        // Find reference and delta files in reduced subdirectory
        let reduced_dir = db_dir.join("reduced").join(&profile);
        if !reduced_dir.exists() {
            anyhow::bail!("Reduction profile '{}' not found for {}/{}", 
                          profile, db_ref.source, db_ref.dataset);
        }
        
        let references = db_manager.find_fasta_in_dir(&reduced_dir)?;
        let deltas = find_delta_file(&reduced_dir)?;
        
        // Generate output path if not specified
        let output = args.output.unwrap_or_else(|| {
            std::path::PathBuf::from(format!("{}-{}-reconstructed.fasta", 
                                            db_ref.dataset, profile))
        });
        
        pb.set_message(format!("Reconstructing {}/{}:{}", db_ref.source, db_ref.dataset, profile));

        (references, deltas, output, Some((db_ref.dataset, profile)))
    } else {
        // Traditional file-based usage
        let references = args.references.ok_or_else(|| anyhow::anyhow!("Reference file (-r) is required"))?;
        let deltas = args.deltas.ok_or_else(|| anyhow::anyhow!("Delta file (-d) is required"))?;

        if !references.exists() {
            anyhow::bail!("Reference file does not exist: {:?}", references);
        }
        if !deltas.exists() {
            anyhow::bail!("Delta file does not exist: {:?}", deltas);
        }

        // Generate output path if not specified
        let output = args.output.unwrap_or_else(|| {
            std::path::PathBuf::from("reconstructed.fasta")
        });

        pb.set_message("Reconstructing sequences from references and deltas...");

        (references, deltas, output, None)
    };
    
    // Get file sizes
    let ref_size = get_file_size(&references_path).unwrap_or(0);
    let delta_size = get_file_size(&deltas_path).unwrap_or(0);

    // Load reference sequences
    pb.set_message("Loading reference sequences...");
    let references = crate::bio::fasta::parse_fasta(&references_path)?;
    pb.set_message(format!("Loaded {} reference sequences ({})",
                          references.len(), format_bytes(ref_size)));

    // Load delta metadata
    pb.set_message("Loading delta metadata...");
    let deltas = crate::storage::metadata::load_metadata(&deltas_path)?;
    pb.set_message(format!("Loaded {} delta records ({})",
                          deltas.len(), format_bytes(delta_size)));
    
    // If list-only mode, show available sequences and exit
    if args.list_only {
        pb.finish_and_clear();
        
        println!("\nAvailable sequences for reconstruction:");
        println!("=========================================");
        
        // List reference sequences
        println!("\nReference sequences ({}):", references.len());
        for (i, seq) in references.iter().enumerate() {
            if i < 10 {
                println!("  - {} (length: {})", seq.id, seq.sequence.len());
            } else if i == 10 {
                println!("  ... and {} more", references.len() - 10);
                break;
            }
        }
        
        // List delta sequences
        println!("\nDelta sequences ({}):", deltas.len());
        for (i, delta) in deltas.iter().enumerate() {
            if i < 10 {
                println!("  - {} (from reference: {})", delta.child_id, delta.reference_id);
            } else if i == 10 {
                println!("  ... and {} more", deltas.len() - 10);
                break;
            }
        }
        
        println!("\nTotal sequences available: {}", references.len() + deltas.len());
        
        if !args.sequences.is_empty() {
            println!("\nRequested sequences:");
            let ref_ids: std::collections::HashSet<String> = references.iter()
                .map(|r| r.id.clone()).collect();
            let delta_ids: std::collections::HashSet<String> = deltas.iter()
                .map(|d| d.child_id.clone()).collect();
            
            for seq_id in &args.sequences {
                if ref_ids.contains(seq_id) {
                    println!("  ✓ {} (reference)", seq_id);
                } else if delta_ids.contains(seq_id) {
                    println!("  ✓ {} (delta)", seq_id);
                } else {
                    println!("  ✗ {} (NOT FOUND)", seq_id);
                }
            }
        }
        
        println!("\nUse without --list-only to perform reconstruction.");
        return Ok(());
    }

    // Reconstruct sequences
    let reconstructor = crate::core::delta_encoder::DeltaReconstructor::new();
    let reconstructed = if args.sequences.is_empty() {
        pb.set_message(format!("Reconstructing {} sequences...",
                              references.len() + deltas.len()));
        reconstructor.reconstruct_all(references, deltas, vec![])?
    } else {
        pb.set_message(format!("Reconstructing {} specific sequences...",
                              args.sequences.len()));
        reconstructor.reconstruct_all(references, deltas, args.sequences)?
    };

    // Write output
    pb.set_message("Writing reconstructed FASTA...");
    crate::bio::fasta::write_fasta(&output_path, &reconstructed)?;

    // Get output file size
    let output_size = get_file_size(&output_path).unwrap_or(0);

    // Print summary with progress bar completion
    if let Some((dataset, profile)) = db_info {
        pb.finish_with_message(format!("Reconstructed {} sequences from {}:{} to {} ({})",
                                      reconstructed.len(), dataset, profile,
                                      output_path.display(), format_bytes(output_size)));
    } else {
        pb.finish_with_message(format!("Reconstructed {} sequences to {} ({})",
                                      reconstructed.len(), output_path.display(),
                                      format_bytes(output_size)));
    }

    Ok(())
}

/// Parse a database reference that may include a reduction profile
/// Format: "source/dataset[:profile][@version]"
/// Returns: (base_reference, Option<profile>)
fn parse_database_with_profile(reference: &str) -> anyhow::Result<(String, Option<String>)> {
    // Check for reduction profile (colon separator)
    if let Some(colon_idx) = reference.find(':') {
        // Split at colon
        let base = &reference[..colon_idx];
        let remainder = &reference[colon_idx + 1..];
        
        // Check if remainder has version (@)
        if let Some(at_idx) = remainder.find('@') {
            // Format: source/dataset:profile@version
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