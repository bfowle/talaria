/// Clean SEQUOIA databases by removing unreferenced data
///
/// Removes unreferenced data:
/// - Orphaned chunks
/// - Unreferenced canonical sequences
/// - Expired temporal data
/// - Invalid cache entries
/// - Incomplete downloads
/// - Old download workspaces
use anyhow::{anyhow, Result};
use clap::Args;
use std::collections::HashSet;
use talaria_sequoia::database::DatabaseManager;
use talaria_sequoia::SequoiaRepository;

/// Results from cleaning operations by category
#[derive(Debug, Default)]
struct CleanResults {
    orphaned_chunks: (usize, usize),      // (count, bytes)
    unreferenced_seqs: (usize, usize),    // (count, bytes)
    expired_cache: (usize, usize),        // (count, bytes)
    incomplete_downloads: (usize, usize), // (count, bytes)
}

#[derive(Debug, Args)]
pub struct CleanCmd {
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

    /// Check for unreferenced sequences (slow with large databases)
    #[arg(long)]
    check_sequences: bool,

    /// Report output file path
    #[arg(long = "report-output", value_name = "FILE")]
    report_output: Option<std::path::PathBuf>,

    /// Report output format (text, html, json, csv)
    #[arg(long = "report-format", value_name = "FORMAT", default_value = "text")]
    report_format: String,
}

impl CleanCmd {
    pub async fn run(&self) -> Result<()> {
        if self.database == "all" {
            self.clean_all_databases().await
        } else {
            self.clean_single_database().await
        }
    }

    async fn clean_single_database(&self) -> Result<()> {
        use crate::cli::formatting::output::*;

        // SAFETY CHECK: Per-database clean is not safe with unified storage
        if self.database != "all" {
            return Err(anyhow!(
                "Per-database cleaning is not supported.\n\
                 \n\
                 SEQUOIA uses unified storage where chunks and sequences are shared across\n\
                 all databases. To safely remove unreferenced data, use:\n\
                 \n\
                 \x1b[1m    talaria database clean all\x1b[0m\n\
                 \n\
                 This will scan ALL databases to ensure nothing in use is deleted.\n\
                 \n\
                 \x1b[33mNote:\x1b[0m Running clean on a single database would delete shared data still\n\
                 referenced by other databases, causing data loss."
            ));
        }

        // Use DatabaseManager to access the unified repository
        let mut manager = DatabaseManager::new(None)?;

        // Get mutable access to repository for clean operations
        let repository = manager.get_repository_mut();

        section_header_with_line(&format!("Database Cleaning: {}", self.database));

        // Show initial spinner
        use crate::cli::progress::create_spinner;
        use std::time::Instant;
        let init_spinner = create_spinner("Initializing database cleaning...");

        let mut results = CleanResults::default();
        let start_time = Instant::now();

        init_spinner.finish_and_clear();

        // Phase 1: Find orphaned chunks
        if self.orphaned_chunks {
            results.orphaned_chunks = self.remove_orphaned_chunks(repository)?;
        }

        // Phase 2: Find unreferenced sequences (optional - very slow)
        if self.unreferenced_sequences && self.check_sequences {
            use crate::cli::formatting::output::*;
            warning(
                "Checking unreferenced sequences - this may take a long time with large databases",
            );
            results.unreferenced_seqs = self.remove_unreferenced_sequences(repository)?;
        } else if self.unreferenced_sequences && !self.check_sequences {
            use crate::cli::formatting::output::*;
            info("Skipped unreferenced sequence check (use --check-sequences to enable)");
        }

        // Phase 3: Clean expired cache
        if self.expired_cache {
            results.expired_cache = self.clean_expired_cache(repository)?;
        }

        // Phase 4: Clean incomplete downloads
        if self.incomplete_downloads {
            results.incomplete_downloads = self.clean_incomplete_downloads(repository)?;
        }

        let elapsed = start_time.elapsed();

        // Display summary table
        self.display_results(&results, elapsed)?;

        if self.dry_run {
            println!();
            warning("This was a dry run. No data was actually removed.");
        }

        // Generate report if requested
        if let Some(report_path) = &self.report_output {
            use talaria_sequoia::operations::GarbageCollectionResult;

            let total_removed = results.orphaned_chunks.0
                + results.unreferenced_seqs.0
                + results.expired_cache.0
                + results.incomplete_downloads.0;

            let total_bytes = results.orphaned_chunks.1
                + results.unreferenced_seqs.1
                + results.expired_cache.1
                + results.incomplete_downloads.1;

            let result = GarbageCollectionResult {
                chunks_removed: total_removed,
                space_reclaimed: total_bytes as u64,
                orphaned_chunks: Vec::new(), // Would need to collect actual hashes
                compaction_performed: false,
                duration: start_time.elapsed(),
            };

            crate::cli::commands::save_report(&result, &self.report_format, report_path)?;
            println!("✓ Report saved to {}", report_path.display());
        }

        Ok(())
    }

    async fn clean_all_databases(&self) -> Result<()> {
        use crate::cli::formatting::output::*;
        use std::time::Instant;
        use talaria_sequoia::database::DatabaseManager;

        section_header_with_line("Database Cleaning: All Databases");

        // Show initial spinner
        use crate::cli::progress::create_spinner;
        let init_spinner = create_spinner("Scanning all databases for references...");

        // Use DatabaseManager to access unified repository
        let mut manager = DatabaseManager::new(None)?;
        let databases = manager.list_databases()?;

        info(&format!(
            "Found {} databases in unified storage",
            databases.len()
        ));

        // Get mutable access to repository for clean operations
        let repository = manager.get_repository_mut();

        let mut results = CleanResults::default();
        let start_time = Instant::now();

        init_spinner.finish_and_clear();

        // Phase 1: Find orphaned chunks (scans ALL databases)
        if self.orphaned_chunks {
            results.orphaned_chunks =
                self.remove_orphaned_chunks_all_databases(repository, &databases)?;
        }

        // Phase 2: Find unreferenced sequences (optional - very slow)
        if self.unreferenced_sequences && self.check_sequences {
            warning(
                "Checking unreferenced sequences - this may take a long time with large databases",
            );
            results.unreferenced_seqs =
                self.remove_unreferenced_sequences_all_databases(repository, &databases)?;
        } else if self.unreferenced_sequences && !self.check_sequences {
            info("Skipped unreferenced sequence check (use --check-sequences to enable)");
        }

        // Phase 3: Clean expired cache
        if self.expired_cache {
            results.expired_cache = self.clean_expired_cache(repository)?;
        }

        // Phase 4: Clean incomplete downloads
        if self.incomplete_downloads {
            results.incomplete_downloads = self.clean_incomplete_downloads(repository)?;
        }

        let elapsed = start_time.elapsed();

        // Display summary table
        self.display_results(&results, elapsed)?;

        if self.dry_run {
            println!();
            warning("This was a dry run. No data was actually removed.");
        }

        // Generate report if requested
        if let Some(report_path) = &self.report_output {
            use talaria_sequoia::operations::GarbageCollectionResult;

            let total_removed = results.orphaned_chunks.0
                + results.unreferenced_seqs.0
                + results.expired_cache.0
                + results.incomplete_downloads.0;

            let total_bytes = results.orphaned_chunks.1
                + results.unreferenced_seqs.1
                + results.expired_cache.1
                + results.incomplete_downloads.1;

            let result = GarbageCollectionResult {
                chunks_removed: total_removed,
                space_reclaimed: total_bytes as u64,
                orphaned_chunks: Vec::new(),
                compaction_performed: false,
                duration: start_time.elapsed(),
            };

            crate::cli::commands::save_report(&result, &self.report_format, report_path)?;
            success(&format!("Report saved to {}", report_path.display()));
        }

        Ok(())
    }

    fn remove_orphaned_chunks(&self, repository: &mut SequoiaRepository) -> Result<(usize, usize)> {
        use crate::cli::formatting::output::*;
        use indicatif::{ProgressBar, ProgressStyle};
        use std::time::Instant;

        let start = Instant::now();
        subsection_header("Finding Orphaned Chunks");

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
        let total_chunks = stored_chunks.len();

        // Create progress bar
        let pb = ProgressBar::new(total_chunks as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("  Scanning chunks [{bar:40}] {pos}/{len} ({per_sec})")
                .unwrap()
                .progress_chars("=>-"),
        );

        // Find orphans
        let mut orphans = Vec::new();
        for (idx, chunk_hash) in stored_chunks.iter().enumerate() {
            if !referenced_chunks.contains(chunk_hash) {
                orphans.push(chunk_hash.clone());
            }
            if idx % 10000 == 0 {
                pb.set_position(idx as u64);
            }
        }
        pb.finish_and_clear();

        let mut total_removed = 0usize;
        let count = orphans.len();

        if !orphans.is_empty() {
            // Calculate total size before deletion
            for orphan in &orphans {
                if let Ok(size) = repository.storage.get_chunk_size(orphan) {
                    total_removed += size;
                }
            }

            if !self.dry_run {
                // Batch delete for performance (much faster than individual deletes)
                repository.storage.remove_chunks_batch(&orphans)?;
            }

            println!(
                "  Found {} orphaned chunks ({})",
                format_number(count),
                format_size(total_removed)
            );
            success(&format!("Removed in {:.1}s", start.elapsed().as_secs_f64()));
        } else {
            empty("No orphaned chunks found");
        }

        Ok((count, total_removed))
    }

    fn remove_unreferenced_sequences(
        &self,
        repository: &mut SequoiaRepository,
    ) -> Result<(usize, usize)> {
        use crate::cli::formatting::output::*;
        use std::time::Instant;

        let start = Instant::now();
        subsection_header("Finding Unreferenced Sequences");

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
        let count = unreferenced.len();

        if !unreferenced.is_empty() {
            // Calculate total size before deletion
            for seq_hash in &unreferenced {
                if let Ok(size) = repository.storage.sequence_storage.get_size(seq_hash) {
                    total_removed += size;
                }
            }

            if !self.dry_run {
                // Delete sequences (sequence storage may batch internally)
                for seq_hash in &unreferenced {
                    repository.storage.sequence_storage.remove(seq_hash)?;
                }
            }

            println!(
                "  Found {} unreferenced sequences ({})",
                format_number(count),
                format_size(total_removed)
            );
            success(&format!("Removed in {:.1}s", start.elapsed().as_secs_f64()));
        } else {
            empty("No unreferenced sequences found");
        }

        Ok((count, total_removed))
    }

    fn clean_expired_cache(&self, repository: &SequoiaRepository) -> Result<(usize, usize)> {
        use crate::cli::formatting::output::*;
        use std::time::Instant;

        let start = Instant::now();
        subsection_header("Cleaning Expired Cache Entries");

        let cache_dir = repository.storage.base_path.join("cache");
        if !cache_dir.exists() {
            empty("No cache directory found");
            return Ok((0, 0));
        }

        let mut total_removed = 0usize;
        let mut count = 0usize;
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
                        count += 1;
                    }
                }
            }
        }

        if total_removed > 0 {
            println!(
                "  Found {} expired cache entries ({})",
                format_number(count),
                format_size(total_removed)
            );
            success(&format!("Removed in {:.1}s", start.elapsed().as_secs_f64()));
        } else {
            empty("No expired cache entries found");
        }

        Ok((count, total_removed))
    }

    fn clean_incomplete_downloads(&self, repository: &SequoiaRepository) -> Result<(usize, usize)> {
        use crate::cli::formatting::output::*;
        use std::time::Instant;

        let start = Instant::now();
        subsection_header("Cleaning Incomplete Downloads");

        let downloads_dir = repository.storage.base_path.join("downloads");
        if !downloads_dir.exists() {
            empty("No downloads directory found");
            return Ok((0, 0));
        }

        let mut total_removed = 0usize;
        let mut count = 0usize;

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
                    count += 1;
                }
            }
        }

        if total_removed > 0 {
            println!(
                "  Found {} incomplete downloads ({})",
                format_number(count),
                format_size(total_removed)
            );
            success(&format!("Removed in {:.1}s", start.elapsed().as_secs_f64()));
        } else {
            empty("No incomplete downloads found");
        }

        Ok((count, total_removed))
    }

    fn remove_orphaned_chunks_all_databases(
        &self,
        repository: &mut SequoiaRepository,
        databases: &[talaria_sequoia::database::manager::DatabaseInfo],
    ) -> Result<(usize, usize)> {
        use crate::cli::formatting::output::*;
        use indicatif::{ProgressBar, ProgressStyle};
        use std::time::Instant;

        let start = Instant::now();
        subsection_header("Finding Orphaned Chunks");
        info(&format!(
            "Scanning {} databases for chunk references...",
            databases.len()
        ));

        // Get all referenced chunks from ALL databases
        let mut referenced_chunks = HashSet::new();

        // Add chunks from current manifest (unified across all databases)
        let manifest_chunks = repository.manifest.get_chunks();
        for chunk in &manifest_chunks {
            referenced_chunks.insert(chunk.hash.clone());
        }

        // Add chunks from ALL temporal versions (all databases)
        let temporal_manifests = repository.temporal.list_all_manifests()?;
        info(&format!(
            "Scanning {} temporal versions...",
            temporal_manifests.len()
        ));
        for manifest in temporal_manifests {
            for chunk in manifest.chunks {
                referenced_chunks.insert(chunk);
            }
        }

        // Get all stored chunks
        let stored_chunks = repository.storage.list_chunks()?;
        let total_chunks = stored_chunks.len();

        // Create progress bar
        let pb = ProgressBar::new(total_chunks as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("  Scanning chunks [{bar:40}] {pos}/{len} ({per_sec})")
                .unwrap()
                .progress_chars("=>-"),
        );

        // Find orphans
        let mut orphans = Vec::new();
        for (idx, chunk_hash) in stored_chunks.iter().enumerate() {
            if !referenced_chunks.contains(chunk_hash) {
                orphans.push(chunk_hash.clone());
            }
            if idx % 10000 == 0 {
                pb.set_position(idx as u64);
            }
        }
        pb.finish_and_clear();

        let mut total_removed = 0usize;
        let count = orphans.len();

        if !orphans.is_empty() {
            // Calculate total size before deletion
            for orphan in &orphans {
                if let Ok(size) = repository.storage.get_chunk_size(orphan) {
                    total_removed += size;
                }
            }

            if !self.dry_run {
                // Batch delete for performance
                repository.storage.remove_chunks_batch(&orphans)?;
            }

            println!(
                "  Found {} orphaned chunks ({})",
                format_number(count),
                format_size(total_removed)
            );
            success(&format!("Removed in {:.1}s", start.elapsed().as_secs_f64()));
        } else {
            empty("No orphaned chunks found");
        }

        Ok((count, total_removed))
    }

    fn remove_unreferenced_sequences_all_databases(
        &self,
        repository: &mut SequoiaRepository,
        databases: &[talaria_sequoia::database::manager::DatabaseInfo],
    ) -> Result<(usize, usize)> {
        use crate::cli::formatting::output::*;
        use std::time::Instant;

        let start = Instant::now();
        subsection_header("Finding Unreferenced Sequences");
        info(&format!(
            "Scanning {} databases for sequence references...",
            databases.len()
        ));

        // Get all referenced sequences from chunks across ALL databases
        let mut referenced_sequences = HashSet::new();
        let chunks = repository.storage.list_chunks()?;

        info(&format!("Loading {} chunks...", chunks.len()));
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
        let count = unreferenced.len();

        if !unreferenced.is_empty() {
            // Calculate total size before deletion
            for seq_hash in &unreferenced {
                if let Ok(size) = repository.storage.sequence_storage.get_size(seq_hash) {
                    total_removed += size;
                }
            }

            if !self.dry_run {
                // Delete sequences
                for seq_hash in &unreferenced {
                    repository.storage.sequence_storage.remove(seq_hash)?;
                }
            }

            println!(
                "  Found {} unreferenced sequences ({})",
                format_number(count),
                format_size(total_removed)
            );
            success(&format!("Removed in {:.1}s", start.elapsed().as_secs_f64()));
        } else {
            empty("No unreferenced sequences found");
        }

        Ok((count, total_removed))
    }

    fn get_total_size(&self, repository: &SequoiaRepository) -> Result<usize> {
        let stats = repository.storage.get_statistics()?;
        Ok(stats.total_size)
    }

    fn display_results(&self, results: &CleanResults, elapsed: std::time::Duration) -> Result<()> {
        use crate::cli::formatting::output::*;

        println!();
        subsection_header("Database Cleaning Summary");

        let mut table = create_standard_table();
        table.set_header(vec![
            header_cell("Category"),
            header_cell("Count"),
            header_cell("Space Freed"),
        ]);

        // Calculate totals
        let total_count = results.orphaned_chunks.0
            + results.unreferenced_seqs.0
            + results.expired_cache.0
            + results.incomplete_downloads.0;
        let total_bytes = results.orphaned_chunks.1
            + results.unreferenced_seqs.1
            + results.expired_cache.1
            + results.incomplete_downloads.1;

        // Add rows for each category
        table.add_row(vec![
            "Orphaned chunks",
            &format_number(results.orphaned_chunks.0),
            &format_size(results.orphaned_chunks.1),
        ]);

        table.add_row(vec![
            "Unreferenced sequences",
            &format_number(results.unreferenced_seqs.0),
            &format_size(results.unreferenced_seqs.1),
        ]);

        table.add_row(vec![
            "Expired cache entries",
            &format_number(results.expired_cache.0),
            &format_size(results.expired_cache.1),
        ]);

        table.add_row(vec![
            "Incomplete downloads",
            &format_number(results.incomplete_downloads.0),
            &format_size(results.incomplete_downloads.1),
        ]);

        // Add total row
        table.add_row(vec![
            "Total",
            &format_number(total_count),
            &format_size(total_bytes),
        ]);

        println!("{}", table);

        println!();
        println!("  Completed in {:.1}s", elapsed.as_secs_f64());

        if total_count > 0 {
            println!();
            success(&format!(
                "Database cleaning freed {} across {} items",
                format_size(total_bytes),
                format_number(total_count)
            ));
        } else {
            println!();
            info("No items found for cleaning");
        }

        Ok(())
    }
}

impl Clone for CleanCmd {
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
            check_sequences: self.check_sequences,
            report_output: self.report_output.clone(),
            report_format: self.report_format.clone(),
        }
    }
}
