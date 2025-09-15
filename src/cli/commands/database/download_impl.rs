/// Database download implementation using content-addressed storage

use crate::cli::commands::database::download::DownloadArgs;
use crate::core::database_manager::{DatabaseManager, DownloadResult};
use crate::download::DatabaseSource;
use crate::utils::progress::create_spinner;
use anyhow::Result;

pub fn run_database_download(args: DownloadArgs, database_source: DatabaseSource) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;

    // Show immediate feedback during initialization
    let spinner = create_spinner("Initializing content-addressed storage...");

    // Initialize database manager
    let mut manager = DatabaseManager::new(None)?;

    spinner.finish_and_clear();

    println!("► Using content-addressed storage for efficient database management");
    println!("   Provides deduplication, incremental updates, and cryptographic verification\n");

    // Check for resumable operations
    if args.resume {
        println!("● Checking for resumable operations...");
        let resumable_ops = manager.list_resumable_operations()?;

        if !resumable_ops.is_empty() {
            println!("Found {} resumable operation(s):", resumable_ops.len());
            for (op_id, state) in &resumable_ops {
                println!("  - {}: {}", op_id, state.summary());
            }
            println!();
        } else {
            println!("No resumable operations found.\n");
        }
    }

    // Progress callback
    let progress = |msg: &str| {
        println!("  {}", msg);
    };

    // Run the download
    let result = runtime.block_on(async {
        manager.download(&database_source, progress).await
    })?;

    // Report results
    match result {
        DownloadResult::UpToDate => {
            println!("\n✓ Database is already up to date!");
            println!("   No downloads needed - saved bandwidth and time");
        }
        DownloadResult::Updated { chunks_added, chunks_removed } => {
            println!("\n✓ Database updated successfully!");
            println!("   Added {} new chunks", chunks_added);
            if chunks_removed > 0 {
                println!("   Removed {} obsolete chunks", chunks_removed);
            }
            println!("   Only downloaded what changed - efficient!");
        }
        DownloadResult::InitialDownload => {
            println!("\n✓ Initial database setup complete!");
            println!("   Database has been chunked and stored");
            println!("   Future updates will only download changed chunks");
            println!("\n   [TIP] Set TALARIA_MANIFEST_SERVER environment variable");
            println!("      to enable incremental updates from a manifest server");
        }
    }

    // Show stats
    let stats = manager.get_stats()?;
    println!("\n● Repository Statistics:");
    println!("   Total chunks: {}", stats.total_chunks);
    println!("   Total size: {:.2} GB", stats.total_size as f64 / 1_073_741_824.0);
    println!("   Compressed chunks: {}", stats.compressed_chunks);
    println!("   Deduplication ratio: {:.2}x", stats.deduplication_ratio);
    println!("   Databases managed: {}", stats.database_count);

    Ok(())
}