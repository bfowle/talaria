/// Remote chunk storage client
/// Supports downloading chunks from S3, GCS, Azure Blob Storage, and HTTP(S)
use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use std::env;
use std::time::Duration;
use tokio::time::sleep;
use url::Url;

use crate::SHA256Hash;
use serde::{Deserialize, Serialize};

/// Remote manifest structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteManifest {
    pub chunks: Vec<SHA256Hash>,
    pub version: String,
    pub timestamp: i64,
}

/// Supported storage protocols
#[derive(Debug, Clone, PartialEq)]
pub enum Protocol {
    S3,
    GCS,
    Azure,
    Http,
    Https,
    File, // Local filesystem for testing
}

impl Protocol {
    fn from_url(url: &str) -> Result<Self> {
        let parsed = Url::parse(url)?;
        match parsed.scheme() {
            "s3" => Ok(Protocol::S3),
            "gs" => Ok(Protocol::GCS),
            "az" | "azure" => Ok(Protocol::Azure),
            "http" => Ok(Protocol::Http),
            "https" => Ok(Protocol::Https),
            "file" => Ok(Protocol::File),
            scheme => bail!("Unsupported protocol: {}", scheme),
        }
    }
}

/// Error types for chunk downloading
#[derive(Debug, thiserror::Error)]
pub enum ChunkDownloadError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("Chunk not found: {0}")]
    NotFound(SHA256Hash),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Rate limited, retry after {0} seconds")]
    RateLimited(u64),

    #[error("Invalid chunk data")]
    InvalidData,

    #[error("Protocol not supported: {0:?}")]
    UnsupportedProtocol(Protocol),
}

/// Client for downloading chunks from remote storage
pub struct ChunkClient {
    base_url: String,
    protocol: Protocol,
    client: Client,
    max_retries: usize,
    retry_delay: Duration,
}

impl ChunkClient {
    /// Create a new chunk client from environment variable or explicit URL
    pub fn new(base_url: Option<String>) -> Result<Self> {
        let url = base_url
            .or_else(|| env::var("TALARIA_CHUNK_SERVER").ok())
            .ok_or_else(|| {
                anyhow!("No chunk server configured. Set TALARIA_CHUNK_SERVER or provide URL")
            })?;

        let protocol = Protocol::from_url(&url)?;

        let client = Client::builder()
            .timeout(Duration::from_secs(300)) // 5 minute timeout for large chunks
            .connect_timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            base_url: url,
            protocol,
            client,
            max_retries: 3,
            retry_delay: Duration::from_secs(2),
        })
    }

    /// Download a single chunk by hash
    pub async fn download_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>> {
        let url = self.build_chunk_url(hash)?;

        for attempt in 0..self.max_retries {
            match self.download_with_protocol(&url).await {
                Ok(data) => {
                    // Verify the downloaded data matches the expected hash
                    let computed_hash = SHA256Hash::compute(&data);
                    if computed_hash != *hash {
                        return Err(ChunkDownloadError::InvalidData.into());
                    }
                    return Ok(data);
                }
                Err(e) if attempt < self.max_retries - 1 => {
                    tracing::warn!("Download attempt {} failed: {}", attempt + 1, e);

                    // Check if we should retry
                    if let Some(delay) = self.should_retry(&e) {
                        sleep(delay).await;
                        continue;
                    }
                    return Err(e);
                }
                Err(e) => return Err(e),
            }
        }

        bail!(
            "Failed to download chunk after {} attempts",
            self.max_retries
        )
    }

    /// Download multiple chunks in parallel
    pub async fn download_chunks(
        &self,
        hashes: &[SHA256Hash],
        parallel: usize,
    ) -> Result<Vec<(SHA256Hash, Vec<u8>)>> {
        use futures::stream::{self, StreamExt};

        let results: Vec<Result<(SHA256Hash, Vec<u8>)>> = stream::iter(hashes.iter())
            .map(|hash| async move {
                let data = self.download_chunk(hash).await?;
                Ok((hash.clone(), data))
            })
            .buffer_unordered(parallel)
            .collect()
            .await;

        // Collect successful downloads and report errors
        let mut downloaded = Vec::new();
        let mut errors = Vec::new();

        for result in results {
            match result {
                Ok(chunk) => downloaded.push(chunk),
                Err(e) => errors.push(e),
            }
        }

        if !errors.is_empty() {
            tracing::error!("Failed to download {} chunks", errors.len());
            for error in &errors {
                tracing::error!("  {}", error);
            }
            bail!(
                "Failed to download {} out of {} chunks",
                errors.len(),
                hashes.len()
            );
        }

        Ok(downloaded)
    }

    /// Build URL for a specific chunk
    fn build_chunk_url(&self, hash: &SHA256Hash) -> Result<String> {
        let hash_str = hash.to_string();

        // Use first 2 chars as prefix for sharding (like git)
        let prefix = &hash_str[..2];
        let suffix = &hash_str[2..];

        match self.protocol {
            Protocol::S3 => {
                // s3://bucket/path -> https://bucket.s3.amazonaws.com/path
                let url = self.base_url.replace("s3://", "");
                let parts: Vec<&str> = url.splitn(2, '/').collect();
                if parts.len() != 2 {
                    bail!("Invalid S3 URL format");
                }
                let bucket = parts[0];
                let path = parts[1];
                Ok(format!(
                    "https://{}.s3.amazonaws.com/{}/chunks/{}/{}",
                    bucket, path, prefix, suffix
                ))
            }
            Protocol::GCS => {
                // gs://bucket/path -> https://storage.googleapis.com/bucket/path
                let url = self.base_url.replace("gs://", "");
                Ok(format!(
                    "https://storage.googleapis.com/{}/chunks/{}/{}",
                    url, prefix, suffix
                ))
            }
            Protocol::Azure => {
                // azure://account.blob.core.windows.net/container/path
                let url = self.base_url.replace("azure://", "https://");
                Ok(format!("{}/chunks/{}/{}", url, prefix, suffix))
            }
            Protocol::Http | Protocol::Https => {
                // Direct HTTP(S) URL
                Ok(format!("{}/chunks/{}/{}", self.base_url, prefix, suffix))
            }
            Protocol::File => {
                // Local filesystem (for testing)
                let path = self.base_url.replace("file://", "");
                Ok(format!("{}/chunks/{}/{}", path, prefix, suffix))
            }
        }
    }

    /// Download using protocol-specific method
    async fn download_with_protocol(&self, url: &str) -> Result<Vec<u8>> {
        match self.protocol {
            Protocol::File => {
                // Local file access
                let path = url.replace("file://", "");
                std::fs::read(&path).with_context(|| format!("Failed to read chunk from {}", path))
            }
            _ => {
                // HTTP-based protocols
                let response = self
                    .client
                    .get(url)
                    .send()
                    .await
                    .with_context(|| format!("Failed to download from {}", url))?;

                if response.status() == reqwest::StatusCode::NOT_FOUND {
                    let hash_part = url.split('/').last().unwrap_or("unknown");
                    return Err(ChunkDownloadError::NotFound(
                        SHA256Hash::from_hex(hash_part).unwrap_or_default(),
                    )
                    .into());
                }

                if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    if let Some(retry_after) = response.headers().get("Retry-After") {
                        if let Ok(seconds) = retry_after.to_str()?.parse::<u64>() {
                            return Err(ChunkDownloadError::RateLimited(seconds).into());
                        }
                    }
                    return Err(ChunkDownloadError::RateLimited(60).into());
                }

                response
                    .error_for_status()?
                    .bytes()
                    .await
                    .map(|b| b.to_vec())
                    .with_context(|| "Failed to read response body")
            }
        }
    }

    /// Determine if we should retry based on error type
    fn should_retry(&self, error: &anyhow::Error) -> Option<Duration> {
        if let Some(download_error) = error.downcast_ref::<ChunkDownloadError>() {
            match download_error {
                ChunkDownloadError::Network(_) => Some(self.retry_delay),
                ChunkDownloadError::RateLimited(seconds) => Some(Duration::from_secs(*seconds)),
                _ => None,
            }
        } else if let Some(req_error) = error.downcast_ref::<reqwest::Error>() {
            if req_error.is_timeout() || req_error.is_connect() {
                Some(self.retry_delay)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Check if chunk server is configured
    pub fn is_configured() -> bool {
        env::var("TALARIA_CHUNK_SERVER").is_ok()
    }

    /// Upload a chunk to remote repository
    pub async fn upload_chunk(&self, hash: &SHA256Hash, data: &[u8]) -> Result<()> {
        let url = self.build_chunk_url(hash)?;

        for attempt in 0..self.max_retries {
            let response = self.client.put(&url).body(data.to_vec()).send().await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    return Ok(());
                }
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    if attempt < self.max_retries - 1 {
                        tracing::warn!(
                            "Upload attempt {} failed with status {}: {}",
                            attempt + 1,
                            status,
                            body
                        );
                        sleep(self.retry_delay).await;
                        continue;
                    }
                    bail!("Failed to upload chunk: {} - {}", status, body);
                }
                Err(e) if attempt < self.max_retries - 1 => {
                    tracing::warn!("Upload attempt {} failed: {}", attempt + 1, e);
                    sleep(self.retry_delay).await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }

        bail!("Failed to upload chunk after {} attempts", self.max_retries)
    }

    /// Fetch the remote manifest
    pub async fn fetch_manifest(&self) -> Result<RemoteManifest> {
        let manifest_url = format!("{}/manifest.json", self.base_url.trim_end_matches('/'));

        let response = self.client.get(&manifest_url).send().await?;

        if !response.status().is_success() {
            bail!("Failed to fetch manifest: {}", response.status());
        }

        let manifest: RemoteManifest = response.json().await?;
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[serial_test::serial]
    fn test_protocol_detection() {
        assert_eq!(
            Protocol::from_url("s3://bucket/path").unwrap(),
            Protocol::S3
        );
        assert_eq!(
            Protocol::from_url("gs://bucket/path").unwrap(),
            Protocol::GCS
        );
        assert_eq!(
            Protocol::from_url("https://example.com").unwrap(),
            Protocol::Https
        );
        assert_eq!(
            Protocol::from_url("file:///tmp/test").unwrap(),
            Protocol::File
        );
    }

    #[test]
    #[serial_test::serial]
    fn test_chunk_url_building() {
        let client = ChunkClient {
            base_url: "https://example.com/repo".to_string(),
            protocol: Protocol::Https,
            client: Client::new(),
            max_retries: 3,
            retry_delay: Duration::from_secs(1),
        };

        let hash = SHA256Hash::from_hex(
            "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
        )
        .unwrap();
        let url = client.build_chunk_url(&hash).unwrap();

        assert_eq!(url, "https://example.com/repo/chunks/ab/cdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890");
    }
}
