#![allow(dead_code)]

use talaria_sequoia::{SEQUOIARepository, DiffResult};
use talaria_sequoia::operations::{TemporalManifestDiffer, StandardTemporalManifestDiffer, DiffOptions, ChangeType,
                                   DatabaseDiffer, format_bytes};
use talaria_core::system::paths;
use clap::Args;
use colored::*;
use std::path::{Path, PathBuf};

#[derive(Args)]
pub struct DiffArgs {
    /// First database or version to compare
    #[arg(value_name = "FROM")]
    pub from: String,

    /// Second database or version to compare
    #[arg(value_name = "TO")]
    pub to: String,

    /// Show detailed chunk-level differences
    #[arg(long, short = 'd')]
    pub detailed: bool,

    /// Show only summary statistics
    #[arg(long, short = 's')]
    pub summary: bool,

    /// Show taxonomy differences
    #[arg(long, short = 't')]
    pub taxonomy: bool,

    /// Show sequence-level comparisons
    #[arg(long)]
    pub sequences: bool,

    /// Show chunk-level comparisons (default)
    #[arg(long)]
    pub chunks: bool,

    /// Show all comparison types
    #[arg(long, short = 'a')]
    pub all: bool,

    /// Export diff to JSON file
    #[arg(long, value_name = "FILE")]
    pub export: Option<PathBuf>,

    /// First sequence date for bi-temporal comparison (e.g., "2020-01-01")
    #[arg(long)]
    pub sequence_date: Option<String>,

    /// First taxonomy date for bi-temporal comparison
    #[arg(long)]
    pub taxonomy_date: Option<String>,

    /// Second sequence date for bi-temporal comparison (vs-)
    #[arg(long)]
    pub vs_sequence_date: Option<String>,

    /// Second taxonomy date for bi-temporal comparison (vs-)
    #[arg(long)]
    pub vs_taxonomy_date: Option<String>,
}

pub fn run(args: DiffArgs) -> anyhow::Result<()> {
    // Check if we need bi-temporal diff
    if args.sequence_date.is_some() || args.taxonomy_date.is_some() ||
       args.vs_sequence_date.is_some() || args.vs_taxonomy_date.is_some() {
        return run_bitemporal_diff(args);
    }

    // Parse the from/to specifications
    let (from_path, from_version) = parse_spec(&args.from)?;
    let (to_path, to_version) = parse_spec(&args.to)?;

    // Check if we should use the new comprehensive diff
    if args.all || args.sequences || (!args.detailed && !args.summary && !args.chunks) {
        return run_comprehensive_diff(args, from_path, to_path);
    }

    println!(
        "{} Computing differences between '{}' and '{}'...",
        "►".cyan().bold(),
        args.from,
        args.to
    );

    // Load repositories
    let from_repo = SEQUOIARepository::open(&from_path)?;
    let to_repo = SEQUOIARepository::open(&to_path)?;

    // Get manifests at specified versions
    let from_manifest = if let Some(version) = from_version {
        from_repo.temporal.get_manifest_at_version(&version)?
    } else {
        from_repo.manifest.clone()
    };

    let to_manifest = if let Some(version) = to_version {
        to_repo.temporal.get_manifest_at_version(&version)?
    } else {
        to_repo.manifest.clone()
    };

    // Get the actual manifest data
    let from_data = from_manifest.get_data()
        .ok_or_else(|| anyhow::anyhow!("No manifest data in 'from' database"))?;
    let to_data = to_manifest.get_data()
        .ok_or_else(|| anyhow::anyhow!("No manifest data in 'to' database"))?;

    // Compute differences using async runtime
    let runtime = tokio::runtime::Runtime::new()?;
    let diff_result = runtime.block_on(async {
        let differ = StandardTemporalManifestDiffer;
        differ.diff(from_data, to_data, DiffOptions::default()).await
    })?;

    // Display results
    if args.summary {
        display_summary(&diff_result)?;
    } else if args.detailed {
        display_detailed(&diff_result)?;
    } else {
        display_normal(&diff_result)?;
    }

    if args.taxonomy {
        display_taxonomy_diff(&from_repo, &to_repo)?;
    }

    // Export if requested
    if let Some(export_path) = args.export {
        export_diff(&diff_result, &export_path)?;
        println!(
            "{} Diff exported to {}",
            "✓".green().bold(),
            export_path.display()
        );
    }

    Ok(())
}

fn parse_spec(spec: &str) -> anyhow::Result<(PathBuf, Option<String>)> {
    if let Some((db, version)) = spec.split_once('@') {
        // Format: database@version
        let path = if db.contains('/') {
            PathBuf::from(db)
        } else {
            paths::talaria_databases_dir().join("data").join(db)
        };
        Ok((path, Some(version.to_string())))
    } else {
        // Just database name or path
        let path = if spec.contains('/') {
            PathBuf::from(spec)
        } else {
            paths::talaria_databases_dir().join("data").join(spec)
        };
        Ok((path, None))
    }
}

fn display_summary(diff: &DiffResult) -> anyhow::Result<()> {
    println!("\n{}", "═".repeat(60));
    println!("{:^60}", "DIFF SUMMARY");
    println!("{}", "═".repeat(60));

    let stats = &diff.stats;
    println!("{} {} chunks", "Added:".green().bold(), stats.chunks_added);
    println!("{} {} chunks", "Removed:".red().bold(), stats.chunks_removed);
    println!("{} {} chunks", "Modified:".yellow().bold(), stats.chunks_modified);
    println!("{} {} chunks", "Moved:".blue().bold(), stats.chunks_moved);

    let size_mb = stats.total_size_delta.abs() as f64 / 1_048_576.0;
    if stats.total_size_delta > 0 {
        println!("{} +{:.2} MB", "Size change:".bold(), size_mb);
    } else if stats.total_size_delta < 0 {
        println!("{} -{:.2} MB", "Size change:".bold(), size_mb);
    } else {
        println!("{} No size change", "Size change:".bold());
    }

    println!("{} {} sequences", "Affected:".bold(), stats.sequences_affected);
    println!("{} {:.1}%", "Change rate:".bold(), stats.change_percentage);

    Ok(())
}

fn display_normal(diff: &DiffResult) -> anyhow::Result<()> {
    println!("\n{}", "─".repeat(60));
    println!("{:^60}", "DIFFERENCES");
    println!("{}", "─".repeat(60));

    // Group changes by type
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();
    let mut moved = Vec::new();

    for change in &diff.changes {
        match change.change_type {
            ChangeType::Added => added.push(change),
            ChangeType::Removed => removed.push(change),
            ChangeType::Modified => modified.push(change),
            ChangeType::Moved => moved.push(change),
        }
    }

    if !added.is_empty() {
        println!("\n{} ({}):", "Added chunks".green().bold(), added.len());
        for (i, change) in added.iter().enumerate() {
            if i >= 10 {
                println!("  ... and {} more", added.len() - 10);
                break;
            }
            if let Some(new) = &change.new_chunk {
                println!(
                    "  + {} ({:.1} KB)",
                    &new.hash.to_hex()[..12],
                    new.size as f64 / 1024.0
                );
            }
        }
    }

    if !removed.is_empty() {
        println!("\n{} ({}):", "Removed chunks".red().bold(), removed.len());
        for (i, change) in removed.iter().enumerate() {
            if i >= 10 {
                println!("  ... and {} more", removed.len() - 10);
                break;
            }
            if let Some(old) = &change.old_chunk {
                println!(
                    "  - {} ({:.1} KB)",
                    &old.hash.to_hex()[..12],
                    old.size as f64 / 1024.0
                );
            }
        }
    }

    if !modified.is_empty() {
        println!("\n{} ({}):", "Modified chunks".yellow().bold(), modified.len());
        for (i, change) in modified.iter().enumerate() {
            if i >= 10 {
                println!("  ... and {} more", modified.len() - 10);
                break;
            }
            if let (Some(old), Some(new)) = (&change.old_chunk, &change.new_chunk) {
                println!(
                    "  ~ {} -> {} ({:.1} KB -> {:.1} KB)",
                    &old.hash.to_hex()[..12],
                    &new.hash.to_hex()[..12],
                    old.size as f64 / 1024.0,
                    new.size as f64 / 1024.0
                );
            }
        }
    }

    if !moved.is_empty() {
        println!("\n{} ({}):", "Moved chunks".blue().bold(), moved.len());
        for (i, _change) in moved.iter().enumerate() {
            if i >= 10 {
                println!("  ... and {} more", moved.len() - 10);
                break;
            }
            println!("  ↻ Chunk relocated");
        }
    }

    Ok(())
}

fn display_detailed(diff: &DiffResult) -> anyhow::Result<()> {
    display_normal(diff)?;

    println!("\n{}", "─".repeat(60));
    println!("{:^60}", "DETAILED ANALYSIS");
    println!("{}", "─".repeat(60));

    // Show upgrade requirements if any
    if !diff.upgrade_requirements.is_empty() {
        println!("\n{}:", "Upgrade requirements".red().bold());
        for req in &diff.upgrade_requirements {
            println!("  • {}", req);
        }
    }

    // Note: Taxonomy distribution would require loading chunks to get taxon info
    // This is a placeholder for future enhancement
    println!("\n{}:", "Note".bold());
    println!("  Taxonomy distribution analysis not yet available");

    Ok(())
}

fn display_taxonomy_diff(_from: &SEQUOIARepository, _to: &SEQUOIARepository) -> anyhow::Result<()> {
    println!("\n{}", "─".repeat(60));
    println!("{:^60}", "TAXONOMY DIFFERENCES");
    println!("{}", "─".repeat(60));

    // This would require implementing taxonomy comparison methods
    println!("{} Taxonomy comparison not yet implemented", "⚠".yellow().bold());

    Ok(())
}

fn run_bitemporal_diff(args: DiffArgs) -> anyhow::Result<()> {
    use talaria_sequoia::{SEQUOIAStorage, BiTemporalDatabase};
    use std::sync::Arc;
    use chrono::Utc;

    println!(
        "{} Computing bi-temporal differences...",
        "►".cyan().bold()
    );

    // Parse database path from the first argument
    let (db_path, _) = parse_spec(&args.from)?;

    // Parse times for first coordinate
    let sequence_time1 = if let Some(date_str) = &args.sequence_date {
        parse_time_input(date_str)?
    } else {
        Utc::now()
    };

    let taxonomy_time1 = if let Some(date_str) = &args.taxonomy_date {
        parse_time_input(date_str)?
    } else {
        sequence_time1
    };

    // Parse times for second coordinate
    let sequence_time2 = if let Some(date_str) = &args.vs_sequence_date {
        parse_time_input(date_str)?
    } else {
        Utc::now()
    };

    let taxonomy_time2 = if let Some(date_str) = &args.vs_taxonomy_date {
        parse_time_input(date_str)?
    } else {
        sequence_time2
    };

    println!("  First point:  sequence={}, taxonomy={}",
             sequence_time1.format("%Y-%m-%d"),
             taxonomy_time1.format("%Y-%m-%d"));
    println!("  Second point: sequence={}, taxonomy={}",
             sequence_time2.format("%Y-%m-%d"),
             taxonomy_time2.format("%Y-%m-%d"));

    // Open SEQUOIA storage and bi-temporal database
    let storage = Arc::new(SEQUOIAStorage::open(&db_path)?);
    let mut bi_temporal_db = BiTemporalDatabase::new(storage)?;

    // Create coordinates
    let coord1 = talaria_sequoia::BiTemporalCoordinate {
        sequence_time: sequence_time1,
        taxonomy_time: taxonomy_time1,
    };

    let coord2 = talaria_sequoia::BiTemporalCoordinate {
        sequence_time: sequence_time2,
        taxonomy_time: taxonomy_time2,
    };

    // Compute diff
    let diff = bi_temporal_db.diff(coord1.clone(), coord2.clone())?;

    // Display results
    println!("\n{}", "═".repeat(60));
    println!("{}", "BI-TEMPORAL DIFF RESULTS".bold());
    println!("{}", "═".repeat(60));

    println!("\n{}", "Sequence Changes:".bold());
    println!("  {} Sequences added:   {}", "+".green().bold(), diff.sequences_added);
    println!("  {} Sequences removed: {}", "-".red().bold(), diff.sequences_removed);

    if args.taxonomy && !diff.taxonomic_changes.is_empty() {
        println!("\n{}", "Taxonomy Changes:".bold());
        for change in diff.taxonomic_changes.iter().take(10) {
            match change.change_type {
                talaria_sequoia::TaxonomicChangeType::Reclassified => {
                    println!("  {} TaxID {} reclassified from {:?} to {:?}",
                             "↻".yellow(),
                             change.taxon_id.0,
                             change.old_parent.map(|t| t.0),
                             change.new_parent.map(|t| t.0));
                }
                talaria_sequoia::TaxonomicChangeType::New => {
                    println!("  {} TaxID {} newly added",
                             "+".green(),
                             change.taxon_id.0);
                }
                talaria_sequoia::TaxonomicChangeType::Deprecated => {
                    println!("  {} TaxID {} deprecated",
                             "✗".red(),
                             change.taxon_id.0);
                }
                _ => {}
            }
        }
        if diff.taxonomic_changes.len() > 10 {
            println!("  ... and {} more changes", diff.taxonomic_changes.len() - 10);
        }
    }

    // Export if requested
    if let Some(export_path) = &args.export {
        let export_data = serde_json::json!({
            "coord1": {
                "sequence_time": coord1.sequence_time.to_rfc3339(),
                "taxonomy_time": coord1.taxonomy_time.to_rfc3339(),
            },
            "coord2": {
                "sequence_time": coord2.sequence_time.to_rfc3339(),
                "taxonomy_time": coord2.taxonomy_time.to_rfc3339(),
            },
            "sequences_added": diff.sequences_added,
            "sequences_removed": diff.sequences_removed,
            "taxonomic_changes": diff.taxonomic_changes.len(),
        });

        std::fs::write(export_path, serde_json::to_string_pretty(&export_data)?)?;
        println!("\n{} Diff exported to: {}", "✓".green().bold(), export_path.display());
    }

    Ok(())
}

fn parse_time_input(input: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    use chrono::{DateTime, NaiveDate, Utc};

    // Try parsing as full RFC3339 timestamp first
    if let Ok(dt) = DateTime::parse_from_rfc3339(input) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try parsing as date only (assume 00:00:00 UTC)
    if let Ok(dt) = NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        let time = dt.and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid time"))?;
        return Ok(DateTime::from_naive_utc_and_offset(time, Utc));
    }

    anyhow::bail!("Invalid time format '{}'. Use YYYY-MM-DD or RFC3339 format.", input)
}

fn export_diff(diff: &DiffResult, path: &Path) -> anyhow::Result<()> {
    use std::fs;

    let json = serde_json::to_string_pretty(diff)?;
    fs::write(path, json)?;

    Ok(())
}

/// Run comprehensive database comparison
fn run_comprehensive_diff(args: DiffArgs, from_path: PathBuf, to_path: PathBuf) -> anyhow::Result<()> {
    println!(
        "{} DATABASE COMPARISON: {} vs {}",
        "►".cyan().bold(),
        args.from,
        args.to
    );
    println!("{}", "═".repeat(60));

    // Create the differ and perform comparison
    let differ = DatabaseDiffer::new(&from_path, &to_path)?;
    let comparison = differ.compare()?;

    // Display results based on flags
    let show_chunks = args.chunks || args.all || (!args.sequences && !args.taxonomy);
    let show_sequences = args.sequences || args.all;
    let show_taxonomy = args.taxonomy || args.all;

    if show_chunks {
        display_chunk_analysis(&comparison.chunk_analysis)?;
    }

    if show_sequences {
        display_sequence_analysis(&comparison.sequence_analysis)?;
    }

    if show_taxonomy {
        display_taxonomy_analysis(&comparison.taxonomy_analysis)?;
    }

    // Always show storage metrics
    display_storage_metrics(&comparison.storage_metrics)?;

    // Export if requested
    if let Some(export_path) = args.export {
        let json = serde_json::to_string_pretty(&comparison)?;
        std::fs::write(&export_path, json)?;
        println!(
            "\n{} Comparison exported to {}",
            "✓".green().bold(),
            export_path.display()
        );
    }

    Ok(())
}

fn display_chunk_analysis(analysis: &talaria_sequoia::ChunkAnalysis) -> anyhow::Result<()> {
    println!("\n{}", "CHUNK-LEVEL ANALYSIS".bold());
    println!("{}", "─".repeat(40));

    println!("Total chunks in first:     {:>8}", analysis.total_chunks_a);
    println!("Total chunks in second:    {:>8}", analysis.total_chunks_b);
    println!(
        "Shared chunks:             {:>8} ({:.1}% / {:.1}%)",
        analysis.shared_chunks.len(),
        analysis.shared_percentage_a,
        analysis.shared_percentage_b
    );
    println!(
        "Unique to first:           {:>8} ({:.1}%)",
        analysis.unique_to_a.len(),
        100.0 - analysis.shared_percentage_a
    );
    println!(
        "Unique to second:          {:>8} ({:.1}%)",
        analysis.unique_to_b.len(),
        100.0 - analysis.shared_percentage_b
    );

    Ok(())
}

fn display_sequence_analysis(analysis: &talaria_sequoia::SequenceAnalysis) -> anyhow::Result<()> {
    println!("\n{}", "SEQUENCE-LEVEL ANALYSIS".bold());
    println!("{}", "─".repeat(40));

    println!("Total sequences in first:  {:>8}", analysis.total_sequences_a);
    println!("Total sequences in second: {:>8}", analysis.total_sequences_b);
    println!(
        "Shared sequences:          {:>8} ({:.1}% / {:.1}%)",
        analysis.shared_sequences,
        analysis.shared_percentage_a,
        analysis.shared_percentage_b
    );
    println!(
        "Unique to first:           {:>8} ({:.1}%)",
        analysis.unique_to_a,
        100.0 - analysis.shared_percentage_a
    );
    println!(
        "Unique to second:          {:>8} ({:.1}%)",
        analysis.unique_to_b,
        100.0 - analysis.shared_percentage_b
    );

    if !analysis.sample_shared_ids.is_empty() {
        println!("\nSample shared sequences:");
        for id in analysis.sample_shared_ids.iter().take(5) {
            println!("  • {}", id);
        }
    }

    Ok(())
}

fn display_taxonomy_analysis(analysis: &talaria_sequoia::TaxonomyAnalysis) -> anyhow::Result<()> {
    println!("\n{}", "TAXONOMY DISTRIBUTION".bold());
    println!("{}", "─".repeat(40));

    println!("Taxa in first:             {:>8}", analysis.total_taxa_a);
    println!("Taxa in second:            {:>8}", analysis.total_taxa_b);
    println!(
        "Shared taxa:               {:>8} ({:.1}% / {:.1}%)",
        analysis.shared_taxa.len(),
        analysis.shared_percentage_a,
        analysis.shared_percentage_b
    );

    if !analysis.top_shared_taxa.is_empty() {
        println!("\nTop shared taxa:");
        for (i, taxon) in analysis.top_shared_taxa.iter().enumerate().take(5) {
            println!(
                "  {}. {} ({}): {} / {} sequences",
                i + 1,
                taxon.taxon_name,
                taxon.taxon_id.0,
                taxon.count_in_a,
                taxon.count_in_b
            );
        }
    }

    Ok(())
}

fn display_storage_metrics(metrics: &talaria_sequoia::StorageMetrics) -> anyhow::Result<()> {
    println!("\n{}", "STORAGE METRICS".bold());
    println!("{}", "─".repeat(40));

    println!("Size first:                {}", format_bytes(metrics.size_a_bytes));
    println!("Size second:               {}", format_bytes(metrics.size_b_bytes));

    if metrics.dedup_savings_bytes > 0 {
        println!(
            "Deduplication savings:     {} (shared content)",
            format_bytes(metrics.dedup_savings_bytes)
        );
    }

    if metrics.dedup_ratio_a > 0.0 {
        println!("Deduplication ratio first: {:.2}x", metrics.dedup_ratio_a);
    }
    if metrics.dedup_ratio_b > 0.0 {
        println!("Deduplication ratio second: {:.2}x", metrics.dedup_ratio_b);
    }

    Ok(())
}