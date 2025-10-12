/// Download manager with state machine coordination
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

use super::workspace::{
    find_existing_workspace_for_source, get_download_workspace, DownloadLock, DownloadState,
    FileTracking, Stage,
};
use super::{DownloadProgress, NCBIDownloader, UniProtDownloader};
use crate::resilience::validation::DownloadStateValidator;
use crate::resilience::{RecoveryStrategy, StateValidator, ValidationResult};
use talaria_core::{DatabaseSource, NCBIDatabase, UniProtDatabase};

/// Options for download behavior
#[derive(Debug, Clone)]
pub struct DownloadOptions {
    /// Skip checksum verification
    pub skip_verify: bool,
    /// Resume interrupted downloads
    pub resume: bool,
    /// Preserve workspace on failure
    pub preserve_on_failure: bool,
    /// Preserve workspace even on success (for debugging)
    pub preserve_always: bool,
    /// Force fresh download (ignore existing)
    pub force: bool,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            skip_verify: false,
            resume: true,              // Resume by default
            preserve_on_failure: true, // Keep files on failure by default
            preserve_always: false,
            force: false,
        }
    }
}

/// Format bytes into human-readable string
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

/// Main download manager
pub struct DownloadManager {
    // No internal state needed for now
}

impl DownloadManager {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    /// Download with state machine and workspace isolation
    pub async fn download_with_state(
        &mut self,
        source: DatabaseSource,
        options: DownloadOptions,
        progress: &mut DownloadProgress,
    ) -> Result<PathBuf> {
        let _span = tracing::info_span!(
            "download_with_state",
            source = %source,
            resume = options.resume,
            force = options.force
        ).entered();

        tracing::info!("Starting download with state management for {}", source);
        // Try to find existing workspace if resuming
        let (workspace, mut state) = if options.resume && !options.force {
            progress.set_message(&format!(
                "Searching for existing downloads of {}...",
                source
            ));

            match find_existing_workspace_for_source(&source)? {
                Some((existing_workspace, existing_state)) => {
                    tracing::info!(
                        "Found existing workspace at: {}",
                        existing_workspace.display()
                    );
                    tracing::Span::current().record(
                        "workspace",
                        existing_workspace.display().to_string().as_str(),
                    );
                    tracing::Span::current()
                        .record("stage", format!("{:?}", existing_state.stage).as_str());
                    progress.set_message(&format!(
                        "Found existing download at: {}",
                        existing_workspace.display()
                    ));
                    (existing_workspace, existing_state)
                }
                None => {
                    tracing::info!("No existing workspace found, creating new one");
                    progress.set_message("No existing download found, starting fresh");
                    let new_workspace = get_download_workspace(&source);
                    tracing::Span::current()
                        .record("workspace", new_workspace.display().to_string().as_str());
                    tracing::Span::current().record("stage", "New");
                    (
                        new_workspace.clone(),
                        DownloadState::new(source.clone(), new_workspace),
                    )
                }
            }
        } else {
            // Not resuming or force mode - create new workspace
            let new_workspace = get_download_workspace(&source);
            if options.force && new_workspace.exists() {
                progress.set_message("Force mode: removing existing workspace and starting fresh");
                fs::remove_dir_all(&new_workspace)?;
            }
            (
                new_workspace.clone(),
                DownloadState::new(source.clone(), new_workspace),
            )
        };

        let state_path = workspace.join("state.json");

        // Validate state before proceeding
        if options.resume && !options.force && state.stage != Stage::Initializing {
            debug!("Validating existing download state");
            let validator = DownloadStateValidator::new(state_path.clone(), workspace.clone());

            match validator.validate()? {
                ValidationResult::Valid => {
                    info!("Download state validation passed");
                }
                ValidationResult::Recoverable(issues) => {
                    warn!(
                        "Found {} recoverable issues in download state",
                        issues.len()
                    );
                    let mut validator_mut =
                        DownloadStateValidator::new(state_path.clone(), workspace.clone());
                    validator_mut.recover(&issues, RecoveryStrategy::AutoRepair)?;
                }
                ValidationResult::Corrupted(issues) => {
                    error!("Download state corrupted with {} issues", issues.len());
                    if options.preserve_on_failure {
                        warn!("Preserving corrupted state for debugging");
                    }
                    // Reset to fresh state
                    let mut validator_mut =
                        DownloadStateValidator::new(state_path.clone(), workspace.clone());
                    validator_mut.recover(&issues, RecoveryStrategy::Reset)?;
                    state = DownloadState::new(source.clone(), workspace.clone());
                }
            }

            // Check if owner is still alive (for locking)
            if state.is_owner_alive() && DownloadLock::is_locked(&workspace) {
                bail!("Download already in progress by PID {}", state.pid);
            }

            // Provide detailed resume information
            let age = state.age();
            let age_str = if age.num_hours() > 24 {
                format!("{} days ago", age.num_days())
            } else if age.num_hours() > 0 {
                format!("{} hours ago", age.num_hours())
            } else {
                format!("{} minutes ago", age.num_minutes())
            };

            progress.set_message(&format!("Found download from {}", age_str));

            // Provide stage-specific details
            match &state.stage {
                Stage::Downloading {
                    bytes_done,
                    total_bytes,
                    ..
                } => {
                    if *total_bytes > 0 {
                        let percent = (*bytes_done as f64 / *total_bytes as f64 * 100.0) as u32;
                        progress.set_message(&format!(
                            "Download was {}% complete ({} of {})",
                            percent,
                            format_bytes(*bytes_done),
                            format_bytes(*total_bytes)
                        ));
                    }
                }
                Stage::Decompressing { source_file, .. } => {
                    if source_file.exists() {
                        if let Ok(metadata) = source_file.metadata() {
                            progress.set_message(&format!(
                                "Resuming decompression of {} ({})",
                                source_file
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy(),
                                format_bytes(metadata.len())
                            ));
                        }
                    }
                }
                Stage::Processing {
                    chunks_done,
                    total_chunks,
                } => {
                    if *total_chunks > 0 {
                        progress.set_message(&format!(
                            "Resuming processing: {} of {} chunks complete",
                            chunks_done, total_chunks
                        ));
                    } else {
                        progress.set_message("Resuming processing stage");
                    }
                }
                Stage::Complete => {
                    // Don't show download messages since it's already done
                    let final_path = state
                        .files
                        .decompressed
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("Complete but no output file"))?;
                    if final_path.exists() {
                        let file_size = final_path.metadata()?.len();
                        progress.set_message(&format!(
                            "Found completed download: {} ({:.2} GB), ready for processing",
                            final_path.file_name().unwrap_or_default().to_string_lossy(),
                            file_size as f64 / 1_073_741_824.0
                        ));
                    } else {
                        progress.set_message(
                            "Download marked complete but file missing, will re-download...",
                        );
                    }
                }
                _ => {
                    progress.set_message(&format!("Resuming from stage: {}", state.stage.name()));
                }
            }
        }

        // Acquire lock
        let _lock =
            DownloadLock::try_acquire(&workspace).context("Failed to acquire download lock")?;

        // Set preserve options in environment for child processes
        if options.preserve_on_failure {
            std::env::set_var("TALARIA_PRESERVE_ON_FAILURE", "1");
        }
        if options.preserve_always {
            std::env::set_var("TALARIA_PRESERVE_ALWAYS", "1");
        }

        // Execute state machine
        let result = self
            .execute_state_machine(&mut state, &state_path, options.clone(), progress)
            .await;

        // Handle result and cleanup
        match result {
            Ok(output_path) => {
                // Success - DO NOT cleanup workspace yet!
                // The DatabaseManager still needs to process the files.
                // Cleanup will happen after DatabaseManager processes the files.
                if options.preserve_always || options.preserve_on_failure {
                    progress
                        .set_message(&format!("Workspace preserved at: {}", workspace.display()));
                }
                // Note: Workspace cleanup should be done by the caller after processing
                Ok(output_path)
            }
            Err(e) => {
                // Failure - preserve important files
                state.transition_to(Stage::Failed {
                    error: e.to_string(),
                    recoverable: true,
                    failed_at: chrono::Utc::now(),
                })?;
                state.save(&state_path)?;

                if options.preserve_on_failure {
                    progress.set_message(&format!(
                        "Download failed. Workspace preserved at: {}",
                        workspace.display()
                    ));
                    progress.set_message("Run with --resume to retry from last checkpoint");
                } else {
                    self.cleanup_workspace(&workspace, &state.files, true)?;
                }

                Err(e)
            }
        }
    }

    /// Execute the download state machine
    async fn execute_state_machine(
        &mut self,
        state: &mut DownloadState,
        state_path: &Path,
        options: DownloadOptions,
        progress: &mut DownloadProgress,
    ) -> Result<PathBuf> {
        loop {
            // Save state after each transition
            state.save(state_path)?;

            match &state.stage {
                Stage::Initializing => {
                    progress.set_message("Initializing workspace...");
                    self.initialize_workspace(&state.workspace)?;
                    state.transition_to(Stage::Downloading {
                        bytes_done: 0,
                        total_bytes: 0,
                        url: self.get_download_url(&state.source)?,
                    })?;
                }

                Stage::Downloading {
                    bytes_done, url, ..
                } => {
                    if *bytes_done > 0 {
                        progress.set_message(&format!(
                            "Resuming download from byte position {}...",
                            format_bytes(*bytes_done)
                        ));
                    } else {
                        progress.set_message("Starting fresh database download...");
                    }

                    let (compressed_file, _total_bytes) = self
                        .download_file(
                            &state.source,
                            &state.workspace,
                            url,
                            *bytes_done,
                            options.skip_verify,
                            progress,
                        )
                        .await?;

                    state.files.compressed = Some(compressed_file.clone());
                    state.files.track_temp_file(compressed_file.clone());
                    state.files.preserve_on_failure(compressed_file.clone());

                    state.transition_to(Stage::Verifying { checksum: None })?;
                }

                Stage::Verifying { .. } => {
                    if options.skip_verify {
                        progress.set_message("Skipping verification");
                    } else {
                        progress.set_message("Verifying download...");

                        // Verify checksum if available
                        let compressed = state
                            .files
                            .compressed
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("No compressed file found"))?;

                        // Try to download and verify checksum files
                        let checksum_verified =
                            self.verify_checksum(&compressed, &state, &options).await?;

                        if checksum_verified {
                            progress.set_message("✓ Checksum verification passed");
                        } else {
                            progress.set_message("⚠ No checksum available for verification");
                        }
                    }

                    let compressed = state
                        .files
                        .compressed
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No compressed file found"))?;

                    // Determine target name
                    let decompressed = if compressed.extension() == Some(std::ffi::OsStr::new("gz"))
                    {
                        state.workspace.join(compressed.file_stem().unwrap())
                    } else {
                        state.workspace.join("decompressed.fasta")
                    };

                    state.transition_to(Stage::Decompressing {
                        source_file: compressed.clone(),
                        target_file: decompressed,
                    })?;
                }

                Stage::Decompressing {
                    source_file,
                    target_file: _,
                } => {
                    if !source_file.exists() {
                        bail!("Compressed file not found: {}", source_file.display());
                    }

                    // OPTIMIZATION: Skip decompression entirely!
                    // FASTA parser (talaria-bio/src/formats/fasta.rs:302) can read .gz files directly
                    // This saves ~50-100 GB of disk I/O and 40-60 minutes of processing time

                    if let Ok(metadata) = source_file.metadata() {
                        progress.set_message(&format!(
                            "Skipping decompression - FASTA parser will stream-decompress {} file directly",
                            format_bytes(metadata.len())
                        ));
                    } else {
                        progress.set_message(
                            "Skipping decompression - will stream-decompress during processing",
                        );
                    }

                    // Use compressed file directly as "decompressed" output
                    // FASTA parser auto-detects .gz extension and decompresses on-the-fly
                    state.files.decompressed = Some(source_file.clone());
                    state.files.preserve_on_failure(source_file.clone());

                    state.transition_to(Stage::Processing {
                        chunks_done: 0,
                        total_chunks: 0,
                    })?;
                }

                Stage::Processing {
                    chunks_done,
                    total_chunks,
                } => {
                    if *chunks_done > 0 || *total_chunks > 0 {
                        progress.set_message(&format!(
                            "Resuming processing: {} of {} chunks complete",
                            chunks_done, total_chunks
                        ));
                    } else {
                        progress.set_message("Processing downloaded file...");
                    }

                    let _decompressed = state
                        .files
                        .decompressed
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No decompressed file found"))?;

                    // Just return the decompressed file
                    // The DatabaseManager will handle chunking separately
                    progress.set_message("Download ready for HERALD processing...");

                    state.transition_to(Stage::Finalizing)?;
                }

                Stage::Finalizing => {
                    progress.set_message("Finalizing download...");

                    // Move to final location if needed
                    let final_path = state
                        .files
                        .decompressed
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No output file"))?
                        .clone();

                    state.transition_to(Stage::Complete)?;
                    state.save(state_path)?;

                    progress.set_message("Download complete!");
                    return Ok(final_path);
                }

                Stage::Complete => {
                    // Download is complete, return the decompressed file path
                    let final_path = state
                        .files
                        .decompressed
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("Complete but no output file"))?;

                    // Verify the file still exists
                    if !final_path.exists() {
                        // File was deleted, need to restart
                        progress.set_message("Downloaded file missing, restarting download...");
                        progress
                            .set_message("Downloading full database (this may take a while)...");
                        state.transition_to(Stage::Initializing)?;
                        continue;
                    }

                    // Don't show download progress - file is already complete
                    // Just return the path immediately
                    return Ok(final_path.clone());
                }

                Stage::Failed { recoverable, .. } => {
                    if *recoverable {
                        progress.set_message("Attempting recovery from last checkpoint...");
                        state.restore_last_checkpoint()?;
                    } else {
                        bail!("Download failed and is not recoverable");
                    }
                }
            }
        }
    }

    /// Initialize workspace structure
    fn initialize_workspace(&self, workspace: &Path) -> Result<()> {
        fs::create_dir_all(workspace)?;
        // Note: chunks/ directory removed - data streams directly to RocksDB
        Ok(())
    }

    /// Clean up download workspace after successful processing
    pub fn cleanup_download_workspace(source: &DatabaseSource) -> Result<()> {
        let workspace = get_download_workspace(source);

        // Check environment for preservation flags
        let preserve_always = std::env::var("TALARIA_PRESERVE_ALWAYS").is_ok();
        let preserve_downloads = std::env::var("TALARIA_PRESERVE_DOWNLOADS").is_ok();

        if preserve_always || preserve_downloads {
            return Ok(());
        }

        // Remove the entire workspace
        if workspace.exists() {
            fs::remove_dir_all(&workspace)?;
        }

        Ok(())
    }

    /// Get download URL for source
    fn get_download_url(&self, source: &DatabaseSource) -> Result<String> {
        match source {
            DatabaseSource::UniProt(db) => {
                let base = "https://ftp.ebi.ac.uk/pub/databases/uniprot";
                Ok(match db {
                    UniProtDatabase::SwissProt => format!(
                        "{}/current_release/knowledgebase/complete/uniprot_sprot.fasta.gz",
                        base
                    ),
                    UniProtDatabase::TrEMBL => format!(
                        "{}/current_release/knowledgebase/complete/uniprot_trembl.fasta.gz",
                        base
                    ),
                    UniProtDatabase::UniRef50 => {
                        format!("{}/current_release/uniref/uniref50/uniref50.fasta.gz", base)
                    }
                    UniProtDatabase::UniRef90 => {
                        format!("{}/current_release/uniref/uniref90/uniref90.fasta.gz", base)
                    }
                    UniProtDatabase::UniRef100 => format!(
                        "{}/current_release/uniref/uniref100/uniref100.fasta.gz",
                        base
                    ),
                    _ => bail!("Unsupported UniProt database: {:?}", db),
                })
            }
            DatabaseSource::NCBI(db) => {
                let base = "https://ftp.ncbi.nlm.nih.gov";
                Ok(match db {
                    NCBIDatabase::NR => format!("{}/blast/db/FASTA/nr.gz", base),
                    NCBIDatabase::NT => format!("{}/blast/db/FASTA/nt.gz", base),
                    NCBIDatabase::RefSeqProtein => {
                        format!("{}/refseq/release/complete/complete.protein.faa.gz", base)
                    }
                    NCBIDatabase::RefSeqGenomic => {
                        format!("{}/refseq/release/complete/complete.genomic.fna.gz", base)
                    }
                    NCBIDatabase::Taxonomy => format!("{}/pub/taxonomy/taxdump.tar.gz", base),
                    NCBIDatabase::ProtAccession2TaxId => format!(
                        "{}/pub/taxonomy/accession2taxid/prot.accession2taxid.gz",
                        base
                    ),
                    NCBIDatabase::NuclAccession2TaxId => format!(
                        "{}/pub/taxonomy/accession2taxid/nucl_gb.accession2taxid.gz",
                        base
                    ),
                    _ => bail!("Unsupported NCBI database: {:?}", db),
                })
            }
            _ => bail!("Unsupported database source: {:?}", source),
        }
    }

    /// Download file with resume support
    async fn download_file(
        &self,
        source: &DatabaseSource,
        workspace: &Path,
        url: &str,
        _resume_from: u64,
        _skip_verify: bool,
        progress: &mut DownloadProgress,
    ) -> Result<(PathBuf, u64)> {
        // Determine filename
        let filename = url
            .split('/')
            .last()
            .ok_or_else(|| anyhow::anyhow!("Invalid URL"))?;
        let final_path = workspace.join(filename);

        // Check for partial download file (.tmp extension used by downloaders)
        let temp_path = workspace.join(format!("{}.tmp", filename));
        let should_resume = temp_path.exists();

        if should_resume {
            let partial_size = fs::metadata(&temp_path)?.len();
            progress.set_message(&format!(
                "Found partial download: {:.2} MB, resuming...",
                partial_size as f64 / 1_048_576.0
            ));
            info!(
                "Resuming download from {} bytes (partial file: {})",
                partial_size,
                temp_path.display()
            );
        }

        // Check if already complete
        if final_path.exists() {
            let size = fs::metadata(&final_path)?.len();
            progress.set_message("Using existing downloaded file");
            return Ok((final_path, size));
        }

        // Download based on source type
        // Note: Retry logic would need refactoring of async closures to handle mutable progress reference
        match source {
            DatabaseSource::UniProt(_) => {
                let downloader = UniProtDownloader::new();
                info!("Starting UniProt download from {}", url);
                // Download compressed file to workspace
                downloader
                    .download_compressed_with_resume(url, &final_path, progress, should_resume)
                    .await
                    .context("Failed to download UniProt file")?;
            }
            DatabaseSource::NCBI(_) => {
                let downloader = NCBIDownloader::new();
                info!("Starting NCBI download from {}", url);
                // Download compressed file to workspace
                downloader
                    .download_compressed_with_resume(url, &final_path, progress, should_resume)
                    .await
                    .context("Failed to download NCBI file")?;
            }
            _ => bail!("Unsupported source for download"),
        }

        let size = fs::metadata(&final_path)?.len();
        Ok((final_path, size))
    }

    // decompress_file method removed - no longer needed
    // FASTA parser (talaria-bio/src/formats/fasta.rs:302) handles .gz files directly
    // This saves ~50-100 GB of disk I/O per large database download

    /// Clean up workspace
    fn cleanup_workspace(
        &self,
        workspace: &Path,
        files: &FileTracking,
        preserve_important: bool,
    ) -> Result<()> {
        // Check environment for preservation flags
        let preserve_always = std::env::var("TALARIA_PRESERVE_ALWAYS").is_ok();
        let preserve_on_failure = std::env::var("TALARIA_PRESERVE_ON_FAILURE").is_ok();
        let preserve_downloads = std::env::var("TALARIA_PRESERVE_DOWNLOADS").is_ok();

        if preserve_always || preserve_downloads {
            return Ok(());
        }

        if preserve_important && preserve_on_failure {
            // Only clean non-essential files
            for temp_file in &files.temp_files {
                if !files.preserve_on_failure.contains(temp_file) {
                    if let Err(e) = fs::remove_file(temp_file) {
                        debug!("Failed to remove temp file {:?}: {}", temp_file, e);
                    } else {
                        debug!("Removed temp file: {:?}", temp_file);
                    }
                }
            }
        } else {
            // Clean all temp files
            for temp_file in &files.temp_files {
                if let Err(e) = fs::remove_file(temp_file) {
                    debug!("Failed to remove temp file {:?}: {}", temp_file, e);
                } else {
                    debug!("Removed temp file: {:?}", temp_file);
                }
            }

            // Remove compressed file if decompressed exists
            if files.decompressed.is_some() {
                if let Some(compressed) = &files.compressed {
                    if let Err(e) = fs::remove_file(compressed) {
                        debug!("Failed to remove compressed file {:?}: {}", compressed, e);
                    } else {
                        info!("Removed compressed file after successful decompression");
                    }
                }
            }

            // Try to remove workspace if empty
            if workspace.exists() {
                match fs::read_dir(workspace) {
                    Ok(entries) => {
                        if entries.count() == 0 {
                            if let Err(e) = fs::remove_dir(workspace) {
                                debug!("Failed to remove empty workspace {:?}: {}", workspace, e);
                            } else {
                                info!("Removed empty workspace directory");
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Failed to read workspace directory {:?}: {}", workspace, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Verify checksum of downloaded file
    async fn verify_checksum(
        &self,
        file_path: &Path,
        state: &DownloadState,
        _options: &DownloadOptions,
    ) -> Result<bool> {
        use sha2::{Digest, Sha256};
        use std::io::Read;

        // Try common checksum file extensions
        let checksum_extensions = vec![".sha256", ".md5", ".sha256sum", ".md5sum"];
        let file_name = file_path.file_name().unwrap().to_str().unwrap();

        // Get base URL from the source
        let base_url = self.get_source_url(&state.source);

        for ext in &checksum_extensions {
            let checksum_url = format!("{}/{}{}", base_url, file_name, ext);
            let _checksum_file = state.workspace.join(format!("{}{}", file_name, ext));

            // Try to download checksum file
            let client = reqwest::Client::new();
            match client.get(&checksum_url).send().await {
                Ok(response) if response.status().is_success() => {
                    let checksum_content = response.text().await?;

                    // Parse checksum (format: "hash  filename" or just "hash")
                    let expected_hash = checksum_content
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_lowercase();

                    if expected_hash.is_empty() {
                        continue;
                    }

                    // Compute actual file hash
                    let mut file = std::fs::File::open(file_path)?;
                    let mut hasher = Sha256::new();
                    let mut buffer = vec![0; 8192];

                    loop {
                        let bytes_read = file.read(&mut buffer)?;
                        if bytes_read == 0 {
                            break;
                        }
                        hasher.update(&buffer[..bytes_read]);
                    }

                    let actual_hash = format!("{:x}", hasher.finalize());

                    if actual_hash == expected_hash {
                        return Ok(true);
                    } else {
                        bail!(
                            "Checksum verification failed: expected {}, got {}",
                            expected_hash,
                            actual_hash
                        );
                    }
                }
                _ => {
                    // Checksum file not available, continue trying other extensions
                    continue;
                }
            }
        }

        // No checksum file found
        Ok(false)
    }

    /// Get the base URL for a database source
    fn get_source_url(&self, source: &DatabaseSource) -> String {
        match source {
            DatabaseSource::NCBI(_) => "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy".to_string(),
            DatabaseSource::UniProt(_) => {
                "https://ftp.uniprot.org/pub/databases/uniprot".to_string()
            }
            DatabaseSource::Custom(url) => url.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use talaria_test::fixtures::test_database_source;
    use tempfile::TempDir;

    #[test]
    fn test_download_options_default() {
        let options = DownloadOptions::default();
        assert!(!options.skip_verify);
        assert!(options.resume);
        assert!(options.preserve_on_failure);
        assert!(!options.preserve_always);
        assert!(!options.force);
    }

    #[test]
    fn test_get_download_url_uniprot() {
        let manager = DownloadManager::new().unwrap();

        // Test SwissProt URL
        let url = manager
            .get_download_url(&DatabaseSource::UniProt(UniProtDatabase::SwissProt))
            .unwrap();
        assert!(url.contains("uniprot_sprot.fasta.gz"));
        assert!(url.contains("ftp.ebi.ac.uk"));

        // Test TrEMBL URL
        let url = manager
            .get_download_url(&DatabaseSource::UniProt(UniProtDatabase::TrEMBL))
            .unwrap();
        assert!(url.contains("uniprot_trembl.fasta.gz"));

        // Test UniRef URLs
        let url = manager
            .get_download_url(&DatabaseSource::UniProt(UniProtDatabase::UniRef50))
            .unwrap();
        assert!(url.contains("uniref50/uniref50.fasta.gz"));

        let url = manager
            .get_download_url(&DatabaseSource::UniProt(UniProtDatabase::UniRef90))
            .unwrap();
        assert!(url.contains("uniref90/uniref90.fasta.gz"));

        let url = manager
            .get_download_url(&DatabaseSource::UniProt(UniProtDatabase::UniRef100))
            .unwrap();
        assert!(url.contains("uniref100/uniref100.fasta.gz"));
    }

    #[test]
    fn test_get_download_url_ncbi() {
        let manager = DownloadManager::new().unwrap();

        // Test NR URL
        let url = manager
            .get_download_url(&DatabaseSource::NCBI(NCBIDatabase::NR))
            .unwrap();
        assert!(url.contains("blast/db/FASTA/nr.gz"));
        assert!(url.contains("ftp.ncbi.nlm.nih.gov"));

        // Test NT URL
        let url = manager
            .get_download_url(&DatabaseSource::NCBI(NCBIDatabase::NT))
            .unwrap();
        assert!(url.contains("blast/db/FASTA/nt.gz"));

        // Test RefSeq URLs
        let url = manager
            .get_download_url(&DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein))
            .unwrap();
        assert!(url.contains("complete.protein.faa.gz"));

        let url = manager
            .get_download_url(&DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic))
            .unwrap();
        assert!(url.contains("complete.genomic.fna.gz"));

        // Test Taxonomy URL
        let url = manager
            .get_download_url(&DatabaseSource::NCBI(NCBIDatabase::Taxonomy))
            .unwrap();
        assert!(url.contains("taxonomy/taxdump.tar.gz"));

        // Test Accession2TaxId URLs
        let url = manager
            .get_download_url(&DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId))
            .unwrap();
        assert!(url.contains("prot.accession2taxid.gz"));

        let url = manager
            .get_download_url(&DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId))
            .unwrap();
        assert!(url.contains("nucl_gb.accession2taxid.gz"));
    }

    #[test]
    fn test_get_download_url_unsupported() {
        let manager = DownloadManager::new().unwrap();

        // Test unsupported sources
        let result = manager.get_download_url(&DatabaseSource::Custom("test".to_string()));
        assert!(result.is_err());

        let result = manager.get_download_url(&test_database_source("download_manager"));
        assert!(result.is_err());
    }

    #[test]
    fn test_initialize_workspace() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().join("test_workspace");

        let manager = DownloadManager::new().unwrap();
        manager.initialize_workspace(&workspace).unwrap();

        // Check workspace was created
        assert!(workspace.exists());
    }

    // test_decompress_file_creates_output removed - decompression no longer happens
    // The optimization skips decompression entirely and passes .gz files directly to FASTA parser
    // See tests/download_gz_optimization_test.rs for new optimization tests

    #[test]
    fn test_cleanup_workspace_respects_environment() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().join("test_workspace");
        fs::create_dir_all(&workspace).unwrap();

        // Create test files
        let test_file = workspace.join("test.txt");
        fs::write(&test_file, b"test").unwrap();

        let mut files = FileTracking::new();
        files.track_temp_file(test_file.clone());

        let manager = DownloadManager::new().unwrap();

        // Test with TALARIA_PRESERVE_ALWAYS
        std::env::set_var("TALARIA_PRESERVE_ALWAYS", "1");
        manager
            .cleanup_workspace(&workspace, &files, false)
            .unwrap();
        assert!(
            test_file.exists(),
            "File should be preserved with TALARIA_PRESERVE_ALWAYS"
        );
        std::env::remove_var("TALARIA_PRESERVE_ALWAYS");

        // Test normal cleanup
        manager
            .cleanup_workspace(&workspace, &files, false)
            .unwrap();
        assert!(!test_file.exists(), "File should be cleaned up normally");
    }

    #[test]
    fn test_cleanup_workspace_preserve_on_failure() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().join("test_workspace");
        fs::create_dir_all(&workspace).unwrap();

        // Create test files
        let important_file = workspace.join("important.dat");
        let temp_file = workspace.join("temp.tmp");

        fs::write(&important_file, b"important").unwrap();
        fs::write(&temp_file, b"temp").unwrap();

        let mut files = FileTracking::new();
        files.track_temp_file(important_file.clone());
        files.track_temp_file(temp_file.clone());
        files.preserve_on_failure(important_file.clone());

        let manager = DownloadManager::new().unwrap();

        // Clean with preserve_important = true and TALARIA_PRESERVE_ON_FAILURE set
        std::env::set_var("TALARIA_PRESERVE_ON_FAILURE", "1");
        manager.cleanup_workspace(&workspace, &files, true).unwrap();

        assert!(
            important_file.exists(),
            "Important file should be preserved"
        );
        assert!(!temp_file.exists(), "Temp file should be cleaned");

        std::env::remove_var("TALARIA_PRESERVE_ON_FAILURE");
    }
}
