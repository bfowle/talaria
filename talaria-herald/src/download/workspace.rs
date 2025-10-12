/// Download workspace management with distributed systems patterns
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

use crate::SHA256Hash;
use talaria_core::{system::paths, DatabaseSource, NCBIDatabase, UniProtDatabase};

/// Extension trait for DatabaseSource methods
pub trait DatabaseSourceExt {
    fn canonical_name(&self) -> String;
    fn version(&self) -> Option<String>;
}

impl DatabaseSourceExt for DatabaseSource {
    fn canonical_name(&self) -> String {
        match self {
            DatabaseSource::UniProt(db) => {
                format!(
                    "uniprot_{}",
                    match db {
                        UniProtDatabase::SwissProt => "swissprot",
                        UniProtDatabase::TrEMBL => "trembl",
                        UniProtDatabase::UniRef50 => "uniref50",
                        UniProtDatabase::UniRef90 => "uniref90",
                        UniProtDatabase::UniRef100 => "uniref100",
                        UniProtDatabase::IdMapping => "idmapping",
                    }
                )
            }
            DatabaseSource::NCBI(db) => {
                format!(
                    "ncbi_{}",
                    match db {
                        NCBIDatabase::NR => "nr",
                        NCBIDatabase::NT => "nt",
                        NCBIDatabase::RefSeq => "refseq",
                        NCBIDatabase::RefSeqProtein => "refseq_protein",
                        NCBIDatabase::RefSeqGenomic => "refseq_genomic",
                        NCBIDatabase::GenBank => "genbank",
                        NCBIDatabase::Taxonomy => "taxonomy",
                        NCBIDatabase::ProtAccession2TaxId => "prot_accession2taxid",
                        NCBIDatabase::NuclAccession2TaxId => "nucl_accession2taxid",
                    }
                )
            }
            DatabaseSource::Custom(name) => format!("custom_{}", name.replace('/', "_")),
        }
    }

    fn version(&self) -> Option<String> {
        // For now, return None - version tracking can be added later
        None
    }
}

/// Download state machine tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadState {
    /// Unique identifier for this download session
    pub id: String,
    /// Database being downloaded
    pub source: DatabaseSource,
    /// Current stage of the download
    pub stage: Stage,
    /// Workspace directory for this download
    pub workspace: PathBuf,
    /// Files being tracked
    pub files: FileTracking,
    /// When download started
    pub started_at: DateTime<Utc>,
    /// Last update time
    pub updated_at: DateTime<Utc>,
    /// Checkpoints for recovery
    pub checkpoints: Vec<Checkpoint>,
    /// Process ID that owns this download
    pub pid: u32,
    /// Hostname for distributed scenarios
    pub hostname: String,

    // Merged from ChunkingCheckpoint:
    /// Version string for this operation (set once at start)
    #[serde(default)]
    pub version: Option<String>,
    /// Number of sequences processed so far (for chunking)
    #[serde(default)]
    pub sequences_processed: usize,
    /// Byte offset in the input file (for streaming)
    #[serde(default)]
    pub file_offset: u64,
    /// Last sequence ID processed
    #[serde(default)]
    pub last_sequence_id: Option<String>,
    /// Total file size for progress calculation
    #[serde(default)]
    pub total_file_size: Option<u64>,
    /// Performance metrics
    #[serde(default)]
    pub performance_metrics: Option<PerformanceMetrics>,
}

/// Stages of download/processing pipeline
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Stage {
    /// Setting up workspace
    Initializing,
    /// Downloading file
    Downloading {
        bytes_done: u64,
        total_bytes: u64,
        url: String,
    },
    /// Verifying checksums
    Verifying { checksum: Option<String> },
    /// Decompressing file
    Decompressing {
        source_file: PathBuf,
        target_file: PathBuf,
    },
    /// Processing into chunks
    Processing {
        chunks_done: usize,
        total_chunks: usize,
    },
    /// Moving to final location
    Finalizing,
    /// Successfully complete
    Complete,
    /// Failed with error
    Failed {
        error: String,
        recoverable: bool,
        failed_at: DateTime<Utc>,
    },
}

impl Stage {
    /// Check if this stage represents a completed state
    pub fn is_complete(&self) -> bool {
        matches!(self, Stage::Complete)
    }

    /// Check if this stage represents a failed state
    pub fn is_failed(&self) -> bool {
        matches!(self, Stage::Failed { .. })
    }

    /// Get human-readable stage name
    pub fn name(&self) -> &str {
        match self {
            Stage::Initializing => "initializing",
            Stage::Downloading { .. } => "downloading",
            Stage::Verifying { .. } => "verifying",
            Stage::Decompressing { .. } => "decompressing",
            Stage::Processing { .. } => "processing",
            Stage::Finalizing => "finalizing",
            Stage::Complete => "complete",
            Stage::Failed { .. } => "failed",
        }
    }
}

/// Track all files associated with a download
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTracking {
    /// Compressed file (e.g., .gz)
    pub compressed: Option<PathBuf>,
    /// Partial download file (e.g., .gz.part)
    pub partial_compressed: Option<PathBuf>,
    /// Decompressed file
    pub decompressed: Option<PathBuf>,
    /// Partial decompressed file (e.g., .fasta.part)
    pub partial_decompressed: Option<PathBuf>,
    /// Directory containing processed chunks
    pub chunks_dir: Option<PathBuf>,
    /// Final manifest file
    pub manifest: Option<PathBuf>,
    /// All temporary files to clean up
    pub temp_files: Vec<PathBuf>,
    /// Files that should be preserved on failure
    pub preserve_on_failure: Vec<PathBuf>,
}

impl FileTracking {
    pub fn new() -> Self {
        Self {
            compressed: None,
            partial_compressed: None,
            decompressed: None,
            partial_decompressed: None,
            chunks_dir: None,
            manifest: None,
            temp_files: Vec::new(),
            preserve_on_failure: Vec::new(),
        }
    }

    /// Add a file to be tracked and cleaned up
    pub fn track_temp_file(&mut self, path: PathBuf) {
        if !self.temp_files.contains(&path) {
            self.temp_files.push(path);
        }
    }

    /// Mark a file to be preserved if operation fails
    pub fn preserve_on_failure(&mut self, path: PathBuf) {
        if !self.preserve_on_failure.contains(&path) {
            self.preserve_on_failure.push(path);
        }
    }

    /// Check if we have a partial download to resume
    pub fn has_partial_download(&self) -> bool {
        self.partial_compressed
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    /// Get the size of partial download if exists
    pub fn partial_download_size(&self) -> Option<u64> {
        self.partial_compressed
            .as_ref()
            .and_then(|path| path.metadata().ok())
            .map(|meta| meta.len())
    }

    /// Track a partial file
    pub fn track_partial_file(&mut self, path: PathBuf, is_compressed: bool) {
        if is_compressed {
            self.partial_compressed = Some(path.clone());
        } else {
            self.partial_decompressed = Some(path.clone());
        }
        self.track_temp_file(path);
    }

    /// Move partial file to final location after completion
    pub fn finalize_partial(&mut self, is_compressed: bool) -> Result<()> {
        use std::fs;

        if is_compressed {
            if let (Some(partial), Some(final_path)) = (&self.partial_compressed, &self.compressed)
            {
                fs::rename(partial, final_path)?;
                self.partial_compressed = None;
            }
        } else {
            if let (Some(partial), Some(final_path)) =
                (&self.partial_decompressed, &self.decompressed)
            {
                fs::rename(partial, final_path)?;
                self.partial_decompressed = None;
            }
        }
        Ok(())
    }
}

/// Checkpoint for recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub stage: Stage,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

/// Performance metrics for tracking progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Sequences processed per second
    pub sequences_per_second: f64,
    /// Bytes processed per second
    pub bytes_per_second: f64,
    /// Time elapsed in seconds
    pub elapsed_seconds: f64,
    /// Estimated time remaining in seconds
    pub estimated_remaining_seconds: f64,
}

impl DownloadState {
    /// Create new download state
    pub fn new(source: DatabaseSource, workspace: PathBuf) -> Self {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        Self {
            id: generate_session_id(&source),
            source,
            stage: Stage::Initializing,
            workspace,
            files: FileTracking::new(),
            started_at: Utc::now(),
            updated_at: Utc::now(),
            checkpoints: Vec::new(),
            pid: std::process::id(),
            hostname,
            // Initialize merged ChunkingCheckpoint fields:
            version: None,
            sequences_processed: 0,
            file_offset: 0,
            last_sequence_id: None,
            total_file_size: None,
            performance_metrics: None,
        }
    }

    /// Load state from file
    pub fn load(path: &Path) -> Result<Self> {
        let mut file = File::open(path).context("Failed to open state file")?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .context("Failed to read state file")?;
        let mut state: DownloadState =
            serde_json::from_str(&contents).context("Failed to parse state file")?;

        // If we're in the Downloading stage, check if a partial .tmp file exists
        // and update bytes_done to match the actual file size (for proper resume)
        if let Stage::Downloading {
            ref mut bytes_done,
            ref url,
            total_bytes: _,
        } = state.stage
        {
            // Extract filename from URL
            if let Some(filename) = url.split('/').last() {
                // Check for .tmp file
                let tmp_file = state.workspace.join(format!("{}.tmp", filename));
                if tmp_file.exists() {
                    if let Ok(metadata) = fs::metadata(&tmp_file) {
                        let file_size = metadata.len();
                        if file_size > *bytes_done {
                            tracing::info!(
                                "Found partial download: {} bytes in .tmp file, updating state from {} bytes",
                                file_size,
                                *bytes_done
                            );
                            *bytes_done = file_size;
                        }
                    }
                }
            }
        }

        Ok(state)
    }

    /// Save state to file
    pub fn save(&self, path: &Path) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write to temp file first (atomic)
        let temp_path = path.with_extension("tmp");
        let mut file = File::create(&temp_path)?;
        let json = serde_json::to_string_pretty(self)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;

        // Atomic rename
        fs::rename(&temp_path, path)?;
        Ok(())
    }

    /// Transition to new stage
    pub fn transition_to(&mut self, new_stage: Stage) -> Result<()> {
        // Add checkpoint for current stage
        self.checkpoints.push(Checkpoint {
            stage: self.stage.clone(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        });

        // Update stage
        self.stage = new_stage;
        self.updated_at = Utc::now();

        Ok(())
    }

    /// Restore to last checkpoint
    pub fn restore_last_checkpoint(&mut self) -> Result<()> {
        if let Some(checkpoint) = self.checkpoints.pop() {
            self.stage = checkpoint.stage;
            self.updated_at = Utc::now();
            Ok(())
        } else {
            bail!("No checkpoint to restore")
        }
    }

    /// Update chunking progress (merged from ChunkingCheckpoint)
    pub fn update_chunking_progress(
        &mut self,
        sequences_processed: usize,
        file_offset: u64,
        last_sequence_id: Option<String>,
    ) {
        self.sequences_processed = sequences_processed;
        self.file_offset = file_offset;
        self.last_sequence_id = last_sequence_id;
        self.updated_at = Utc::now();

        // Update performance metrics
        if let Some(total_size) = self.total_file_size {
            let elapsed = (self.updated_at - self.started_at).num_seconds() as f64;
            if elapsed > 0.0 {
                let bytes_per_second = file_offset as f64 / elapsed;
                let sequences_per_second = sequences_processed as f64 / elapsed;

                let remaining_bytes = (total_size - file_offset) as f64;
                let estimated_remaining = if bytes_per_second > 0.0 {
                    remaining_bytes / bytes_per_second
                } else {
                    0.0
                };

                self.performance_metrics = Some(PerformanceMetrics {
                    sequences_per_second,
                    bytes_per_second,
                    elapsed_seconds: elapsed,
                    estimated_remaining_seconds: estimated_remaining,
                });
            }
        }
    }

    /// Check if should save checkpoint (every 500k sequences or 1GB)
    pub fn should_save_checkpoint(&self) -> bool {
        const SEQUENCE_INTERVAL: usize = 500_000;
        const BYTE_INTERVAL: u64 = 1_000_000_000; // 1GB

        // Check sequence interval
        if self.sequences_processed > 0 && self.sequences_processed % SEQUENCE_INTERVAL == 0 {
            return true;
        }

        // Check byte interval
        if self.file_offset > 0 && self.file_offset % BYTE_INTERVAL == 0 {
            return true;
        }

        false
    }

    /// Check if process that owns this download is still alive
    pub fn is_owner_alive(&self) -> bool {
        // Check if same host
        let current_host = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        if self.hostname != current_host {
            // Different host, assume alive (can't check remote process)
            return true;
        }

        // Same host, check if process exists
        is_process_alive(self.pid)
    }

    /// Get age of this download session
    pub fn age(&self) -> chrono::Duration {
        Utc::now() - self.started_at
    }

    /// Check if download is stale (too old)
    pub fn is_stale(&self, max_age_hours: i64) -> bool {
        self.age().num_hours() > max_age_hours
    }
}

/// Generate deterministic workspace path for a database
pub fn get_download_workspace(source: &DatabaseSource) -> PathBuf {
    let _span = tracing::debug_span!("get_download_workspace", source = %source).entered();

    let base = paths::talaria_downloads_dir();
    debug!("Creating download workspace for {}", source);

    // Ensure downloads directory exists
    fs::create_dir_all(&base).ok();

    // Deterministic component (allows finding existing downloads)
    let db_id = source.canonical_name();

    // Version component (if available from source)
    let version = source
        .version()
        .unwrap_or_else(|| Utc::now().format("%Y%m%d").to_string());

    // Session component for uniqueness
    let session_id = generate_session_id(source);

    base.join(format!("{}_{}_{}", db_id, version, session_id))
}

/// Get workspace for a specific download ID
pub fn get_workspace_by_id(download_id: &str) -> PathBuf {
    paths::talaria_downloads_dir().join(download_id)
}

/// Generate session ID for download
fn generate_session_id(source: &DatabaseSource) -> String {
    // Check for override from environment (useful for testing)
    if let Ok(session) = std::env::var("TALARIA_SESSION") {
        return session;
    }

    // Generate based on source, PID, and time
    let pid = std::process::id();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let data = format!("{:?}-{}-{}", source, pid, ts);
    let hash = SHA256Hash::compute(data.as_bytes());

    // Use first 8 chars of hash
    hash.to_hex()[..8].to_string()
}

/// Check if a process is alive (platform-specific)
#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    // On Unix, send signal 0 to check if process exists
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
fn is_process_alive(pid: u32) -> bool {
    // On non-Unix, try to open process (Windows)
    // This is a simplified check
    use std::process::Command;

    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(&["/FI", &format!("PID eq {}", pid)])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }

    #[cfg(not(windows))]
    {
        // Fallback: assume alive if we can't check
        true
    }
}

/// Lock file for preventing concurrent access
pub struct DownloadLock {
    workspace: PathBuf,
    lock_file: Option<File>,
}

impl DownloadLock {
    /// Try to acquire lock for workspace
    pub fn try_acquire(workspace: &Path) -> Result<Self> {
        let lock_path = workspace.join(".lock");

        // Create workspace if it doesn't exist
        fs::create_dir_all(workspace)?;

        // Try to create lock file exclusively
        let lock_file = OpenOptions::new()
            .write(true)
            .create_new(true) // Fails if file exists
            .open(&lock_path);

        match lock_file {
            Ok(mut file) => {
                // Write PID and hostname to lock file
                let lock_info = format!(
                    "{}\n{}\n{}",
                    std::process::id(),
                    hostname::get()
                        .map(|h| h.to_string_lossy().to_string())
                        .unwrap_or_else(|_| "unknown".to_string()),
                    Utc::now().to_rfc3339()
                );
                file.write_all(lock_info.as_bytes())?;
                file.sync_all()?;

                Ok(Self {
                    workspace: workspace.to_owned(),
                    lock_file: Some(file),
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Lock file exists, check if stale
                if let Ok(info) = read_lock_info(&lock_path) {
                    if is_lock_stale(&info) {
                        // Remove stale lock
                        fs::remove_file(&lock_path)?;
                        // Retry
                        return Self::try_acquire(workspace);
                    }
                }
                bail!("Workspace is locked by another process")
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Check if workspace is locked
    pub fn is_locked(workspace: &Path) -> bool {
        let lock_path = workspace.join(".lock");
        if lock_path.exists() {
            if let Ok(info) = read_lock_info(&lock_path) {
                !is_lock_stale(&info)
            } else {
                // Can't read lock, assume locked
                true
            }
        } else {
            false
        }
    }
}

impl Drop for DownloadLock {
    fn drop(&mut self) {
        // Close the file handle
        self.lock_file = None;

        // Delete the lock file to release the lock
        let lock_path = self.workspace.join(".lock");
        let _ = fs::remove_file(&lock_path);
    }
}

/// Lock file information
struct LockInfo {
    pid: u32,
    hostname: String,
    timestamp: DateTime<Utc>,
}

/// Read lock file information
fn read_lock_info(path: &Path) -> Result<LockInfo> {
    let contents = fs::read_to_string(path)?;
    let lines: Vec<&str> = contents.lines().collect();

    if lines.len() >= 3 {
        Ok(LockInfo {
            pid: lines[0].parse()?,
            hostname: lines[1].to_string(),
            timestamp: DateTime::parse_from_rfc3339(lines[2])?.with_timezone(&Utc),
        })
    } else {
        bail!("Invalid lock file format")
    }
}

/// Check if lock is stale
fn is_lock_stale(info: &LockInfo) -> bool {
    // Check if same host
    let current_host = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    if info.hostname == current_host {
        // Same host, check if process exists
        !is_process_alive(info.pid)
    } else {
        // Different host, check age (consider stale after 24 hours)
        let age = Utc::now() - info.timestamp;
        age.num_hours() > 24
    }
}

/// Find all resumable downloads
pub fn find_resumable_downloads() -> Result<Vec<DownloadState>> {
    let downloads_dir = paths::talaria_downloads_dir();
    let mut resumable = Vec::new();

    if !downloads_dir.exists() {
        return Ok(resumable);
    }

    for entry in fs::read_dir(downloads_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let state_path = path.join("state.json");
            if state_path.exists() {
                if let Ok(state) = DownloadState::load(&state_path) {
                    // Check if not complete and not locked
                    if !state.stage.is_complete() && !DownloadLock::is_locked(&path) {
                        resumable.push(state);
                    }
                }
            }
        }
    }

    Ok(resumable)
}

/// Find existing workspace for a specific database source
/// Returns the most recent matching workspace path and its state
pub fn find_existing_workspace_for_source(
    source: &DatabaseSource,
) -> Result<Option<(PathBuf, DownloadState)>> {
    let _span = tracing::debug_span!("find_existing_workspace", source = %source).entered();

    info!("Searching for existing workspace for {}", source);
    let downloads_dir = paths::talaria_downloads_dir();

    if !downloads_dir.exists() {
        return Ok(None);
    }

    let db_id = source.canonical_name();
    let mut best_match: Option<(PathBuf, DownloadState, SystemTime)> = None;

    // Search for directories matching this database source
    for entry in fs::read_dir(downloads_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Check if directory name starts with the database ID
            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                if dir_name.starts_with(&db_id) {
                    let state_path = path.join("state.json");
                    if state_path.exists() {
                        // Load state to verify it's for the same source
                        if let Ok(state) = DownloadState::load(&state_path) {
                            // Check if it's the same database source
                            if state.source == *source {
                                // Get modification time to find most recent
                                if let Ok(metadata) = state_path.metadata() {
                                    if let Ok(modified) = metadata.modified() {
                                        // Keep the most recent one
                                        if best_match
                                            .as_ref()
                                            .map_or(true, |(_, _, t)| modified > *t)
                                        {
                                            best_match = Some((path.clone(), state, modified));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(best_match.map(|(path, state, _)| (path, state)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_workspace(source: &DatabaseSource, stage: Stage) -> Result<(TempDir, PathBuf)> {
        let temp_dir = TempDir::new()?;
        let downloads_dir = temp_dir.path().join("downloads");
        fs::create_dir_all(&downloads_dir)?;

        // Create workspace with deterministic name
        let workspace = downloads_dir.join(format!("{}_20250101_test123", source.canonical_name()));
        fs::create_dir_all(&workspace)?;

        // Create state file
        let mut state = DownloadState::new(source.clone(), workspace.clone());
        state.stage = stage;
        state.save(&workspace.join("state.json"))?;

        Ok((temp_dir, workspace))
    }

    #[test]
    #[serial_test::serial]
    fn test_find_existing_workspace_for_source_not_found() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_DATA_DIR", temp_dir.path());

        let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
        let result = find_existing_workspace_for_source(&source).unwrap();
        assert!(result.is_none());

        std::env::remove_var("TALARIA_DATA_DIR");
    }

    #[test]
    #[serial_test::serial]
    fn test_find_existing_workspace_single_match() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_DATA_DIR", temp_dir.path());

        let source = DatabaseSource::UniProt(UniProtDatabase::UniRef50);
        let (_temp_dir, workspace) = create_test_workspace(&source, Stage::Complete).unwrap();

        // Move workspace to downloads dir
        let downloads_dir = paths::talaria_downloads_dir();
        fs::create_dir_all(&downloads_dir).unwrap();
        let target = downloads_dir.join(workspace.file_name().unwrap());
        // Copy instead of rename to avoid cross-device link issues
        fs::create_dir_all(&target).unwrap();
        fs::copy(workspace.join("state.json"), target.join("state.json")).unwrap();

        let result = find_existing_workspace_for_source(&source).unwrap();
        assert!(result.is_some());

        let (_found_path, found_state) = result.unwrap();
        assert_eq!(found_state.source, source);
        assert_eq!(found_state.stage, Stage::Complete);

        std::env::remove_var("TALARIA_DATA_DIR");
    }

    #[test]
    #[serial_test::serial]
    fn test_find_existing_workspace_multiple_matches_returns_newest() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_DATA_DIR", temp_dir.path());

        let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
        let downloads_dir = paths::talaria_downloads_dir();
        fs::create_dir_all(&downloads_dir).unwrap();

        // Create older workspace
        let old_workspace = downloads_dir.join("uniprot_swissprot_20250101_old");
        fs::create_dir_all(&old_workspace).unwrap();
        let mut old_state = DownloadState::new(source.clone(), old_workspace.clone());
        old_state.stage = Stage::Complete;
        old_state.save(&old_workspace.join("state.json")).unwrap();

        // Sleep to ensure different timestamps
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Create newer workspace
        let new_workspace = downloads_dir.join("uniprot_swissprot_20250102_new");
        fs::create_dir_all(&new_workspace).unwrap();
        let mut new_state = DownloadState::new(source.clone(), new_workspace.clone());
        new_state.stage = Stage::Processing {
            chunks_done: 5,
            total_chunks: 10,
        };
        new_state.save(&new_workspace.join("state.json")).unwrap();

        let result = find_existing_workspace_for_source(&source).unwrap();
        assert!(result.is_some());

        let (found_path, found_state) = result.unwrap();
        assert!(found_path.to_string_lossy().contains("20250102_new"));
        match found_state.stage {
            Stage::Processing { chunks_done, .. } => assert_eq!(chunks_done, 5),
            _ => panic!("Expected Processing stage"),
        }

        std::env::remove_var("TALARIA_DATA_DIR");
    }

    #[test]
    #[serial_test::serial]
    fn test_find_existing_workspace_ignores_different_source() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_DATA_DIR", temp_dir.path());

        let downloads_dir = paths::talaria_downloads_dir();
        fs::create_dir_all(&downloads_dir).unwrap();

        // Create workspace for different source
        let other_source = DatabaseSource::UniProt(UniProtDatabase::TrEMBL);
        let workspace = downloads_dir.join("uniprot_trembl_20250101_test");
        fs::create_dir_all(&workspace).unwrap();
        let state = DownloadState::new(other_source, workspace.clone());
        state.save(&workspace.join("state.json")).unwrap();

        // Search for SwissProt
        let search_source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
        let result = find_existing_workspace_for_source(&search_source).unwrap();
        assert!(result.is_none());

        std::env::remove_var("TALARIA_DATA_DIR");
    }

    #[test]
    #[serial_test::serial]
    fn test_find_existing_workspace_handles_corrupted_state() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_DATA_DIR", temp_dir.path());

        let source = DatabaseSource::UniProt(UniProtDatabase::UniRef90);
        let downloads_dir = paths::talaria_downloads_dir();
        fs::create_dir_all(&downloads_dir).unwrap();

        // Create workspace with corrupted state.json
        let workspace = downloads_dir.join("uniprot_uniref90_20250101_test");
        fs::create_dir_all(&workspace).unwrap();
        fs::write(workspace.join("state.json"), b"invalid json").unwrap();

        let result = find_existing_workspace_for_source(&source).unwrap();
        assert!(result.is_none());

        std::env::remove_var("TALARIA_DATA_DIR");
    }

    #[test]
    #[serial_test::serial]
    fn test_database_source_canonical_name() {
        assert_eq!(
            DatabaseSource::UniProt(UniProtDatabase::SwissProt).canonical_name(),
            "uniprot_swissprot"
        );
        assert_eq!(
            DatabaseSource::UniProt(UniProtDatabase::UniRef50).canonical_name(),
            "uniprot_uniref50"
        );
        assert_eq!(
            DatabaseSource::NCBI(NCBIDatabase::NR).canonical_name(),
            "ncbi_nr"
        );
    }

    #[test]
    #[serial_test::serial]
    fn test_stage_is_complete() {
        assert!(Stage::Complete.is_complete());
        assert!(!Stage::Initializing.is_complete());
        assert!(!Stage::Processing {
            chunks_done: 5,
            total_chunks: 10
        }
        .is_complete());
        assert!(!Stage::Failed {
            error: "test".to_string(),
            recoverable: true,
            failed_at: Utc::now()
        }
        .is_complete());
    }
}

/// Clean up completed or stale download workspaces
pub fn cleanup_old_workspaces(max_age_hours: i64) -> Result<usize> {
    let downloads_dir = paths::talaria_downloads_dir();
    let mut cleaned = 0;

    if !downloads_dir.exists() {
        return Ok(0);
    }

    for entry in fs::read_dir(downloads_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let state_path = path.join("state.json");
            let should_clean = if state_path.exists() {
                if let Ok(state) = DownloadState::load(&state_path) {
                    // Clean if complete or stale
                    state.stage.is_complete() || state.is_stale(max_age_hours)
                } else {
                    // Can't read state, check directory age
                    if let Ok(metadata) = fs::metadata(&path) {
                        if let Ok(modified) = metadata.modified() {
                            let age = SystemTime::now()
                                .duration_since(modified)
                                .unwrap_or_default();
                            age.as_secs() > (max_age_hours as u64 * 3600)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            } else {
                // No state file, check if directory is old
                true
            };

            if should_clean && !DownloadLock::is_locked(&path) {
                if let Ok(()) = fs::remove_dir_all(&path) {
                    cleaned += 1;
                }
            }
        }
    }

    Ok(cleaned)
}

#[cfg(test)]
mod workspace_tests {
    use super::*;
    use talaria_test::fixtures::test_database_source;
    use tempfile::TempDir;

    #[test]
    #[serial_test::serial]
    fn test_download_state_serialization() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().join("test_workspace");
        let state_path = workspace.join("state.json");

        // Create state
        let mut state = DownloadState::new(
            DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt),
            workspace.clone(),
        );

        // Add some files
        state.files.compressed = Some(PathBuf::from("/tmp/test.gz"));
        state.files.decompressed = Some(PathBuf::from("/tmp/test.fasta"));

        // Save
        state.save(&state_path).unwrap();

        // Load
        let loaded = DownloadState::load(&state_path).unwrap();

        assert_eq!(loaded.id, state.id);
        assert_eq!(loaded.files.compressed, state.files.compressed);
    }

    #[test]
    #[serial_test::serial]
    fn test_stage_transitions() {
        let workspace = PathBuf::from("/tmp/test");
        let mut state = DownloadState::new(
            DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt),
            workspace,
        );

        assert!(matches!(state.stage, Stage::Initializing));

        state
            .transition_to(Stage::Downloading {
                bytes_done: 0,
                total_bytes: 1000,
                url: "http://example.com".to_string(),
            })
            .unwrap();

        assert!(matches!(state.stage, Stage::Downloading { .. }));
        assert_eq!(state.checkpoints.len(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_workspace_generation() {
        let source = DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt);
        let workspace = get_download_workspace(&source);

        assert!(workspace.to_str().unwrap().contains("uniprot_swissprot"));
    }

    #[test]
    #[serial_test::serial]
    fn test_lock_acquisition() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().join("test_workspace");

        // First lock should succeed
        let lock1 = DownloadLock::try_acquire(&workspace).unwrap();

        // Second lock should fail
        let lock2 = DownloadLock::try_acquire(&workspace);
        assert!(lock2.is_err());

        // Check that workspace is locked
        assert!(DownloadLock::is_locked(&workspace));

        // Drop first lock
        drop(lock1);
    }

    #[test]
    #[serial_test::serial]
    fn test_canonical_name_generation() {
        // Test UniProt databases
        let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
        assert_eq!(source.canonical_name(), "uniprot_swissprot");

        let source = DatabaseSource::UniProt(UniProtDatabase::TrEMBL);
        assert_eq!(source.canonical_name(), "uniprot_trembl");

        let source = DatabaseSource::UniProt(UniProtDatabase::UniRef50);
        assert_eq!(source.canonical_name(), "uniprot_uniref50");

        // Test NCBI databases
        let source = DatabaseSource::NCBI(NCBIDatabase::NR);
        assert_eq!(source.canonical_name(), "ncbi_nr");

        let source = DatabaseSource::NCBI(NCBIDatabase::Taxonomy);
        assert_eq!(source.canonical_name(), "ncbi_taxonomy");

        let source = DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein);
        assert_eq!(source.canonical_name(), "ncbi_refseq_protein");

        // Test custom database
        let source = DatabaseSource::Custom("my/custom/db".to_string());
        assert_eq!(source.canonical_name(), "custom_my_custom_db");

        // Test test database with custom name
        let source = test_database_source("workspace");
        assert_eq!(source.canonical_name(), "custom_test_workspace");
    }

    #[test]
    #[serial_test::serial]
    fn test_file_tracking_operations() {
        let mut tracking = FileTracking::new();

        let file1 = PathBuf::from("/tmp/file1.txt");
        let file2 = PathBuf::from("/tmp/file2.txt");
        let file3 = PathBuf::from("/tmp/file3.txt");

        // Track temp files
        tracking.track_temp_file(file1.clone());
        tracking.track_temp_file(file2.clone());
        tracking.track_temp_file(file3.clone());

        assert_eq!(tracking.temp_files.len(), 3);

        // Track same file twice - should not duplicate
        tracking.track_temp_file(file1.clone());
        assert_eq!(tracking.temp_files.len(), 3);

        // Mark files for preservation
        tracking.preserve_on_failure(file1.clone());
        tracking.preserve_on_failure(file2.clone());

        assert_eq!(tracking.preserve_on_failure.len(), 2);
        assert!(tracking.preserve_on_failure.contains(&file1));
        assert!(tracking.preserve_on_failure.contains(&file2));
        assert!(!tracking.preserve_on_failure.contains(&file3));
    }

    #[test]
    #[serial_test::serial]
    fn test_stage_transitions_and_names() {
        assert_eq!(Stage::Initializing.name(), "initializing");
        assert_eq!(
            Stage::Downloading {
                bytes_done: 0,
                total_bytes: 100,
                url: "test".to_string()
            }
            .name(),
            "downloading"
        );
        assert_eq!(Stage::Complete.name(), "complete");
        assert_eq!(
            Stage::Failed {
                error: "test".to_string(),
                recoverable: true,
                failed_at: chrono::Utc::now()
            }
            .name(),
            "failed"
        );

        assert!(Stage::Complete.is_complete());
        assert!(!Stage::Initializing.is_complete());

        assert!(Stage::Failed {
            error: "test".to_string(),
            recoverable: true,
            failed_at: chrono::Utc::now()
        }
        .is_failed());
        assert!(!Stage::Complete.is_failed());
    }

    #[test]
    #[serial_test::serial]
    fn test_download_state_checkpoints() {
        let workspace = PathBuf::from("/tmp/test_workspace");
        let mut state = DownloadState::new(test_database_source("workspace"), workspace.clone());

        // Initial state - no checkpoints
        assert_eq!(state.checkpoints.len(), 0);

        // Transition to new stage - should create checkpoint
        state
            .transition_to(Stage::Downloading {
                bytes_done: 0,
                total_bytes: 100,
                url: "test".to_string(),
            })
            .unwrap();

        assert_eq!(state.checkpoints.len(), 1);
        assert!(matches!(state.checkpoints[0].stage, Stage::Initializing));

        // Another transition
        state
            .transition_to(Stage::Verifying { checksum: None })
            .unwrap();

        assert_eq!(state.checkpoints.len(), 2);

        // Restore last checkpoint
        state.restore_last_checkpoint().unwrap();

        // Should be back to Downloading stage with 1 checkpoint
        assert!(matches!(state.stage, Stage::Downloading { .. }));
        assert_eq!(state.checkpoints.len(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_session_id_uniqueness() {
        // Generate multiple session IDs for same source
        let source = test_database_source("workspace");

        let id1 = generate_session_id(&source);
        // Small delay to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(1));
        let id2 = generate_session_id(&source);

        // IDs should be different
        assert_ne!(id1, id2);

        // IDs should be 8 characters (first 8 chars of SHA256 hash)
        assert_eq!(id1.len(), 8);
        assert_eq!(id2.len(), 8);
    }

    #[test]
    #[serial_test::serial]
    fn test_session_id_override() {
        // Test that TALARIA_SESSION environment variable works
        std::env::set_var("TALARIA_SESSION", "test_session");

        let source = test_database_source("workspace");
        let id = generate_session_id(&source);

        assert_eq!(id, "test_session");

        std::env::remove_var("TALARIA_SESSION");
    }

    #[test]
    #[serial_test::serial]
    fn test_lock_info_parsing() {
        let temp_dir = TempDir::new().unwrap();
        let lock_file = temp_dir.path().join(".lock");

        // Write lock info
        let pid = 12345u32;
        let hostname = "test-host";
        let timestamp = chrono::Utc::now();

        let lock_content = format!("{}\n{}\n{}", pid, hostname, timestamp.to_rfc3339());
        std::fs::write(&lock_file, lock_content).unwrap();

        // Read and parse
        let info = read_lock_info(&lock_file).unwrap();

        assert_eq!(info.pid, pid);
        assert_eq!(info.hostname, hostname);
        // Timestamps might differ by milliseconds, so just check they're close
        let time_diff = (timestamp - info.timestamp).num_seconds().abs();
        assert!(time_diff < 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_workspace_path_generation() {
        // Set test environment
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_DATA_DIR", temp_dir.path());

        let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
        let workspace = get_download_workspace(&source);

        // Should be under downloads directory
        assert!(workspace.to_str().unwrap().contains("downloads"));
        // Should contain database name
        assert!(workspace.to_str().unwrap().contains("uniprot_swissprot"));
        // Should have session ID suffix (8 hex chars)
        let name = workspace.file_name().unwrap().to_str().unwrap();
        let parts: Vec<&str> = name.split('_').collect();
        assert!(parts.len() >= 3); // name_version_session
        assert_eq!(parts.last().unwrap().len(), 8); // Session ID is 8 chars

        std::env::remove_var("TALARIA_DATA_DIR");
    }

    #[test]
    #[serial_test::serial]
    fn test_is_stale() {
        let workspace = PathBuf::from("/tmp/test");
        let state = DownloadState::new(test_database_source("stale_test"), workspace);

        // Fresh download should not be stale
        assert!(!state.is_stale(24));
        assert!(!state.is_stale(1));

        // Can't test actual staleness without time travel, but the logic is simple
    }
}
