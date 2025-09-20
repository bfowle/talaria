pub mod add;
pub mod check_discrepancies;
pub mod download;
pub mod download_impl;
pub mod export;
pub mod info;
pub mod list;
pub mod list_sequences;
pub mod taxa_coverage;
pub mod update;
pub mod update_taxonomy;
pub mod versions;

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct DatabaseArgs {
    #[command(subcommand)]
    pub command: DatabaseCommands,
}

#[derive(Subcommand)]
pub enum DatabaseCommands {
    /// List downloaded databases
    List(list::ListArgs),

    /// Show information about a database
    Info(info::InfoArgs),

    /// Download biological databases
    Download(download::DownloadArgs),

    /// Update existing databases (check for new versions)
    Update(update::UpdateArgs),

    /// Add a custom database from a local FASTA file
    Add(add::AddArgs),

    /// Export database from CASG to FASTA format
    Export(export::ExportArgs),

    /// Manage database versions
    Versions(versions::VersionsArgs),

    /// Show repository statistics
    Stats,

    /// List sequences in a database
    ListSequences(list_sequences::ListSequencesArgs),

    /// Analyze taxonomic coverage of databases
    TaxaCoverage(taxa_coverage::TaxaCoverageArgs),

    /// Update NCBI taxonomy data
    UpdateTaxonomy(update_taxonomy::UpdateTaxonomyArgs),

    /// Check database for discrepancies and issues
    Check(check_discrepancies::CheckDiscrepanciesArgs),

    /// Initialize database repository
    Init,
}

pub fn run(args: DatabaseArgs) -> anyhow::Result<()> {
    match args.command {
        DatabaseCommands::List(args) => list::run(args),
        DatabaseCommands::Info(args) => info::run(args),
        DatabaseCommands::Download(args) => download::run(args),
        DatabaseCommands::Update(args) => update::run(args),
        DatabaseCommands::Add(args) => add::run(args),
        DatabaseCommands::Export(args) => export::run(args),
        DatabaseCommands::Versions(args) => versions::run(args),
        DatabaseCommands::Stats => run_stats(),
        DatabaseCommands::ListSequences(args) => list_sequences::run(args),
        DatabaseCommands::TaxaCoverage(args) => taxa_coverage::run(args),
        DatabaseCommands::UpdateTaxonomy(args) => update_taxonomy::run(args),
        DatabaseCommands::Check(args) => check_discrepancies::run(args),
        DatabaseCommands::Init => run_init(),
    }
}

fn run_init() -> anyhow::Result<()> {
    use crate::casg::CASGRepository;
    use crate::core::paths;
    use colored::*;

    let path = paths::talaria_databases_dir();

    println!(
        "{} Initializing database repository at {}...",
        "►".cyan().bold(),
        path.display()
    );

    if path.exists() && path.join("manifest.json").exists() {
        println!("{} Database repository already exists", "⚠".yellow().bold());
        return Ok(());
    }

    std::fs::create_dir_all(&path)?;
    CASGRepository::init(&path)?;

    println!(
        "{} Database repository initialized successfully!",
        "✓".green().bold()
    );
    println!("  Path: {}", path.display());

    Ok(())
}

fn run_stats() -> anyhow::Result<()> {
    use crate::core::database_manager::DatabaseManager;
    use crate::utils::progress::create_spinner;
    use colored::*;

    let spinner = create_spinner("Loading repository statistics...");
    let mut manager = DatabaseManager::new(None)?;

    // Initialize temporal tracking for existing data if needed
    let _ = manager.init_temporal_for_existing();

    let stats = manager.get_stats()?;
    spinner.finish_and_clear();

    println!("\n{}", "═".repeat(60));
    println!("{:^60}", "DATABASE REPOSITORY STATISTICS");
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
