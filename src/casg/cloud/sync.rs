/// Synchronization logic for cloud storage
use super::{CloudStorage, SyncDirection, SyncOptions, SyncResult};
use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::sync::Semaphore;

pub async fn perform_sync(
    storage: &dyn CloudStorage,
    local_path: &Path,
    remote_prefix: &str,
    options: &SyncOptions,
) -> Result<SyncResult> {
    let start_time = Instant::now();
    let mut result = SyncResult {
        uploaded: Vec::new(),
        downloaded: Vec::new(),
        deleted: Vec::new(),
        skipped: Vec::new(),
        errors: Vec::new(),
        bytes_transferred: 0,
        duration: std::time::Duration::from_secs(0),
    };

    // Verify access first
    storage
        .verify_access()
        .await
        .context("Failed to verify cloud storage access")?;

    // Collect local and remote files
    let local_files = collect_local_files(
        local_path,
        &options.exclude_patterns,
        &options.include_patterns,
    )?;
    let remote_objects = storage.list_objects(Some(remote_prefix)).await?;

    let local_set: HashSet<String> = local_files
        .iter()
        .map(|p| path_to_key(p, local_path))
        .collect();

    let remote_set: HashSet<String> = remote_objects
        .iter()
        .map(|o| strip_prefix(&o.key, remote_prefix))
        .collect();

    // Determine what needs to be synced
    let mut to_upload = Vec::new();
    let mut to_download = Vec::new();
    let mut to_delete_local: Vec<PathBuf> = Vec::new();
    let mut to_delete_remote: Vec<String> = Vec::new();

    match options.direction {
        SyncDirection::Upload => {
            // Upload new and modified files
            for local_file in &local_files {
                let key = path_to_key(local_file, local_path);
                let full_key = format!("{}/{}", remote_prefix, key);

                if should_upload(local_file, &remote_objects, &full_key)? {
                    to_upload.push((local_file.clone(), full_key));
                } else {
                    result.skipped.push(key);
                }
            }

            // Delete remote files not in local
            if options.delete_missing {
                for remote_obj in &remote_objects {
                    let key = strip_prefix(&remote_obj.key, remote_prefix);
                    if !local_set.contains(&key) {
                        to_delete_remote.push(remote_obj.key.clone());
                    }
                }
            }
        }
        SyncDirection::Download => {
            // Download new and modified files
            for remote_obj in &remote_objects {
                let key = strip_prefix(&remote_obj.key, remote_prefix);
                let local_file = local_path.join(&key);

                if should_download(&local_file, remote_obj)? {
                    to_download.push((remote_obj.key.clone(), local_file));
                } else {
                    result.skipped.push(key);
                }
            }

            // Delete local files not in remote
            if options.delete_missing {
                for local_file in &local_files {
                    let key = path_to_key(local_file, local_path);
                    if !remote_set.contains(&key) {
                        to_delete_local.push(local_file.clone());
                    }
                }
            }
        }
        SyncDirection::Bidirectional => {
            // Sync both ways, preferring newer files
            for local_file in &local_files {
                let key = path_to_key(local_file, local_path);
                let full_key = format!("{}/{}", remote_prefix, key);

                if let Some(remote_obj) = remote_objects.iter().find(|o| o.key == full_key) {
                    // File exists on both sides, check which is newer
                    let local_modified = std::fs::metadata(local_file)?
                        .modified()?
                        .elapsed()
                        .unwrap_or_default();
                    let local_time =
                        chrono::Utc::now() - chrono::Duration::from_std(local_modified).unwrap();

                    if local_time > remote_obj.last_modified {
                        to_upload.push((local_file.clone(), full_key));
                    } else if remote_obj.last_modified > local_time {
                        to_download.push((remote_obj.key.clone(), local_file.clone()));
                    } else {
                        result.skipped.push(key);
                    }
                } else {
                    // File only exists locally, upload it
                    to_upload.push((local_file.clone(), full_key));
                }
            }

            // Download files that only exist remotely
            for remote_obj in &remote_objects {
                let key = strip_prefix(&remote_obj.key, remote_prefix);
                if !local_set.contains(&key) {
                    let local_file = local_path.join(&key);
                    to_download.push((remote_obj.key.clone(), local_file));
                }
            }
        }
    }

    // Execute operations (unless dry run)
    if options.dry_run {
        println!("DRY RUN - No changes will be made");
        println!("Would upload: {} files", to_upload.len());
        println!("Would download: {} files", to_download.len());
        println!(
            "Would delete: {} files",
            to_delete_local.len() + to_delete_remote.len()
        );

        result.uploaded = to_upload.iter().map(|(_, k)| k.clone()).collect();
        result.downloaded = to_download.iter().map(|(k, _)| k.clone()).collect();

        // Combine both delete lists for the result
        for path in &to_delete_local {
            result.deleted.push(path.to_string_lossy().to_string());
        }
        for key in &to_delete_remote {
            result.deleted.push(key.clone());
        }
    } else {
        // Create progress bars
        let multi_progress = MultiProgress::new();
        let style = ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=>-");

        // Perform uploads
        if !to_upload.is_empty() {
            let upload_pb = multi_progress.add(ProgressBar::new(to_upload.len() as u64));
            upload_pb.set_style(style.clone());
            upload_pb.set_message("Uploading files");

            let semaphore = Semaphore::new(options.parallel_transfers);

            let upload_futures = to_upload.iter().map(|(local_file, key)| {
                let sem = &semaphore;
                let pb = &upload_pb;

                async move {
                    let _permit = sem.acquire().await.unwrap();
                    let result = storage.upload(local_file, key, None).await;
                    pb.inc(1);
                    (key.clone(), result)
                }
            });

            let mut upload_stream =
                stream::iter(upload_futures).buffer_unordered(options.parallel_transfers);

            while let Some((key, upload_result)) = upload_stream.next().await {
                match upload_result {
                    Ok(_) => {
                        if let Ok(metadata) =
                            std::fs::metadata(&to_upload.iter().find(|(_, k)| k == &key).unwrap().0)
                        {
                            result.bytes_transferred += metadata.len() as usize;
                        }
                        result.uploaded.push(key);
                    }
                    Err(e) => {
                        result.errors.push((key, e.to_string()));
                    }
                }
            }

            upload_pb.finish_with_message(format!("Uploaded {} files", result.uploaded.len()));
        }

        // Perform downloads
        if !to_download.is_empty() {
            let download_pb = multi_progress.add(ProgressBar::new(to_download.len() as u64));
            download_pb.set_style(style.clone());
            download_pb.set_message("Downloading files");

            let semaphore = Semaphore::new(options.parallel_transfers);

            let download_futures = to_download.iter().map(|(key, local_file)| {
                let sem = &semaphore;
                let pb = &download_pb;

                async move {
                    let _permit = sem.acquire().await.unwrap();
                    let result = storage.download(key, local_file, None).await;
                    pb.inc(1);
                    (key.clone(), result)
                }
            });

            let mut download_stream =
                stream::iter(download_futures).buffer_unordered(options.parallel_transfers);

            while let Some((key, download_result)) = download_stream.next().await {
                match download_result {
                    Ok(_) => {
                        result.downloaded.push(key.clone());
                        if let Some(obj) = remote_objects.iter().find(|o| o.key == key) {
                            result.bytes_transferred += obj.size;
                        }
                    }
                    Err(e) => {
                        result.errors.push((key, e.to_string()));
                    }
                }
            }

            download_pb
                .finish_with_message(format!("Downloaded {} files", result.downloaded.len()));
        }

        // Perform deletions
        let total_deletes = to_delete_local.len() + to_delete_remote.len();
        if total_deletes > 0 {
            let delete_pb = multi_progress.add(ProgressBar::new(total_deletes as u64));
            delete_pb.set_style(style);
            delete_pb.set_message("Deleting files");

            // Delete local files
            for path in &to_delete_local {
                if let Err(e) = std::fs::remove_file(path) {
                    result
                        .errors
                        .push((path.to_string_lossy().to_string(), e.to_string()));
                } else {
                    result.deleted.push(path.to_string_lossy().to_string());
                }
                delete_pb.inc(1);
            }

            // Delete remote files
            for key in &to_delete_remote {
                if let Err(e) = storage.delete(key).await {
                    result.errors.push((key.clone(), e.to_string()));
                } else {
                    result.deleted.push(key.clone());
                }
                delete_pb.inc(1);
            }

            delete_pb.finish_with_message(format!("Deleted {} files", result.deleted.len()));
        }
    }

    result.duration = start_time.elapsed();

    Ok(result)
}

fn collect_local_files(
    base_path: &Path,
    exclude_patterns: &[String],
    include_patterns: &[String],
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_recursive(
        base_path,
        base_path,
        &mut files,
        exclude_patterns,
        include_patterns,
    )?;
    Ok(files)
}

fn collect_files_recursive(
    base_path: &Path,
    current_path: &Path,
    files: &mut Vec<PathBuf>,
    exclude_patterns: &[String],
    include_patterns: &[String],
) -> Result<()> {
    for entry in std::fs::read_dir(current_path)? {
        let entry = entry?;
        let path = entry.path();

        // Check exclude patterns
        let relative_path = path.strip_prefix(base_path).unwrap_or(&path);
        let path_str = relative_path.to_string_lossy();

        if !exclude_patterns.is_empty() {
            let mut excluded = false;
            for pattern in exclude_patterns {
                if glob::Pattern::new(pattern)?.matches(&path_str) {
                    excluded = true;
                    break;
                }
            }
            if excluded {
                continue;
            }
        }

        if !include_patterns.is_empty() {
            let mut included = false;
            for pattern in include_patterns {
                if glob::Pattern::new(pattern)?.matches(&path_str) {
                    included = true;
                    break;
                }
            }
            if !included {
                continue;
            }
        }

        if path.is_dir() {
            collect_files_recursive(base_path, &path, files, exclude_patterns, include_patterns)?;
        } else {
            files.push(path);
        }
    }

    Ok(())
}

fn path_to_key(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn strip_prefix(key: &str, prefix: &str) -> String {
    key.strip_prefix(prefix)
        .unwrap_or(key)
        .trim_start_matches('/')
        .to_string()
}

fn should_upload(
    local_file: &Path,
    remote_objects: &[super::CloudObject],
    key: &str,
) -> Result<bool> {
    if let Some(remote) = remote_objects.iter().find(|o| o.key == key) {
        // Compare modification times and sizes
        let local_metadata = std::fs::metadata(local_file)?;
        let local_size = local_metadata.len() as usize;

        if local_size != remote.size {
            return Ok(true);
        }

        let local_modified = local_metadata.modified()?;
        let local_time: chrono::DateTime<chrono::Utc> = local_modified.into();

        Ok(local_time > remote.last_modified)
    } else {
        Ok(true) // File doesn't exist remotely
    }
}

fn should_download(local_file: &Path, remote: &super::CloudObject) -> Result<bool> {
    if !local_file.exists() {
        return Ok(true);
    }

    let local_metadata = std::fs::metadata(local_file)?;
    let local_size = local_metadata.len() as usize;

    if local_size != remote.size {
        return Ok(true);
    }

    let local_modified = local_metadata.modified()?;
    let local_time: chrono::DateTime<chrono::Utc> = local_modified.into();

    Ok(remote.last_modified > local_time)
}
