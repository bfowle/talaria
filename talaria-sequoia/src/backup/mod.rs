use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use talaria_core::system::paths;
use crate::{TemporalManifest, SHA256Hash};

/// Manages database backups and restore operations
pub struct BackupManager {
    backups_dir: PathBuf,
    databases_dir: PathBuf,
    chunks_dir: PathBuf,
    taxonomy_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub databases: Vec<DatabaseBackupEntry>,
    pub taxonomy_version: Option<String>,
    pub total_size: u64,
    pub chunk_count: usize,
}

/// Binary backup manifest for efficient chunk storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub version: u8,
    pub created_at: DateTime<Utc>,
    pub chunks: Vec<SHA256Hash>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseBackupEntry {
    pub source: String,      // e.g., "uniprot"
    pub dataset: String,     // e.g., "swissprot"
    pub version: String,     // e.g., "20250920_174231"
    pub profiles: Vec<String>, // e.g., ["auto-detect"]
    pub manifest_hash: String, // SHA256 of manifest for verification
}

impl BackupManager {
    /// Create a new backup manager
    pub fn new() -> Result<Self> {
        let backups_dir = paths::talaria_home().join("backups");
        let databases_dir = paths::talaria_databases_dir();
        let chunks_dir = databases_dir.join("chunks");
        let taxonomy_dir = databases_dir.join("taxonomy");

        // Ensure backups directory exists
        fs::create_dir_all(&backups_dir)?;

        Ok(Self {
            backups_dir,
            databases_dir,
            chunks_dir,
            taxonomy_dir,
        })
    }

    /// Create a backup of the current database state
    pub fn create_backup(&self, name: &str, description: Option<String>) -> Result<Backup> {
        // Check if backup already exists
        let backup_dir = self.backups_dir.join(name);
        if backup_dir.exists() {
            anyhow::bail!("Backup '{}' already exists", name);
        }

        // Create backup directory
        fs::create_dir_all(&backup_dir)?;
        let manifests_dir = backup_dir.join("manifests");
        fs::create_dir_all(&manifests_dir)?;

        // Collect database information
        let versions_dir = self.databases_dir.join("versions");
        let mut databases = Vec::new();
        let mut all_chunks: HashSet<SHA256Hash> = HashSet::new();

        if versions_dir.exists() {
            for source_entry in fs::read_dir(&versions_dir)? {
                let source_entry = source_entry?;
                let source_name = source_entry.file_name().to_string_lossy().to_string();

                for dataset_entry in fs::read_dir(source_entry.path())? {
                    let dataset_entry = dataset_entry?;
                    let dataset_name = dataset_entry.file_name().to_string_lossy().to_string();

                    // Find current version
                    let current_link = dataset_entry.path().join("current");
                    if !current_link.exists() {
                        continue;
                    }

                    let version_dir = fs::read_link(&current_link)
                        .or_else(|_| Ok::<PathBuf, anyhow::Error>(current_link.clone()))?;

                    let version_name = version_dir
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "current".to_string());

                    // Copy manifest and collect profiles
                    let mut profiles = Vec::new();

                    // Main manifest
                    let manifest_path = dataset_entry.path().join(&version_name).join("manifest.tal");
                    if manifest_path.exists() {
                        let backup_manifest_path = manifests_dir.join(format!("{}_{}_{}_manifest.tal",
                            source_name, dataset_name, version_name));
                        fs::copy(&manifest_path, &backup_manifest_path)?;

                        // Collect chunks from manifest
                        if let Ok(manifest_data) = fs::read(&manifest_path) {
                            let chunk_hashes = extract_chunk_hashes(&manifest_data);
                            all_chunks.extend(chunk_hashes);
                        }
                    }

                    // Profile manifests
                    let profiles_dir = dataset_entry.path().join(&version_name).join("profiles");
                    if profiles_dir.exists() {
                        for profile_entry in fs::read_dir(&profiles_dir)? {
                            let profile_entry = profile_entry?;
                            let profile_name = profile_entry.file_name().to_string_lossy().to_string();
                            if profile_name.ends_with(".tal") {
                                let profile_base = profile_name.trim_end_matches(".tal");
                                profiles.push(profile_base.to_string());

                                let backup_profile_path = manifests_dir.join(format!("{}_{}_{}_{}.tal",
                                    source_name, dataset_name, version_name, profile_base));
                                fs::copy(profile_entry.path(), &backup_profile_path)?;

                                // Collect chunks from profile manifest
                                if let Ok(manifest_data) = fs::read(profile_entry.path()) {
                                    all_chunks.extend(extract_chunk_hashes(&manifest_data));
                                }
                            }
                        }
                    }

                    // Calculate manifest hash for verification
                    let manifest_hash = if manifest_path.exists() {
                        calculate_file_hash(&manifest_path)?
                    } else {
                        String::new()
                    };

                    databases.push(DatabaseBackupEntry {
                        source: source_name.clone(),
                        dataset: dataset_name.clone(),
                        version: version_name,
                        profiles,
                        manifest_hash,
                    });
                }
            }
        }

        // Get current taxonomy version
        let taxonomy_version = self.get_current_taxonomy_version()?;

        // Save taxonomy reference
        if let Some(ref tax_version) = taxonomy_version {
            let tax_info = serde_json::json!({
                "version": tax_version,
                "path": self.taxonomy_dir.join("current").to_string_lossy()
            });
            fs::write(
                backup_dir.join("taxonomy_version.json"),
                serde_json::to_string_pretty(&tax_info)?
            )?;
        }

        // Calculate total size
        let total_size: u64 = all_chunks.iter()
            .filter_map(|hash| {
                // Chunks are stored as {hex}.zst files
                let chunk_filename = format!("{}.zst", hash.to_hex());
                let chunk_path = self.chunks_dir.join(chunk_filename);
                fs::metadata(&chunk_path).ok().map(|m| m.len())
            })
            .sum();

        // Create backup metadata
        let backup = Backup {
            name: name.to_string(),
            description,
            created_at: Utc::now(),
            databases: databases.clone(),
            taxonomy_version,
            total_size,
            chunk_count: all_chunks.len(),
        };

        // Save metadata
        fs::write(
            backup_dir.join("metadata.json"),
            serde_json::to_string_pretty(&backup)?
        )?;

        // Save database list for quick reference
        fs::write(
            backup_dir.join("databases.json"),
            serde_json::to_string_pretty(&databases)?
        )?;

        // Save binary chunk manifest for efficient storage
        let backup_manifest = BackupManifest {
            version: 1,
            created_at: backup.created_at,
            chunks: all_chunks.iter().cloned().collect(),
        };

        // Serialize to MessagePack with TAL header
        let mut manifest_data = vec![b'T', b'A', b'L', 1]; // TAL magic + version
        manifest_data.extend(rmp_serde::to_vec(&backup_manifest)?);

        fs::write(
            backup_dir.join("chunk_index.talb"),
            manifest_data
        )?;

        println!("✓ Backup '{}' created successfully", name);
        println!("  - {} databases backed up", databases.len());
        println!("  - {} chunks referenced ({:.2} MB total)",
            all_chunks.len(),
            total_size as f64 / 1_048_576.0
        );

        Ok(backup)
    }

    /// Restore from a backup
    pub fn restore_backup(&self, name: &str, verify: bool) -> Result<()> {
        let backup_dir = self.backups_dir.join(name);
        if !backup_dir.exists() {
            anyhow::bail!("Backup '{}' not found", name);
        }

        // Load backup metadata
        let metadata_path = backup_dir.join("metadata.json");
        let backup: Backup = serde_json::from_str(
            &fs::read_to_string(&metadata_path)?
        )?;

        println!("Restoring backup '{}'...", name);
        println!("Created: {}", backup.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
        if let Some(ref desc) = backup.description {
            println!("Description: {}", desc);
        }

        // Verify chunks if requested
        if verify {
            println!("Verifying chunk availability...");

            // Try to load from binary manifest first (more efficient)
            let mut missing_chunks: HashSet<SHA256Hash> = HashSet::new();
            let chunk_index_path = backup_dir.join("chunk_index.talb");

            if chunk_index_path.exists() {
                // Load binary manifest
                let manifest_data = fs::read(&chunk_index_path)?;
                if manifest_data.starts_with(b"TAL") && manifest_data.len() > 4 {
                    if let Ok(manifest) = rmp_serde::from_slice::<BackupManifest>(&manifest_data[4..]) {
                        for hash in manifest.chunks {
                            let chunk_filename = format!("{}.zst", hash.to_hex());
                            let chunk_path = self.chunks_dir.join(chunk_filename);
                            if !chunk_path.exists() {
                                missing_chunks.insert(hash);
                            }
                        }
                    }
                }
            } else {
                // Fallback to scanning manifests
                let manifests_dir = backup_dir.join("manifests");
                for manifest_file in fs::read_dir(&manifests_dir)? {
                    let manifest_file = manifest_file?;
                    if let Ok(manifest_data) = fs::read(manifest_file.path()) {
                        for hash in extract_chunk_hashes(&manifest_data) {
                            let chunk_filename = format!("{}.zst", hash.to_hex());
                            let chunk_path = self.chunks_dir.join(chunk_filename);
                            if !chunk_path.exists() {
                                missing_chunks.insert(hash);
                            }
                        }
                    }
                }
            }

            if !missing_chunks.is_empty() {
                anyhow::bail!(
                    "Cannot restore: {} chunks are missing from storage",
                    missing_chunks.len()
                );
            }
            println!("✓ All chunks verified");
        }

        // Restore each database
        let versions_dir = self.databases_dir.join("versions");
        for db in &backup.databases {
            println!("  Restoring {}/{} (version: {})", db.source, db.dataset, db.version);

            let db_dir = versions_dir.join(&db.source).join(&db.dataset);
            fs::create_dir_all(&db_dir)?;

            // Update current symlink to point to the backed-up version
            let current_link = db_dir.join("current");
            if current_link.exists() {
                fs::remove_file(&current_link)?;
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                symlink(&db.version, &current_link)?;
            }
            #[cfg(not(unix))]
            {
                // On Windows, just copy the version name to a file
                fs::write(&current_link, &db.version)?;
            }

            // Ensure version directory exists
            let version_dir = db_dir.join(&db.version);
            fs::create_dir_all(&version_dir)?;

            // Restore manifests
            let manifests_dir = backup_dir.join("manifests");

            // Main manifest
            let backup_manifest = manifests_dir.join(format!("{}_{}_{}_manifest.tal",
                db.source, db.dataset, db.version));
            if backup_manifest.exists() {
                let target_manifest = version_dir.join("manifest.tal");
                fs::copy(&backup_manifest, &target_manifest)?;
            }

            // Profile manifests
            if !db.profiles.is_empty() {
                let profiles_dir = version_dir.join("profiles");
                fs::create_dir_all(&profiles_dir)?;

                for profile in &db.profiles {
                    let backup_profile = manifests_dir.join(format!("{}_{}_{}_{}.tal",
                        db.source, db.dataset, db.version, profile));
                    if backup_profile.exists() {
                        let target_profile = profiles_dir.join(format!("{}.tal", profile));
                        fs::copy(&backup_profile, &target_profile)?;
                    }
                }
            }
        }

        // Restore taxonomy version if present
        if let Some(ref tax_version) = backup.taxonomy_version {
            println!("  Restoring taxonomy version: {}", tax_version);
            // Update taxonomy current symlink
            let tax_current = self.taxonomy_dir.join("current");
            if tax_current.exists() {
                fs::remove_file(&tax_current)?;
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                symlink(tax_version, &tax_current)?;
            }
            #[cfg(not(unix))]
            {
                fs::write(&tax_current, tax_version)?;
            }
        }

        println!("✓ Backup '{}' restored successfully", name);
        Ok(())
    }

    /// List all available backups
    pub fn list_backups(&self, detailed: bool) -> Result<Vec<Backup>> {
        let mut backups = Vec::new();

        if !self.backups_dir.exists() {
            return Ok(backups);
        }

        for entry in fs::read_dir(&self.backups_dir)? {
            let entry = entry?;
            let metadata_path = entry.path().join("metadata.json");

            if metadata_path.exists() {
                let backup: Backup = serde_json::from_str(
                    &fs::read_to_string(&metadata_path)?
                )?;
                backups.push(backup);
            }
        }

        // Sort by creation date (newest first)
        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        if detailed {
            for backup in &backups {
                println!("\n📦 {}", backup.name.bold());
                println!("   Created: {}", backup.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
                if let Some(ref desc) = backup.description {
                    println!("   Description: {}", desc);
                }
                println!("   Databases: {}", backup.databases.len());
                for db in &backup.databases {
                    println!("     - {}/{} (v: {})", db.source, db.dataset, db.version);
                }
                if let Some(ref tax) = backup.taxonomy_version {
                    println!("   Taxonomy: {}", tax);
                }
                println!("   Size: {:.2} MB ({} chunks)",
                    backup.total_size as f64 / 1_048_576.0,
                    backup.chunk_count
                );
            }
        } else {
            use colored::*;
            println!("\nAvailable backups:");
            for backup in &backups {
                let desc = backup.description.as_ref()
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default();
                println!("  • {} ({}){}",
                    backup.name.cyan().bold(),
                    backup.created_at.format("%Y-%m-%d"),
                    desc
                );
            }
        }

        Ok(backups)
    }

    /// Export a backup with all referenced chunks
    pub fn export_backup(&self, name: &str, output_path: &Path) -> Result<()> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use tar::Builder;

        let backup_dir = self.backups_dir.join(name);
        if !backup_dir.exists() {
            anyhow::bail!("Backup '{}' not found", name);
        }

        println!("Exporting backup '{}'...", name);

        // Load backup metadata to get chunk list
        let metadata_path = backup_dir.join("metadata.json");
        let backup: Backup = serde_json::from_str(
            &fs::read_to_string(&metadata_path)?
        )?;

        // Create tar.gz archive
        let tar_gz = fs::File::create(output_path)?;
        let enc = GzEncoder::new(tar_gz, Compression::default());
        let mut tar = Builder::new(enc);

        // Add backup metadata and manifests
        tar.append_dir_all(format!("backup/{}", name), &backup_dir)?;

        // Collect all referenced chunks
        println!("Collecting chunks...");
        let mut chunk_hashes: HashSet<SHA256Hash> = HashSet::new();

        // Try binary manifest first (most efficient)
        let chunk_index_path = backup_dir.join("chunk_index.talb");
        if chunk_index_path.exists() {
            let manifest_data = fs::read(&chunk_index_path)?;
            if manifest_data.starts_with(b"TAL") && manifest_data.len() > 4 {
                if let Ok(manifest) = rmp_serde::from_slice::<BackupManifest>(&manifest_data[4..]) {
                    chunk_hashes.extend(manifest.chunks);
                }
            }
        } else {
            // Fallback to scanning manifests
            let manifests_dir = backup_dir.join("manifests");
            for manifest_file in fs::read_dir(&manifests_dir)? {
                let manifest_file = manifest_file?;
                if let Ok(manifest_data) = fs::read(manifest_file.path()) {
                    chunk_hashes.extend(extract_chunk_hashes(&manifest_data));
                }
            }
        }

        // Add chunks to archive
        let mut exported_chunks = 0;
        let total_chunks = chunk_hashes.len();

        for hash in chunk_hashes {
            let chunk_filename = format!("{}.zst", hash.to_hex());
            let chunk_path = self.chunks_dir.join(&chunk_filename);
            if chunk_path.exists() {
                tar.append_path_with_name(&chunk_path, format!("chunks/{}", chunk_filename))?;
                exported_chunks += 1;

                if exported_chunks % 100 == 0 {
                    println!("  Exported {}/{} chunks...", exported_chunks, total_chunks);
                }
            }
        }

        // Add taxonomy data if referenced
        if let Some(ref tax_version) = backup.taxonomy_version {
            let tax_dir = self.taxonomy_dir.join(tax_version);
            if tax_dir.exists() {
                println!("Adding taxonomy data...");
                tar.append_dir_all(format!("taxonomy/{}", tax_version), &tax_dir)?;
            }
        }

        tar.finish()?;

        let file_size = fs::metadata(output_path)?.len();
        println!("✓ Backup exported to {} ({:.2} MB)",
            output_path.display(),
            file_size as f64 / 1_048_576.0
        );

        Ok(())
    }

    /// Import a backup from an archive
    pub fn import_backup(&self, archive_path: &Path, name: &str) -> Result<()> {
        use flate2::read::GzDecoder;
        use tar::Archive;

        if !archive_path.exists() {
            anyhow::bail!("Archive file not found: {}", archive_path.display());
        }

        let backup_dir = self.backups_dir.join(name);
        if backup_dir.exists() {
            anyhow::bail!("Backup '{}' already exists", name);
        }

        println!("Importing backup from {}...", archive_path.display());

        let tar_gz = fs::File::open(archive_path)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);

        // Extract to temporary directory first
        let temp_dir = self.backups_dir.join(format!(".import_{}", name));
        fs::create_dir_all(&temp_dir)?;

        archive.unpack(&temp_dir)?;

        // Move backup metadata to final location
        let extracted_backup = temp_dir.join("backup");
        if extracted_backup.exists() {
            // Find the actual backup directory (might have different name)
            for entry in fs::read_dir(&extracted_backup)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    fs::rename(entry.path(), &backup_dir)?;
                    break;
                }
            }
        }

        // Import chunks
        let extracted_chunks = temp_dir.join("chunks");
        if extracted_chunks.exists() {
            println!("Importing chunks...");
            let mut imported = 0;
            for chunk_entry in fs::read_dir(&extracted_chunks)? {
                let chunk_entry = chunk_entry?;
                let chunk_name = chunk_entry.file_name();
                let target_chunk = self.chunks_dir.join(&chunk_name);

                if !target_chunk.exists() {
                    fs::copy(chunk_entry.path(), &target_chunk)?;
                    imported += 1;
                }
            }
            println!("  Imported {} new chunks", imported);
        }

        // Import taxonomy if present
        let extracted_taxonomy = temp_dir.join("taxonomy");
        if extracted_taxonomy.exists() {
            println!("Importing taxonomy data...");
            for tax_entry in fs::read_dir(&extracted_taxonomy)? {
                let tax_entry = tax_entry?;
                let tax_version = tax_entry.file_name();
                let target_tax = self.taxonomy_dir.join(&tax_version);

                if !target_tax.exists() {
                    fs::create_dir_all(&target_tax)?;
                    copy_dir_all(&tax_entry.path(), &target_tax)?;
                }
            }
        }

        // Clean up temporary directory
        fs::remove_dir_all(&temp_dir).ok();

        // Update backup name in metadata if different
        let metadata_path = backup_dir.join("metadata.json");
        if metadata_path.exists() {
            let mut backup: Backup = serde_json::from_str(
                &fs::read_to_string(&metadata_path)?
            )?;
            backup.name = name.to_string();
            fs::write(
                &metadata_path,
                serde_json::to_string_pretty(&backup)?
            )?;
        }

        println!("✓ Backup imported successfully as '{}'", name);
        Ok(())
    }

    /// Delete a backup
    pub fn delete_backup(&self, name: &str) -> Result<()> {
        let backup_dir = self.backups_dir.join(name);
        if !backup_dir.exists() {
            anyhow::bail!("Backup '{}' not found", name);
        }

        fs::remove_dir_all(&backup_dir)?;
        println!("✓ Backup '{}' deleted", name);
        Ok(())
    }

    /// Get current taxonomy version
    fn get_current_taxonomy_version(&self) -> Result<Option<String>> {
        let current_link = self.taxonomy_dir.join("current");
        if !current_link.exists() {
            return Ok(None);
        }

        // Try to read as symlink first
        if let Ok(target) = fs::read_link(&current_link) {
            if let Some(version) = target.file_name() {
                return Ok(Some(version.to_string_lossy().to_string()));
            }
        }

        // Fall back to reading as file (Windows compatibility)
        if let Ok(version) = fs::read_to_string(&current_link) {
            return Ok(Some(version.trim().to_string()));
        }

        Ok(None)
    }
}

/// Extract chunk hashes from manifest data
fn extract_chunk_hashes(manifest_data: &[u8]) -> Vec<SHA256Hash> {
    let mut hashes = Vec::new();

    // Check if this is a database manifest (TemporalManifest)
    if manifest_data.starts_with(b"TAL") {
        // Skip TAL magic header (3 bytes) and version (1 byte) = 4 bytes total
        let data = &manifest_data[4..];

        // Try to deserialize as TemporalManifest
        match rmp_serde::from_slice::<TemporalManifest>(data) {
            Ok(manifest) => {
                // Extract hashes from chunk_index
                for chunk in manifest.chunk_index {
                    // Keep as binary SHA256Hash
                    hashes.push(chunk.hash);
                }
            }
            Err(_) => {
                // This might be a ReductionManifest or other format
                // For now, we'll skip these as they need different handling
            }
        }
    } else {
        // Try as JSON format (legacy support)
        if let Ok(manifest) = serde_json::from_slice::<TemporalManifest>(manifest_data) {
            for chunk in manifest.chunk_index {
                hashes.push(chunk.hash);
            }
        }
    }

    // Remove duplicates by converting to HashSet and back
    let unique: HashSet<_> = hashes.into_iter().collect();
    unique.into_iter().collect()
}

/// Calculate SHA256 hash of a file
fn calculate_file_hash(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let data = fs::read(path)?;
    let hash = Sha256::digest(&data);
    Ok(format!("{:x}", hash))
}

/// Recursively copy directory
fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

// Re-export for colored output
use colored::*;