/// Database optimization command for SEQUOIA
///
/// Optimizes storage by:
/// - Repacking chunks for better compression
/// - Rebuilding indices for faster queries
/// - Removing obsolete temporal versions
/// - Consolidating small chunks

use anyhow::{anyhow, Result};
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use talaria_core::system::paths::talaria_home;
use talaria_sequoia::SEQUOIARepository;

#[derive(Debug, Args)]
pub struct OptimizeCmd {
    /// Database reference (e.g., "uniprot/swissprot")
    #[arg(value_name = "DATABASE")]
    database: String,

    /// Repack chunks for better compression
    #[arg(long)]
    repack: bool,

    /// Rebuild all indices
    #[arg(long)]
    rebuild_indices: bool,

    /// Remove temporal versions older than N days
    #[arg(long, value_name = "DAYS")]
    prune_temporal: Option<u32>,

    /// Target compression ratio (0.1-1.0)
    #[arg(long, default_value = "0.5")]
    compression_target: f32,

    /// Dry run - show what would be done without making changes
    #[arg(long)]
    dry_run: bool,

    /// Show detailed statistics
    #[arg(long)]
    stats: bool,
}

impl OptimizeCmd {
    pub async fn run(&self) -> Result<()> {
        let base_path = self.get_database_path()?;

        println!("🔧 Optimizing SEQUOIA database: {}", self.database);

        // Open repository
        let mut repository = SEQUOIARepository::open(&base_path)?;

        // Get current statistics
        if self.stats {
            self.print_current_stats(&repository)?;
        }

        // Options for optimization
        let _repack = self.repack;
        let _rebuild_indices = self.rebuild_indices;
        let _prune_temporal = self.prune_temporal;
        let _dry_run = self.dry_run;

        // Run optimization with progress tracking
        let pb = ProgressBar::new(100);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{bar:40.cyan/blue}] {pos}% {msg}")
                .unwrap()
                .progress_chars("━━╸"),
        );

        let mut total_saved = 0usize;

        // Step 1: Repack chunks if requested
        if self.repack {
            pb.set_message("Repacking chunks...");
            let saved = self.repack_chunks(&mut repository, self.dry_run)?;
            total_saved += saved;
            pb.set_position(33);
        }

        // Step 2: Rebuild indices if requested
        if self.rebuild_indices {
            pb.set_message("Rebuilding indices...");
            self.rebuild_indices(&mut repository, self.dry_run)?;
            pb.set_position(66);
        }

        // Step 3: Prune temporal versions if requested
        if let Some(days) = self.prune_temporal {
            pb.set_message("Pruning old temporal versions...");
            let pruned = self.prune_temporal(&mut repository, days, self.dry_run)?;
            println!("  Pruned {} old versions", pruned);
            pb.set_position(100);
        }

        pb.finish_with_message("Optimization complete");

        // Report results
        println!("\n✅ Optimization Results:");
        println!("  Space saved: {} MB", total_saved / 1_048_576);

        if self.dry_run {
            println!("\n⚠️  This was a dry run. No changes were made.");
        }

        Ok(())
    }

    fn get_database_path(&self) -> Result<PathBuf> {
        let parts: Vec<&str> = self.database.split('/').collect();
        if parts.len() != 2 {
            return Err(anyhow!("Invalid database reference: {}", self.database));
        }

        let path = talaria_home()
            .join("databases")
            .join("sequences")
            .join(parts[0])
            .join(parts[1])
            .join("current");

        if !path.exists() {
            return Err(anyhow!("Database not found: {}", self.database));
        }

        Ok(path)
    }

    fn print_current_stats(&self, repository: &SEQUOIARepository) -> Result<()> {
        println!("\n📊 Current Database Statistics:");

        // Get storage stats
        let storage_stats = repository.storage.get_statistics()?;
        println!("  Total chunks: {}", storage_stats.chunk_count);
        println!("  Total size: {} MB", storage_stats.total_size / 1_048_576);
        println!("  Compression ratio: {:.1}%", storage_stats.compression_ratio * 100.0);

        // Get temporal stats
        let temporal_stats = repository.temporal.get_statistics()?;
        println!("  Temporal versions: {}", temporal_stats.version_count);
        println!("  Oldest version: {} days ago", temporal_stats.oldest_days);

        Ok(())
    }

    fn repack_chunks(&self, repository: &mut SEQUOIARepository, dry_run: bool) -> Result<usize> {
        println!("\n📦 Repacking chunks for better compression...");

        // Get all chunks
        let chunks = repository.storage.list_chunks()?;
        let mut space_saved = 0usize;
        let mut repacked_count = 0;

        for chunk_hash in &chunks {
            // Check if chunk needs repacking
            let metadata = repository.storage.get_chunk_metadata(chunk_hash)?;

            // Repack if compression is poor
            if let Some(ratio) = metadata.compression_ratio {
                if ratio < self.compression_target {
                    if !dry_run {
                        let saved = repository.storage.repack_chunk(chunk_hash)?;
                        space_saved += saved;
                    } else {
                        // Estimate savings
                        space_saved += ((1.0 - self.compression_target) * metadata.size as f32) as usize;
                    }
                    repacked_count += 1;
                }
            }
        }

        println!("  Repacked {} chunks, saved {} bytes", repacked_count, space_saved);
        Ok(space_saved)
    }

    fn rebuild_indices(&self, repository: &mut SEQUOIARepository, dry_run: bool) -> Result<()> {
        println!("\n🔍 Rebuilding indices for faster queries...");

        if !dry_run {
            // Rebuild sequence index
            repository.storage.rebuild_sequence_index()?;

            // Rebuild taxonomy index
            repository.storage.rebuild_taxonomy_index()?;

            // Rebuild temporal index
            repository.temporal.rebuild_index()?;

            println!("  All indices rebuilt successfully");
        } else {
            println!("  [DRY RUN] Would rebuild sequence, taxonomy, and temporal indices");
        }

        Ok(())
    }

    fn prune_temporal(&self, repository: &mut SEQUOIARepository, days: u32, dry_run: bool) -> Result<usize> {
        println!("\n🕐 Pruning temporal versions older than {} days...", days);

        let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);

        if !dry_run {
            let pruned = repository.temporal.prune_before(cutoff)?;
            println!("  Pruned {} temporal versions", pruned);
            Ok(pruned)
        } else {
            // Count versions that would be pruned
            let versions = repository.temporal.list_versions_before(cutoff)?;
            println!("  [DRY RUN] Would prune {} versions", versions.len());
            Ok(versions.len())
        }
    }
}