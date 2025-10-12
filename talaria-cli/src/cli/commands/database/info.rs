#![allow(dead_code)]

use clap::Args;

#[derive(Args)]
pub struct InfoArgs {
    /// Database reference (e.g., "uniprot/swissprot") or file path
    pub database: String,

    /// Show sequence statistics
    #[arg(long)]
    pub stats: bool,

    /// Show taxonomic distribution
    #[arg(long)]
    pub taxonomy: bool,

    /// Output format
    #[arg(long, value_enum, default_value = "text")]
    pub format: OutputFormat,

    /// Show reduction profiles if available
    #[arg(long)]
    pub show_reductions: bool,

    /// Show global repository statistics (may be slow with large databases)
    #[arg(long)]
    pub show_global_stats: bool,

    /// Query at specific sequence date (e.g., "2020-01-01")
    #[arg(long)]
    pub sequence_date: Option<String>,

    /// Query at specific taxonomy date (e.g., "2020-01-01")
    #[arg(long)]
    pub taxonomy_date: Option<String>,

    /// Report output file path
    #[arg(long = "report-output", value_name = "FILE")]
    pub report_output: Option<std::path::PathBuf>,

    /// Report output format (text, html, json, csv)
    #[arg(long = "report-format", value_name = "FORMAT", default_value = "text")]
    pub report_format: String,
}

// Use OutputFormat from talaria-core
use talaria_core::OutputFormat;

pub fn run(args: InfoArgs) -> anyhow::Result<()> {
    use crate::cli::formatting::output::*;
    use crate::cli::progress::create_spinner;
    use colored::*;
    use humansize::{format_size, BINARY};
    use talaria_core::system::paths;
    use talaria_herald::database::DatabaseManager;
    use talaria_utils::database::database_ref::parse_database_reference;
    use talaria_utils::display::output::format_number;

    // Check if we need bi-temporal info
    if args.sequence_date.is_some() || args.taxonomy_date.is_some() {
        return run_bitemporal_info(args);
    }

    // Parse the database reference to separate database and profile
    let db_ref = parse_database_reference(&args.database)?;
    let base_name = db_ref.base_ref();

    // Initialize database manager with spinner
    let spinner = create_spinner("Loading database information...");
    let manager = DatabaseManager::new(None)?;
    let databases = manager.list_databases()?;
    spinner.finish_and_clear();

    section_header("Database Information");

    // Check if a profile was specified
    if let Some(profile) = &db_ref.profile {
        // Show profile-specific information
        return show_profile_info(&manager, &db_ref, profile, &[]);
    }

    // Find the requested database (handle both slash and hyphen formats)
    let db_info = databases
        .iter()
        .find(|db| {
            // Exact match or partial match at the end
            db.name == base_name || db.name.ends_with(&base_name)
        })
        .ok_or_else(|| anyhow::anyhow!("Database '{}' not found in repository", base_name))?;

    // Build tree structure for database info
    tree_item(false, "Name", Some(&db_info.name));
    tree_item(false, "Version", Some(&db_info.version));
    tree_item(
        false,
        "Created",
        Some(&db_info.created_at.format("%Y-%m-%d %H:%M:%S").to_string()),
    );

    // Show additional details in detailed format
    if matches!(args.format, OutputFormat::Detailed) {
        // Load and show manifest details
        if let Ok(manifest) = manager.get_manifest(&db_info.name) {
            // Note: upstream_version field may not exist in TemporalManifest
            // tree_item(false, "Upstream Version", manifest.upstream_version.as_ref().map(|s| s.as_str()));
            tree_item(false, "ETag", Some(&manifest.etag));

            // Show taxonomy and sequence roots if available
            let seq_root = if manifest.sequence_root.0.iter().all(|&b| b == 0) {
                if manifest.etag.starts_with("streaming-") {
                    "Not computed (streaming mode)".to_string()
                } else {
                    "Not available (older format)".to_string()
                }
            } else {
                format!("{:?}", manifest.sequence_root)
            };
            tree_item(false, "Sequence Root", Some(&seq_root));

            let tax_root = if manifest.taxonomy_root.0.iter().all(|&b| b == 0) {
                if manifest.etag.starts_with("streaming-") {
                    "Not computed (streaming mode)".to_string()
                } else {
                    "Not available (older format)".to_string()
                }
            } else {
                format!("{:?}", manifest.taxonomy_root)
            };
            tree_item(false, "Taxonomy Root", Some(&tax_root));
        }
    }

    // Storage section - expand in detailed mode
    // If size is 0 (streaming manifest), recalculate from manifest
    let total_size: usize = if db_info.total_size == 0 {
        if let Ok(manifest) = manager.get_manifest(&db_info.name) {
            manifest.chunk_index.iter().map(|c| c.size).sum()
        } else {
            0
        }
    } else {
        db_info.total_size
    };

    let mut storage_items = vec![
        ("Sequences", format_number(db_info.sequence_count)),
        ("Chunks", format_number(db_info.chunk_count)),
        ("Size", format_size(total_size, BINARY)),
    ];

    if matches!(args.format, OutputFormat::Detailed) {
        // Add more storage details
        if let Ok(manifest) = manager.get_manifest(&db_info.name) {

            // Show chunk distribution
            let min_chunk_size = manifest
                .chunk_index
                .iter()
                .map(|c| c.size)
                .min()
                .unwrap_or(0);
            let max_chunk_size = manifest
                .chunk_index
                .iter()
                .map(|c| c.size)
                .max()
                .unwrap_or(0);
            let avg_chunk_size = if !manifest.chunk_index.is_empty() {
                total_size / manifest.chunk_index.len()
            } else {
                0
            };

            storage_items.push(("Avg Chunk Size", format_size(avg_chunk_size, BINARY)));
            storage_items.push(("Min Chunk Size", format_size(min_chunk_size, BINARY)));
            storage_items.push(("Max Chunk Size", format_size(max_chunk_size, BINARY)));
        }

        // Show storage paths
        // Parse the database name to get source and dataset
        let parts: Vec<&str> = db_info.name.split('/').collect();
        let (source, dataset) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            (&db_info.name[..], "")
        };

        let db_path = paths::talaria_databases_dir()
            .join("versions")
            .join(source)
            .join(dataset)
            .join(&db_info.version);
        storage_items.push(("Path", db_path.display().to_string()));
    }

    tree_section("Storage", storage_items, false);

    // RocksDB Section (always show in detailed mode)
    if matches!(args.format, OutputFormat::Detailed) {
        let parts: Vec<&str> = db_info.name.split('/').collect();
        if parts.len() == 2 {
            let source = parts[0];
            let dataset = parts[1];

            let mut rocksdb_items = vec![
                (
                    "Manifest Key",
                    format!("manifest:{}:{}:{}", source, dataset, db_info.version),
                ),
                (
                    "Alias Keys",
                    format!(
                        "alias:{}:{}:current, alias:{}:{}:latest",
                        source, dataset, source, dataset
                    ),
                ),
                (
                    "Storage Path",
                    paths::talaria_databases_dir()
                        .join("sequences/rocksdb")
                        .display()
                        .to_string(),
                ),
            ];

            // Try to get version count
            if let Ok(versions) = manager.list_database_versions(source, dataset) {
                rocksdb_items.push(("Total Versions", versions.len().to_string()));
            }

            tree_section("RocksDB Storage", rocksdb_items, false);
        }
    }

    // Reductions section
    if !db_info.reduction_profiles.is_empty() {
        tree_item(false, "Reductions", None);
        for (i, profile) in db_info.reduction_profiles.iter().enumerate() {
            let is_last = i == db_info.reduction_profiles.len() - 1;
            if is_last {
                tree_item_continued_last(profile, None);
            } else {
                tree_item_continued(profile, None);
            }
        }

        // Show detailed reduction info if requested
        if args.show_reductions {
            subsection_header("Reduction Details");
            let storage = manager.get_storage();
            for (idx, profile) in db_info.reduction_profiles.iter().enumerate() {
                // Parse the database name to get source and dataset
                let parts: Vec<&str> = db_info.name.split('/').collect();
                if parts.len() == 2 {
                    let source = parts[0];
                    let dataset = parts[1];
                    if let Ok(Some(manifest)) =
                        storage.get_database_reduction_by_profile(source, dataset, &db_info.version, profile)
                    {
                        let is_last_profile = idx == db_info.reduction_profiles.len() - 1;
                        let reduction_items = vec![
                            (
                                "Reduction Ratio",
                                format!(
                                    "{:.1}%",
                                    manifest.statistics.actual_reduction_ratio * 100.0
                                ),
                            ),
                            (
                                "Reference Sequences",
                                format_number(manifest.statistics.reference_sequences),
                            ),
                            (
                                "Child Sequences",
                                format_number(manifest.statistics.child_sequences),
                            ),
                            (
                                "Coverage",
                                format!("{:.1}%", manifest.statistics.sequence_coverage * 100.0),
                            ),
                            (
                                "Size",
                                format!(
                                    "{} → {}",
                                    format_size(manifest.statistics.original_size as usize, BINARY),
                                    format_size(manifest.statistics.reduced_size as usize, BINARY)
                                ),
                            ),
                        ];
                        tree_section(profile, reduction_items, is_last_profile);
                    }
                }
            }
        }
    } else {
        empty("No reduced versions available");
    }

    if args.stats {
        stats_header("Statistics");
        // We'd need to assemble and analyze to get full stats
        info("Full statistics require assembling chunks");
        info("This will be implemented in a future update");
    }

    // Show storage benefits as tree (only if explicitly requested)
    // This loads all databases into memory and can cause OOM on large repositories
    if args.show_global_stats {
        let stats = manager.get_stats()?;
        let benefits_items = vec![
            (
                "Deduplication ratio",
                format!("{:.2}x", stats.deduplication_ratio),
            ),
            (
                "Storage saved",
                format!(
                    "~{}%",
                    ((1.0 - 1.0 / stats.deduplication_ratio) * 100.0) as i32
                ),
            ),
            ("Incremental updates", "Enabled".to_string()),
            ("Cryptographic verification", "SHA256".to_string()),
        ];
        tree_section("Storage Benefits", benefits_items, true);
    } else {
        info("Tip: Use --show-global-stats to see repository-wide storage statistics");
    }

    println!();

    // Add hint for detailed view
    if !matches!(args.format, OutputFormat::Detailed) {
        println!(
            "{}",
            "Tip: Use --format detailed for more information".dimmed()
        );
    }

    // Generate report if requested
    if let Some(report_path) = &args.report_output {
        use talaria_herald::operations::DatabaseInfoResult;

        // Parse database name to get source and dataset
        let parts: Vec<&str> = db_info.name.split('/').collect();
        let (source, dataset) = if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            (db_info.name.clone(), "".to_string())
        };

        // Calculate total sequences
        let total_sequences = if let Ok(manifest) = manager.get_manifest(&db_info.name) {
            manifest.chunk_index.iter().map(|c| c.sequence_count).sum()
        } else {
            0
        };

        let result = DatabaseInfoResult {
            database_name: db_info.name.clone(),
            source,
            dataset,
            total_sequences,
            total_chunks: db_info.chunk_count,
            total_size: total_size as u64,
            versions: 1, // TODO: Get actual version count from temporal index
            current_version: Some(db_info.version.clone()),
            last_updated: Some(db_info.created_at.format("%Y-%m-%d %H:%M:%S").to_string()),
            taxonomy_coverage: None, // TODO: Calculate if requested
        };

        crate::cli::commands::save_report(&result, &args.report_format, report_path)?;
        success(&format!("Report saved to {}", report_path.display()));
    }

    Ok(())
}

fn run_bitemporal_info(args: InfoArgs) -> anyhow::Result<()> {
    use crate::cli::formatting::output::*;
    use chrono::{NaiveDate, Utc};
    use std::sync::Arc;
    use talaria_core::system::paths;
    use talaria_herald::{BiTemporalDatabase, HeraldStorage};

    // Parse the database reference
    let db_ref = talaria_utils::database::database_ref::parse_database_reference(&args.database)?;
    let db_path = paths::talaria_databases_dir()
        .join(&db_ref.source)
        .join(&db_ref.dataset);

    // Parse the dates
    let sequence_date = if let Some(date_str) = &args.sequence_date {
        NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("Invalid sequence date format. Use YYYY-MM-DD"))?
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
    } else {
        Utc::now()
    };

    let taxonomy_date = if let Some(date_str) = &args.taxonomy_date {
        NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("Invalid taxonomy date format. Use YYYY-MM-DD"))?
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
    } else {
        sequence_date
    };

    // Create bi-temporal database
    let storage = Arc::new(HeraldStorage::new(&db_path)?);
    let mut bitemporal_db = BiTemporalDatabase::new(storage)?;

    // Query at specified dates
    section_header("Bi-Temporal Database Information");

    tree_item(false, "Database", Some(&args.database));
    tree_item(
        false,
        "Sequence Date",
        Some(&sequence_date.format("%Y-%m-%d").to_string()),
    );
    tree_item(
        false,
        "Taxonomy Date",
        Some(&taxonomy_date.format("%Y-%m-%d").to_string()),
    );

    // Try to get snapshot at this time
    match bitemporal_db.query_at(sequence_date, taxonomy_date) {
        Ok(snapshot) => {
            // Show snapshot info
            let coord_items = vec![
                ("Sequences", snapshot.sequence_count().to_string()),
                (
                    "Sequence Root",
                    format!("{:8}...", &snapshot.sequence_root().to_string()[..8]),
                ),
                (
                    "Taxonomy Root",
                    format!("{:8}...", &snapshot.taxonomy_root().to_string()[..8]),
                ),
            ];
            tree_section("Snapshot", coord_items, false);

            // Show available coordinates
            if let Ok(coords) = bitemporal_db.get_available_coordinates() {
                if !coords.is_empty() {
                    let coord_strings: Vec<String> = coords
                        .iter()
                        .take(5) // Show first 5
                        .map(|c| {
                            format!(
                                "Seq: {}, Tax: {}",
                                c.sequence_time.format("%Y-%m-%d"),
                                c.taxonomy_time.format("%Y-%m-%d")
                            )
                        })
                        .collect();

                    let coord_items: Vec<(&str, String)> = coord_strings
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            let label = Box::leak(format!("{}", i + 1).into_boxed_str());
                            (label as &str, s.clone())
                        })
                        .collect();
                    tree_section("Available Coordinates", coord_items, true);

                    if coords.len() > 5 {
                        info(&format!("... and {} more coordinates", coords.len() - 5));
                    }
                }
            }
        }
        Err(e) => {
            error(&format!("Cannot query at this coordinate: {}", e));
            info("The database may not have data for these dates");
        }
    }

    Ok(())
}

fn show_profile_info(
    manager: &talaria_herald::database::DatabaseManager,
    db_ref: &talaria_utils::database::database_ref::DatabaseReference,
    profile: &str,
    databases: &[talaria_herald::database::DatabaseInfo],
) -> anyhow::Result<()> {
    use crate::cli::formatting::output::*;
    use humansize::{format_size, BINARY};

    let base_name = db_ref.base_ref();

    // Find the database info
    let db_info = databases
        .iter()
        .find(|db| db.name == base_name)
        .ok_or_else(|| anyhow::anyhow!("Database '{}' not found in repository", base_name))?;

    // Parse source and dataset from the database name
    let parts: Vec<&str> = db_info.name.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid database name format: {}", db_info.name);
    }
    let source = parts[0];
    let dataset = parts[1];

    // Load the reduction profile manifest
    let storage = manager.get_storage();
    let profile_manifest = storage
        .get_database_reduction_by_profile(source, dataset, &db_info.version, profile)?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Profile '{}' not found for database '{}'",
                profile,
                base_name
            )
        })?;

    // Build tree structure for profile info
    tree_item(false, "Database", Some(&db_info.name));
    tree_item(false, "Version", Some(&db_info.version));
    tree_item(false, "Profile", Some(profile));
    tree_item(
        false,
        "Created",
        Some(
            &profile_manifest
                .created_at
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        ),
    );

    // Show reduction parameters
    let params_items = vec![
        (
            "Reduction ratio",
            format!(
                "{:.1}%",
                profile_manifest.parameters.reduction_ratio * 100.0
            ),
        ),
        (
            "Target aligner",
            profile_manifest
                .parameters
                .target_aligner
                .as_ref()
                .map(|a| format!("{:?}", a))
                .unwrap_or_else(|| "Generic".to_string()),
        ),
        (
            "Min sequence length",
            profile_manifest.parameters.min_length.to_string(),
        ),
        (
            "Similarity threshold",
            format!(
                "{:.1}%",
                profile_manifest.parameters.similarity_threshold * 100.0
            ),
        ),
        (
            "Taxonomy-aware",
            if profile_manifest.parameters.taxonomy_aware {
                "Yes"
            } else {
                "No"
            }
            .to_string(),
        ),
    ];
    tree_section("Parameters", params_items, false);

    // Show storage information
    let storage_items = vec![
        (
            "Reference chunks",
            profile_manifest.reference_chunks.len().to_string(),
        ),
        (
            "Delta chunks",
            profile_manifest.delta_chunks.len().to_string(),
        ),
        (
            "Total references",
            profile_manifest
                .reference_chunks
                .iter()
                .map(|c| c.sequence_count)
                .sum::<usize>()
                .to_string(),
        ),
    ];
    tree_section("Storage", storage_items, false);

    // Show reduction statistics if available
    let stats = &profile_manifest.statistics;
    let original_count = stats.original_sequences;
    let reference_count = stats.reference_sequences;
    let delta_count = stats.child_sequences;
    let coverage = (reference_count as f64 + delta_count as f64) / original_count as f64 * 100.0;

    let stats_items = vec![
        ("Original sequences", original_count.to_string()),
        ("Reference sequences", reference_count.to_string()),
        ("Delta sequences", delta_count.to_string()),
        ("Coverage", format!("{:.1}%", coverage)),
        ("Original size", format_size(stats.original_size, BINARY)),
        ("Reduced size", format_size(stats.reduced_size, BINARY)),
        (
            "Compression ratio",
            format!(
                "{:.2}x",
                stats.original_size as f64 / stats.reduced_size as f64
            ),
        ),
        (
            "Size reduction",
            format!(
                "{:.1}%",
                (1.0 - stats.reduced_size as f64 / stats.original_size as f64) * 100.0
            ),
        ),
    ];
    tree_section("Statistics", stats_items, false);

    // Show benefits compared to original
    let benefits_items = vec![
        ("Memory usage", "Optimized for aligner indexing".to_string()),
        ("Query coverage", format!("{:.1}%", coverage)),
        (
            "Reconstruction",
            "Full sequences recoverable via deltas".to_string(),
        ),
        (
            "Verification",
            format!(
                "SHA256 hash: {}",
                profile_manifest
                    .reduction_id
                    .to_hex()
                    .chars()
                    .take(16)
                    .collect::<String>()
            ),
        ),
    ];
    tree_section("Benefits", benefits_items, true);

    Ok(())
}
