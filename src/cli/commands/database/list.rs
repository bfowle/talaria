use clap::Args;
use std::path::PathBuf;
use comfy_table::{Table, Cell, Attribute, ContentArrangement, Color};
use comfy_table::presets::UTF8_FULL;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;

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

pub fn run(_args: ListArgs) -> anyhow::Result<()> {
    use crate::core::database_manager::DatabaseManager;
    use crate::utils::progress::create_spinner;
    use humansize::{format_size, BINARY};

    println!("[•] Database Repository\n");

    // Initialize database manager
    let spinner = create_spinner("Scanning for databases...");
    let manager = DatabaseManager::new(None)?;
    let databases = manager.list_databases()?;
    spinner.finish_and_clear();

    if databases.is_empty() {
        println!("No databases found in repository.");
        println!("Use 'talaria database download' to get started.");
        return Ok(());
    }

    // Create table
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(vec![
        Cell::new("Database").add_attribute(Attribute::Bold).fg(Color::Green),
        Cell::new("Version").add_attribute(Attribute::Bold).fg(Color::Green),
        Cell::new("Chunks").add_attribute(Attribute::Bold).fg(Color::Green),
        Cell::new("Size").add_attribute(Attribute::Bold).fg(Color::Green),
        Cell::new("Created").add_attribute(Attribute::Bold).fg(Color::Green),
    ]);

    for db in databases {
        table.add_row(vec![
            Cell::new(&db.name),
            Cell::new(&db.version),
            Cell::new(db.chunk_count.to_string()),
            Cell::new(format_size(db.total_size, BINARY)),
            Cell::new(db.created_at.format("%Y-%m-%d").to_string()),
        ]);
    }

    println!("{}", table);

    // Show repository stats
    let stats = manager.get_stats()?;
    println!("\n● Repository Statistics:");
    println!("   Total chunks: {}", stats.total_chunks);
    println!("   Total size: {}", format_size(stats.total_size, BINARY));
    println!("   Compressed chunks: {}", stats.compressed_chunks);
    println!("   Deduplication ratio: {:.2}x", stats.deduplication_ratio);

    Ok(())
}