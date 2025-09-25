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
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum SortField {
    Name,
    Size,
    Date,
}

pub fn run(args: ListArgs) -> anyhow::Result<()> {
    use crate::core::database::database_manager::DatabaseManager;
    use crate::cli::progress::create_spinner;
    use humansize::{format_size, BINARY};

    section_header("Database Repository");

    // Initialize database manager
    let spinner = create_spinner("Scanning for databases...");
    let manager = DatabaseManager::new(None)?;
    let databases = manager.list_databases()?;
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
                Cell::new(format_number(db.chunk_count)),
                Cell::new(format_size(db.total_size, BINARY)),
                Cell::new(&reductions),
                Cell::new(db.created_at.format("%Y-%m-%d").to_string()),
            ]);
        }

        println!("{}", table);
    }

    // Show repository stats as tree
    let stats = manager.get_stats()?;
    subsection_header("Repository Statistics");
    let stats_items = [("Total chunks", format_number(stats.total_chunks)),
        ("Total size", format_size(stats.total_size, BINARY)),
        ("Compressed chunks", format_number(stats.compressed_chunks)),
        (
            "Deduplication ratio",
            format!("{:.2}x", stats.deduplication_ratio),
        )];

    for (i, (label, value)) in stats_items.iter().enumerate() {
        tree_item(i == stats_items.len() - 1, label, Some(value));
    }

    Ok(())
}
