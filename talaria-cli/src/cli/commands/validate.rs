use anyhow::Result;
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

/// Validate database reduction from SEQUOIA system
fn validate_from_sequoia(_db_ref_str: &str, profile: String) -> Result<()> {
    use talaria_sequoia::SEQUOIAStorage;
    use talaria_sequoia::{Validator, verification::validator::{ValidationOptions, TemporalManifestValidator}};
    use crate::cli::formatting::output::*;
    use talaria_utils::display::format::format_bytes;
    use crate::cli::progress::create_spinner;

    let pb = create_spinner("Initializing SEQUOIA storage...");

    // Initialize SEQUOIA storage
    let sequoia_path = talaria_core::system::paths::talaria_databases_dir();
    let storage = SEQUOIAStorage::open(&sequoia_path)?;

    pb.set_message("Loading reduction manifest...");

    // Get the reduction manifest for the profile
    let manifest = storage
        .get_reduction_by_profile(&profile)?
        .ok_or_else(|| anyhow::anyhow!("Reduction manifest not found for profile: {}", profile))?;

    // Get the temporal manifest from the manifest file
    let manifest_path = sequoia_path.join("manifest.json");
    let temporal_manifest: talaria_sequoia::TemporalManifest = if manifest_path.exists() {
        let data = std::fs::read_to_string(&manifest_path)?;
        serde_json::from_str(&data)?
    } else {
        anyhow::bail!("Temporal manifest not found at {:?}", manifest_path);
    };

    pb.set_message("Validating manifest integrity...");

    // Create validator and validation options
    let chunks_dir = sequoia_path.join("chunks");
    let validator = Validator::new(chunks_dir);
    let options = ValidationOptions {
        verify_hashes: true,
        check_storage: true,
        verify_sizes: true,
        check_overlaps: false,
        check_metadata: true,
        fail_fast: false,
        max_chunks: 0, // Check all chunks
    };

    // Perform validation
    let validation_result =
        futures::executor::block_on(validator.validate(&temporal_manifest, options))?;

    pb.finish_and_clear();

    // Display validation results as tree
    if validation_result.is_valid {
        success("✓ Manifest validation successful");
    } else {
        error(&format!(
            "✗ Manifest validation failed with {} errors",
            validation_result.errors.len()
        ));
    }

    subsection_header("Validation Details");

    tree_item(false, "Reduction profile", Some(&profile));
    tree_item(
        false,
        "Reference chunks",
        Some(&manifest.reference_chunks.len().to_string()),
    );
    tree_item(
        false,
        "Delta chunks",
        Some(&manifest.delta_chunks.len().to_string()),
    );
    tree_item(
        false,
        "Coverage",
        Some(&format!(
            "{:.1}%",
            manifest.statistics.sequence_coverage * 100.0
        )),
    );

    // Validation statistics
    tree_item(
        false,
        "Chunks validated",
        Some(&validation_result.stats.chunks_validated.to_string()),
    );
    tree_item(
        false,
        "Bytes validated",
        Some(&format_bytes(
            validation_result.stats.bytes_validated as u64,
        )),
    );
    tree_item(
        false,
        "Chunks verified",
        Some(&validation_result.stats.chunks_verified.to_string()),
    );
    tree_item(
        true,
        "Validation time",
        Some(&format!("{}ms", validation_result.stats.validation_time_ms)),
    );

    // Show errors if any
    if !validation_result.errors.is_empty() {
        subsection_header("Validation Errors");
        for (i, error) in validation_result.errors.iter().enumerate() {
            let is_last = i == validation_result.errors.len() - 1;
            tree_item(
                is_last,
                &format!("Error {}", i + 1),
                Some(&format!("{:?}", error)),
            );
        }
    }

    // Show warnings if any
    if !validation_result.warnings.is_empty() {
        subsection_header("Warnings");
        for (i, warning) in validation_result.warnings.iter().enumerate() {
            let is_last = i == validation_result.warnings.len() - 1;
            tree_item(is_last, &format!("Warning {}", i + 1), Some(warning));
        }
    }

    Ok(())
}

pub fn run(args: ValidateArgs) -> anyhow::Result<()> {
    use crate::cli::formatting::output::*;
    use talaria_utils::display::format::{format_bytes, get_file_size};
    use crate::cli::progress::create_spinner;

    section_header("Validation Report");

    let pb = create_spinner("Initializing validation...");

    // Validate arguments: either database or all file paths must be specified
    if args.database.is_none()
        && (args.original.is_none() || args.reduced.is_none() || args.deltas.is_none())
    {
        anyhow::bail!("Must specify either a database reference or all three files (-o, -r, -d)");
    }

    if args.database.is_some()
        && (args.original.is_some() || args.reduced.is_some() || args.deltas.is_some())
    {
        anyhow::bail!(
            "Cannot specify both database reference and file paths. Use one or the other."
        );
    }

    // Resolve file paths based on input method
    let (original_path, reduced_path, deltas_path) = if let Some(db_ref_str) = &args.database {
        // Parse database reference with reduction profile
        let (_base_ref, profile) = parse_database_with_profile(db_ref_str)?;

        // Profile is required for validation
        let _profile = profile.ok_or_else(|| anyhow::anyhow!(
            "Reduction profile required for validation. Use format: 'database:profile' (e.g., 'uniprot/swissprot:blast-30')"
        ))?;

        // Implement database validation for SEQUOIA
        validate_from_sequoia(db_ref_str, _profile.to_string())?;
        return Ok(());
    } else {
        // Traditional file-based usage
        let original = args
            .original
            .ok_or_else(|| anyhow::anyhow!("Original file (-o) is required"))?;
        let reduced = args
            .reduced
            .ok_or_else(|| anyhow::anyhow!("Reduced file (-r) is required"))?;
        let deltas = args
            .deltas
            .ok_or_else(|| anyhow::anyhow!("Delta file (-d) is required"))?;

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
    let original_seqs = talaria_bio::parse_fasta(&original_path)?;
    pb.set_message(format!(
        "Loaded {} original sequences ({})",
        original_seqs.len(),
        format_bytes(original_size)
    ));

    pb.set_message("Loading reduced FASTA file...");
    let reduced_seqs = talaria_bio::parse_fasta(&reduced_path)?;
    pb.set_message(format!(
        "Loaded {} reference sequences ({})",
        reduced_seqs.len(),
        format_bytes(reduced_size)
    ));

    pb.set_message("Loading delta metadata...");
    let deltas = talaria_storage::io::metadata::load_metadata(&deltas_path)?;
    pb.set_message(format!("Loaded {} delta records", deltas.len()));

    // Calculate coverage metrics
    pb.set_message("Calculating validation metrics...");
    let validator = talaria_sequoia::operations::validator::ValidatorImpl::new();
    let metrics = validator.calculate_metrics(
        &original_seqs,
        &reduced_seqs,
        &deltas,
        original_size,
        reduced_size,
    )?;

    // Compare alignment results if provided
    if let (Some(orig_results), Some(red_results)) = (args.original_results, args.reduced_results) {
        pb.set_message("Comparing alignment results...");
        let alignment_metrics = validator.compare_alignments(&orig_results, &red_results)?;
        println!(
            "\nAlignment similarity: {:.2}%",
            alignment_metrics.similarity * 100.0
        );
    }

    pb.finish_and_clear();

    // Print results using tree structure
    subsection_header("Coverage Metrics");

    let coverage_items = vec![
        (
            "Sequences",
            format!("{:.1}%", metrics.sequence_coverage * 100.0),
        ),
        (
            "Taxonomy",
            format!("{:.1}%", metrics.taxonomic_coverage * 100.0),
        ),
    ];
    tree_section("Coverage", coverage_items, false);

    let reduction_items = vec![
        ("References", format_number(metrics.reference_count)),
        ("Deltas", format_number(metrics.child_count)),
        ("Total Original", format_number(metrics.total_sequences)),
        (
            "Ratio",
            format!(
                "{:.1}%",
                (metrics.reference_count as f64 / metrics.total_sequences as f64) * 100.0
            ),
        ),
    ];
    tree_section("Reduction", reduction_items, false);

    let size_items = vec![
        ("Original", format_bytes(metrics.original_file_size)),
        ("Reduced", format_bytes(metrics.reduced_file_size)),
        (
            "Compression",
            format!(
                "{:.1}%",
                (1.0 - metrics.reduced_file_size as f64 / metrics.original_file_size as f64)
                    * 100.0
            ),
        ),
    ];
    tree_section("File Sizes", size_items, false);

    // Status indicator
    if metrics.sequence_coverage > 0.99 && metrics.taxonomic_coverage > 0.95 {
        tree_item(true, "Status", Some("✓ Valid"));
    } else if metrics.sequence_coverage > 0.95 {
        tree_item(true, "Status", Some("⚠ Partial Coverage"));
    } else {
        tree_item(true, "Status", Some("✗ Low Coverage"));
    }

    if let Some(report_path) = args.report {
        // Use Reporter trait based on file extension
        use talaria_utils::report::create_reporter_from_path;
        use talaria_utils::report::traits::{
            ReportData, ReportSection, ReportStatistics, SectionContent, StatValue,
        };

        // Create report metadata
        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            "description".to_string(),
            format!(
                "Validation report for {} sequences",
                metrics.total_sequences
            ),
        );
        metadata.insert("footer".to_string(), "Generated by Talaria".to_string());

        // Create statistics
        let mut custom_stats = std::collections::HashMap::new();
        custom_stats.insert(
            "coverage".to_string(),
            StatValue::Float(metrics.sequence_coverage),
        );
        custom_stats.insert(
            "status".to_string(),
            StatValue::String(
                if metrics.sequence_coverage > 0.99 {
                    "valid"
                } else {
                    "partial"
                }
                .to_string(),
            ),
        );

        let statistics = ReportStatistics {
            total_sequences: metrics.total_sequences,
            total_size: metrics.original_file_size as usize,
            processing_time_ms: 0,
            custom_stats,
        };

        // Create report data
        let report_data = ReportData {
            title: "Talaria Validation Report".to_string(),
            timestamp: chrono::Utc::now(),
            sections: vec![
                ReportSection {
                    title: "Coverage Metrics".to_string(),
                    content: SectionContent::Text(format!(
                        "Sequence Coverage: {:.1}%\nTaxonomic Coverage: {:.1}%",
                        metrics.sequence_coverage * 100.0,
                        metrics.taxonomic_coverage * 100.0
                    )),
                    level: 2,
                },
                ReportSection {
                    title: "Reduction Statistics".to_string(),
                    content: SectionContent::Text(format!(
                        "References: {}\nDeltas: {}\nTotal Original: {}\nReduction Ratio: {:.1}%",
                        metrics.reference_count,
                        metrics.child_count,
                        metrics.total_sequences,
                        (metrics.reference_count as f64 / metrics.total_sequences as f64) * 100.0
                    )),
                    level: 2,
                },
                ReportSection {
                    title: "File Sizes".to_string(),
                    content: SectionContent::Text(format!(
                        "Original: {}\nReduced: {}\nCompression: {:.1}%",
                        format_bytes(metrics.original_file_size),
                        format_bytes(metrics.reduced_file_size),
                        (1.0 - metrics.reduced_file_size as f64
                            / metrics.original_file_size as f64)
                            * 100.0
                    )),
                    level: 2,
                },
            ],
            metadata,
            statistics,
        };

        let reporter = create_reporter_from_path(&report_path);
        let report_content = reporter.generate(&report_data)?;
        reporter.export(&report_content, &report_path)?;

        info(&format!(
            "Detailed report saved to {:?} ({})",
            report_path,
            reporter.name()
        ));
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
