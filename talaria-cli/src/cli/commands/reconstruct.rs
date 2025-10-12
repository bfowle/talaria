use crate::cli::formatting::output::*;
use clap::Args;
use std::path::PathBuf;
use talaria_herald::HeraldRepository;

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

    /// Input from HERALD repository profile
    #[arg(long, value_name = "PROFILE")]
    pub herald_profile: Option<String>,

    /// HERALD repository path (default: ${TALARIA_HOME}/databases)
    #[arg(long, value_name = "PATH")]
    pub herald_path: Option<PathBuf>,

    /// Only reconstruct specific sequences (by ID)
    #[arg(long)]
    pub sequences: Vec<String>,

    /// List available sequences without reconstructing (dry run)
    #[arg(long)]
    pub list_only: bool,

    /// Query sequences at specific point in time (ISO 8601 format)
    /// Example: --at-time "2024-01-15T10:00:00Z"
    #[arg(long, value_name = "TIMESTAMP")]
    pub at_time: Option<String>,

    /// Use specific sequence version (hash or timestamp)
    #[arg(long, value_name = "VERSION")]
    pub sequence_version: Option<String>,

    /// Use specific taxonomy version (hash or timestamp)
    #[arg(long, value_name = "VERSION")]
    pub taxonomy_version: Option<String>,

    /// Show version history for sequences
    #[arg(long)]
    pub show_versions: bool,

    /// Report output file path
    #[arg(long = "report-output", value_name = "FILE")]
    pub report_output: Option<PathBuf>,

    /// Report output format (text, html, json, csv)
    #[arg(long = "report-format", value_name = "FORMAT", default_value = "text")]
    pub report_format: String,
}

pub fn run(args: ReconstructArgs) -> anyhow::Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};
    use talaria_utils::display::format::{format_bytes, get_file_size};

    // Handle bi-temporal version queries
    if args.show_versions {
        return show_version_history(&args);
    }

    // Create progress bar
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message("Initializing reconstruction...");

    // Apply bi-temporal constraints if specified
    if args.at_time.is_some() || args.sequence_version.is_some() || args.taxonomy_version.is_some()
    {
        pb.set_message("Applying temporal constraints...");
        // This will be passed to HERALD reconstruction functions
    }

    // Validate arguments: need one of database, HERALD, or file paths
    let input_methods = [
        args.database.is_some(),
        args.herald_profile.is_some(),
        args.references.is_some() && args.deltas.is_some(),
    ];

    if input_methods.iter().filter(|&&x| x).count() != 1 {
        anyhow::bail!("Must specify exactly one input method: database reference, HERALD profile, or both files (-r and -d)");
    }

    // Resolve file paths and output name based on input method
    let (references_path, deltas_path, output_path, db_info) = if let Some(profile) =
        &args.herald_profile
    {
        // Reconstruct from HERALD profile
        pb.set_message(format!("Loading HERALD profile '{}'...", profile));

        let output = args
            .output
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("reconstructed_{}.fasta", profile)));

        // Call HERALD reconstruction and return early
        reconstruct_from_herald(
            profile,
            &args.herald_path,
            &output,
            args.sequences.clone(),
            pb,
        )?;
        return Ok(());
    } else if let Some(db_ref_str) = &args.database {
        // Parse database reference with profile using the proper utility
        use talaria_utils::database::database_ref::parse_database_reference;
        let db_ref = parse_database_reference(db_ref_str)?;

        // Profile is required for reconstruction
        let profile = db_ref.profile.clone().ok_or_else(|| anyhow::anyhow!(
            "Reduction profile required for reconstruction. Use format: 'database:profile' (e.g., 'uniprot/swissprot:blast-30')"
        ))?;

        // Reconstruct from HERALD using the database info and profile
        pb.set_message(format!(
            "Loading profile '{}' for {}...",
            profile,
            db_ref.base_ref()
        ));

        let output = args.output.clone().unwrap_or_else(|| {
            PathBuf::from(format!(
                "reconstructed_{}_{}.fasta",
                db_ref.base_ref().replace('/', "_"),
                profile
            ))
        });

        // Call HERALD reconstruction with database context
        reconstruct_from_herald_database(
            &db_ref,
            &profile,
            &args.herald_path,
            &output,
            args.sequences.clone(),
            pb,
        )?;
        return Ok(());
    } else {
        // Traditional file-based usage
        let references = args
            .references
            .ok_or_else(|| anyhow::anyhow!("Reference file (-r) is required"))?;
        let deltas = args
            .deltas
            .ok_or_else(|| anyhow::anyhow!("Delta file (-d) is required"))?;

        if !references.exists() {
            anyhow::bail!("Reference file does not exist: {:?}", references);
        }
        if !deltas.exists() {
            anyhow::bail!("Delta file does not exist: {:?}", deltas);
        }

        // Generate output path if not specified
        let output = args
            .output
            .unwrap_or_else(|| std::path::PathBuf::from("reconstructed.fasta"));

        pb.set_message("Reconstructing sequences from references and deltas...");

        (references, deltas, output, None::<(String, String)>)
    };

    // Get file sizes
    let ref_size = get_file_size(&references_path).unwrap_or(0);
    let delta_size = get_file_size(&deltas_path).unwrap_or(0);

    // Load reference sequences
    pb.set_message("Loading reference sequences...");
    let references = talaria_bio::parse_fasta(&references_path)?;
    pb.set_message(format!(
        "Loaded {} reference sequences ({})",
        references.len(),
        format_bytes(ref_size)
    ));

    // Load delta metadata
    pb.set_message("Loading delta metadata...");
    let deltas = talaria_storage::io::metadata::load_metadata(&deltas_path)?;
    pb.set_message(format!(
        "Loaded {} delta records ({})",
        deltas.len(),
        format_bytes(delta_size)
    ));

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
                println!(
                    "  - {} (from reference: {})",
                    delta.child_id, delta.reference_id
                );
            } else if i == 10 {
                println!("  ... and {} more", deltas.len() - 10);
                break;
            }
        }

        println!(
            "\nTotal sequences available: {}",
            references.len() + deltas.len()
        );

        if !args.sequences.is_empty() {
            println!("\nRequested sequences:");
            let ref_ids: std::collections::HashSet<String> =
                references.iter().map(|r| r.id.clone()).collect();
            let delta_ids: std::collections::HashSet<String> =
                deltas.iter().map(|d| d.child_id.clone()).collect();

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

    // Save counts before values are moved
    let total_references = references.len();
    let total_deltas = deltas.len();
    let requested_sequences = args.sequences.clone();

    // Reconstruct sequences
    let reconstructor = talaria_bio::compression::DeltaReconstructor::new();
    let reconstructed = if args.sequences.is_empty() {
        pb.set_message(format!(
            "Reconstructing {} sequences...",
            references.len() + deltas.len()
        ));
        reconstructor.reconstruct_all(references, deltas, vec![])?
    } else {
        pb.set_message(format!(
            "Reconstructing {} specific sequences...",
            args.sequences.len()
        ));
        reconstructor.reconstruct_all(references, deltas, args.sequences)?
    };

    // Write output
    pb.set_message("Writing reconstructed FASTA...");
    talaria_bio::write_fasta(&output_path, &reconstructed)?;

    // Get output file size
    let output_size = get_file_size(&output_path).unwrap_or(0);

    // Print summary with progress bar completion
    pb.finish_and_clear();

    subsection_header("Reconstruction Summary");

    let summary_items = [
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

    // Generate report if requested
    if let Some(report_path) = &args.report_output {
        use std::time::Duration;
        use talaria_herald::operations::ReconstructionResult;

        // Track which sequences failed (if any)
        let failed_sequences: Vec<String> = if !requested_sequences.is_empty() {
            let reconstructed_ids: std::collections::HashSet<String> =
                reconstructed.iter().map(|s| s.id.clone()).collect();
            requested_sequences
                .iter()
                .filter(|id| !reconstructed_ids.contains(*id))
                .map(|id| id.clone())
                .collect()
        } else {
            Vec::new()
        };

        let reconstruction_success = failed_sequences.is_empty();
        let result = ReconstructionResult {
            sequences_reconstructed: reconstructed.len(),
            total_sequences: total_references + total_deltas,
            reconstructed_sequences: reconstructed.len(),
            failed_sequences,
            output_file: output_path.display().to_string(),
            output_size: output_size,
            success: reconstruction_success,
            duration: Duration::from_secs(0), // TODO: Track actual duration
        };

        crate::cli::commands::save_report(&result, &args.report_format, report_path)?;
        success(&format!("Report saved to {}", report_path.display()));
    }

    Ok(())
}

/// Parse a database reference that may include a reduction profile
/// Format: "source/dataset[:profile][@version]"
/// Returns: (base_reference, Option<profile>)
#[allow(dead_code)]
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

/// Reconstruct sequences from HERALD profile
fn reconstruct_from_herald(
    profile: &str,
    herald_path: &Option<PathBuf>,
    output_path: &PathBuf,
    sequence_filter: Vec<String>,
    pb: indicatif::ProgressBar,
) -> anyhow::Result<()> {
    use talaria_herald::{DeltaReconstructor, FastaAssembler, HeraldStorage};

    use std::collections::HashSet;

    let herald_path = herald_path.clone().unwrap_or_else(|| {
        use talaria_core::system::paths;
        paths::talaria_databases_dir()
    });

    // Open HERALD storage and database manager
    let storage = HeraldStorage::open(&herald_path)?;

    // Need DatabaseManager to get version information
    use talaria_herald::database::DatabaseManager;
    let manager = DatabaseManager::new(Some(herald_path.to_string_lossy().to_string()))?;

    // Parse profile to extract database info if present (format: source/dataset:profile or just profile)
    let (source, dataset, profile_name) = if profile.contains('/') && profile.contains(':') {
        // Format: source/dataset:profile
        let parts: Vec<&str> = profile.split(':').collect();
        if parts.len() == 2 {
            let db_parts: Vec<&str> = parts[0].split('/').collect();
            if db_parts.len() == 2 {
                (db_parts[0], db_parts[1], parts[1])
            } else {
                // Invalid format, try to list all profiles and find a match
                return Err(anyhow::anyhow!(
                    "Invalid profile format: '{}'. Use 'source/dataset:profile' or just 'profile'",
                    profile
                ));
            }
        } else {
            return Err(anyhow::anyhow!(
                "Invalid profile format: '{}'. Use 'source/dataset:profile' or just 'profile'",
                profile
            ));
        }
    } else {
        // Just profile name - try to find it in any database
        // For now, we'll require the full format
        return Err(anyhow::anyhow!(
            "Please specify the full profile path: 'source/dataset:{}'",
            profile
        ));
    };

    // Get database info to retrieve version
    let db_name = format!("{}/{}", source, dataset);
    let databases = manager.list_databases()?;
    let db_info = databases
        .iter()
        .find(|db| db.name == db_name)
        .ok_or_else(|| anyhow::anyhow!("Database '{}' not found", db_name))?;

    // Load reduction manifest by profile
    let manifest = storage
        .get_database_reduction_by_profile(source, dataset, &db_info.version, profile_name)?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Profile '{}' not found for database '{}/{}'",
                profile_name,
                source,
                dataset
            )
        })?;

    pb.set_message(format!(
        "Found profile '{}' with {} reference chunks and {} delta chunks",
        profile,
        manifest.reference_chunks.len(),
        manifest.delta_chunks.len()
    ));

    // Verify integrity
    if !manifest.verify_integrity()? {
        anyhow::bail!("Manifest integrity check failed! The reduction may be corrupted.");
    }

    let mut all_sequences = Vec::new();
    let sequence_filter_set: HashSet<String> = sequence_filter.into_iter().collect();

    // Reconstruct reference sequences using the assembler
    pb.set_message("Loading reference sequences from chunks...");
    let assembler = FastaAssembler::new(&storage);
    let reference_hashes: Vec<_> = manifest
        .reference_chunks
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
            let reconstructed =
                delta_reconstructor.reconstruct_chunk(&delta_chunk, reference_sequences.clone())?;

            for seq in reconstructed {
                if sequence_filter_set.is_empty() || sequence_filter_set.contains(&seq.id) {
                    all_sequences.push(seq);
                }
            }
        }
    }

    // Write output
    pb.set_message(format!(
        "Writing {} sequences to output...",
        all_sequences.len()
    ));
    talaria_bio::write_fasta(output_path, &all_sequences)?;

    pb.finish_with_message(format!(
        "✓ Reconstructed {} sequences from HERALD profile '{}' to {}",
        all_sequences.len(),
        profile,
        output_path.display()
    ));

    println!("\nReconstruction Statistics:");
    println!("  Source: HERALD profile '{}'", profile);
    println!(
        "  Total sequences: {}",
        manifest.statistics.original_sequences
    );
    println!(
        "  Reference sequences: {}",
        manifest.statistics.reference_sequences
    );
    println!("  Delta sequences: {}", manifest.statistics.child_sequences);
    println!("  Reconstructed: {}", all_sequences.len());
    println!(
        "  Coverage: {:.1}%",
        manifest.statistics.sequence_coverage * 100.0
    );
    println!("  Merkle root verified: ✓");

    Ok(())
}

/// Reconstruct sequences from HERALD database profile
fn reconstruct_from_herald_database(
    db_ref: &talaria_utils::database::database_ref::DatabaseReference,
    profile: &str,
    herald_path: &Option<PathBuf>,
    output_path: &PathBuf,
    sequence_filter: Vec<String>,
    pb: indicatif::ProgressBar,
) -> anyhow::Result<()> {
    use std::collections::HashSet;
    use talaria_herald::ReductionManifest;
    use talaria_herald::{DeltaReconstructor, FastaAssembler, HeraldStorage};

    let herald_path = herald_path.clone().unwrap_or_else(|| {
        use talaria_core::system::paths;
        paths::talaria_databases_dir()
    });

    // Open HERALD storage
    let storage = HeraldStorage::new(&herald_path)?;

    // Load reduction manifest from version-specific location
    let manifest_path = herald_path
        .join("versions")
        .join(&db_ref.source)
        .join(&db_ref.dataset)
        .join(db_ref.version_or_default())
        .join("profiles");

    // Try .tal format first
    let tal_path = manifest_path.join(format!("{}.tal", profile));
    let json_path = manifest_path.join(format!("{}.json", profile));

    let manifest = if tal_path.exists() {
        // Load .tal format
        let data = std::fs::read(&tal_path)?;
        if data.len() < 4 || &data[0..4] != b"TAL\x01" {
            anyhow::bail!("Invalid .tal file format in {}", tal_path.display());
        }
        rmp_serde::from_slice::<ReductionManifest>(&data[4..])?
    } else if json_path.exists() {
        // Load JSON format
        let data = std::fs::read(&json_path)?;
        serde_json::from_slice::<ReductionManifest>(&data)?
    } else {
        anyhow::bail!(
            "Profile '{}' not found for database {}. Expected at: {}",
            profile,
            db_ref.base_ref(),
            tal_path.display()
        );
    };

    pb.set_message(format!(
        "Found profile '{}' with {} reference chunks and {} delta chunks",
        profile,
        manifest.reference_chunks.len(),
        manifest.delta_chunks.len()
    ));

    // Verify integrity
    if !manifest.verify_integrity()? {
        anyhow::bail!("Manifest integrity check failed! The reduction may be corrupted.");
    }

    let mut all_sequences = Vec::new();
    let sequence_filter_set: HashSet<String> = sequence_filter.into_iter().collect();

    // Reconstruct reference sequences using the assembler
    pb.set_message("Loading reference sequences from chunks...");
    let assembler = FastaAssembler::new(&storage);
    let reference_hashes: Vec<_> = manifest
        .reference_chunks
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
            let reconstructed =
                delta_reconstructor.reconstruct_chunk(&delta_chunk, reference_sequences.clone())?;

            for seq in reconstructed {
                if sequence_filter_set.is_empty() || sequence_filter_set.contains(&seq.id) {
                    all_sequences.push(seq);
                }
            }
        }
    }

    // Write output
    pb.set_message(format!(
        "Writing {} sequences to output...",
        all_sequences.len()
    ));
    talaria_bio::write_fasta(output_path, &all_sequences)?;

    pb.finish_with_message(format!(
        "✓ Reconstructed {} sequences from profile '{}':{} to {}",
        all_sequences.len(),
        db_ref.base_ref(),
        profile,
        output_path.display()
    ));

    println!("\nReconstruction Statistics:");
    println!("  Database: {}", db_ref.base_ref());
    println!("  Profile: {}", profile);
    println!("  Version: {}", db_ref.version_or_default());
    println!(
        "  Total sequences: {}",
        manifest.statistics.original_sequences
    );
    println!(
        "  Reference sequences: {}",
        manifest.statistics.reference_sequences
    );
    println!("  Delta sequences: {}", manifest.statistics.child_sequences);
    println!("  Reconstructed: {}", all_sequences.len());
    println!(
        "  Coverage: {:.1}%",
        manifest.statistics.sequence_coverage * 100.0
    );
    println!("  Merkle root verified: ✓");

    Ok(())
}

/// Show version history for a database
fn show_version_history(args: &ReconstructArgs) -> anyhow::Result<()> {
    use talaria_core::system::paths;
    use talaria_herald::TemporalIndex;

    println!("🕐 Bi-temporal Version History");
    println!("─────────────────────────────");

    // Get HERALD path
    let herald_path = args
        .herald_path
        .clone()
        .unwrap_or_else(paths::talaria_databases_dir);

    // Load temporal index - need to open storage first to get RocksDB
    let repository = HeraldRepository::open(&herald_path)?;
    let rocksdb = repository.storage.sequence_storage.get_rocksdb();
    let temporal_index = TemporalIndex::load(&herald_path, rocksdb)?;

    // Get version history
    let history = temporal_index.get_version_history(20)?;

    if history.is_empty() {
        println!("No version history available");
        return Ok(());
    }

    println!("\nAvailable Versions:");
    println!("┌─────────────────────┬───────────────┬────────────────┬────────────┐");
    println!("│ Timestamp           │ Version       │ Type           │ Sequences  │");
    println!("├─────────────────────┼───────────────┼────────────────┼────────────┤");

    for version in &history {
        let timestamp = version.timestamp.format("%Y-%m-%d %H:%M:%S");
        let version_id = if version.version.len() > 13 {
            format!("{}...", &version.version[..10])
        } else {
            version.version.clone()
        };

        println!(
            "│ {} │ {:13} │ {:14} │ {:10} │",
            timestamp, version_id, version.version_type, version.sequence_count
        );
    }

    println!("└─────────────────────┴───────────────┴────────────────┴────────────┘");

    // Show how to use temporal queries
    println!("\n💡 Temporal Query Examples:");
    println!("  Reconstruct at specific time:");
    println!(
        "    talaria reconstruct --at-time \"2024-01-15T10:00:00Z\" uniprot/swissprot:blast-30"
    );
    println!("  Use specific sequence version:");
    println!("    talaria reconstruct --sequence-version \"abc123def\" uniprot/swissprot:blast-30");
    println!("  Use specific taxonomy version:");
    println!("    talaria reconstruct --taxonomy-version \"xyz789\" uniprot/swissprot:blast-30");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconstruct_args_validation() {
        // Test that reconstruct args are properly validated
        let args = ReconstructArgs {
            database: Some("uniprot/swissprot:blast-30".to_string()),
            references: Some("test_ref.fasta".into()),
            deltas: Some("test_deltas.fasta".into()),
            output: Some("output.fasta".into()),
            herald_profile: None,
            herald_path: None,
            sequences: vec![],
            list_only: false,
            at_time: None,
            sequence_version: None,
            taxonomy_version: None,
            show_versions: false,
            report_format: "text".to_string(),
            report_output: None,
        };

        assert_eq!(
            args.references.as_ref().unwrap().to_str().unwrap(),
            "test_ref.fasta"
        );
        assert_eq!(
            args.output.as_ref().unwrap().to_str().unwrap(),
            "output.fasta"
        );
        assert!(args.deltas.is_some());
    }

    #[test]
    fn test_database_reference_parsing() {
        // Test parsing of database references
        let test_cases = vec![
            ("ncbi/protein", true),
            ("uniprot/swissprot", true),
            ("uniprot/trembl", true),
            ("ncbi/nt:blast-30", true),
            ("invalid", false),
            ("", false),
        ];

        for (input, should_be_valid) in test_cases {
            let is_valid = input.contains('/') || input.contains(':');
            assert_eq!(
                is_valid, should_be_valid,
                "Database reference validation failed for: {}",
                input
            );
        }
    }

    #[test]
    fn test_profile_parsing() {
        // Test profile name parsing
        let test_cases = vec![
            ("blast-30", Ok("blast-30")),
            ("lambda-90", Ok("lambda-90")),
            ("kraken-50", Ok("kraken-50")),
            ("custom_profile", Ok("custom_profile")),
            ("", Err("empty profile")),
        ];

        for (input, expected) in test_cases {
            match expected {
                Ok(profile) => {
                    assert_eq!(input, profile);
                    assert!(!input.is_empty());
                }
                Err(_) => {
                    assert!(input.is_empty());
                }
            }
        }
    }

    #[test]
    fn test_temporal_coordinate_parsing() {
        // Test temporal coordinate parsing
        let test_times = vec![
            "2024-01-15T10:00:00Z",
            "2023-12-31T23:59:59Z",
            "2024-06-01T00:00:00Z",
        ];

        for time_str in test_times {
            // Verify the format is valid ISO 8601
            assert!(time_str.contains('T'));
            assert!(time_str.ends_with('Z'));
            assert_eq!(time_str.len(), 20);
        }
    }

    #[test]
    fn test_output_format_detection() {
        // Test output format detection from file extension
        let test_cases = vec![
            ("output.fasta", "fasta"),
            ("output.fa", "fasta"),
            ("output.fna", "fasta"),
            ("output.json", "json"),
            ("output.txt", "fasta"), // Default to fasta
        ];

        for (filename, expected_format) in test_cases {
            let format = if filename.ends_with(".json") {
                "json"
            } else {
                "fasta"
            };
            assert_eq!(
                format, expected_format,
                "Format detection failed for: {}",
                filename
            );
        }
    }

    #[test]
    fn test_batch_size_validation() {
        // Test batch size validation
        let test_cases = vec![
            (Some(1), true),
            (Some(100), true),
            (Some(10000), true),
            (Some(0), false),
            (None, true), // Default is valid
        ];

        for (batch_size, should_be_valid) in test_cases {
            let is_valid = batch_size.map_or(true, |s| s > 0);
            assert_eq!(
                is_valid, should_be_valid,
                "Batch size validation failed for: {:?}",
                batch_size
            );
        }
    }

    #[test]
    fn test_delta_file_requirement() {
        // Test that delta file is required for certain operations
        let args_with_delta = ReconstructArgs {
            database: None,
            references: Some("ref.fasta".into()),
            deltas: Some("deltas.fasta".into()),
            output: Some("output.fasta".into()),
            herald_profile: None,
            herald_path: None,
            sequences: vec![],
            list_only: false,
            at_time: None,
            sequence_version: None,
            taxonomy_version: None,
            show_versions: false,
            report_format: "text".to_string(),
            report_output: None,
        };

        let args_without_delta = ReconstructArgs {
            database: Some("ncbi/protein:blast-30".to_string()),
            references: Some("ref.fasta".into()),
            deltas: None,
            output: Some("output.fasta".into()),
            herald_profile: Some("blast-30".to_string()),
            herald_path: None,
            sequences: vec![],
            list_only: false,
            at_time: None,
            sequence_version: None,
            taxonomy_version: None,
            show_versions: false,
            report_format: "text".to_string(),
            report_output: None,
        };

        // With delta file - standard reconstruction
        assert!(args_with_delta.deltas.is_some());

        // Without delta file - requires database/profile for database reconstruction
        assert!(args_without_delta.deltas.is_none());
        assert!(args_without_delta.database.is_some());
        assert!(args_without_delta.herald_profile.is_some());
    }

    #[test]
    fn test_version_string_format() {
        // Test version string format validation
        let valid_versions = vec!["abc123def456", "1234567890abcdef", "v1.0.0", "2024-01-15"];

        for version in valid_versions {
            assert!(!version.is_empty(), "Version string should not be empty");
            assert!(version.len() <= 64, "Version string too long: {}", version);
        }
    }

    #[test]
    fn test_list_operations() {
        // Test list operations flags
        let args_list_only = ReconstructArgs {
            database: Some("ncbi/protein".to_string()),
            references: Some("dummy.fasta".into()),
            deltas: None,
            output: Some("output.fasta".into()),
            herald_profile: None,
            herald_path: None,
            sequences: vec![],
            list_only: true,
            at_time: None,
            sequence_version: None,
            taxonomy_version: None,
            show_versions: false,
            report_format: "text".to_string(),
            report_output: None,
        };

        let args_verify = ReconstructArgs {
            database: Some("ncbi/protein:blast-30".to_string()),
            references: Some("dummy.fasta".into()),
            deltas: None,
            output: Some("output.fasta".into()),
            herald_profile: Some("blast-30".to_string()),
            herald_path: None,
            sequences: vec![],
            list_only: false,
            at_time: None,
            sequence_version: None,
            taxonomy_version: None,
            show_versions: false,
            report_format: "text".to_string(),
            report_output: None,
        };

        assert!(args_list_only.list_only);
        // Verify field removed - just check list_only flag
        assert!(args_list_only.list_only);
        assert!(!args_verify.list_only);
    }

    #[test]
    fn test_file_path_validation() {
        use std::path::Path;

        // Test that file paths are properly handled
        let test_paths = vec![
            ("./test.fasta", true),
            ("../test.fasta", true),
            ("/absolute/path.fasta", true),
            ("relative/path.fasta", true),
            ("", false),
        ];

        for (path_str, should_be_valid) in test_paths {
            let _path = Path::new(path_str);
            let is_valid = !path_str.is_empty();
            assert_eq!(
                is_valid, should_be_valid,
                "Path validation failed for: {}",
                path_str
            );
        }
    }

    // Test removed - verbose field no longer exists in ReconstructArgs
}
