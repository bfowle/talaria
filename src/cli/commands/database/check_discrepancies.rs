use clap::Args;
use colored::Colorize;
use std::collections::HashMap;

use crate::casg::taxonomy::discrepancy::DiscrepancyDetector;
use crate::casg::types::{DiscrepancyType, TaxonId, TaxonomicDiscrepancy};
use crate::core::database_manager::DatabaseManager;
use crate::utils::progress::create_spinner;

#[derive(Args)]
pub struct CheckDiscrepanciesArgs {
    /// Database name or path
    #[arg(value_name = "DATABASE")]
    pub database: String,

    /// Only show discrepancies of specific type
    #[arg(short = 't', long = "type", value_name = "TYPE")]
    pub discrepancy_type: Option<String>,

    /// Show detailed output
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Export results to JSON
    #[arg(long = "json", value_name = "FILE")]
    pub json_output: Option<String>,
}

pub fn run(args: CheckDiscrepanciesArgs) -> anyhow::Result<()> {
    let spinner = create_spinner("Loading database...");
    let manager = DatabaseManager::new(None)?;

    // Get the database manifest
    let manifest = manager.get_manifest(&args.database)?;
    spinner.finish_and_clear();

    println!("\n{}", "═".repeat(60));
    println!("{:^60}", format!("DISCREPANCY CHECK: {}", args.database));
    println!("{}", "═".repeat(60));
    println!();

    let spinner = create_spinner("Analyzing sequences for discrepancies...");

    // Initialize the discrepancy detector
    let mut detector = DiscrepancyDetector::new();

    // Load taxonomy mappings if available
    if let Ok(mappings) = manager.load_taxonomy_mappings(&args.database) {
        detector.set_taxonomy_mappings(mappings);
    }

    let mut all_discrepancies = Vec::new();
    let mut chunk_count = 0;
    let mut sequence_count = 0;

    // Process each chunk in the manifest
    for chunk_meta in &manifest.chunk_index {
        chunk_count += 1;

        // Load the actual chunk data
        if let Ok(chunk_data) = manager.load_chunk(&chunk_meta.hash) {
            sequence_count += chunk_data.sequences.len();

            // Detect discrepancies in this chunk
            let chunk_discrepancies = detector.detect(&chunk_data);
            all_discrepancies.extend(chunk_discrepancies);
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

        if args.verbose {
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
