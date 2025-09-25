/// Garbage collection for SEQUOIA databases
///
/// Removes unreferenced data:
/// - Orphaned chunks
/// - Unreferenced canonical sequences
/// - Expired temporal data
/// - Invalid cache entries

use anyhow::{anyhow, Result};
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashSet;
use std::path::PathBuf;
use talaria_core::system::paths::talaria_home;
use talaria_sequoia::SEQUOIARepository;

#[derive(Debug, Args)]
pub struct GcCmd {
    /// Database reference (e.g., "uniprot/swissprot") or "all"
    #[arg(value_name = "DATABASE")]
    database: String,

    /// Remove orphaned chunks
    #[arg(long, default_value = "true")]
    orphaned_chunks: bool,

    /// Remove unreferenced sequences
    #[arg(long, default_value = "true")]
    unreferenced_sequences: bool,

    /// Remove expired cache entries
    #[arg(long, default_value = "true")]
    expired_cache: bool,

    /// Remove incomplete downloads
    #[arg(long, default_value = "true")]
    incomplete_downloads: bool,

    /// Aggressive mode - also removes data that might be recoverable
    #[arg(long)]
    aggressive: bool,

    /// Dry run - show what would be removed without actually removing
    #[arg(long)]
    dry_run: bool,

    /// Show detailed statistics before and after
    #[arg(long)]
    stats: bool,
}

impl GcCmd {
    pub async fn run(&self) -> Result<()> {
        println!("🗑️  Running garbage collection for: {}", self.database);

        if self.database == "all" {
            self.gc_all_databases().await
        } else {
            self.gc_single_database().await
        }
    }

    async fn gc_single_database(&self) -> Result<()> {
        let base_path = self.get_database_path()?;

        // Open repository
        let mut repository = SEQUOIARepository::open(&base_path)?;

        // Show initial stats if requested
        let initial_size = if self.stats {
            self.get_total_size(&repository)?
        } else {
            0
        };

        let pb = ProgressBar::new(100);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{bar:40.cyan/blue}] {pos}% {msg}")
                .unwrap()
                .progress_chars("━━╸"),
        );

        let mut total_removed = 0usize;

        // Phase 1: Find orphaned chunks
        if self.orphaned_chunks {
            pb.set_message("Scanning for orphaned chunks...");
            let removed = self.remove_orphaned_chunks(&mut repository)?;
            total_removed += removed;
            pb.set_position(25);
        }

        // Phase 2: Find unreferenced sequences
        if self.unreferenced_sequences {
            pb.set_message("Scanning for unreferenced sequences...");
            let removed = self.remove_unreferenced_sequences(&mut repository)?;
            total_removed += removed;
            pb.set_position(50);
        }

        // Phase 3: Clean expired cache
        if self.expired_cache {
            pb.set_message("Cleaning expired cache entries...");
            let removed = self.clean_expired_cache(&repository)?;
            total_removed += removed;
            pb.set_position(75);
        }

        // Phase 4: Clean incomplete downloads
        if self.incomplete_downloads {
            pb.set_message("Removing incomplete downloads...");
            let removed = self.clean_incomplete_downloads(&repository)?;
            total_removed += removed;
            pb.set_position(100);
        }

        pb.finish_with_message("Garbage collection complete");

        // Report results
        println!("\n✅ Garbage Collection Results:");
        println!("  Total space freed: {} MB", total_removed / 1_048_576);

        if self.stats && initial_size > 0 {
            let final_size = self.get_total_size(&repository)?;
            let reduction = ((initial_size - final_size) as f64 / initial_size as f64) * 100.0;
            println!("  Size reduction: {:.1}%", reduction);
            println!("  Before: {} MB", initial_size / 1_048_576);
            println!("  After: {} MB", final_size / 1_048_576);
        }

        if self.dry_run {
            println!("\n⚠️  This was a dry run. No data was actually removed.");
        }

        Ok(())
    }

    async fn gc_all_databases(&self) -> Result<()> {
        println!("Running garbage collection on ALL databases...");

        let db_path = talaria_home().join("databases").join("sequences");

        // Find all databases
        let mut databases = Vec::new();
        for source_entry in std::fs::read_dir(&db_path)? {
            let source_entry = source_entry?;
            if source_entry.file_type()?.is_dir() {
                let source_name = source_entry.file_name().to_string_lossy().to_string();

                for dataset_entry in std::fs::read_dir(source_entry.path())? {
                    let dataset_entry = dataset_entry?;
                    if dataset_entry.file_type()?.is_dir() {
                        let dataset_name = dataset_entry.file_name().to_string_lossy().to_string();
                        databases.push(format!("{}/{}", source_name, dataset_name));
                    }
                }
            }
        }

        let total_freed = 0usize;

        for database in databases {
            println!("\n📦 Processing: {}", database);

            // Create a new instance with the specific database
            let mut cmd = self.clone();
            cmd.database = database;

            match cmd.gc_single_database().await {
                Ok(()) => {
                    // Success
                },
                Err(e) => {
                    eprintln!("  ⚠️  Error processing {}: {}", cmd.database, e);
                }
            }
        }

        println!("\n✅ All databases processed");
        println!("  Total space freed: {} MB", total_freed / 1_048_576);

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

    fn remove_orphaned_chunks(&self, repository: &mut SEQUOIARepository) -> Result<usize> {
        println!("\n🔍 Finding orphaned chunks...");

        // Get all referenced chunks from manifests
        let mut referenced_chunks = HashSet::new();

        // Add chunks from current manifest
        let manifest_chunks = repository.manifest.get_chunks();
        for chunk in &manifest_chunks {
            referenced_chunks.insert(chunk.hash.clone());
        }

        // Add chunks from temporal versions
        let temporal_manifests = repository.temporal.list_all_manifests()?;
        for manifest in temporal_manifests {
            for chunk in manifest.chunks {
                referenced_chunks.insert(chunk);
            }
        }

        // Get all stored chunks
        let stored_chunks = repository.storage.list_chunks()?;

        // Find orphans
        let mut orphans = Vec::new();
        for chunk_hash in &stored_chunks {
            if !referenced_chunks.contains(chunk_hash) {
                orphans.push(chunk_hash.clone());
            }
        }

        let mut total_removed = 0usize;

        if !orphans.is_empty() {
            println!("  Found {} orphaned chunks", orphans.len());

            if !self.dry_run {
                for orphan in &orphans {
                    let size = repository.storage.get_chunk_size(orphan)?;
                    repository.storage.remove_chunk(orphan)?;
                    total_removed += size;
                }
            } else {
                // Estimate size
                for orphan in &orphans {
                    let size = repository.storage.get_chunk_size(orphan)?;
                    total_removed += size;
                }
                println!("  [DRY RUN] Would remove {} bytes", total_removed);
            }
        } else {
            println!("  No orphaned chunks found");
        }

        Ok(total_removed)
    }

    fn remove_unreferenced_sequences(&self, repository: &mut SEQUOIARepository) -> Result<usize> {
        println!("\n🔍 Finding unreferenced sequences...");

        // Get all referenced sequences from chunks
        let mut referenced_sequences = HashSet::new();
        let chunks = repository.storage.list_chunks()?;

        for chunk_hash in &chunks {
            let chunk = repository.storage.load_chunk(chunk_hash)?;
            for seq_ref in &chunk.sequence_refs {
                referenced_sequences.insert(seq_ref.clone());
            }
        }

        // Get all stored sequences
        let stored_sequences = repository.storage.sequence_storage.list_all_hashes()?;

        // Find unreferenced
        let mut unreferenced = Vec::new();
        for seq_hash in &stored_sequences {
            if !referenced_sequences.contains(seq_hash) {
                unreferenced.push(seq_hash.clone());
            }
        }

        let mut total_removed = 0usize;

        if !unreferenced.is_empty() {
            println!("  Found {} unreferenced sequences", unreferenced.len());

            if !self.dry_run {
                for seq_hash in &unreferenced {
                    let size = repository.storage.sequence_storage.get_size(seq_hash)?;
                    repository.storage.sequence_storage.remove(seq_hash)?;
                    total_removed += size;
                }
            } else {
                // Estimate size
                for seq_hash in &unreferenced {
                    let size = repository.storage.sequence_storage.get_size(seq_hash)?;
                    total_removed += size;
                }
                println!("  [DRY RUN] Would remove {} bytes", total_removed);
            }
        } else {
            println!("  No unreferenced sequences found");
        }

        Ok(total_removed)
    }

    fn clean_expired_cache(&self, repository: &SEQUOIARepository) -> Result<usize> {
        println!("\n🔍 Cleaning expired cache entries...");

        let cache_dir = repository.storage.base_path.join("cache");
        if !cache_dir.exists() {
            println!("  No cache directory found");
            return Ok(0);
        }

        let mut total_removed = 0usize;
        let now = std::time::SystemTime::now();
        let max_age = std::time::Duration::from_secs(30 * 24 * 60 * 60); // 30 days

        for entry in std::fs::read_dir(&cache_dir)? {
            let entry = entry?;
            let metadata = entry.metadata()?;

            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = now.duration_since(modified) {
                    if age > max_age {
                        let size = metadata.len() as usize;

                        if !self.dry_run {
                            std::fs::remove_file(entry.path())?;
                        }

                        total_removed += size;
                    }
                }
            }
        }

        if total_removed > 0 {
            println!("  Removed {} bytes of expired cache", total_removed);
        } else {
            println!("  No expired cache entries found");
        }

        Ok(total_removed)
    }

    fn clean_incomplete_downloads(&self, repository: &SEQUOIARepository) -> Result<usize> {
        println!("\n🔍 Cleaning incomplete downloads...");

        let downloads_dir = repository.storage.base_path.join("downloads");
        if !downloads_dir.exists() {
            println!("  No downloads directory found");
            return Ok(0);
        }

        let mut total_removed = 0usize;

        for entry in std::fs::read_dir(&downloads_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Check for .partial or .tmp files
            if let Some(ext) = path.extension() {
                if ext == "partial" || ext == "tmp" {
                    let metadata = entry.metadata()?;
                    let size = metadata.len() as usize;

                    if !self.dry_run {
                        if path.is_dir() {
                            std::fs::remove_dir_all(&path)?;
                        } else {
                            std::fs::remove_file(&path)?;
                        }
                    }

                    total_removed += size;
                }
            }
        }

        if total_removed > 0 {
            println!("  Removed {} bytes of incomplete downloads", total_removed);
        } else {
            println!("  No incomplete downloads found");
        }

        Ok(total_removed)
    }

    fn get_total_size(&self, repository: &SEQUOIARepository) -> Result<usize> {
        let stats = repository.storage.get_statistics()?;
        Ok(stats.total_size)
    }
}

impl Clone for GcCmd {
    fn clone(&self) -> Self {
        Self {
            database: self.database.clone(),
            orphaned_chunks: self.orphaned_chunks,
            unreferenced_sequences: self.unreferenced_sequences,
            expired_cache: self.expired_cache,
            incomplete_downloads: self.incomplete_downloads,
            aggressive: self.aggressive,
            dry_run: self.dry_run,
            stats: self.stats,
        }
    }
}