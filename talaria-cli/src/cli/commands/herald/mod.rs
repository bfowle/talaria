#![allow(dead_code)]

pub mod history;
pub mod sync;
pub mod time_travel;
pub mod verify_storage;

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct HeraldArgs {
    #[command(subcommand)]
    pub command: HeraldCommands,
}

#[derive(Subcommand)]
pub enum HeraldCommands {
    /// Synchronize HERALD repository with cloud storage
    Sync(sync::SyncArgs),

    /// Show version history of HERALD repository
    History(history::HistoryArgs),

    /// Initialize a new HERALD repository
    Init(InitArgs),

    /// Show HERALD repository statistics
    Stats(StatsArgs),

    /// Query database at specific time points (bi-temporal)
    TimeTravel(time_travel::TimeTravelArgs),

    /// Verify and repair HERALD storage integrity
    VerifyStorage(verify_storage::VerifyStorageArgs),
}

#[derive(Args)]
pub struct InitArgs {
    /// Path to initialize HERALD repository
    #[arg(short, long)]
    pub path: Option<std::path::PathBuf>,
}

#[derive(Args)]
pub struct StatsArgs {
    /// Path to HERALD repository
    #[arg(short, long)]
    pub path: Option<std::path::PathBuf>,
}

pub fn run(args: HeraldArgs) -> anyhow::Result<()> {
    match args.command {
        HeraldCommands::Sync(args) => sync::run(args),
        HeraldCommands::History(args) => history::run(args),
        HeraldCommands::Init(args) => run_init(args),
        HeraldCommands::Stats(args) => run_stats(args),
        HeraldCommands::TimeTravel(args) => time_travel::run(args),
        HeraldCommands::VerifyStorage(args) => verify_storage::run(args),
    }
}

fn run_init(args: InitArgs) -> anyhow::Result<()> {
    use colored::*;
    use talaria_herald::HeraldRepository;

    let path = if let Some(p) = args.path {
        p
    } else {
        use talaria_core::system::paths;
        paths::talaria_databases_dir()
    };

    println!(
        "{} Initializing HERALD repository at {}...",
        "►".cyan().bold(),
        path.display()
    );

    if path.exists() && path.join("manifest.json").exists() {
        println!("{} HERALD repository already exists", "⚠".yellow().bold());
        return Ok(());
    }

    std::fs::create_dir_all(&path)?;
    HeraldRepository::init(&path)?;

    println!(
        "{} HERALD repository initialized successfully!",
        "✓".green().bold()
    );
    println!("  Path: {}", path.display());

    Ok(())
}

fn run_stats(args: StatsArgs) -> anyhow::Result<()> {
    use crate::cli::progress::create_spinner;
    use colored::*;
    use talaria_herald::database::DatabaseManager as HeraldDatabaseManager;

    let path = if let Some(p) = args.path {
        p
    } else {
        use talaria_core::system::paths;
        paths::talaria_databases_dir()
    };

    if !path.exists() {
        anyhow::bail!(
            "HERALD repository not found at {}. Initialize it first with 'talaria herald init'",
            path.display()
        );
    }

    let spinner = create_spinner("Loading HERALD repository statistics...");
    let mut manager = HeraldDatabaseManager::new(Some(path.to_string_lossy().to_string()))?;

    // Initialize temporal tracking for existing data if needed
    let _ = manager.init_temporal_for_existing();

    let stats = manager.get_stats()?;
    spinner.finish_and_clear();

    println!("\n{}", "═".repeat(60));
    println!("{:^60}", "HERALD REPOSITORY STATISTICS");
    println!("{}", "═".repeat(60));
    println!();
    println!("{} {}", "Total chunks:".bold(), stats.total_chunks);
    println!(
        "{} {:.2} MB",
        "Total size:".bold(),
        stats.total_size as f64 / 1_048_576.0
    );
    println!(
        "{} {}",
        "Compressed chunks:".bold(),
        stats.compressed_chunks
    );
    println!(
        "{} {:.2}x",
        "Deduplication ratio:".bold(),
        stats.deduplication_ratio
    );
    println!("{} {}", "Databases:".bold(), stats.database_count);

    if !stats.databases.is_empty() {
        println!("\n{}", "Databases:".bold().underline());
        for db in &stats.databases {
            println!(
                "  • {} (v{}, {} chunks, {:.2} MB)",
                db.name,
                db.version,
                db.chunk_count,
                db.total_size as f64 / 1_048_576.0
            );
        }
    }

    println!();

    Ok(())
}
