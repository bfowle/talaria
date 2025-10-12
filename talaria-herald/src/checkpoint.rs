/// Checkpoint system for resumable chunking operations
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingCheckpoint {
    /// Database source being processed
    pub database_source: String,

    /// Version string for this operation (set once at start)
    pub version: Option<String>,

    /// Number of sequences processed so far
    pub sequences_processed: usize,

    /// Byte offset in the input file
    pub file_offset: u64,

    /// Last sequence ID processed
    pub last_sequence_id: Option<String>,

    /// Timestamp of last update
    pub last_updated: DateTime<Utc>,

    /// Total file size for progress calculation
    pub total_file_size: u64,

    /// Performance metrics
    pub metrics: PerformanceMetrics,

    /// Batch-level tracking for streaming processing
    pub batch_info: Option<BatchCheckpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCheckpoint {
    /// Current batch number being processed
    pub current_batch: usize,

    /// Total batches expected (if known)
    pub total_batches: Option<usize>,

    /// Completed batch numbers
    pub completed_batches: Vec<usize>,

    /// Path to partial manifest directory
    pub manifest_dir: PathBuf,
}

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

impl ChunkingCheckpoint {
    /// Create a new checkpoint
    pub fn new(database_source: String, total_file_size: u64) -> Self {
        Self {
            database_source,
            version: None,
            sequences_processed: 0,
            file_offset: 0,
            last_sequence_id: None,
            last_updated: Utc::now(),
            total_file_size,
            metrics: PerformanceMetrics {
                sequences_per_second: 0.0,
                bytes_per_second: 0.0,
                elapsed_seconds: 0.0,
                estimated_remaining_seconds: 0.0,
            },
            batch_info: None,
        }
    }

    /// Update checkpoint with progress
    pub fn update(
        &mut self,
        sequences_processed: usize,
        file_offset: u64,
        last_sequence_id: Option<String>,
    ) {
        let now = Utc::now();
        let elapsed = (now - self.last_updated).num_seconds() as f64;

        if elapsed > 0.0 {
            // Calculate performance metrics
            let seq_delta = (sequences_processed - self.sequences_processed) as f64;
            let byte_delta = (file_offset - self.file_offset) as f64;

            self.metrics.sequences_per_second = seq_delta / elapsed;
            self.metrics.bytes_per_second = byte_delta / elapsed;
            self.metrics.elapsed_seconds += elapsed;

            // Estimate remaining time
            if self.metrics.bytes_per_second > 0.0 {
                let bytes_remaining = (self.total_file_size - file_offset) as f64;
                self.metrics.estimated_remaining_seconds =
                    bytes_remaining / self.metrics.bytes_per_second;
            }
        }

        self.sequences_processed = sequences_processed;
        self.file_offset = file_offset;
        self.last_sequence_id = last_sequence_id;
        self.last_updated = now;
    }

    /// Update batch completion status
    pub fn update_batch_completion(&mut self, batch_num: usize) {
        if self.batch_info.is_none() {
            let manifest_dir = Path::new(
                &std::env::var("TALARIA_HOME")
                    .unwrap_or_else(|_| std::env::var("HOME").unwrap_or_else(|_| ".".to_string())),
            )
            .join(".talaria")
            .join("databases")
            .join("partials")
            .join(&self.database_source);

            self.batch_info = Some(BatchCheckpoint {
                current_batch: batch_num,
                total_batches: None,
                completed_batches: vec![],
                manifest_dir,
            });
        }

        if let Some(ref mut batch_info) = self.batch_info {
            batch_info.current_batch = batch_num;
            if !batch_info.completed_batches.contains(&batch_num) {
                batch_info.completed_batches.push(batch_num);
            }
        }
    }

    /// Save checkpoint immediately (for batch completion)
    pub fn save_after_batch(&self) -> Result<()> {
        self.save()?;
        tracing::debug!(
            "Checkpoint saved after batch completion: {} sequences processed",
            self.sequences_processed
        );
        Ok(())
    }

    /// Get checkpoint file path
    pub fn get_checkpoint_path(database_source: &str) -> PathBuf {
        let home_dir = std::env::var("TALARIA_HOME")
            .unwrap_or_else(|_| std::env::var("HOME").unwrap_or_else(|_| ".".to_string()));

        Path::new(&home_dir)
            .join(".talaria")
            .join("checkpoints")
            .join(format!(
                "{}_chunking.json",
                database_source.replace('/', "_")
            ))
    }

    /// Save checkpoint to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::get_checkpoint_path(&self.database_source);

        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;

        Ok(())
    }

    /// Load checkpoint from disk
    pub fn load(database_source: &str) -> Result<Option<Self>> {
        let path = Self::get_checkpoint_path(database_source);

        if !path.exists() {
            return Ok(None);
        }

        let json = fs::read_to_string(path)?;
        let checkpoint: Self = serde_json::from_str(&json)?;

        Ok(Some(checkpoint))
    }

    /// Delete checkpoint (when chunking is complete)
    pub fn delete(&self) -> Result<()> {
        let path = Self::get_checkpoint_path(&self.database_source);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Check if should save (every 500k sequences)
    pub fn should_save(&self, current_sequences: usize) -> bool {
        const CHECKPOINT_INTERVAL: usize = 500_000;
        current_sequences / CHECKPOINT_INTERVAL > self.sequences_processed / CHECKPOINT_INTERVAL
    }

    /// Format progress message
    pub fn format_progress(&self) -> String {
        use talaria_utils::display::format_bytes;
        use talaria_utils::display::output::format_number;

        let seq_per_sec = format_number(self.metrics.sequences_per_second as usize);
        let bytes_per_sec = format_bytes(self.metrics.bytes_per_second as u64);
        let remaining = if self.metrics.estimated_remaining_seconds < 86400.0 {
            format!("{:.0}s", self.metrics.estimated_remaining_seconds)
        } else if self.metrics.estimated_remaining_seconds < 86400.0 * 30.0 {
            format!("{:.1}d", self.metrics.estimated_remaining_seconds / 86400.0)
        } else {
            format!(
                "{:.1}mo",
                self.metrics.estimated_remaining_seconds / (86400.0 * 30.0)
            )
        };

        format!(
            "{} seq/s | {}/s | ETA: {}",
            seq_per_sec, bytes_per_sec, remaining
        )
    }

    /// Initialize batch checkpoint info
    pub fn init_batch_checkpoint(&mut self, manifest_dir: PathBuf) {
        self.batch_info = Some(BatchCheckpoint {
            current_batch: 0,
            total_batches: None,
            completed_batches: Vec::new(),
            manifest_dir,
        });
    }

    /// Mark a batch as complete
    pub fn complete_batch(&mut self, batch_num: usize) {
        if let Some(ref mut batch_info) = self.batch_info {
            batch_info.completed_batches.push(batch_num);
            batch_info.current_batch = batch_num + 1;
        }
    }

    /// Check if a batch was already processed
    pub fn is_batch_complete(&self, batch_num: usize) -> bool {
        if let Some(ref batch_info) = self.batch_info {
            batch_info.completed_batches.contains(&batch_num)
        } else {
            false
        }
    }

    /// Get path for batch manifest
    pub fn get_batch_manifest_path(&self, batch_num: usize) -> Option<PathBuf> {
        if let Some(ref batch_info) = self.batch_info {
            Some(
                batch_info
                    .manifest_dir
                    .join(format!("batch_{:06}.json", batch_num)),
            )
        } else {
            None
        }
    }
}
