#![allow(dead_code)]

use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::Args;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Args)]
pub struct TimeTravelArgs {
    /// Path to HERALD repository
    #[arg(short = 'p', long, default_value = ".")]
    pub path: PathBuf,

    /// Sequence data time (e.g., "2024-01-15" or "2024-01-15T12:00:00Z")
    #[arg(short = 's', long)]
    pub sequence_time: String,

    /// Taxonomy data time (e.g., "2024-03-15" or "2024-03-15T12:00:00Z")
    #[arg(short = 't', long)]
    pub taxonomy_time: Option<String>,

    /// Export snapshot to FASTA file
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,

    /// Show detailed statistics
    #[arg(long)]
    pub stats: bool,

    /// Compare with another time point
    #[arg(long)]
    pub diff_with: Option<String>,
}

pub fn run(args: TimeTravelArgs) -> Result<()> {
    use crate::cli::formatting::output::create_standard_table;
    use talaria_herald::{BiTemporalDatabase, HeraldStorage};

    // Parse sequence time
    let sequence_time = parse_time_input(&args.sequence_time)?;

    // Parse taxonomy time (defaults to sequence time if not specified)
    let taxonomy_time = if let Some(tax_time) = &args.taxonomy_time {
        parse_time_input(tax_time)?
    } else {
        sequence_time
    };

    println!("\u{25cf} Opening HERALD repository at {:?}", args.path);

    // Create storage and bi-temporal database
    let storage = Arc::new(HeraldStorage::open(&args.path)?);
    let mut db = BiTemporalDatabase::new(storage)?;

    println!("\u{25cf} Querying database state:");
    println!(
        "  Sequence time: {}",
        sequence_time.format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!(
        "  Taxonomy time: {}",
        taxonomy_time.format("%Y-%m-%d %H:%M:%S UTC")
    );

    // Query the database
    let snapshot = match db.query_at(sequence_time, taxonomy_time) {
        Ok(snap) => snap,
        Err(e) => {
            eprintln!("\u{2717} Failed to query database: {}", e);
            eprintln!("\nNote: The database may not have data for the requested time.");
            eprintln!("Use 'talaria herald history' to see available time points.");
            return Err(e);
        }
    };

    // Display basic information
    println!("\n\u{2713} Database snapshot retrieved:");
    println!("  Total sequences: {}", snapshot.sequence_count());
    println!("  Total chunks: {}", snapshot.chunks().len());
    println!(
        "  Sequence root: {}",
        &snapshot.sequence_root().to_string()[..12]
    );
    println!(
        "  Taxonomy root: {}",
        &snapshot.taxonomy_root().to_string()[..12]
    );

    // Show detailed stats if requested
    if args.stats {
        show_snapshot_stats(&snapshot)?;
    }

    // Handle diff if requested
    if let Some(diff_time_str) = &args.diff_with {
        let diff_time = parse_time_input(diff_time_str)?;

        println!(
            "\n\u{25cf} Computing diff with {}",
            diff_time.format("%Y-%m-%d %H:%M:%S UTC")
        );

        let coord1 = talaria_herald::BiTemporalCoordinate {
            sequence_time,
            taxonomy_time,
        };

        let coord2 = talaria_herald::BiTemporalCoordinate {
            sequence_time: diff_time,
            taxonomy_time: diff_time,
        };

        let diff = db.diff(coord1, coord2)?;

        println!("\n\u{2192} Changes:");
        println!("  Sequences added: {}", diff.sequences_added);
        println!("  Sequences removed: {}", diff.sequences_removed);
        println!("  Taxonomic changes: {}", diff.taxonomic_changes.len());
    }

    // Export to FASTA if requested
    if let Some(output_path) = &args.output {
        println!("\n\u{25cf} Exporting snapshot to {:?}", output_path);
        snapshot.export_fasta(output_path)?;
        println!("\u{2713} Export complete");
    }

    // Show available coordinates
    println!("\n\u{25cf} Available time points:");
    let coords = db.get_available_coordinates()?;

    if coords.is_empty() {
        println!("  No historical data available yet.");
        println!("  Data will be added as the database is updated.");
    } else {
        let mut table = create_standard_table();
        table.set_header(vec!["Sequence Time", "Taxonomy Time"]);

        for coord in coords.iter().take(5) {
            table.add_row(vec![
                coord.sequence_time.format("%Y-%m-%d %H:%M").to_string(),
                coord.taxonomy_time.format("%Y-%m-%d %H:%M").to_string(),
            ]);
        }

        if coords.len() > 5 {
            println!(
                "  (showing first 5 of {} available time points)",
                coords.len()
            );
        }

        println!("{}", table);
    }

    Ok(())
}

fn parse_time_input(input: &str) -> Result<DateTime<Utc>> {
    // Try parsing as full RFC3339 timestamp first
    if let Ok(dt) = DateTime::parse_from_rfc3339(input) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try parsing as date only (assume 00:00:00 UTC)
    if let Ok(dt) = chrono::NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        let time = dt
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid time"))?;
        return Ok(DateTime::from_naive_utc_and_offset(time, Utc));
    }

    // Try parsing as "now", "today", "yesterday"
    match input.to_lowercase().as_str() {
        "now" => Ok(Utc::now()),
        "today" => {
            let today = Utc::now().date_naive();
            let time = today
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| anyhow::anyhow!("Invalid time"))?;
            Ok(DateTime::from_naive_utc_and_offset(time, Utc))
        }
        "yesterday" => {
            let yesterday = Utc::now().date_naive() - chrono::Duration::days(1);
            let time = yesterday
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| anyhow::anyhow!("Invalid time"))?;
            Ok(DateTime::from_naive_utc_and_offset(time, Utc))
        }
        _ => Err(anyhow::anyhow!(
            "Invalid time format '{}'. Use YYYY-MM-DD or RFC3339 format.",
            input
        )),
    }
}

fn show_snapshot_stats(snapshot: &talaria_herald::temporal::DatabaseSnapshot) -> Result<()> {
    use crate::cli::formatting::output::format_number;

    println!("\n\u{2192} Detailed Statistics:");

    // Chunk size distribution
    let chunks = snapshot.chunks();
    if !chunks.is_empty() {
        let total_size: usize = chunks.iter().map(|c| c.size).sum();
        let total_compressed: usize = chunks
            .iter()
            .map(|c| c.compressed_size.unwrap_or(c.size))
            .sum();
        let avg_sequences = chunks.iter().map(|c| c.sequence_count).sum::<usize>() / chunks.len();

        println!("  Chunk statistics:");
        println!("    Total size: {}", format_bytes(total_size));
        println!("    Compressed size: {}", format_bytes(total_compressed));
        println!(
            "    Compression ratio: {:.1}%",
            (total_compressed as f64 / total_size as f64) * 100.0
        );
        println!(
            "    Average sequences per chunk: {}",
            format_number(avg_sequences)
        );

        // Taxonomy distribution
        let mut taxon_counts = std::collections::HashMap::new();
        for chunk in &chunks {
            for taxon in &chunk.taxon_ids {
                *taxon_counts.entry(taxon).or_insert(0) += 1;
            }
        }

        if !taxon_counts.is_empty() {
            println!("\n  Taxonomy distribution:");
            println!("    Unique taxa: {}", format_number(taxon_counts.len()));
            println!("    Most common taxa (by chunk presence):");

            let mut sorted_taxa: Vec<_> = taxon_counts.iter().collect();
            sorted_taxa.sort_by_key(|(_, count)| std::cmp::Reverse(**count));

            for (taxon, count) in sorted_taxa.iter().take(5) {
                println!("      TaxID {}: {} chunks", taxon.0, count);
            }
        }
    }

    Ok(())
}

fn format_bytes(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_idx])
}
