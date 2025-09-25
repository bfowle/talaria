#![allow(dead_code)]

use clap::Args;
use colored::Colorize;
use std::collections::HashMap;

use talaria_sequoia::taxonomy::discrepancy::DiscrepancyDetector;
use talaria_sequoia::{DiscrepancyType, TaxonId, TaxonomicDiscrepancy};
use crate::cli::global_config;
use crate::core::database::database_manager::DatabaseManager;
use crate::cli::progress::create_spinner;

#[derive(Args)]
pub struct CheckDiscrepanciesArgs {
    /// Database name or path
    #[arg(value_name = "DATABASE")]
    pub database: String,

    /// Only show discrepancies of specific type
    #[arg(short = 't', long = "type", value_name = "TYPE")]
    pub discrepancy_type: Option<String>,

    /// Export results to JSON
    #[arg(long = "json", value_name = "FILE")]
    pub json_output: Option<String>,
}

pub fn run(args: CheckDiscrepanciesArgs) -> anyhow::Result<()> {
    let spinner = create_spinner("Loading database...");
    let manager = DatabaseManager::new(None)?;

    // Parse database reference to check for profile
    let db_ref = crate::core::database::database_ref::parse_database_reference(&args.database)?;

    // Load appropriate manifest based on whether profile is specified
    let (chunk_metadata, _total_sequences) = if let Some(profile) = &db_ref.profile {
        // Load reduction manifest for profile
        spinner.set_message(format!("Loading reduction profile: {}...", profile));

        let versions_dir = talaria_core::system::paths::talaria_databases_dir().join("versions");
        let version = db_ref.version.as_deref().unwrap_or("current");
        let profile_path = versions_dir
            .join(&db_ref.source)
            .join(&db_ref.dataset)
            .join(version)
            .join("profiles")
            .join(format!("{}.tal", profile));

        if !profile_path.exists() {
            anyhow::bail!("Profile '{}' not found for {}/{}", profile, db_ref.source, db_ref.dataset);
        }

        // Read and parse reduction manifest
        let mut content = std::fs::read(&profile_path)?;
        if content.starts_with(b"TAL") && content.len() > 4 {
            content = content[4..].to_vec();
        }

        let reduction_manifest: talaria_sequoia::ReductionManifest =
            rmp_serde::from_slice(&content)?;

        // Convert reference chunks to chunk metadata format
        let mut chunk_metadata = Vec::new();
        for ref_chunk in &reduction_manifest.reference_chunks {
            chunk_metadata.push(talaria_sequoia::ManifestMetadata {
                hash: ref_chunk.chunk_hash.clone(),
                taxon_ids: ref_chunk.taxon_ids.clone(),
                sequence_count: ref_chunk.sequence_count,
                size: ref_chunk.size,
                compressed_size: ref_chunk.compressed_size,
            });
        }

        spinner.finish_and_clear();
        println!("\n{}", "═".repeat(60));
        println!("{:^60}", format!("DISCREPANCY CHECK: {} (REDUCED)", args.database));
        println!("{}", "═".repeat(60));
        println!();

        (chunk_metadata, reduction_manifest.statistics.reference_sequences)
    } else {
        // Load regular database manifest
        let manifest = manager.get_manifest(&args.database)?;
        let total_seqs = manifest.chunk_index.iter().map(|c| c.sequence_count).sum();

        spinner.finish_and_clear();
        println!("\n{}", "═".repeat(60));
        println!("{:^60}", format!("DISCREPANCY CHECK: {}", args.database));
        println!("{}", "═".repeat(60));
        println!();

        (manifest.chunk_index, total_seqs)
    };

    // Initialize the discrepancy detector
    let mut detector = DiscrepancyDetector::new();

    // Load taxonomy mappings if available (this shows its own progress bar)
    let spinner = create_spinner("Loading taxonomy mappings...");
    spinner.finish_and_clear(); // Clear spinner before progress bar appears

    if let Ok(mappings) = manager.load_taxonomy_mappings(&args.database) {
        detector.set_taxonomy_mappings(mappings);
    }

    // Now analyze sequences for discrepancies
    let spinner = create_spinner("Analyzing sequences for discrepancies...");

    let mut all_discrepancies = Vec::new();
    let mut chunk_count = 0;
    let mut sequence_count = 0;

    // Process each chunk
    for chunk_meta in &chunk_metadata {
        chunk_count += 1;

        // Try new manifest-based approach
        match manager.load_manifest(&chunk_meta.hash) {
            Ok(manifest) => {
                // Load sequences from canonical storage
                match manager.load_sequences_from_manifest(&manifest, None, usize::MAX) {
                    Ok(sequences) => {
                        sequence_count += sequences.len();

                        // Detect discrepancies using the new method
                        let chunk_discrepancies = detector.detect_from_manifest(&manifest, sequences);
                        all_discrepancies.extend(chunk_discrepancies);
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to load sequences for chunk {}: {}", chunk_meta.hash, e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to load manifest for chunk {}: {}", chunk_meta.hash, e);
            }
        }
    }

    spinner.finish_and_clear();

    // Filter by type if requested
    let filtered_discrepancies: Vec<_> = if let Some(ref filter_type) = args.discrepancy_type {
        all_discrepancies
            .into_iter()
            .filter(|d| matches_type(d, filter_type))
            .collect()
    } else {
        all_discrepancies
    };

    // Group by type for summary
    let mut by_type: HashMap<String, Vec<&TaxonomicDiscrepancy>> = HashMap::new();
    for disc in &filtered_discrepancies {
        let type_str = format_discrepancy_type(&disc.discrepancy_type);
        by_type.entry(type_str).or_default().push(disc);
    }

    // Display summary
    println!("{} {} chunks analyzed", "►".cyan().bold(), chunk_count);
    println!("{} {} sequences checked", "►".cyan().bold(), sequence_count);
    println!(
        "{} {} discrepancies found",
        if filtered_discrepancies.is_empty() {
            "✓".green()
        } else {
            "⚠".yellow()
        }
        .bold(),
        filtered_discrepancies.len()
    );

    if !filtered_discrepancies.is_empty() {
        println!("\n{}", "Discrepancy Summary:".bold().underline());
        for (type_name, discs) in &by_type {
            println!("  {} {}: {}", "•".cyan(), type_name.bold(), discs.len());
        }

        // Use global verbose flag for detailed output
        if global_config::is_verbose() {
            println!("\n{}", "Detailed Discrepancies:".bold().underline());
            for (i, disc) in filtered_discrepancies.iter().enumerate() {
                if i > 0 {
                    println!("  {}", "─".repeat(50));
                }
                print_discrepancy(disc, i + 1);
            }
        } else {
            println!("\nUse {} for detailed output", "--verbose".cyan());
        }
    }

    // Export to JSON if requested
    if let Some(json_path) = args.json_output {
        let json = serde_json::to_string_pretty(&filtered_discrepancies)?;
        std::fs::write(&json_path, json)?;
        println!(
            "\n{} Results exported to: {}",
            "✓".green().bold(),
            json_path.cyan()
        );
    }

    Ok(())
}

fn matches_type(disc: &TaxonomicDiscrepancy, filter: &str) -> bool {
    let type_str = format_discrepancy_type(&disc.discrepancy_type).to_lowercase();
    filter.to_lowercase() == type_str
}

fn format_discrepancy_type(disc_type: &DiscrepancyType) -> String {
    match disc_type {
        DiscrepancyType::Missing => "Missing".to_string(),
        DiscrepancyType::Conflict => "Conflict".to_string(),
        DiscrepancyType::Outdated => "Outdated".to_string(),
        DiscrepancyType::Reclassified => "Reclassified".to_string(),
        DiscrepancyType::Invalid => "Invalid".to_string(),
    }
}

fn print_discrepancy(disc: &TaxonomicDiscrepancy, index: usize) {
    println!("\n  {}. {}", index, disc.sequence_id.bold());
    println!(
        "     Type: {}",
        format_discrepancy_type(&disc.discrepancy_type).yellow()
    );
    println!("     Confidence: {:.2}%", disc.confidence * 100.0);

    if let Some(header_taxon) = &disc.header_taxon {
        println!("     Header claims: {}", format_taxon(header_taxon));
    }

    if let Some(mapped_taxon) = &disc.mapped_taxon {
        println!("     Mapping says: {}", format_taxon(mapped_taxon));
    }

    if let Some(inferred_taxon) = &disc.inferred_taxon {
        println!("     Inferred as: {}", format_taxon(inferred_taxon));
    }

    println!(
        "     Detected: {}",
        disc.detection_date.format("%Y-%m-%d %H:%M:%S")
    );
}

fn format_taxon(taxon: &TaxonId) -> String {
    format!("taxid:{}", taxon.0)
}
