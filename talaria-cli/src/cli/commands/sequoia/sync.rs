#![allow(dead_code)]

use anyhow::Result;
use clap::Args;
use colored::*;
use std::path::PathBuf;

#[derive(Args)]
pub struct SyncArgs {
    /// Cloud provider (s3, gcs, azure)
    #[arg(short = 'p', long, default_value = "s3")]
    pub provider: String,

    /// Cloud storage bucket/container name
    #[arg(short = 'b', long)]
    pub bucket: String,

    /// Region for cloud storage (required for S3)
    #[arg(short = 'r', long)]
    pub region: Option<String>,

    /// Prefix/path in cloud storage
    #[arg(long, default_value = "talaria-sequoia")]
    pub prefix: String,

    /// Sync direction (upload, download, bidirectional)
    #[arg(short = 'd', long, default_value = "bidirectional")]
    pub direction: String,

    /// Delete files that don't exist in source
    #[arg(long)]
    pub delete: bool,

    /// Perform a dry run without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Number of parallel transfers
    #[arg(long, default_value = "4")]
    pub parallel: usize,

    /// Bandwidth limit in MB/s
    #[arg(long)]
    pub bandwidth_limit: Option<usize>,

    /// Exclude patterns (can be specified multiple times)
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Include patterns (can be specified multiple times)
    #[arg(long)]
    pub include: Vec<String>,

    /// Local SEQUOIA repository path
    #[arg(short = 'l', long)]
    pub local_path: Option<PathBuf>,

    /// Custom S3 endpoint URL (for S3-compatible services)
    #[arg(long)]
    pub endpoint: Option<String>,

    /// Show sync status without performing sync
    #[arg(long)]
    pub status: bool,
}

pub fn run(args: SyncArgs) -> Result<()> {
    use talaria_sequoia::cloud::{CloudConfig, CloudSyncManager, SyncDirection, SyncOptions, create_storage};

    // Store verbose flag before moving args
    let verbose = args.verbose();

    // Determine local SEQUOIA path
    let local_path = if let Some(path) = args.local_path {
        path
    } else {
        use talaria_core::paths;
        paths::talaria_databases_dir()
    };

    if !local_path.exists() {
        anyhow::bail!("SEQUOIA repository not found at {}. Initialize it first with 'talaria sequoia init'", local_path.display());
    }

    // Create cloud configuration
    let config = match args.provider.to_lowercase().as_str() {
        "s3" => {
            let region = args.region.unwrap_or_else(|| {
                std::env::var("AWS_DEFAULT_REGION")
                    .unwrap_or_else(|_| "us-east-1".to_string())
            });

            CloudConfig::S3 {
                bucket: args.bucket.clone(),
                region,
                prefix: Some(args.prefix.clone()),
                endpoint: args.endpoint,
            }
        }
        "gcs" | "google" => {
            anyhow::bail!("Google Cloud Storage support not yet implemented");
        }
        "azure" => {
            anyhow::bail!("Azure Blob Storage support not yet implemented");
        }
        _ => {
            anyhow::bail!("Unsupported cloud provider: {}", args.provider);
        }
    };

    // Create cloud storage
    let storage = tokio::runtime::Runtime::new()?.block_on(async {
        create_storage(&config)
    })?;

    // Create sync manager
    let sync_manager = CloudSyncManager::new(
        storage,
        local_path.clone(),
        args.prefix.clone(),
    );

    // Show status if requested
    if args.status {
        let status = tokio::runtime::Runtime::new()?.block_on(async {
            sync_manager.get_status().await
        })?;

        println!("\n{}", "═".repeat(60));
        println!("{:^60}", "CLOUD SYNC STATUS");
        println!("{}", "═".repeat(60));
        println!();
        println!("{} {}", "Provider:".bold(), args.provider);
        println!("{} {}", "Bucket:".bold(), args.bucket);
        println!("{} {}", "Prefix:".bold(), args.prefix);
        println!("{} {}", "Local path:".bold(), local_path.display());
        println!();
        println!("{} {}", "Local chunks:".bold(), status.local_chunks);
        println!("{} {}", "Cloud chunks:".bold(), status.cloud_chunks);

        if let Some(last_sync) = status.last_sync {
            println!("{} {}", "Last sync:".bold(), last_sync.format("%Y-%m-%d %H:%M:%S UTC"));
        } else {
            println!("{} Never synced", "Last sync:".bold());
        }

        println!();
        return Ok(());
    }

    // Parse sync direction
    let direction = match args.direction.to_lowercase().as_str() {
        "upload" | "push" => SyncDirection::Upload,
        "download" | "pull" => SyncDirection::Download,
        "bidirectional" | "both" | "sync" => SyncDirection::Bidirectional,
        _ => {
            anyhow::bail!("Invalid sync direction: {}", args.direction);
        }
    };

    // Create sync options
    let options = SyncOptions {
        direction,
        delete_missing: args.delete,
        dry_run: args.dry_run,
        parallel_transfers: args.parallel,
        bandwidth_limit: args.bandwidth_limit.map(|mb| mb * 1024 * 1024),
        exclude_patterns: args.exclude.clone(),
        include_patterns: args.include.clone(),
    };

    // Perform sync
    println!("{} Starting sync...", "►".cyan().bold());

    if args.dry_run {
        println!("{} Running in dry-run mode (no changes will be made)", "[DRY RUN]".yellow().bold());
    }

    let result = tokio::runtime::Runtime::new()?.block_on(async {
        sync_manager.sync(&options).await
    })?;

    // Update last sync time if not dry run
    if !args.dry_run {
        sync_manager.update_last_sync_time()?;
    }

    // Display results
    println!("\n{}", "═".repeat(60));
    println!("{:^60}", "SYNC COMPLETE");
    println!("{}", "═".repeat(60));
    println!();

    if !result.uploaded.is_empty() {
        println!("{} Uploaded {} files", "▲".green().bold(), result.uploaded.len());
        if verbose {
            for file in &result.uploaded[..5.min(result.uploaded.len())] {
                println!("  • {}", file);
            }
            if result.uploaded.len() > 5 {
                println!("  ... and {} more", result.uploaded.len() - 5);
            }
        }
    }

    if !result.downloaded.is_empty() {
        println!("{} Downloaded {} files", "▼".blue().bold(), result.downloaded.len());
        if verbose {
            for file in &result.downloaded[..5.min(result.downloaded.len())] {
                println!("  • {}", file);
            }
            if result.downloaded.len() > 5 {
                println!("  ... and {} more", result.downloaded.len() - 5);
            }
        }
    }

    if !result.deleted.is_empty() {
        println!("{} Deleted {} files", "✗".red().bold(), result.deleted.len());
    }

    if !result.skipped.is_empty() {
        println!("{} Skipped {} files (unchanged)", "○".white().bold(), result.skipped.len());
    }

    if !result.errors.is_empty() {
        println!("{} {} errors occurred", "⚠".red().bold(), result.errors.len());
        for (file, error) in &result.errors[..5.min(result.errors.len())] {
            println!("  • {}: {}", file, error);
        }
        if result.errors.len() > 5 {
            println!("  ... and {} more errors", result.errors.len() - 5);
        }
    }

    println!();
    println!("{} {:.2} MB", "Data transferred:".bold(),
             result.bytes_transferred as f64 / 1_048_576.0);
    println!("{} {:.2} seconds", "Duration:".bold(),
             result.duration.as_secs_f64());

    if result.bytes_transferred > 0 {
        let mbps = (result.bytes_transferred as f64 / 1_048_576.0) / result.duration.as_secs_f64();
        println!("{} {:.2} MB/s", "Average speed:".bold(), mbps);
    }

    println!();

    if !result.errors.is_empty() {
        println!("{} Sync completed with {} errors", "⚠".yellow().bold(), result.errors.len());
    } else {
        println!("{} Sync completed successfully!", "✓".green().bold());
    }

    Ok(())
}

// Add verbose flag to Args struct
impl SyncArgs {
    pub fn verbose(&self) -> bool {
        std::env::var("TALARIA_VERBOSE").is_ok()
    }
}