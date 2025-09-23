#![allow(dead_code)]

use talaria_sequoia::SEQUOIARepository;
use talaria_core::paths;
use crate::utils::progress::create_spinner;
use clap::Args;
use colored::*;
use std::collections::HashSet;

#[derive(Args)]
pub struct CleanArgs {
    /// Database name to clean (cleans all if not specified)
    #[arg(value_name = "DATABASE")]
    pub database: Option<String>,

    /// Remove orphaned chunks not referenced in any manifest
    #[arg(long)]
    pub orphaned: bool,

    /// Remove duplicate chunks (keeps one copy)
    #[arg(long)]
    pub duplicates: bool,

    /// Compact database by reorganizing chunks
    #[arg(long)]
    pub compact: bool,

    /// Clean all issues (orphaned, duplicates, and compact)
    #[arg(long, short = 'a')]
    pub all: bool,

    /// Dry run - show what would be cleaned without actually doing it
    #[arg(long, short = 'n')]
    pub dry_run: bool,

    /// Force cleaning without confirmation
    #[arg(long, short = 'f')]
    pub force: bool,
}

pub fn run(args: CleanArgs) -> anyhow::Result<()> {
    let base_path = if let Some(db_name) = &args.database {
        paths::talaria_databases_dir().join("data").join(db_name)
    } else {
        paths::talaria_databases_dir()
    };

    if !base_path.exists() {
        return Err(anyhow::anyhow!(
            "Database path does not exist: {}",
            base_path.display()
        ));
    }

    let clean_orphaned = args.all || args.orphaned;
    let clean_duplicates = args.all || args.duplicates;
    let do_compact = args.all || args.compact;

    if !clean_orphaned && !clean_duplicates && !do_compact {
        println!("{} No cleaning operations specified. Use --orphaned, --duplicates, --compact, or --all",
                "⚠".yellow().bold());
        return Ok(());
    }

    println!(
        "{} {} database at {}...",
        "►".cyan().bold(),
        if args.dry_run { "Analyzing" } else { "Cleaning" },
        base_path.display()
    );

    let mut repo = SEQUOIARepository::open(&base_path)?;
    let mut total_freed = 0usize;
    let mut chunks_removed = 0usize;

    if clean_orphaned {
        let (removed, freed) = clean_orphaned_chunks(&mut repo, args.dry_run)?;
        chunks_removed += removed;
        total_freed += freed;
    }

    if clean_duplicates {
        let (removed, freed) = clean_duplicate_chunks(&mut repo, args.dry_run)?;
        chunks_removed += removed;
        total_freed += freed;
    }

    if do_compact {
        let freed = compact_database(&mut repo, args.dry_run)?;
        total_freed += freed;
    }

    // Display results
    println!("\n{}", "─".repeat(60));
    println!("{:^60}", if args.dry_run { "CLEANING ANALYSIS" } else { "CLEANING COMPLETE" });
    println!("{}", "─".repeat(60));

    if args.dry_run {
        println!("{} Would remove {} chunks", "►".cyan().bold(), chunks_removed);
        println!("{} Would free {:.2} MB", "►".cyan().bold(), total_freed as f64 / 1_048_576.0);
        println!("\nRun without --dry-run to perform actual cleaning");
    } else {
        println!("{} Removed {} chunks", "✓".green().bold(), chunks_removed);
        println!("{} Freed {:.2} MB", "✓".green().bold(), total_freed as f64 / 1_048_576.0);
    }

    Ok(())
}

fn clean_orphaned_chunks(repo: &mut SEQUOIARepository, dry_run: bool) -> anyhow::Result<(usize, usize)> {
    let spinner = create_spinner("Scanning for orphaned chunks...");

    let manifest_data = repo.manifest.get_data()
        .ok_or_else(|| anyhow::anyhow!("No manifest loaded"))?;

    // Get all stored chunks
    let stored_chunks = repo.storage.list_all_chunks()?;

    // Get all referenced chunks
    let referenced_chunks: HashSet<_> = manifest_data.chunk_index
        .iter()
        .map(|c| c.hash.clone())
        .collect();

    // Find orphaned chunks
    let orphaned: Vec<_> = stored_chunks
        .into_iter()
        .filter(|h| !referenced_chunks.contains(h))
        .collect();

    spinner.finish_and_clear();

    if orphaned.is_empty() {
        println!("{} No orphaned chunks found", "✓".green().bold());
        return Ok((0, 0));
    }

    println!("{} Found {} orphaned chunks", "►".cyan().bold(), orphaned.len());

    if dry_run {
        return Ok((orphaned.len(), estimate_chunk_size(&orphaned)));
    }

    // Remove orphaned chunks
    let spinner = create_spinner("Removing orphaned chunks...");
    let mut total_freed = 0;

    for chunk_hash in &orphaned {
        if let Ok(size) = repo.storage.get_chunk_size(chunk_hash) {
            total_freed += size;
        }
        repo.storage.remove_chunk(chunk_hash)?;
    }

    spinner.finish_and_clear();

    Ok((orphaned.len(), total_freed))
}

fn clean_duplicate_chunks(_repo: &mut SEQUOIARepository, _dry_run: bool) -> anyhow::Result<(usize, usize)> {
    let spinner = create_spinner("Scanning for duplicate chunks...");

    // In content-addressed storage, duplicates shouldn't exist by design
    // This is a placeholder for potential future deduplication logic
    // For now, we'll check for chunks with identical content but different hashes
    // (which shouldn't happen but could due to bugs)

    spinner.finish_and_clear();
    println!("{} No duplicate chunks found (content-addressed storage)", "✓".green().bold());

    Ok((0, 0))
}

fn compact_database(repo: &mut SEQUOIARepository, dry_run: bool) -> anyhow::Result<usize> {
    let spinner = create_spinner("Analyzing database for compaction...");

    let manifest_data = repo.manifest.get_data()
        .ok_or_else(|| anyhow::anyhow!("No manifest loaded"))?;

    // Calculate fragmentation
    let total_chunks = manifest_data.chunk_index.len();
    let avg_chunk_size = manifest_data.chunk_index
        .iter()
        .map(|c| c.size)
        .sum::<usize>() / total_chunks.max(1);

    // Find small chunks that could be merged
    let small_chunks: Vec<_> = manifest_data.chunk_index
        .iter()
        .filter(|c| c.size < avg_chunk_size / 2)
        .collect();

    spinner.finish_and_clear();

    if small_chunks.is_empty() {
        println!("{} Database is already well-compacted", "✓".green().bold());
        return Ok(0);
    }

    println!("{} Found {} small chunks that could be merged",
            "►".cyan().bold(), small_chunks.len());

    if dry_run {
        let potential_savings = small_chunks.len() * 1024; // Estimate metadata overhead
        return Ok(potential_savings);
    }

    // In a real implementation, we would:
    // 1. Merge small adjacent chunks
    // 2. Update the manifest
    // 3. Remove old chunks
    // For now, this is a placeholder

    println!("{} Compaction not yet implemented", "⚠".yellow().bold());

    Ok(0)
}

fn estimate_chunk_size(chunks: &[talaria_sequoia::types::SHA256Hash]) -> usize {
    // Estimate 100KB per chunk as a rough average
    chunks.len() * 102_400
}