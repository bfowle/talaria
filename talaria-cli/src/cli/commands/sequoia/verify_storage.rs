use clap::Args;
use colored::*;
use std::path::PathBuf;
use talaria_core::system::paths;
use talaria_sequoia::SequoiaStorage;

#[derive(Args)]
pub struct VerifyStorageArgs {
    /// Path to SEQUOIA repository
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Fix issues if found (rebuild indices)
    #[arg(long)]
    pub fix: bool,

    /// Show detailed information
    #[arg(short = 'd', long)]
    pub detailed: bool,
}

pub fn run(args: VerifyStorageArgs) -> anyhow::Result<()> {
    let base_path = if let Some(p) = args.path {
        p
    } else {
        paths::talaria_databases_dir()
    };

    println!(
        "{} Verifying SEQUOIA storage integrity...",
        "►".cyan().bold()
    );
    println!("  Base path: {}", base_path.display());
    println!();

    // Check canonical sequence storage
    let sequences_dir = paths::canonical_sequence_storage_dir();
    let packs_dir = paths::canonical_sequence_packs_dir();
    let _indices_dir = paths::canonical_sequence_indices_dir();
    let index_path = paths::canonical_sequence_index_path();

    println!("{}", "Canonical Sequence Storage:".bold());
    println!("  Directory: {}", sequences_dir.display());

    if !sequences_dir.exists() {
        println!(
            "  {} Sequence storage directory does not exist",
            "✗".red().bold()
        );
        if args.fix {
            std::fs::create_dir_all(&sequences_dir)?;
            println!(
                "  {} Created sequence storage directory",
                "✓".green().bold()
            );
        }
    } else {
        println!("  {} Sequence storage directory exists", "✓".green().bold());
    }

    // Check pack files
    println!("\n{}", "Pack Files:".bold());
    println!("  Directory: {}", packs_dir.display());

    let mut pack_count = 0;
    let mut total_pack_size = 0u64;

    if packs_dir.exists() {
        for entry in std::fs::read_dir(&packs_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("tal") {
                pack_count += 1;
                total_pack_size += entry.metadata()?.len();
                if args.detailed {
                    println!(
                        "  • {} ({:.2} MB)",
                        path.file_name().unwrap().to_string_lossy(),
                        entry.metadata()?.len() as f64 / 1_048_576.0
                    );
                }
            }
        }
        println!(
            "  {} Found {} pack files ({:.2} MB total)",
            "✓".green().bold(),
            pack_count,
            total_pack_size as f64 / 1_048_576.0
        );
    } else {
        println!("  {} Pack directory does not exist", "⚠".yellow().bold());
        if args.fix {
            std::fs::create_dir_all(&packs_dir)?;
            println!("  {} Created pack directory", "✓".green().bold());
        }
    }

    // Check sequence index
    println!("\n{}", "Sequence Index:".bold());
    println!("  File: {}", index_path.display());

    if !index_path.exists() {
        println!("  {} Index file does not exist", "✗".red().bold());
        if args.fix && pack_count > 0 {
            println!(
                "  {} Rebuilding index from pack files...",
                "►".cyan().bold()
            );
            rebuild_index(&base_path)?;
        }
    } else {
        let index_size = std::fs::metadata(&index_path)?.len();
        if index_size < 100 {
            println!(
                "  {} Index file appears empty ({} bytes)",
                "✗".red().bold(),
                index_size
            );
            if args.fix && pack_count > 0 {
                println!(
                    "  {} Rebuilding index from pack files...",
                    "►".cyan().bold()
                );
                rebuild_index(&base_path)?;
            }
        } else {
            println!(
                "  {} Index file exists ({:.2} KB)",
                "✓".green().bold(),
                index_size as f64 / 1024.0
            );

            // Verify index integrity
            if args.detailed {
                match SequoiaStorage::open(&base_path) {
                    Ok(storage) => {
                        let stats = storage.sequence_storage.get_stats()?;
                        println!(
                            "  Indexed sequences: {}",
                            stats.total_sequences.unwrap_or(0)
                        );
                    }
                    Err(e) => {
                        println!("  {} Could not open storage: {}", "✗".red().bold(), e);
                    }
                }
            }
        }
    }

    // Check chunk storage
    println!("\n{}", "Chunk Storage:".bold());
    let chunks_dir = base_path.join("chunks");
    if chunks_dir.exists() {
        let chunk_count = std::fs::read_dir(&chunks_dir)?
            .filter_map(|e| e.ok())
            .count();
        println!("  {} Found {} chunks", "✓".green().bold(), chunk_count);
    } else {
        println!("  {} Chunk directory does not exist", "⚠".yellow().bold());
    }

    // Summary
    println!("\n{}", "═".repeat(60));
    if args.fix {
        println!(
            "{} Storage verification complete. Issues were fixed.",
            "✓".green().bold()
        );
    } else {
        println!("{} Storage verification complete.", "✓".green().bold());
        println!("  Run with --fix to rebuild indices if needed.");
    }

    Ok(())
}

fn rebuild_index(base_path: &PathBuf) -> anyhow::Result<()> {
    let storage = SequoiaStorage::open(base_path)?;
    storage.rebuild_sequence_index()?;
    println!("  {} Index rebuilt successfully", "✓".green().bold());
    Ok(())
}
