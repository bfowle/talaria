#![allow(dead_code)]

use talaria_sequoia::{SEQUOIARepository, DiffResult, StandardTemporalManifestDiffer};
use talaria_sequoia::differ::{TemporalManifestDiffer, DiffOptions, ChangeType};
use talaria_core::paths;
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

    /// Export diff to JSON file
    #[arg(long, value_name = "FILE")]
    pub export: Option<PathBuf>,
}

pub fn run(args: DiffArgs) -> anyhow::Result<()> {
    println!(
        "{} Computing differences between '{}' and '{}'...",
        "►".cyan().bold(),
        args.from,
        args.to
    );

    // Parse the from/to specifications
    let (from_path, from_version) = parse_spec(&args.from)?;
    let (to_path, to_version) = parse_spec(&args.to)?;

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

fn export_diff(diff: &DiffResult, path: &Path) -> anyhow::Result<()> {
    use std::fs;

    let json = serde_json::to_string_pretty(diff)?;
    fs::write(path, json)?;

    Ok(())
}