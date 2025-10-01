use super::progress::DownloadProgress;
/// Resumable downloader that integrates with ProcessingState
use super::resume::{DownloadResumeState, ResumeDownload};
use crate::storage::SequoiaStorage;
use crate::types::SHA256Hash;
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use reqwest::Client;
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, Write};
use std::path::Path;

pub struct ResumableDownloader {
    client: Client,
    storage: Option<SequoiaStorage>,
    operation_id: Option<String>,
}

impl ResumableDownloader {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            storage: None,
            operation_id: None,
        }
    }

    pub fn with_storage(mut self, storage: SequoiaStorage, operation_id: String) -> Self {
        self.storage = Some(storage);
        self.operation_id = Some(operation_id);
        self
    }

    /// Download with full resume support
    pub async fn download_with_resume(
        &self,
        url: &str,
        output_path: &Path,
        progress: &mut DownloadProgress,
        resume: bool,
    ) -> Result<()> {
        // Create state directory for resume metadata
        let state_dir = output_path.parent().unwrap_or(Path::new("."));
        let state_path = state_dir.join(format!(
            ".{}.resume",
            output_path.file_name().unwrap().to_string_lossy()
        ));

        // Try to load existing state if resume is enabled
        let mut resume_state = if resume && state_path.exists() {
            match DownloadResumeState::load(&state_path) {
                Ok(state) if state.validate_partial_file()? => {
                    progress.set_message(&format!(
                        "Resuming download from {} MB",
                        state.bytes_downloaded / (1024 * 1024)
                    ));
                    state
                }
                _ => {
                    // State invalid, start fresh
                    progress.set_message("Previous download state invalid, starting fresh");
                    let state =
                        DownloadResumeState::new(url.to_string(), output_path.to_path_buf());
                    // Clean up any partial files
                    if state.temp_path.exists() {
                        std::fs::remove_file(&state.temp_path).ok();
                    }
                    state
                }
            }
        } else {
            DownloadResumeState::new(url.to_string(), output_path.to_path_buf())
        };

        // Check server support for resume
        let supports_resume = self.client.supports_resume(url).await?;
        if resume && !supports_resume {
            progress.set_message("Server doesn't support resume, starting from beginning");
            resume_state.bytes_downloaded = 0;
            if resume_state.temp_path.exists() {
                std::fs::remove_file(&resume_state.temp_path).ok();
            }
        }

        // Get file metadata
        let (total_size, etag, last_modified) = self.client.get_file_metadata(url).await?;

        // Check if server file has changed (ETag mismatch)
        if let Some(new_etag) = &etag {
            if let Some(old_etag) = &resume_state.etag {
                if new_etag != old_etag {
                    progress.set_message("Server file has changed, starting fresh download");
                    resume_state.bytes_downloaded = 0;
                    if resume_state.temp_path.exists() {
                        std::fs::remove_file(&resume_state.temp_path).ok();
                    }
                }
            }
        }

        resume_state.set_server_metadata(etag, last_modified);
        resume_state.total_size = total_size;

        // Perform the download
        let result = self
            .download_internal(&mut resume_state, progress, supports_resume)
            .await;

        match result {
            Ok(()) => {
                // Download complete, clean up state
                resume_state.cleanup(&state_path)?;

                // Move temp file to final location
                if resume_state.temp_path.exists() {
                    std::fs::rename(&resume_state.temp_path, output_path)
                        .context("Failed to move file to final location")?;
                }

                // Update processing state if we have storage
                if let Some(storage) = &self.storage {
                    storage.complete_processing()?;
                }

                Ok(())
            }
            Err(e) => {
                // Save state for resume
                resume_state.save(&state_path)?;

                // Update processing state if we have storage
                if let Some(storage) = &self.storage {
                    let completed_chunks = vec![SHA256Hash::compute(
                        &resume_state.bytes_downloaded.to_le_bytes(),
                    )];
                    storage.update_processing_state(&completed_chunks)?;
                }

                Err(e)
            }
        }
    }

    async fn download_internal(
        &self,
        state: &mut DownloadResumeState,
        progress: &mut DownloadProgress,
        supports_resume: bool,
    ) -> Result<()> {
        // Build request with range header if resuming
        let mut request = self.client.get(&state.url);
        if state.bytes_downloaded > 0 && supports_resume {
            request = request.header("Range", format!("bytes={}-", state.bytes_downloaded));
            progress.set_message(&format!("Resuming from byte {}", state.bytes_downloaded));
        }

        let response = request.send().await.context("Failed to start download")?;

        // Check response status
        if state.bytes_downloaded > 0
            && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
            && supports_resume
        {
            // Server doesn't actually support resume despite headers
            return Err(anyhow::anyhow!(
                "Server reported resume support but didn't return partial content"
            ));
        }

        let content_length = response.content_length().unwrap_or(0);
        let total_size = if state.bytes_downloaded > 0 && supports_resume {
            state.bytes_downloaded + content_length
        } else {
            content_length
        };

        progress.set_total(total_size as usize);
        progress.set_current(state.bytes_downloaded as usize);

        // Open file for writing
        let mut file = if state.bytes_downloaded > 0 && supports_resume {
            OpenOptions::new()
                .append(true)
                .open(&state.temp_path)
                .context("Failed to open file for resume")?
        } else {
            File::create(&state.temp_path).context("Failed to create temporary file")?
        };

        // Download with progress tracking
        let mut stream = response.bytes_stream();
        let mut consecutive_errors = 0;
        const MAX_CONSECUTIVE_ERRORS: u32 = 3;

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(chunk) => {
                    consecutive_errors = 0;
                    chunk
                }
                Err(e) => {
                    consecutive_errors += 1;
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        // Save current progress before failing
                        file.flush()?;
                        state.compute_partial_hash()?;
                        return Err(anyhow::anyhow!(
                            "Download interrupted after {} consecutive errors: {}. Progress saved for resume.",
                            consecutive_errors, e
                        ));
                    }
                    continue;
                }
            };

            file.write_all(&chunk).context("Failed to write chunk")?;
            state.update_progress(chunk.len() as u64);
            progress.set_current(state.bytes_downloaded as usize);

            // Periodically flush to disk and update state
            if state.bytes_downloaded % (10 * 1024 * 1024) == 0 {
                // Every 10MB
                file.flush()?;

                // Update processing state if we have storage
                if let Some(storage) = &self.storage {
                    let chunk_hash = SHA256Hash::compute(&state.bytes_downloaded.to_le_bytes());
                    storage.update_processing_state(&[chunk_hash]).ok();
                }
            }
        }

        // Final flush
        file.flush()?;
        progress.set_message("Download complete!");

        Ok(())
    }

    /// Download and extract compressed file with resume support
    pub async fn download_and_extract_with_resume(
        &self,
        url: &str,
        output_path: &Path,
        progress: &mut DownloadProgress,
        resume: bool,
    ) -> Result<()> {
        let temp_compressed = output_path.with_extension("gz.tmp");

        // First download the compressed file
        self.download_with_resume(url, &temp_compressed, progress, resume)
            .await?;

        progress.set_message("Decompressing file...");

        // Then decompress it
        let gz_file = File::open(&temp_compressed).context("Failed to open compressed file")?;
        let mut decoder = GzDecoder::new(BufReader::new(gz_file));
        let mut output_file = File::create(output_path).context("Failed to create output file")?;

        io::copy(&mut decoder, &mut output_file).context("Failed to decompress file")?;

        // Clean up temporary file
        std::fs::remove_file(&temp_compressed).context("Failed to remove temporary file")?;

        progress.set_message("Extraction complete!");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Simple unit tests without mock server
    #[test]
    #[serial_test::serial]
    fn test_resumable_downloader_creation() {
        let client = reqwest::Client::new();
        let downloader = ResumableDownloader::new(client);
        assert!(downloader.storage.is_none());
        assert!(downloader.operation_id.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn test_resumable_downloader_with_storage() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let client = reqwest::Client::new();
        let storage = crate::storage::SequoiaStorage::new(temp_dir.path())?;

        let downloader =
            ResumableDownloader::new(client).with_storage(storage, "test-op".to_string());

        assert!(downloader.storage.is_some());
        assert_eq!(downloader.operation_id, Some("test-op".to_string()));
        Ok(())
    }

    // Test the download state management without actual network calls
    #[tokio::test]
    #[serial_test::serial]
    async fn test_download_state_management() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let output_path = temp_dir.path().join("test.txt");
        let state_path = temp_dir.path().join(".test.txt.resume");

        // Create a resume state
        let mut resume_state = DownloadResumeState::new(
            "https://example.com/file.txt".to_string(),
            output_path.clone(),
        );
        resume_state.temp_path = temp_dir.path().join("test.txt.download.tmp");
        resume_state.bytes_downloaded = 1024;
        resume_state.total_size = Some(2048);
        resume_state.etag = Some("\"test-etag\"".to_string());

        // Save and verify state
        resume_state.save(&state_path)?;
        assert!(state_path.exists());

        // Load and verify
        let loaded_state = DownloadResumeState::load(&state_path)?;
        assert_eq!(loaded_state.bytes_downloaded, 1024);
        assert_eq!(loaded_state.total_size, Some(2048));
        assert_eq!(loaded_state.etag, Some("\"test-etag\"".to_string()));

        // Cleanup
        loaded_state.cleanup(&state_path)?;
        assert!(!state_path.exists());

        Ok(())
    }
}
