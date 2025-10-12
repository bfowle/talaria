#![allow(dead_code)]

pub mod add; // Canonical sequence-based add (the ONLY add)
pub mod backup;
pub mod check_discrepancies;
pub mod clean; // Database cleaning (removes unreferenced data)
pub mod delete;
pub mod diff;
pub mod download;
pub mod download_impl;
pub mod export;
pub mod info;
pub mod list;
pub mod list_sequences;
pub mod mirror; // Database mirroring
pub mod optimize; // Database optimization
pub mod taxa_coverage;
pub mod update;
pub mod update_taxonomy;
pub mod verify;
pub mod versions;

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct DatabaseArgs {
    #[command(subcommand)]
    pub command: DatabaseCommands,
}

#[derive(Subcommand)]
pub enum DatabaseCommands {
    // === Core Operations ===
    /// Initialize database repository
    Init,

    /// Download biological databases
    Download(download::DownloadArgs),

    /// Add a custom database from a local FASTA file
    Add(add::AddArgs),

    /// Update existing databases (check for new versions)
    Update(update::UpdateArgs),

    // === Information & Browsing ===
    /// List downloaded databases
    List(list::ListArgs),

    /// Show information about a database
    Info(info::InfoArgs),

    /// Show repository statistics
    Stats,

    /// List sequences in a database
    ListSequences(list_sequences::ListSequencesArgs),

    // === Version Management ===
    /// Manage database versions
    Versions(versions::VersionsArgs),

    // === Backup & Recovery ===
    /// Manage database backups
    Backup(backup::BackupCommand),

    // === Export & Integration ===
    /// Export database from HERALD to FASTA format
    Export(export::ExportArgs),

    /// Setup and manage database mirrors
    Mirror(mirror::MirrorCmd),

    // === Maintenance & Optimization ===
    /// Delete a database or specific version
    Delete(delete::DeleteArgs),

    /// Verify database integrity
    Verify(verify::VerifyArgs),

    /// Check database for discrepancies and issues
    Check(check_discrepancies::CheckDiscrepanciesArgs),

    /// Clean database (remove unreferenced data, orphaned chunks, etc.)
    Clean(clean::CleanCmd),

    /// Optimize database storage and performance
    Optimize(optimize::OptimizeCmd),

    /// Show differences between databases or versions
    Diff(diff::DiffArgs),

    // === Taxonomy Operations ===
    /// Analyze taxonomic coverage of databases
    TaxaCoverage(taxa_coverage::TaxaCoverageArgs),

    /// Update NCBI taxonomy data
    UpdateTaxonomy(update_taxonomy::UpdateTaxonomyArgs),
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
        DatabaseCommands::Delete(args) => delete::run(args),
        DatabaseCommands::Verify(args) => verify::run(args),
        DatabaseCommands::Clean(args) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(args.run())
        }
        DatabaseCommands::Diff(args) => diff::run(args),
        DatabaseCommands::Backup(args) => backup::execute(&args),
        DatabaseCommands::Optimize(args) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(args.run())
        }
        DatabaseCommands::Mirror(args) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(args.run())
        }
    }
}

fn run_init() -> anyhow::Result<()> {
    use colored::*;
    use talaria_core::system::paths;
    use talaria_herald::HeraldRepository;

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
    HeraldRepository::init(&path)?;

    println!(
        "{} Database repository initialized successfully!",
        "✓".green().bold()
    );
    println!("  Path: {}", path.display());

    Ok(())
}

fn run_stats() -> anyhow::Result<()> {
    use crate::cli::formatting::output::format_number;
    use crate::cli::progress::create_spinner;
    use colored::*;
    use humansize::{format_size, BINARY};
    use talaria_herald::database::DatabaseManager;

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
    println!(
        "{} {}",
        "Total chunks:".bold(),
        format_number(stats.total_chunks).cyan()
    );
    println!(
        "{} {}",
        "Total size:".bold(),
        format_size(stats.total_size, BINARY).cyan()
    );
    println!(
        "{} {}",
        "Compressed chunks:".bold(),
        format_number(stats.compressed_chunks).cyan()
    );
    println!(
        "{} {}",
        "Deduplication ratio:".bold(),
        format!("{:.2}x", stats.deduplication_ratio).green()
    );
    println!(
        "{} {}",
        "Databases:".bold(),
        format_number(stats.database_count).cyan()
    );

    if !stats.databases.is_empty() {
        println!("\n{}", "Databases:".bold().underline());

        // Group by source/dataset to count versions
        use std::collections::HashMap;
        let mut version_counts: HashMap<String, usize> = HashMap::new();
        for db in &stats.databases {
            *version_counts.entry(db.name.clone()).or_insert(0) += 1;
        }

        // Show unique databases with version counts
        let mut shown_databases = std::collections::HashSet::new();
        for db in &stats.databases {
            if shown_databases.insert(db.name.clone()) {
                let version_count = version_counts.get(&db.name).unwrap_or(&0);
                let version_info = if *version_count > 1 {
                    format!("{} versions", version_count).dimmed()
                } else {
                    format!("v{}", db.version).dimmed()
                };
                println!(
                    "  • {} ({}, {} chunks, {})",
                    db.name,
                    version_info,
                    format_number(db.chunk_count).dimmed(),
                    format_size(db.total_size, BINARY).dimmed()
                );
            }
        }
    }

    // RocksDB Statistics
    println!("\n{}", "═".repeat(60));
    println!("{}", "RocksDB Storage".bold());
    println!("{}", "─".repeat(60));

    let rocksdb_path =
        talaria_core::system::paths::talaria_databases_dir().join("sequences/rocksdb");
    println!(
        "{} {}",
        "Storage path:".bold(),
        rocksdb_path.display().to_string().cyan()
    );

    // Count total versions across all databases
    let total_versions = stats.databases.len();
    println!(
        "{} {}",
        "Total versions:".bold(),
        format_number(total_versions).cyan()
    );

    // Try to get RocksDB directory size
    if rocksdb_path.exists() {
        let mut total_size = 0u64;
        let mut sst_count = 0usize;

        if let Ok(entries) = std::fs::read_dir(&rocksdb_path) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    total_size += metadata.len();
                    if let Some(ext) = entry.path().extension() {
                        if ext == "sst" {
                            sst_count += 1;
                        }
                    }
                }
            }
        }

        println!(
            "{} {}",
            "RocksDB size:".bold(),
            format_size(total_size as usize, BINARY).cyan()
        );
        println!(
            "{} {}",
            "SST files:".bold(),
            format_number(sst_count).cyan()
        );
    }

    println!();

    Ok(())
}
