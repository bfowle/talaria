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
    use humansize::{format_size, BINARY};

    println!("[•] Database Information\n");

    // Initialize database manager
    let manager = DatabaseManager::new(None)?;
    let databases = manager.list_databases()?;

    // Find the requested database
    let db_info = databases.iter()
        .find(|db| db.name == args.database || db.name.ends_with(&args.database))
        .ok_or_else(|| anyhow::anyhow!("Database '{}' not found in repository", args.database))?;

    println!("Database: {}", db_info.name);
    println!("Version: {}", db_info.version);
    println!("Created: {}", db_info.created_at.format("%Y-%m-%d %H:%M:%S"));
    println!("Chunks: {}", db_info.chunk_count);
    println!("Total Size: {}", format_size(db_info.total_size, BINARY));

    if args.stats {
        println!("\n● Statistics:");
        // We'd need to assemble and analyze to get full stats
        println!("   Full statistics require assembling chunks");
        println!("   This will be implemented in a future update");
    }

    // Show storage info
    let stats = manager.get_stats()?;
    println!("\n🔗 Storage Benefits:");
    println!("   Deduplication ratio: {:.2}x", stats.deduplication_ratio);
    println!("   Storage saved: ~{}%", ((1.0 - 1.0/stats.deduplication_ratio) * 100.0) as i32);
    println!("   Incremental updates: Enabled");
    println!("   Cryptographic verification: SHA256");

    Ok(())
}