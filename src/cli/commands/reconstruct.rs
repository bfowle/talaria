use clap::Args;
use std::path::PathBuf;
use crate::cli::output::*;

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

    /// Input from CASG repository profile
    #[arg(long, value_name = "PROFILE")]
    pub casg_profile: Option<String>,

    /// CASG repository path (default: ${TALARIA_HOME}/databases)
    #[arg(long, value_name = "PATH")]
    pub casg_path: Option<PathBuf>,
    
    /// Only reconstruct specific sequences (by ID)
    #[arg(long)]
    pub sequences: Vec<String>,
    
    /// List available sequences without reconstructing (dry run)
    #[arg(long)]
    pub list_only: bool,
}

pub fn run(args: ReconstructArgs) -> anyhow::Result<()> {
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

    // Validate arguments: need one of database, CASG, or file paths
    let input_methods = [
        args.database.is_some(),
        args.casg_profile.is_some(),
        args.references.is_some() && args.deltas.is_some(),
    ];

    if input_methods.iter().filter(|&&x| x).count() != 1 {
        anyhow::bail!("Must specify exactly one input method: database reference, CASG profile, or both files (-r and -d)");
    }
    
    // Resolve file paths and output name based on input method
    let (references_path, deltas_path, output_path, db_info) = if let Some(profile) = &args.casg_profile {
        // Reconstruct from CASG profile
        pb.set_message(format!("Loading CASG profile '{}'...", profile));

        let output = args.output.clone().unwrap_or_else(|| {
            PathBuf::from(format!("reconstructed_{}.fasta", profile))
        });

        // Call CASG reconstruction and return early
        reconstruct_from_casg(profile, &args.casg_path, &output, args.sequences.clone(), pb)?;
        return Ok(());
    } else if let Some(db_ref_str) = &args.database {
        // Parse database reference with reduction profile
        let (_base_ref, profile) = parse_database_with_profile(db_ref_str)?;

        // Profile is required for reconstruction
        let profile = profile.ok_or_else(|| anyhow::anyhow!(
            "Reduction profile required for reconstruction. Use format: 'database:profile' (e.g., 'uniprot/swissprot:blast-30')"
        ))?;

        // Reconstruct from CASG using the profile
        pb.set_message(format!("Loading CASG profile '{}'...", profile));

        let output = args.output.clone().unwrap_or_else(|| {
            PathBuf::from(format!("reconstructed_{}.fasta", profile))
        });

        // Call CASG reconstruction and return early
        reconstruct_from_casg(&profile, &args.casg_path, &output, args.sequences.clone(), pb)?;
        return Ok(())
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

        (references, deltas, output, None::<(String, String)>)
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
    pb.finish_and_clear();

    subsection_header("Reconstruction Summary");

    let summary_items = vec![
        ("Total sequences", format_number(reconstructed.len())),
        ("Output file", output_path.display().to_string()),
        ("File size", format_bytes(output_size)),
    ];

    for (i, (label, value)) in summary_items.iter().enumerate() {
        tree_item(i == summary_items.len() - 1, label, Some(value));
    }

    if let Some((dataset, profile)) = db_info {
        info(&format!("Reconstructed from {}:{}", dataset, profile));
    }

    success("Reconstruction complete!");

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

/// Reconstruct sequences from CASG profile
fn reconstruct_from_casg(
    profile: &str,
    casg_path: &Option<PathBuf>,
    output_path: &PathBuf,
    sequence_filter: Vec<String>,
    pb: indicatif::ProgressBar,
) -> anyhow::Result<()> {
    use crate::casg::{storage::CASGStorage, delta_reconstructor::DeltaReconstructor, assembler::FastaAssembler};
    
    use std::collections::HashSet;

    let casg_path = casg_path.clone().unwrap_or_else(|| {
        use crate::core::paths;
        paths::talaria_databases_dir()
    });

    // Open CASG storage
    let storage = CASGStorage::open(&casg_path)?;

    // Load reduction manifest by profile
    let manifest = storage.get_reduction_by_profile(profile)?
        .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found in CASG repository", profile))?;

    pb.set_message(format!("Found profile '{}' with {} reference chunks and {} delta chunks",
                          profile, manifest.reference_chunks.len(), manifest.delta_chunks.len()));

    // Verify integrity
    if !manifest.verify_integrity()? {
        anyhow::bail!("Manifest integrity check failed! The reduction may be corrupted.");
    }

    let mut all_sequences = Vec::new();
    let sequence_filter_set: HashSet<String> = sequence_filter.into_iter().collect();

    // Reconstruct reference sequences using the assembler
    pb.set_message("Loading reference sequences from chunks...");
    let assembler = FastaAssembler::new(&storage);
    let reference_hashes: Vec<_> = manifest.reference_chunks
        .iter()
        .map(|rc| rc.chunk_hash.clone())
        .collect();

    let reference_sequences = assembler.assemble_from_chunks(&reference_hashes)?;

    // Filter and add reference sequences
    for seq in reference_sequences.iter() {
        if sequence_filter_set.is_empty() || sequence_filter_set.contains(&seq.id) {
            all_sequences.push(seq.clone());
        }
    }

    // Reconstruct delta sequences
    if !manifest.delta_chunks.is_empty() {
        pb.set_message("Reconstructing delta sequences...");

        let delta_reconstructor = DeltaReconstructor::default();

        for delta_chunk_ref in &manifest.delta_chunks {
            // Get the delta chunk
            let delta_chunk = storage.get_delta_chunk(&delta_chunk_ref.chunk_hash)?;

            // Reconstruct child sequences using the reference sequences
            let reconstructed = delta_reconstructor.reconstruct_chunk(&delta_chunk, reference_sequences.clone())?;

            for seq in reconstructed {
                if sequence_filter_set.is_empty() || sequence_filter_set.contains(&seq.id) {
                    all_sequences.push(seq);
                }
            }
        }
    }

    // Write output
    pb.set_message(format!("Writing {} sequences to output...", all_sequences.len()));
    crate::bio::fasta::write_fasta(output_path, &all_sequences)?;

    pb.finish_with_message(format!("✓ Reconstructed {} sequences from CASG profile '{}' to {}",
                                  all_sequences.len(), profile, output_path.display()));

    println!("\nReconstruction Statistics:");
    println!("  Source: CASG profile '{}'", profile);
    println!("  Total sequences: {}", manifest.statistics.original_sequences);
    println!("  Reference sequences: {}", manifest.statistics.reference_sequences);
    println!("  Delta sequences: {}", manifest.statistics.child_sequences);
    println!("  Reconstructed: {}", all_sequences.len());
    println!("  Coverage: {:.1}%", manifest.statistics.sequence_coverage * 100.0);
    println!("  Merkle root verified: ✓");

    Ok(())
}

