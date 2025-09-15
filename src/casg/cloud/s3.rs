/// AWS S3 and S3-compatible storage implementation

use super::{CloudConfig, CloudObject, CloudStorage};
use anyhow::{Context, Result};
use async_trait::async_trait;
use aws_sdk_s3::{Client, Config};
use aws_sdk_s3::config::Region;
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::primitives::ByteStream;
use indicatif::ProgressBar;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub struct S3Storage {
    client: Client,
    bucket: String,
    prefix: Option<String>,
}

impl S3Storage {
    pub fn new(config: &CloudConfig) -> Result<Self> {
        let (bucket, region, prefix, endpoint) = match config {
            CloudConfig::S3 {
                bucket,
                region,
                prefix,
                endpoint,
            } => (bucket.clone(), region.clone(), prefix.clone(), endpoint.clone()),
            _ => anyhow::bail!("Invalid config for S3Storage"),
        };

        // Build S3 client configuration
        let sdk_config = tokio::runtime::Handle::current().block_on(async {
            let mut config_builder = Config::builder()
                .region(Region::new(region));

            // Use custom endpoint if provided (for S3-compatible services)
            if let Some(endpoint_url) = endpoint {
                config_builder = config_builder
                    .endpoint_url(endpoint_url)
                    .force_path_style(true);
            }

            // Try to get credentials from environment or use anonymous
            if let (Ok(access_key), Ok(secret_key)) = (
                std::env::var("AWS_ACCESS_KEY_ID"),
                std::env::var("AWS_SECRET_ACCESS_KEY"),
            ) {
                let creds = Credentials::new(
                    access_key,
                    secret_key,
                    std::env::var("AWS_SESSION_TOKEN").ok(),
                    None,
                    "talaria",
                );
                config_builder = config_builder.credentials_provider(creds);
            }

            config_builder.build()
        });

        let client = Client::from_conf(sdk_config);

        Ok(Self {
            client,
            bucket,
            prefix,
        })
    }

    fn full_key(&self, key: &str) -> String {
        match &self.prefix {
            Some(prefix) => format!("{}/{}", prefix.trim_end_matches('/'), key.trim_start_matches('/')),
            None => key.to_string(),
        }
    }
}

#[async_trait]
impl CloudStorage for S3Storage {
    async fn list_objects(&self, prefix: Option<&str>) -> Result<Vec<CloudObject>> {
        let mut objects = Vec::new();
        let list_prefix = match (&self.prefix, prefix) {
            (Some(base), Some(sub)) => format!("{}/{}", base, sub),
            (Some(base), None) => base.clone(),
            (None, Some(sub)) => sub.to_string(),
            (None, None) => String::new(),
        };

        let mut continuation_token = None;

        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket);

            if !list_prefix.is_empty() {
                request = request.prefix(list_prefix.clone());
            }

            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }

            let response = request
                .send()
                .await
                .context("Failed to list S3 objects")?;

            if let Some(contents) = response.contents {
                for object in contents {
                    if let Some(key) = object.key {
                        objects.push(CloudObject {
                            key,
                            size: object.size.unwrap_or(0) as usize,
                            etag: object.e_tag,
                            last_modified: object.last_modified
                                .map(|dt| {
                                    let secs = dt.secs();
                                    let nanos = dt.subsec_nanos();
                                    chrono::DateTime::from_timestamp(secs, nanos)
                                        .unwrap_or_else(chrono::Utc::now)
                                })
                                .unwrap_or_else(chrono::Utc::now),
                            storage_class: object.storage_class.map(|sc| sc.as_str().to_string()),
                        });
                    }
                }
            }

            if response.is_truncated.unwrap_or(false) {
                continuation_token = response.next_continuation_token;
            } else {
                break;
            }
        }

        Ok(objects)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let full_key = self.full_key(key);

        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(full_key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_not_found() {
                    Ok(false)
                } else {
                    Err(anyhow::anyhow!("Failed to check object existence: {}", service_error))
                }
            }
        }
    }

    async fn get_metadata(&self, key: &str) -> Result<CloudObject> {
        let full_key = self.full_key(key);

        let response = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&full_key)
            .send()
            .await
            .context("Failed to get object metadata")?;

        Ok(CloudObject {
            key: full_key,
            size: response.content_length.unwrap_or(0) as usize,
            etag: response.e_tag,
            last_modified: response.last_modified
                .map(|dt| {
                    let secs = dt.secs();
                    let nanos = dt.subsec_nanos();
                    chrono::DateTime::from_timestamp(secs, nanos)
                        .unwrap_or_else(chrono::Utc::now)
                })
                .unwrap_or_else(chrono::Utc::now),
            storage_class: response.storage_class.map(|sc| sc.as_str().to_string()),
        })
    }

    async fn upload(
        &self,
        local_path: &Path,
        key: &str,
        progress: Option<&ProgressBar>,
    ) -> Result<()> {
        let full_key = self.full_key(key);
        let file_size = std::fs::metadata(local_path)?.len();

        if let Some(pb) = progress {
            pb.set_length(file_size);
            pb.set_message(format!("Uploading {}", local_path.display()));
        }

        // Read file and create byte stream
        let body = ByteStream::from_path(local_path)
            .await
            .context("Failed to read file for upload")?;

        // Upload to S3
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(full_key)
            .body(body)
            .send()
            .await
            .context("Failed to upload to S3")?;

        if let Some(pb) = progress {
            pb.set_position(file_size);
            pb.finish_with_message("Upload complete");
        }

        Ok(())
    }

    async fn download(
        &self,
        key: &str,
        local_path: &Path,
        progress: Option<&ProgressBar>,
    ) -> Result<()> {
        let full_key = self.full_key(key);

        // Ensure parent directory exists
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create parent directory")?;
        }

        // Get object
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(full_key)
            .send()
            .await
            .context("Failed to get object from S3")?;

        let content_length = response.content_length.unwrap_or(0) as usize;

        if let Some(pb) = progress {
            pb.set_length(content_length as u64);
            pb.set_message(format!("Downloading to {}", local_path.display()));
        }

        // Write to file
        let mut file = File::create(local_path)
            .await
            .context("Failed to create local file")?;

        let mut byte_stream = response.body;
        let mut downloaded = 0;

        while let Some(bytes) = byte_stream.try_next().await? {
            file.write_all(&bytes).await?;
            downloaded += bytes.len();

            if let Some(pb) = progress {
                pb.set_position(downloaded as u64);
            }
        }

        file.flush().await?;

        if let Some(pb) = progress {
            pb.finish_with_message("Download complete");
        }

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let full_key = self.full_key(key);

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(full_key)
            .send()
            .await
            .context("Failed to delete object from S3")?;

        Ok(())
    }

    async fn delete_batch(&self, keys: &[String]) -> Result<Vec<Result<()>>> {
        use aws_sdk_s3::types::{Delete, ObjectIdentifier};

        if keys.is_empty() {
            return Ok(vec![]);
        }

        // S3 batch delete is limited to 1000 objects
        let mut results = Vec::new();

        for chunk in keys.chunks(1000) {
            let objects: Vec<ObjectIdentifier> = chunk
                .iter()
                .map(|key| {
                    ObjectIdentifier::builder()
                        .key(self.full_key(key))
                        .build()
                        .expect("Failed to build ObjectIdentifier")
                })
                .collect();

            let delete = Delete::builder()
                .set_objects(Some(objects))
                .build()
                .context("Failed to build delete request")?;

            let response = self
                .client
                .delete_objects()
                .bucket(&self.bucket)
                .delete(delete)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    // Check for errors in individual deletions
                    if let Some(errors) = resp.errors {
                        for error in errors {
                            results.push(Err(anyhow::anyhow!(
                                "Failed to delete {}: {}",
                                error.key.unwrap_or_default(),
                                error.message.unwrap_or_default()
                            )));
                        }
                    }

                    if let Some(deleted) = resp.deleted {
                        for _ in deleted {
                            results.push(Ok(()));
                        }
                    }
                }
                Err(e) => {
                    // All deletions in this batch failed
                    for _ in chunk {
                        results.push(Err(anyhow::anyhow!("Batch delete failed: {}", e)));
                    }
                }
            }
        }

        Ok(results)
    }

    async fn get_presigned_url(
        &self,
        key: &str,
        expires_in: std::time::Duration,
    ) -> Result<String> {
        use aws_sdk_s3::presigning::PresigningConfig;

        let full_key = self.full_key(key);

        let presigning_config = PresigningConfig::expires_in(expires_in)
            .context("Failed to create presigning config")?;

        let presigned_request = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(full_key)
            .presigned(presigning_config)
            .await
            .context("Failed to create presigned URL")?;

        Ok(presigned_request.uri().to_string())
    }

    async fn verify_access(&self) -> Result<()> {
        // Try to list objects with max-keys=1 to verify access
        self.client
            .list_objects_v2()
            .bucket(&self.bucket)
            .max_keys(1)
            .send()
            .await
            .context("Failed to verify S3 access")?;

        Ok(())
    }
}