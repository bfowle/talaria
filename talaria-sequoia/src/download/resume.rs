/// Resume support for downloads
use crate::types::SHA256Hash;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Download resume metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadResumeState {
    /// URL being downloaded
    pub url: String,
    /// Final output path
    pub output_path: PathBuf,
    /// Temporary file path for partial download
    pub temp_path: PathBuf,
    /// Bytes already downloaded
    pub bytes_downloaded: u64,
    /// Total size if known
    pub total_size: Option<u64>,
    /// ETag from server for validation
    pub etag: Option<String>,
    /// Last modified timestamp from server
    pub last_modified: Option<String>,
    /// SHA256 hash of downloaded content so far (for validation)
    pub partial_hash: Option<SHA256Hash>,
}

impl DownloadResumeState {
    /// Create new download resume state
    pub fn new(url: String, output_path: PathBuf) -> Self {
        let temp_path = output_path.with_extension("download.tmp");
        Self {
            url,
            output_path: output_path.clone(),
            temp_path,
            bytes_downloaded: 0,
            total_size: None,
            etag: None,
            last_modified: None,
            partial_hash: None,
        }
    }

    /// Check if we can resume from existing partial file
    pub fn can_resume(&self) -> bool {
        self.temp_path.exists() && self.bytes_downloaded > 0
    }

    /// Validate that the partial file matches our state
    pub fn validate_partial_file(&self) -> Result<bool> {
        if !self.temp_path.exists() {
            return Ok(false);
        }

        let metadata = std::fs::metadata(&self.temp_path)?;
        let file_size = metadata.len();

        // Check if file size matches our recorded progress
        if file_size != self.bytes_downloaded {
            tracing::warn!(
                "Partial file size {} doesn't match recorded progress {}",
                file_size,
                self.bytes_downloaded
            );
            return Ok(false);
        }

        // If we have a partial hash, validate it
        if let Some(expected_hash) = &self.partial_hash {
            let actual_hash = Self::compute_file_hash(&self.temp_path, Some(file_size))?;
            if actual_hash != *expected_hash {
                tracing::warn!("Partial file hash mismatch, cannot resume");
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Update state after downloading a chunk
    pub fn update_progress(&mut self, bytes: u64) {
        self.bytes_downloaded += bytes;
    }

    /// Set server metadata for validation
    pub fn set_server_metadata(&mut self, etag: Option<String>, last_modified: Option<String>) {
        self.etag = etag;
        self.last_modified = last_modified;
    }

    /// Compute hash of partial file for validation
    pub fn compute_partial_hash(&mut self) -> Result<()> {
        if self.temp_path.exists() {
            self.partial_hash = Some(Self::compute_file_hash(&self.temp_path, None)?);
        }
        Ok(())
    }

    /// Helper to compute file hash
    fn compute_file_hash(path: &Path, limit: Option<u64>) -> Result<SHA256Hash> {
        use sha2::{Digest, Sha256};
        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 8192];
        let mut total_read = 0u64;

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            let bytes_to_hash = if let Some(limit) = limit {
                let remaining = limit.saturating_sub(total_read);
                std::cmp::min(bytes_read, remaining as usize)
            } else {
                bytes_read
            };

            hasher.update(&buffer[..bytes_to_hash]);
            total_read += bytes_to_hash as u64;

            if let Some(limit) = limit {
                if total_read >= limit {
                    break;
                }
            }
        }

        SHA256Hash::from_bytes(hasher.finalize().as_slice())
    }

    /// Save state to disk
    pub fn save(&self, state_path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(state_path, json).context("Failed to save download state")?;
        Ok(())
    }

    /// Load state from disk
    pub fn load(state_path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(state_path).context("Failed to read download state")?;
        let state = serde_json::from_str(&json).context("Failed to parse download state")?;
        Ok(state)
    }

    /// Clean up temporary files and state
    pub fn cleanup(&self, state_path: &Path) -> Result<()> {
        if self.temp_path.exists() {
            std::fs::remove_file(&self.temp_path).ok();
        }
        if state_path.exists() {
            std::fs::remove_file(state_path).ok();
        }
        Ok(())
    }
}

/// Extension trait for reqwest to support resume
#[allow(async_fn_in_trait)]
pub trait ResumeDownload {
    /// Check if server supports byte-range requests
    async fn supports_resume(&self, url: &str) -> Result<bool>;

    /// Get file metadata without downloading
    async fn get_file_metadata(
        &self,
        url: &str,
    ) -> Result<(Option<u64>, Option<String>, Option<String>)>;
}

impl ResumeDownload for reqwest::Client {
    async fn supports_resume(&self, url: &str) -> Result<bool> {
        let response = self
            .head(url)
            .send()
            .await
            .context("Failed to check resume support")?;

        // Check for Accept-Ranges header
        if let Some(accept_ranges) = response.headers().get("accept-ranges") {
            if let Ok(value) = accept_ranges.to_str() {
                return Ok(value.contains("bytes"));
            }
        }

        // Some servers don't advertise but still support it, try a small range request
        let test_response = self
            .get(url)
            .header("Range", "bytes=0-0")
            .send()
            .await
            .context("Failed to test range request")?;

        Ok(test_response.status() == reqwest::StatusCode::PARTIAL_CONTENT)
    }

    async fn get_file_metadata(
        &self,
        url: &str,
    ) -> Result<(Option<u64>, Option<String>, Option<String>)> {
        let response = self
            .head(url)
            .send()
            .await
            .context("Failed to get file metadata")?;

        let size = response.content_length();

        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let last_modified = response
            .headers()
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        Ok((size, etag, last_modified))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_resume_state_creation() {
        let output_path = PathBuf::from("/tmp/test.fasta");
        let state = DownloadResumeState::new(
            "https://example.com/file.gz".to_string(),
            output_path.clone(),
        );

        assert_eq!(state.url, "https://example.com/file.gz");
        assert_eq!(state.output_path, output_path);
        assert_eq!(state.temp_path, PathBuf::from("/tmp/test.download.tmp"));
        assert_eq!(state.bytes_downloaded, 0);
        assert!(!state.can_resume());
    }

    #[test]
    fn test_resume_state_persistence() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let state_path = temp_dir.path().join("download.state");
        let output_path = temp_dir.path().join("test.fasta");

        let mut state =
            DownloadResumeState::new("https://example.com/file.gz".to_string(), output_path);
        state.bytes_downloaded = 12345;
        state.total_size = Some(100000);
        state.etag = Some("abc123".to_string());

        // Save and reload
        state.save(&state_path)?;
        let loaded_state = DownloadResumeState::load(&state_path)?;

        assert_eq!(loaded_state.bytes_downloaded, 12345);
        assert_eq!(loaded_state.total_size, Some(100000));
        assert_eq!(loaded_state.etag, Some("abc123".to_string()));

        Ok(())
    }

    #[test]
    fn test_partial_file_validation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let output_path = temp_dir.path().join("test.fasta");
        let temp_path = temp_dir.path().join("test.download.tmp");

        // Create a partial file
        std::fs::write(&temp_path, b"partial content")?;

        let mut state =
            DownloadResumeState::new("https://example.com/file.gz".to_string(), output_path);
        state.temp_path = temp_path;
        state.bytes_downloaded = 15; // matches "partial content" length

        assert!(state.validate_partial_file()?);

        // Now with mismatched size
        state.bytes_downloaded = 100;
        assert!(!state.validate_partial_file()?);

        Ok(())
    }
}
