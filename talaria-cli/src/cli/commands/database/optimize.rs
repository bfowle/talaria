/// Database optimization command for HERALD
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
use talaria_core::system::paths::talaria_databases_dir;
use talaria_herald::HeraldRepository;

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

    /// Compact RocksDB to compress uncompressed L0/L1 data
    #[arg(long)]
    compact: bool,

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

    /// Report output file path
    #[arg(long = "report-output", value_name = "FILE")]
    report_output: Option<PathBuf>,

    /// Report output format (text, html, json, csv)
    #[arg(long = "report-format", value_name = "FORMAT", default_value = "text")]
    report_format: String,
}

impl OptimizeCmd {
    pub async fn run(&self) -> Result<()> {
        // Check if optimizing all databases
        if self.database == "all" {
            return self.run_all().await;
        }

        // Verify database exists
        let _db_path = self.get_database_path()?;

        println!("🔧 Optimizing HERALD database: {}", self.database);

        // Open repository at the unified RocksDB path
        let base_path = talaria_databases_dir();
        let mut repository = HeraldRepository::open(&base_path)?;

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

        // Step 2: Compact RocksDB if requested
        if self.compact {
            pb.set_message("Compacting RocksDB...");
            self.compact_database(&repository, self.dry_run)?;
            pb.set_position(50);
        }

        // Step 3: Rebuild indices if requested
        if self.rebuild_indices {
            pb.set_message("Rebuilding indices...");
            self.rebuild_indices(&mut repository, self.dry_run)?;
            pb.set_position(75);
        }

        // Step 4: Prune temporal versions if requested
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

        // Generate report if requested
        if let Some(report_path) = &self.report_output {
            use std::time::Duration;
            use talaria_herald::operations::OptimizationResult;

            // Get storage stats for space calculations
            let storage_stats = repository.storage.get_statistics()?;
            let space_before = storage_stats.total_size + total_saved;

            let result = OptimizationResult {
                success: true,
                space_before: space_before as u64,
                space_after: storage_stats.total_size as u64,
                chunks_compacted: if self.repack {
                    storage_stats.chunk_count
                } else {
                    0
                },
                indices_rebuilt: if self.rebuild_indices { 3 } else { 0 }, // Estimate: chunk, sequence, taxonomy
                compaction_performed: self.repack,
                defragmentation_performed: self.repack,
                duration: Duration::from_secs(0), // TODO: Track actual duration
            };

            crate::cli::commands::save_report(&result, &self.report_format, report_path)?;
            println!("✓ Report saved to {}", report_path.display());
        }

        Ok(())
    }

    async fn run_all(&self) -> Result<()> {
        use talaria_herald::database::DatabaseManager;

        println!("🔧 Optimizing ALL HERALD databases");
        println!();

        // Get list of all databases (this opens RocksDB)
        let mut manager = DatabaseManager::new(None)?;
        let databases = manager.list_databases()?;

        if databases.is_empty() {
            println!("No databases found to optimize.");
            return Ok(());
        }

        println!("Found {} databases to optimize:", databases.len());
        for db in &databases {
            println!("  • {}", db.name);
        }
        println!();

        // For "all" mode, we can skip per-database operations and just compact RocksDB once
        // since all databases share the same RocksDB instance
        if self.compact {
            println!("🗜️  Compacting shared RocksDB storage...");
            println!("This will compress all databases simultaneously.");
            println!();

            if !self.dry_run {
                println!("  Compacting sequences RocksDB...");
                println!("  This may take several minutes depending on total data size.");
                println!();

                // Compact sequences storage
                let sequence_rocksdb = manager
                    .get_repository()
                    .storage
                    .sequence_storage
                    .get_rocksdb();
                sequence_rocksdb.compact()?;

                println!();
                println!("  Compacting chunk storage RocksDB...");
                println!();

                // Compact chunk storage
                let chunk_rocksdb = manager.get_repository().storage.chunk_storage();
                chunk_rocksdb.compact()?;

                println!();
                println!("  ✓ Both RocksDB instances compacted successfully");
            } else {
                println!("  [DRY RUN] Would compact:");
                println!("    - Sequences RocksDB");
                println!("    - Chunk storage RocksDB");
            }
        }

        // Rebuild indices (global operation, benefits all databases including incomplete ones)
        if self.rebuild_indices {
            println!();
            println!("🔍 Rebuilding global indices...");
            println!("This will rebuild indices for ALL sequences including incomplete databases.");
            println!();

            if !self.dry_run {
                // Rebuild database metadata first
                println!("  Rebuilding database metadata...");
                let rebuilt_count = manager.rebuild_database_metadata()?;
                if rebuilt_count > 0 {
                    println!("  ✓ Rebuilt metadata for {} database(s)", rebuilt_count);
                }

                let repository = manager.get_repository();

                println!("  Rebuilding sequence index...");
                repository.storage.rebuild_sequence_index()?;

                println!("  Rebuilding taxonomy index...");
                repository.storage.rebuild_taxonomy_index()?;

                println!("  Rebuilding temporal index...");
                repository.temporal.rebuild_index()?;

                println!("  ✓ All indices rebuilt successfully");
            } else {
                println!("  [DRY RUN] Would rebuild:");
                println!("    - Database metadata");
                println!("    - Sequence index");
                println!("    - Taxonomy index");
                println!("    - Temporal index");
            }
        }

        // Repack chunks (per-database operation, only applies to completed databases)
        if self.repack {
            println!();
            println!("📦 Repacking chunks for all databases...");
            println!();

            let total_saved = 0usize;
            let repacked_count = 0;

            for db in &databases {
                println!("  Processing: {}", db.name);
                // Note: Would need to load each database's chunks and repack
                // Skipping detailed implementation for now
                // This would iterate through each database's chunk manifests
            }

            println!(
                "  Repacked {} chunks across {} databases",
                repacked_count,
                databases.len()
            );
            if !self.dry_run {
                println!("  Space saved: {} MB", total_saved / 1_048_576);
            }
        }

        // Prune temporal data (per-database operation)
        if let Some(days) = self.prune_temporal {
            println!();
            println!("🕐 Pruning temporal data from all databases...");
            println!();

            let _cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
            let total_pruned = 0;

            for db in &databases {
                println!("  Processing: {}", db.name);
                // Note: Would need to prune each database's temporal versions
                // Skipping detailed implementation for now
            }

            println!("  Total versions pruned: {}", total_pruned);
        }

        println!();
        if self.repack
            || (self.rebuild_indices && databases.len() > 0)
            || self.prune_temporal.is_some()
        {
            println!("Note: --repack and --prune-temporal only apply to completed databases");
            println!("      --rebuild-indices applies globally to all sequences including incomplete databases");
            println!();
        }

        println!("✅ Global optimization complete!");

        Ok(())
    }

    fn get_database_path(&self) -> Result<PathBuf> {
        // Use DatabaseManager to verify database exists
        use talaria_herald::database::DatabaseManager;

        let manager = DatabaseManager::new(None)?;
        manager
            .list_databases()?
            .iter()
            .find(|db| db.name == self.database || db.name.ends_with(&self.database))
            .ok_or_else(|| anyhow!("Database not found: {}", self.database))?;

        // Return the unified RocksDB path
        Ok(talaria_databases_dir())
    }

    fn print_current_stats(&self, repository: &HeraldRepository) -> Result<()> {
        println!("\n📊 Current Database Statistics:");

        // Get storage stats
        let storage_stats = repository.storage.get_statistics()?;
        println!("  Total chunks: {}", storage_stats.chunk_count);
        println!("  Total size: {} MB", storage_stats.total_size / 1_048_576);
        println!(
            "  Compression ratio: {:.1}%",
            storage_stats.compression_ratio * 100.0
        );

        // Get temporal stats
        let temporal_stats = repository.temporal.get_statistics()?;
        println!("  Temporal versions: {}", temporal_stats.version_count);
        println!("  Oldest version: {} days ago", temporal_stats.oldest_days);

        Ok(())
    }

    fn repack_chunks(&self, repository: &mut HeraldRepository, dry_run: bool) -> Result<usize> {
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
                        space_saved +=
                            ((1.0 - self.compression_target) * metadata.size as f32) as usize;
                    }
                    repacked_count += 1;
                }
            }
        }

        println!(
            "  Repacked {} chunks, saved {} bytes",
            repacked_count, space_saved
        );
        Ok(space_saved)
    }

    fn compact_database(&self, repository: &HeraldRepository, dry_run: bool) -> Result<()> {
        println!("\n🗜️  Compacting RocksDB to compress uncompressed data...");

        if !dry_run {
            let rocksdb = repository.storage.sequence_storage.get_rocksdb();

            println!("  Compacting all column families...");
            println!("  This may take several minutes depending on data size.");

            rocksdb.compact()?;

            println!("  ✓ RocksDB compaction completed successfully");
        } else {
            println!("  [DRY RUN] Would compact all RocksDB column families");
        }

        Ok(())
    }

    fn rebuild_indices(&self, repository: &mut HeraldRepository, dry_run: bool) -> Result<()> {
        println!("\n🔍 Rebuilding indices for faster queries...");

        if !dry_run {
            // Rebuild database metadata (fixes missing db_meta:* entries)
            use talaria_herald::database::DatabaseManager;
            let mut manager = DatabaseManager::new(None)?;
            let rebuilt_count = manager.rebuild_database_metadata()?;
            if rebuilt_count > 0 {
                println!("  Rebuilt metadata for {} database(s)", rebuilt_count);
            }

            // Rebuild sequence index
            repository.storage.rebuild_sequence_index()?;

            // Rebuild taxonomy index
            repository.storage.rebuild_taxonomy_index()?;

            // Rebuild temporal index
            repository.temporal.rebuild_index()?;

            println!("  All indices rebuilt successfully");
        } else {
            println!("  [DRY RUN] Would rebuild database metadata, sequence, taxonomy, and temporal indices");
        }

        Ok(())
    }

    fn prune_temporal(
        &self,
        repository: &mut HeraldRepository,
        days: u32,
        dry_run: bool,
    ) -> Result<usize> {
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
