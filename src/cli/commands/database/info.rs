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
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

pub fn run(args: InfoArgs) -> anyhow::Result<()> {
    use crate::core::database_manager::DatabaseManager;
    use crate::cli::output::*;
    use crate::utils::progress::create_spinner;
    use humansize::{format_size, BINARY};

    // Initialize database manager with spinner
    let spinner = create_spinner("Loading database information...");
    let manager = DatabaseManager::new(None)?;
    let databases = manager.list_databases()?;
    spinner.finish_and_clear();

    section_header("Database Information");

    // Find the requested database (handle both slash and hyphen formats)
    let db_info = databases.iter()
        .find(|db| {
            // Exact match or partial match at the end
            db.name == args.database || db.name.ends_with(&args.database)
        })
        .ok_or_else(|| anyhow::anyhow!("Database '{}' not found in repository", args.database))?;

    // Build tree structure for database info
    tree_item(false, "Name", Some(&db_info.name));
    tree_item(false, "Version", Some(&db_info.version));
    tree_item(false, "Created", Some(&db_info.created_at.format("%Y-%m-%d %H:%M:%S").to_string()));

    // Storage section
    let storage_items = vec![
        ("Chunks", db_info.chunk_count.to_string()),
        ("Size", format_size(db_info.total_size, BINARY)),
    ];
    tree_section("Storage", storage_items, false);

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
                if let Ok(Some(manifest)) = storage.get_reduction_by_profile(profile) {
                    let is_last_profile = idx == db_info.reduction_profiles.len() - 1;
                    let reduction_items = vec![
                        ("Reduction Ratio", format!("{:.1}%", manifest.statistics.actual_reduction_ratio * 100.0)),
                        ("Reference Sequences", format_number(manifest.statistics.reference_sequences)),
                        ("Child Sequences", format_number(manifest.statistics.child_sequences)),
                        ("Coverage", format!("{:.1}%", manifest.statistics.sequence_coverage * 100.0)),
                        ("Size", format!("{} → {}",
                            format_size(manifest.statistics.original_size as usize, BINARY),
                            format_size(manifest.statistics.reduced_size as usize, BINARY)
                        )),
                    ];
                    tree_section(profile, reduction_items, is_last_profile);
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

    // Show storage benefits as tree
    let stats = manager.get_stats()?;
    let benefits_items = vec![
        ("Deduplication ratio", format!("{:.2}x", stats.deduplication_ratio)),
        ("Storage saved", format!("~{}%", ((1.0 - 1.0/stats.deduplication_ratio) * 100.0) as i32)),
        ("Incremental updates", "Enabled".to_string()),
        ("Cryptographic verification", "SHA256".to_string()),
    ];
    tree_section("Storage Benefits", benefits_items, true);

    Ok(())
}