pub mod history;
pub mod sync;

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct CasgArgs {
    #[command(subcommand)]
    pub command: CasgCommands,
}

#[derive(Subcommand)]
pub enum CasgCommands {
    /// Synchronize CASG repository with cloud storage
    Sync(sync::SyncArgs),

    /// Show version history of CASG repository
    History(history::HistoryArgs),

    /// Initialize a new CASG repository
    Init(InitArgs),

    /// Show CASG repository statistics
    Stats(StatsArgs),
}

#[derive(Args)]
pub struct InitArgs {
    /// Path to initialize CASG repository
    #[arg(short, long)]
    pub path: Option<std::path::PathBuf>,
}

#[derive(Args)]
pub struct StatsArgs {
    /// Path to CASG repository
    #[arg(short, long)]
    pub path: Option<std::path::PathBuf>,
}

pub fn run(args: CasgArgs) -> anyhow::Result<()> {
    match args.command {
        CasgCommands::Sync(args) => sync::run(args),
        CasgCommands::History(args) => history::run(args),
        CasgCommands::Init(args) => run_init(args),
        CasgCommands::Stats(args) => run_stats(args),
    }
}

fn run_init(args: InitArgs) -> anyhow::Result<()> {
    use crate::casg::CASGRepository;
    use colored::*;

    let path = if let Some(p) = args.path {
        p
    } else {
        use crate::core::paths;
        paths::talaria_casg_dir()
    };

    println!("{} Initializing CASG repository at {}...",
             "►".cyan().bold(),
             path.display());

    if path.exists() && path.join("manifest.json").exists() {
        println!("{} CASG repository already exists",
                 "⚠".yellow().bold());
        return Ok(());
    }

    std::fs::create_dir_all(&path)?;
    CASGRepository::init(&path)?;

    println!("{} CASG repository initialized successfully!",
             "✓".green().bold());
    println!("  Path: {}", path.display());

    Ok(())
}

fn run_stats(args: StatsArgs) -> anyhow::Result<()> {
    use crate::core::database_manager::DatabaseManager as CASGDatabaseManager;
    use crate::utils::progress::create_spinner;
    use colored::*;

    let path = if let Some(p) = args.path {
        p
    } else {
        use crate::core::paths;
        paths::talaria_casg_dir()
    };

    if !path.exists() {
        anyhow::bail!("CASG repository not found at {}. Initialize it first with 'talaria casg init'",
                     path.display());
    }

    let spinner = create_spinner("Loading CASG repository statistics...");
    let mut manager = CASGDatabaseManager::new(Some(path.to_string_lossy().to_string()))?;

    // Initialize temporal tracking for existing data if needed
    let _ = manager.init_temporal_for_existing();

    let stats = manager.get_stats()?;
    spinner.finish_and_clear();

    println!("\n{}", "═".repeat(60));
    println!("{:^60}", "CASG REPOSITORY STATISTICS");
    println!("{}", "═".repeat(60));
    println!();
    println!("{} {}", "Total chunks:".bold(), stats.total_chunks);
    println!("{} {:.2} MB", "Total size:".bold(),
             stats.total_size as f64 / 1_048_576.0);
    println!("{} {}", "Compressed chunks:".bold(), stats.compressed_chunks);
    println!("{} {:.2}x", "Deduplication ratio:".bold(), stats.deduplication_ratio);
    println!("{} {}", "Databases:".bold(), stats.database_count);

    if !stats.databases.is_empty() {
        println!("\n{}", "Databases:".bold().underline());
        for db in &stats.databases {
            println!("  • {} (v{}, {} chunks, {:.2} MB)",
                     db.name,
                     db.version,
                     db.chunk_count,
                     db.total_size as f64 / 1_048_576.0);
        }
    }

    println!();

    Ok(())
}