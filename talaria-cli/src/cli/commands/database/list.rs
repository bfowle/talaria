#![allow(dead_code)]

use crate::cli::formatting::output::*;
use clap::Args;
use colored::*;
use std::path::PathBuf;

#[derive(Args)]
pub struct ListArgs {
    /// Directory to search for databases (overrides default)
    #[arg(short, long)]
    pub directory: Option<PathBuf>,

    /// Show detailed information
    #[arg(long)]
    pub detailed: bool,

    /// Show all versions (not just current)
    #[arg(long)]
    pub all_versions: bool,

    /// Specific database to list (e.g., "uniprot/swissprot")
    #[arg(long)]
    pub database: Option<String>,

    /// Sort by field (name, size, date)
    #[arg(long, default_value = "name")]
    pub sort: SortField,

    /// Show reduced versions
    #[arg(long)]
    pub show_reduced: bool,

    /// List versions at specific sequence date (e.g., "2020-01-01")
    #[arg(long)]
    pub sequence_date: Option<String>,

    /// List versions at specific taxonomy date (e.g., "2020-01-01")
    #[arg(long)]
    pub taxonomy_date: Option<String>,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum SortField {
    Name,
    Size,
    Date,
}

pub fn run(args: ListArgs) -> anyhow::Result<()> {
    let _span = tracing::info_span!("database_list").entered();

    use crate::cli::progress::create_spinner;
    use humansize::{format_size, BINARY};
    use talaria_herald::database::DatabaseManager;

    // Check if we need bi-temporal listing
    if args.sequence_date.is_some() || args.taxonomy_date.is_some() {
        return run_bitemporal_list(args);
    }

    section_header("Database Repository");

    // Initialize database manager
    let spinner = create_spinner("Scanning for databases...");
    let manager = {
        let _span = tracing::info_span!("initialize_database_manager").entered();
        DatabaseManager::new(None)?
    };

    // If --all-versions is specified, show version hierarchy
    if args.all_versions {
        spinner.finish_and_clear();
        return run_all_versions_list(&manager);
    }

    let databases = {
        let _span = tracing::info_span!("list_databases").entered();
        manager.list_databases()?
    };
    spinner.finish_and_clear();

    if databases.is_empty() {
        empty("No databases found in repository");
        info("Use 'talaria database download' to get started.");
        return Ok(());
    }

    // Use tree structure for detailed view or table for normal view
    if args.detailed {
        // Tree structure view
        for (i, db) in databases.iter().enumerate() {
            let is_last = i == databases.len() - 1;
            tree_item(false, &db.name, None);

            let items = vec![
                ("Version", db.version.clone()),
                ("Created", db.created_at.format("%Y-%m-%d").to_string()),
                ("Sequences", format_number(db.sequence_count)),
                ("Chunks", format_number(db.chunk_count)),
                ("Size", format_size(db.total_size, BINARY)),
            ];

            // Add reductions if present
            if !db.reduction_profiles.is_empty() {
                tree_item_continued("Storage", None);
                for (label, value) in &items {
                    println!("│  │  {} {}: {}", "├─".dimmed(), label, value);
                }
                tree_item_continued_last("Reductions", None);
                for (j, profile) in db.reduction_profiles.iter().enumerate() {
                    let is_last_profile = j == db.reduction_profiles.len() - 1;
                    if is_last_profile {
                        println!("│     {} {}", "└─".dimmed(), profile);
                    } else {
                        println!("│     {} {}", "├─".dimmed(), profile);
                    }
                }
            } else {
                for (j, (label, value)) in items.iter().enumerate() {
                    let is_last_item = j == items.len() - 1;
                    if is_last_item {
                        tree_item_continued_last(label, Some(value));
                    } else {
                        tree_item_continued(label, Some(value));
                    }
                }
            }

            if !is_last {
                println!();
            }
        }
    } else {
        // Table view
        let mut table = create_standard_table();

        table.set_header(vec![
            header_cell("Database"),
            header_cell("Version"),
            header_cell("Sequences"),
            header_cell("Chunks"),
            header_cell("Size"),
            header_cell("Reductions"),
            header_cell("Created"),
        ]);

        for db in databases {
            use comfy_table::Cell;

            // Format reduction profiles
            let reductions = if db.reduction_profiles.is_empty() {
                "-".to_string()
            } else {
                db.reduction_profiles.join(", ")
            };

            table.add_row(vec![
                Cell::new(&db.name),
                Cell::new(&db.version),
                Cell::new(format_number(db.sequence_count)),
                Cell::new(format_number(db.chunk_count)),
                Cell::new(format_size(db.total_size, BINARY)),
                Cell::new(&reductions),
                Cell::new(db.created_at.format("%Y-%m-%d").to_string()),
            ]);
        }

        println!("{}", table);
    }

    Ok(())
}

fn run_all_versions_list(
    manager: &talaria_herald::database::DatabaseManager,
) -> anyhow::Result<()> {
    let _span = tracing::debug_span!("database_list_all_versions").entered();

    use humansize::{format_size, BINARY};
    use std::collections::HashMap;

    // Get all databases and group by source/dataset
    let databases = manager.list_databases()?;

    if databases.is_empty() {
        empty("No databases found in repository");
        info("Use 'talaria database download' to get started.");
        return Ok(());
    }

    // Group databases by source/dataset name
    let mut grouped: HashMap<String, Vec<_>> = HashMap::new();
    for db in databases {
        grouped.entry(db.name.clone()).or_default().push(db);
    }

    // Sort by database name
    let mut db_names: Vec<_> = grouped.keys().cloned().collect();
    db_names.sort();

    println!();
    for (idx, db_name) in db_names.iter().enumerate() {
        let versions = &grouped[db_name];
        let is_last_db = idx == db_names.len() - 1;

        // Database name with count
        println!(
            "{} {} {} {}",
            "●".cyan().bold(),
            db_name.bold(),
            format!(
                "({} version{})",
                versions.len(),
                if versions.len() == 1 { "" } else { "s" }
            )
            .dimmed(),
            format!("└ RocksDB: manifest:{}:*", db_name.replace('/', ":")).dimmed()
        );

        // Sort versions by date (newest first)
        let mut sorted_versions = versions.clone();
        sorted_versions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Show each version
        for (v_idx, version) in sorted_versions.iter().enumerate() {
            let is_last_version = v_idx == sorted_versions.len() - 1;
            let prefix = if is_last_version { "└─" } else { "├─" };

            // Check if this is current version by looking at version string
            let is_current = version.reduction_profiles.is_empty(); // Simple heuristic for now

            let version_marker = if is_current {
                "▶".green().bold()
            } else {
                "○".dimmed()
            };

            println!(
                "  {} {} {} {} {} {} {}",
                prefix.dimmed(),
                version_marker,
                version.version.cyan(),
                format!("({})", version.created_at.format("%Y-%m-%d %H:%M")).dimmed(),
                format!("─").dimmed(),
                format!("{} chunks", format_number(version.chunk_count)).dimmed(),
                format!("({})", format_size(version.total_size, BINARY)).dimmed()
            );
        }

        if !is_last_db {
            println!();
        }
    }

    println!();
    info("Use 'talaria database versions list <database>' for detailed version info");
    info(&format!(
        "Storage location: {}",
        talaria_core::system::paths::talaria_databases_dir()
            .join("sequences/rocksdb")
            .display()
    ));

    Ok(())
}

fn run_bitemporal_list(args: ListArgs) -> anyhow::Result<()> {
    let _span = tracing::debug_span!("database_list_bitemporal").entered();

    use chrono::{NaiveDate, Utc};
    use std::sync::Arc;
    use talaria_core::system::paths;
    use talaria_herald::{BiTemporalDatabase, HeraldStorage};

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

    section_header("Bi-Temporal Database Listing");

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

    subsection_header("Available Databases");

    // Scan all databases and check their temporal availability
    let db_dir = paths::talaria_databases_dir();
    let mut found_any = false;

    if db_dir.exists() {
        for source_entry in std::fs::read_dir(&db_dir)? {
            let source_entry = source_entry?;
            if source_entry.file_type()?.is_dir() {
                let source_name = source_entry.file_name();
                let source_str = source_name.to_string_lossy();

                // Skip special directories
                if source_str.starts_with('.') || source_str == "exports" {
                    continue;
                }

                for dataset_entry in std::fs::read_dir(source_entry.path())? {
                    let dataset_entry = dataset_entry?;
                    if dataset_entry.file_type()?.is_dir() {
                        let dataset_name = dataset_entry.file_name();
                        let dataset_str = dataset_name.to_string_lossy();

                        let db_path = dataset_entry.path();
                        let db_name = format!("{}/{}", source_str, dataset_str);

                        // Try to create bi-temporal database
                        if let Ok(storage) = HeraldStorage::new(&db_path) {
                            let storage = Arc::new(storage);
                            if let Ok(mut bitemporal_db) = BiTemporalDatabase::new(storage.clone())
                            {
                                // Check if we can query at these dates
                                match bitemporal_db.query_at(sequence_date, taxonomy_date) {
                                    Ok(snapshot) => {
                                        found_any = true;
                                        tree_item(false, &db_name, None);

                                        let items = vec![
                                            ("Sequences", format_number(snapshot.sequence_count())),
                                            (
                                                "Sequence Root",
                                                format!(
                                                    "{:8}...",
                                                    &snapshot.sequence_root().to_string()[..8]
                                                ),
                                            ),
                                            (
                                                "Taxonomy Root",
                                                format!(
                                                    "{:8}...",
                                                    &snapshot.taxonomy_root().to_string()[..8]
                                                ),
                                            ),
                                        ];

                                        for (label, value) in &items {
                                            println!("  {} {}: {}", "├─".dimmed(), label, value);
                                        }
                                    }
                                    Err(_) => {
                                        // Database doesn't have data for these dates
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !found_any {
        empty("No databases found with data at the specified temporal coordinates");
        info("Try different dates or use 'talaria database info --sequence-date <date>' to see available dates");
    }

    Ok(())
}
