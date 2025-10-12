use crate::cli::formatting::formatter::format_number;
use crate::cli::formatting::output::*;
use clap::Args;
use colored::*;
use std::path::{Path, PathBuf};
use talaria_core::system::paths;
use talaria_herald::operations::{
    format_bytes, ChangeType, DatabaseDiffer, DiffOptions, StandardTemporalManifestDiffer,
    TemporalManifestDiffer,
};
use talaria_herald::{DiffResult, HeraldRepository};

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

    /// Show reduction-specific analysis (for comparing database with its reduction profile)
    #[arg(long, short = 'r')]
    pub reduction: bool,

    /// Output file path for the report
    #[arg(long, short = 'o', value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Report output format
    #[arg(long, short = 'f', value_name = "FORMAT", default_value = "text")]
    pub format: String,

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
    if args.sequence_date.is_some()
        || args.taxonomy_date.is_some()
        || args.vs_sequence_date.is_some()
        || args.vs_taxonomy_date.is_some()
    {
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
    let from_repo = HeraldRepository::open(&from_path)?;
    let to_repo = HeraldRepository::open(&to_path)?;

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
    let from_data = from_manifest
        .get_data()
        .ok_or_else(|| anyhow::anyhow!("No manifest data in 'from' database"))?;
    let to_data = to_manifest
        .get_data()
        .ok_or_else(|| anyhow::anyhow!("No manifest data in 'to' database"))?;

    // Compute differences using async runtime
    let runtime = tokio::runtime::Runtime::new()?;
    let diff_result = runtime.block_on(async {
        let differ = StandardTemporalManifestDiffer;
        differ
            .diff(from_data, to_data, DiffOptions::default())
            .await
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

    Ok(())
}

fn parse_spec(spec: &str) -> anyhow::Result<(PathBuf, Option<String>)> {
    if let Some((db, version)) = spec.split_once('@') {
        // Format: database@version
        // If it looks like a database name (contains '/'), use unified RocksDB path
        // Otherwise treat as file path
        let path = if db.contains('/') && !db.starts_with('.') && !db.starts_with('/') {
            // Database name like "uniprot/swissprot" - use unified repository
            paths::talaria_databases_dir()
        } else {
            // File path
            PathBuf::from(db)
        };
        Ok((path, Some(version.to_string())))
    } else {
        // Just database name or path
        // If it looks like a database name (contains '/'), use unified RocksDB path
        // Otherwise treat as file path
        let path = if spec.contains('/') && !spec.starts_with('.') && !spec.starts_with('/') {
            // Database name like "uniprot/swissprot" - use unified repository
            paths::talaria_databases_dir()
        } else {
            // File path
            PathBuf::from(spec)
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
    println!(
        "{} {} chunks",
        "Removed:".red().bold(),
        stats.chunks_removed
    );
    println!(
        "{} {} chunks",
        "Modified:".yellow().bold(),
        stats.chunks_modified
    );
    println!("{} {} chunks", "Moved:".blue().bold(), stats.chunks_moved);

    let size_mb = stats.total_size_delta.abs() as f64 / 1_048_576.0;
    if stats.total_size_delta > 0 {
        println!("{} +{:.2} MB", "Size change:".bold(), size_mb);
    } else if stats.total_size_delta < 0 {
        println!("{} -{:.2} MB", "Size change:".bold(), size_mb);
    } else {
        println!("{} No size change", "Size change:".bold());
    }

    println!(
        "{} {} sequences",
        "Affected:".bold(),
        stats.sequences_affected
    );
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
        println!(
            "\n{} ({}):",
            "Modified chunks".yellow().bold(),
            modified.len()
        );
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

fn display_taxonomy_diff(_from: &HeraldRepository, _to: &HeraldRepository) -> anyhow::Result<()> {
    println!("\n{}", "─".repeat(60));
    println!("{:^60}", "TAXONOMY DIFFERENCES");
    println!("{}", "─".repeat(60));

    // This would require implementing taxonomy comparison methods
    println!(
        "{} Taxonomy comparison not yet implemented",
        "⚠".yellow().bold()
    );

    Ok(())
}

fn run_bitemporal_diff(args: DiffArgs) -> anyhow::Result<()> {
    use chrono::Utc;
    use std::sync::Arc;
    use talaria_herald::{BiTemporalDatabase, HeraldStorage};

    println!("{} Computing bi-temporal differences...", "►".cyan().bold());

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

    println!(
        "  First point:  sequence={}, taxonomy={}",
        sequence_time1.format("%Y-%m-%d"),
        taxonomy_time1.format("%Y-%m-%d")
    );
    println!(
        "  Second point: sequence={}, taxonomy={}",
        sequence_time2.format("%Y-%m-%d"),
        taxonomy_time2.format("%Y-%m-%d")
    );

    // Open HERALD storage and bi-temporal database
    let storage = Arc::new(HeraldStorage::open(&db_path)?);
    let mut bi_temporal_db = BiTemporalDatabase::new(storage)?;

    // Create coordinates
    let coord1 = talaria_herald::BiTemporalCoordinate {
        sequence_time: sequence_time1,
        taxonomy_time: taxonomy_time1,
    };

    let coord2 = talaria_herald::BiTemporalCoordinate {
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
    println!(
        "  {} Sequences added:   {}",
        "+".green().bold(),
        diff.sequences_added
    );
    println!(
        "  {} Sequences removed: {}",
        "-".red().bold(),
        diff.sequences_removed
    );

    if args.taxonomy && !diff.taxonomic_changes.is_empty() {
        println!("\n{}", "Taxonomy Changes:".bold());
        for change in diff.taxonomic_changes.iter().take(10) {
            match change.change_type {
                talaria_herald::TaxonomicChangeType::Reclassified => {
                    println!(
                        "  {} TaxID {} reclassified from {:?} to {:?}",
                        "↻".yellow(),
                        change.taxon_id.0,
                        change.old_parent.map(|t| t.0),
                        change.new_parent.map(|t| t.0)
                    );
                }
                talaria_herald::TaxonomicChangeType::New => {
                    println!("  {} TaxID {} newly added", "+".green(), change.taxon_id.0);
                }
                talaria_herald::TaxonomicChangeType::Deprecated => {
                    println!("  {} TaxID {} deprecated", "✗".red(), change.taxon_id.0);
                }
                _ => {}
            }
        }
        if diff.taxonomic_changes.len() > 10 {
            println!(
                "  ... and {} more changes",
                diff.taxonomic_changes.len() - 10
            );
        }
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
        let time = dt
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid time"))?;
        return Ok(DateTime::from_naive_utc_and_offset(time, Utc));
    }

    anyhow::bail!(
        "Invalid time format '{}'. Use YYYY-MM-DD or RFC3339 format.",
        input
    )
}

/// Run comprehensive database comparison
fn run_comprehensive_diff(
    args: DiffArgs,
    from_path: PathBuf,
    to_path: PathBuf,
) -> anyhow::Result<()> {
    section_header_with_line(&format!(
        "Database Comparison: {} vs {}",
        args.from, args.to
    ));

    // Check if inputs are database names (format: source/dataset) or paths
    let is_from_dbname = args.from.contains('/') && !args.from.contains('.');
    let is_to_dbname = args.to.contains('/') && !args.to.contains('.');

    let (comparison, is_reduction_comparison) = if is_from_dbname && is_to_dbname {
        // Both are database names - use DatabaseManager to load manifests
        use talaria_herald::database::DatabaseManager;
        use talaria_herald::taxonomy::TaxonomyManager;
        use talaria_utils::database::database_ref::parse_database_reference;

        let manager = DatabaseManager::new(None)?;

        // Parse database references to detect reduction profiles
        let db_ref_a = parse_database_reference(&args.from)?;
        let db_ref_b = parse_database_reference(&args.to)?;

        // Check if either side has a reduction profile
        let has_reduction = db_ref_a.profile.is_some() || db_ref_b.profile.is_some();

        // Load base database names (without profile)
        let db_name_a = format!("{}/{}", db_ref_a.source, db_ref_a.dataset);
        let db_name_b = format!("{}/{}", db_ref_b.source, db_ref_b.dataset);

        // Use lightweight version to avoid loading 70M+ chunk indexes
        let manifest_a = manager.get_manifest_lightweight(&db_name_a)?;
        let manifest_b = manager.get_manifest_lightweight(&db_name_b)?;

        // Get storage for sequence hash extraction
        let storage = manager.get_repository().storage.clone();

        // Apply reduction filters if profiles are specified
        let (manifest_a_filtered, manifest_b_filtered) = apply_reduction_filters(
            &manager,
            &db_ref_a,
            &db_ref_b,
            manifest_a,
            manifest_b,
            &storage,
        )?;

        // Load taxonomy manager for scientific name lookup using unified TaxonomyProvider
        use talaria_utils::taxonomy::{get_taxonomy_tree_path, has_taxonomy};

        let tax_mgr = if has_taxonomy() {
            TaxonomyManager::load(&get_taxonomy_tree_path()).ok()
        } else {
            None
        };

        let comp = DatabaseDiffer::compare_manifests(&manifest_a_filtered, &manifest_b_filtered, Some(&storage), tax_mgr.as_ref())?;
        (comp, has_reduction)
    } else {
        // At least one is a path - use repository-based comparison
        let differ = DatabaseDiffer::new(&from_path, &to_path)?;
        (differ.compare()?, false)
    };

    // Display results based on flags
    let show_chunks = args.chunks || args.all || (!args.sequences && !args.taxonomy);
    let show_sequences = args.sequences || args.all;
    let show_taxonomy = args.taxonomy || args.all;

    // For reduction comparisons, show reduction analysis FIRST since it's the meaningful comparison
    if is_reduction_comparison || args.reduction {
        println!();
        info("🔍 Reduction Profile Comparison");
        info("   This compares a full database with its reduced version.");
        info("   The reduction keeps only representative sequences (references).");
        println!();
        display_reduction_analysis(&args.from, &args.to, &comparison)?;

        println!();
        info("ℹ️  Storage Analysis (Advanced)");
        info("   Both databases share the same underlying HERALD storage.");
        info("   The reduction profile is a metadata layer that selects which sequences to include.");
        println!();
    }

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

    // Generate report using new reporting framework if output path specified
    if let Some(output_path) = args.output {
        generate_report(&comparison, &args.format, &output_path)?;
        println!(
            "\n{} Report saved to {}",
            "✓".green().bold(),
            output_path.display()
        );
    }

    Ok(())
}

/// Generate a report from DatabaseComparison
fn generate_report(
    comparison: &talaria_herald::DatabaseComparison,
    format: &str,
    output_path: &Path,
) -> anyhow::Result<()> {
    use talaria_utils::report::{Cell, Metric, Report, Section, Table};

    // Build a generic report from DatabaseComparison
    let mut report = Report::builder("Database Comparison", "database diff")
        .metadata("timestamp", chrono::Utc::now().to_rfc3339());

    // Summary metrics
    let summary_metrics = vec![
        Metric::new(
            "Total Chunks (DB1)",
            comparison.chunk_analysis.total_chunks_a,
        ),
        Metric::new(
            "Total Chunks (DB2)",
            comparison.chunk_analysis.total_chunks_b,
        ),
        Metric::new(
            "Shared Chunks",
            comparison.chunk_analysis.shared_chunks.len(),
        ),
        Metric::new(
            "Total Sequences (DB1)",
            comparison.sequence_analysis.total_sequences_a,
        ),
        Metric::new(
            "Total Sequences (DB2)",
            comparison.sequence_analysis.total_sequences_b,
        ),
        Metric::new(
            "Shared Sequences",
            comparison.sequence_analysis.shared_sequences,
        ),
    ];
    report = report.section(Section::summary("Summary", summary_metrics));

    // Chunk analysis table
    let mut chunk_table = Table::new(vec![
        "Metric".to_string(),
        "First Database".to_string(),
        "Second Database".to_string(),
    ]);
    chunk_table.add_row(vec![
        Cell::new("Total chunks"),
        Cell::new(comparison.chunk_analysis.total_chunks_a),
        Cell::new(comparison.chunk_analysis.total_chunks_b),
    ]);
    chunk_table.add_row(vec![
        Cell::new("Shared chunks"),
        Cell::new(format!(
            "{} ({:.1}%)",
            comparison.chunk_analysis.shared_chunks.len(),
            comparison.chunk_analysis.shared_percentage_a
        )),
        Cell::new(format!(
            "{} ({:.1}%)",
            comparison.chunk_analysis.shared_chunks.len(),
            comparison.chunk_analysis.shared_percentage_b
        )),
    ]);
    chunk_table.add_row(vec![
        Cell::new("Unique chunks"),
        Cell::new(format!(
            "{} ({:.1}%)",
            comparison.chunk_analysis.unique_to_a.len(),
            100.0 - comparison.chunk_analysis.shared_percentage_a
        )),
        Cell::new(format!(
            "{} ({:.1}%)",
            comparison.chunk_analysis.unique_to_b.len(),
            100.0 - comparison.chunk_analysis.shared_percentage_b
        )),
    ]);
    report = report.section(Section::table("Chunk-Level Analysis", chunk_table));

    // Sequence analysis table
    let mut seq_table = Table::new(vec![
        "Metric".to_string(),
        "First Database".to_string(),
        "Second Database".to_string(),
    ]);
    seq_table.add_row(vec![
        Cell::new("Total sequences"),
        Cell::new(comparison.sequence_analysis.total_sequences_a),
        Cell::new(comparison.sequence_analysis.total_sequences_b),
    ]);
    seq_table.add_row(vec![
        Cell::new("Shared sequences"),
        Cell::new(format!(
            "{} ({:.1}%)",
            comparison.sequence_analysis.shared_sequences,
            comparison.sequence_analysis.shared_percentage_a
        )),
        Cell::new(format!(
            "{} ({:.1}%)",
            comparison.sequence_analysis.shared_sequences,
            comparison.sequence_analysis.shared_percentage_b
        )),
    ]);
    report = report.section(Section::table("Sequence-Level Analysis", seq_table));

    // Storage metrics table
    let mut storage_table = Table::new(vec![
        "Metric".to_string(),
        "First Database".to_string(),
        "Second Database".to_string(),
    ]);
    storage_table.add_row(vec![
        Cell::new("Total size"),
        Cell::new(format_bytes(comparison.storage_metrics.size_a_bytes)),
        Cell::new(format_bytes(comparison.storage_metrics.size_b_bytes)),
    ]);
    if comparison.storage_metrics.dedup_ratio_a > 0.0
        || comparison.storage_metrics.dedup_ratio_b > 0.0
    {
        storage_table.add_row(vec![
            Cell::new("Deduplication ratio"),
            Cell::new(format!("{:.2}x", comparison.storage_metrics.dedup_ratio_a)),
            Cell::new(format!("{:.2}x", comparison.storage_metrics.dedup_ratio_b)),
        ]);
    }
    report = report.section(Section::table("Storage Metrics", storage_table));

    let report = report.build();

    // Render based on format
    let content = match format.to_lowercase().as_str() {
        "html" => talaria_utils::report::render_html(&report)?,
        "json" => talaria_utils::report::render_json(&report)?,
        "csv" => talaria_utils::report::render_csv(&report)?,
        "text" | "txt" => talaria_utils::report::render_text(&report)?,
        _ => anyhow::bail!("Unknown format '{}'. Use: text, html, json, csv", format),
    };

    std::fs::write(output_path, content)?;
    Ok(())
}

fn display_chunk_analysis(analysis: &talaria_herald::ChunkAnalysis) -> anyhow::Result<()> {
    println!();
    subsection_header("Chunk-Level Analysis");

    let mut table = create_standard_table();
    table.set_header(vec![
        header_cell("Metric"),
        header_cell("First Database"),
        header_cell("Second Database"),
    ]);

    table.add_row(vec![
        "Total chunks",
        &format_number(analysis.total_chunks_a),
        &format_number(analysis.total_chunks_b),
    ]);

    table.add_row(vec![
        "Shared chunks",
        &format!(
            "{} ({:.1}%)",
            format_number(analysis.shared_chunks.len()),
            analysis.shared_percentage_a
        ),
        &format!(
            "{} ({:.1}%)",
            format_number(analysis.shared_chunks.len()),
            analysis.shared_percentage_b
        ),
    ]);

    table.add_row(vec![
        "Unique chunks",
        &format!(
            "{} ({:.1}%)",
            format_number(analysis.unique_to_a.len()),
            100.0 - analysis.shared_percentage_a
        ),
        &format!(
            "{} ({:.1}%)",
            format_number(analysis.unique_to_b.len()),
            100.0 - analysis.shared_percentage_b
        ),
    ]);

    println!("{}", table);
    Ok(())
}

fn display_sequence_analysis(analysis: &talaria_herald::SequenceAnalysis) -> anyhow::Result<()> {
    println!();
    subsection_header("Sequence-Level Analysis");

    let mut table = create_standard_table();
    table.set_header(vec![
        header_cell("Metric"),
        header_cell("First Database"),
        header_cell("Second Database"),
    ]);

    table.add_row(vec![
        "Total sequences",
        &format_number(analysis.total_sequences_a),
        &format_number(analysis.total_sequences_b),
    ]);

    table.add_row(vec![
        "Shared sequences",
        &format!(
            "{} ({:.1}%)",
            format_number(analysis.shared_sequences),
            analysis.shared_percentage_a
        ),
        &format!(
            "{} ({:.1}%)",
            format_number(analysis.shared_sequences),
            analysis.shared_percentage_b
        ),
    ]);

    table.add_row(vec![
        "Unique sequences",
        &format!(
            "{} ({:.1}%)",
            format_number(analysis.unique_to_a),
            100.0 - analysis.shared_percentage_a
        ),
        &format!(
            "{} ({:.1}%)",
            format_number(analysis.unique_to_b),
            100.0 - analysis.shared_percentage_b
        ),
    ]);

    println!("{}", table);

    // Show sample shared sequences if available
    if !analysis.sample_shared_ids.is_empty() {
        println!("\n{} Sample Shared Sequences:", "◆".cyan());
        for (i, id) in analysis.sample_shared_ids.iter().take(5).enumerate() {
            let prefix = if i == analysis.sample_shared_ids.len().min(5) - 1 {
                "└─"
            } else {
                "├─"
            };
            println!("  {} {}", prefix.dimmed(), id);
        }
    }

    // Interpretation note
    println!("\n{} Interpretation:", "ℹ".bright_blue().bold());
    if analysis.shared_percentage_a < 1.0 && analysis.shared_percentage_b < 1.0 {
        println!("  {}", "Low sequence sharing is expected when comparing:".dimmed());
        println!("    {}", "• Clustered databases (UniRef50/90) vs unclustered (SwissProt)".dimmed());
        println!("    {}", "• Different database sources (UniProt vs NCBI)".dimmed());
        println!("    {}", "• Databases with different sequence representations".dimmed());
        println!("\n  {} {}", "→".bright_black(), "UniRef clustering picks longest sequences as representatives,".dimmed());
        println!("    {}", "so even identical proteins may have different sequences stored.".dimmed());
    } else if analysis.shared_percentage_a > 80.0 || analysis.shared_percentage_b > 80.0 {
        println!("  {}", "High sequence sharing detected!".bright_green());
        println!("    {}", "Content-addressed storage is providing significant deduplication.".dimmed());
    } else if analysis.shared_percentage_a > 10.0 || analysis.shared_percentage_b > 10.0 {
        println!("  {}", "Moderate sequence sharing detected.".bright_yellow());
        println!("    {}", "Some deduplication is occurring across databases.".dimmed());
    }

    Ok(())
}

fn display_taxonomy_analysis(analysis: &talaria_herald::TaxonomyAnalysis) -> anyhow::Result<()> {
    println!();
    subsection_header("Taxonomy Distribution");

    let mut summary_table = create_standard_table();
    summary_table.set_header(vec![
        header_cell("Metric"),
        header_cell("First Database"),
        header_cell("Second Database"),
    ]);

    summary_table.add_row(vec![
        "Total taxa",
        &format_number(analysis.total_taxa_a),
        &format_number(analysis.total_taxa_b),
    ]);

    summary_table.add_row(vec![
        "Shared taxa",
        &format!(
            "{} ({:.1}%)",
            format_number(analysis.shared_taxa.len()),
            analysis.shared_percentage_a
        ),
        &format!(
            "{} ({:.1}%)",
            format_number(analysis.shared_taxa.len()),
            analysis.shared_percentage_b
        ),
    ]);

    println!("{}", summary_table);

    // Show top shared taxa if available
    if !analysis.top_shared_taxa.is_empty() {
        println!();
        println!("{} Top Shared Taxa", "◆".cyan().bold());

        let mut taxa_table = create_standard_table();
        taxa_table.set_header(vec![
            header_cell("#"),
            header_cell("Taxon"),
            header_cell("TaxID"),
            header_cell("First DB"),
            header_cell("Second DB"),
        ]);

        for (i, taxon) in analysis.top_shared_taxa.iter().take(10).enumerate() {
            taxa_table.add_row(vec![
                &format!("{}", i + 1),
                &taxon.taxon_name,
                &format!("{}", taxon.taxon_id.0),
                &format_number(taxon.count_in_a),
                &format_number(taxon.count_in_b),
            ]);
        }

        println!("{}", taxa_table);
    }

    Ok(())
}

fn display_storage_metrics(metrics: &talaria_herald::StorageMetrics) -> anyhow::Result<()> {
    println!();
    subsection_header("Storage Metrics");

    let mut table = create_standard_table();
    table.set_header(vec![
        header_cell("Metric"),
        header_cell("First Database"),
        header_cell("Second Database"),
    ]);

    table.add_row(vec![
        "Total size",
        &format_bytes(metrics.size_a_bytes),
        &format_bytes(metrics.size_b_bytes),
    ]);

    if metrics.dedup_ratio_a > 0.0 || metrics.dedup_ratio_b > 0.0 {
        table.add_row(vec![
            "Deduplication ratio",
            &format!("{:.2}x", metrics.dedup_ratio_a),
            &format!("{:.2}x", metrics.dedup_ratio_b),
        ]);
    }

    if metrics.dedup_savings_bytes > 0 {
        table.add_row(vec![
            "Shared content savings",
            &format_bytes(metrics.dedup_savings_bytes),
            &"(same)".dimmed().to_string(),
        ]);
    }

    println!("{}", table);
    Ok(())
}

/// Display reduction-specific analysis when comparing a database with its reduction profile
fn display_reduction_analysis(
    from_name: &str,
    to_name: &str,
    _comparison: &talaria_herald::DatabaseComparison,
) -> anyhow::Result<()> {
    use talaria_herald::database::DatabaseManager;

    println!();
    section_header("Reduction Analysis");

    // Try to detect which one is the reduction profile
    // Format: "database:profile" or just "database"
    let (from_db, from_profile) = parse_db_with_profile(from_name);
    let (to_db, to_profile) = parse_db_with_profile(to_name);

    let (original_db, reduced_profile) = if from_profile.is_some() {
        (to_db, from_profile)
    } else if to_profile.is_some() {
        (from_db, to_profile)
    } else {
        // Neither has a profile specified, can't do reduction analysis
        info("Reduction analysis requires comparing a database with its reduction profile");
        info("Format: 'database/name:profile' (e.g., 'uniprot/swissprot:auto-detect')");
        return Ok(());
    };

    if reduced_profile.is_none() {
        return Ok(());
    }

    let profile = reduced_profile.unwrap();

    // Load the reduction manifest
    let manager = DatabaseManager::new(None)?;
    let manifest = match load_reduction_manifest(&manager, original_db, profile) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            warning(&format!("Reduction profile '{}' not found for database '{}'", profile, original_db));
            return Ok(());
        }
        Err(e) => {
            warning(&format!("Failed to load reduction profile: {}", e));
            return Ok(());
        }
    };

    subsection_header("Reduction Overview");

    // Display reduction parameters
    tree_item(false, "Profile", Some(profile));
    tree_item(false, "Source Database", Some(original_db));
    tree_item(false, "Target Aligner", Some(&format!("{:?}", manifest.parameters.target_aligner.unwrap_or(talaria_herald::TargetAligner::Lambda))));
    tree_item(false, "Similarity Threshold", Some(&format!("{:.1}%", manifest.parameters.similarity_threshold * 100.0)));
    tree_item(true, "Taxonomy Aware", Some(&format!("{}", manifest.parameters.taxonomy_aware)));

    println!();
    subsection_header("Sequence Breakdown");

    let stats = &manifest.statistics;
    let mut table = create_standard_table();
    table.set_header(vec![
        header_cell("Category"),
        header_cell("Count"),
        header_cell("Percentage"),
        header_cell("Description"),
    ]);

    table.add_row(vec![
        "Original sequences",
        &format_number(stats.original_sequences),
        &"100.0%".to_string(),
        &"Total sequences in source database".dimmed().to_string(),
    ]);

    let ref_pct = (stats.reference_sequences as f64 / stats.original_sequences as f64) * 100.0;
    table.add_row(vec![
        "Reference sequences",
        &format_number(stats.reference_sequences),
        &format!("{:.1}%", ref_pct),
        &"Selected as representatives".green().to_string(),
    ]);

    let child_pct = (stats.child_sequences as f64 / stats.original_sequences as f64) * 100.0;
    table.add_row(vec![
        "Delta-encoded sequences",
        &format_number(stats.child_sequences),
        &format!("{:.1}%", child_pct),
        &"Compressed as deltas".blue().to_string(),
    ]);

    println!("{}", table);

    println!();
    subsection_header("Storage Efficiency");

    let mut storage_table = create_standard_table();
    storage_table.set_header(vec![
        header_cell("Metric"),
        header_cell("Size"),
        header_cell("vs Original"),
    ]);

    storage_table.add_row(vec![
        "Original size",
        &format_bytes(stats.original_size as usize),
        &"100.0%".to_string(),
    ]);

    let reduced_pct = (stats.reduced_size as f64 / stats.original_size as f64) * 100.0;
    let savings_pct = 100.0 - reduced_pct;
    storage_table.add_row(vec![
        "References only",
        &format_bytes(stats.reduced_size as usize),
        &format!("{:.1}% ({:.1}% saved)", reduced_pct, savings_pct).green().to_string(),
    ]);

    let total_pct = (stats.total_size_with_deltas as f64 / stats.original_size as f64) * 100.0;
    let total_savings_pct = 100.0 - total_pct;
    storage_table.add_row(vec![
        "With delta encoding",
        &format_bytes(stats.total_size_with_deltas as usize),
        &format!("{:.1}% ({:.1}% saved)", total_pct, total_savings_pct).cyan().to_string(),
    ]);

    println!("{}", storage_table);

    println!();
    subsection_header("Reduction Metrics");

    tree_item(false, "Achieved Reduction Ratio", Some(&format!("{:.2}x", stats.actual_reduction_ratio)));
    tree_item(false, "Sequence Coverage", Some(&format!("{:.1}%", stats.sequence_coverage)));
    tree_item(false, "Unique Taxa Covered", Some(&format_number(stats.unique_taxa)));
    tree_item(true, "Deduplication Ratio", Some(&format!("{:.2}x", stats.deduplication_ratio)));

    // Add visual representation
    println!();
    subsection_header("Visual Breakdown");

    let bar_width = 60;
    let ref_width = ((stats.reference_sequences as f64 / stats.original_sequences as f64) * bar_width as f64) as usize;
    let delta_width = bar_width - ref_width;

    println!("  References: {}{} {:.1}%",
        "█".repeat(ref_width).green(),
        "░".repeat(delta_width).dimmed(),
        ref_pct
    );
    println!("  Deltas:     {}{} {:.1}%",
        "░".repeat(ref_width).dimmed(),
        "█".repeat(delta_width).blue(),
        child_pct
    );

    println!();

    Ok(())
}

/// Apply reduction filters - for reduction profiles, leave manifests as-is
/// The reduction creates separate chunks not in HERALD storage, so we can't filter
/// Instead, the reduction analysis section shows the meaningful comparison
fn apply_reduction_filters(
    _manager: &talaria_herald::database::DatabaseManager,
    _db_ref_a: &talaria_core::types::DatabaseReference,
    _db_ref_b: &talaria_core::types::DatabaseReference,
    manifest_a: talaria_herald::TemporalManifest,
    manifest_b: talaria_herald::TemporalManifest,
    _storage: &talaria_herald::storage::HeraldStorage,
) -> anyhow::Result<(talaria_herald::TemporalManifest, talaria_herald::TemporalManifest)> {
    // Don't filter - reduction chunks aren't in HERALD storage
    // The meaningful comparison is in the Reduction Analysis section
    Ok((manifest_a, manifest_b))
}

/// Parse database spec with optional profile (format: "database/name:profile")
fn parse_db_with_profile(spec: &str) -> (&str, Option<&str>) {
    if let Some(colon_pos) = spec.rfind(':') {
        // Check if this looks like a profile (not a path with C:/ etc)
        if colon_pos > 1 && !spec.starts_with("C:") && !spec.starts_with("D:") {
            let db = &spec[..colon_pos];
            let profile = &spec[colon_pos + 1..];
            return (db, Some(profile));
        }
    }
    (spec, None)
}

/// Load a reduction manifest for a database and profile
fn load_reduction_manifest(
    manager: &talaria_herald::database::DatabaseManager,
    db_name: &str,
    profile: &str,
) -> anyhow::Result<Option<talaria_herald::operations::ReductionManifest>> {
    // Parse database name (format: source/dataset)
    let parts: Vec<&str> = db_name.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid database name format. Expected 'source/dataset'");
    }

    let source = parts[0];
    let dataset = parts[1];

    // Get the current version for this database
    let databases = manager.list_databases()?;
    let db_info = databases.iter().find(|db| db.name == db_name)
        .ok_or_else(|| anyhow::anyhow!("Database '{}' not found", db_name))?;

    // Load the reduction manifest
    let storage = &manager.get_repository().storage;
    storage.get_database_reduction_by_profile(source, dataset, &db_info.version, profile)
}
