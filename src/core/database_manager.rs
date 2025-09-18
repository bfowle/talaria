/// Database manager using content-addressed storage
///
/// Instead of downloading entire databases and creating dated directories,
/// this uses content-addressed storage with manifests for efficient updates.

use crate::casg::{CASGRepository, TaxonomicChunker, ChunkingStrategy, SHA256Hash, TaxonomyAwareChunk};
use crate::bio::sequence::Sequence;
use crate::core::paths;
use crate::core::taxonomy_manager::{TaxonomyManager, VersionDecision};
use crate::download::{DatabaseSource, NCBIDatabase, UniProtDatabase};

/// Magic bytes for Talaria manifest format
const TALARIA_MAGIC: &[u8] = b"TAL\x01";
use crate::utils::progress::{create_progress_bar, create_spinner};
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct DatabaseManager {
    repository: CASGRepository,
    base_path: PathBuf,
    use_json_manifest: bool,
    taxonomy_manager: TaxonomyManager,
}

impl DatabaseManager {
    /// Create a new CASG database manager
    pub fn new(base_dir: Option<String>) -> Result<Self> {
        let base_path = if let Some(dir) = base_dir {
            PathBuf::from(dir)
        } else {
            // Use centralized path configuration
            paths::talaria_databases_dir()
        };

        // Ensure directory exists
        std::fs::create_dir_all(&base_path)?;

        // Initialize or open CASG repository
        // Always use open if chunks directory exists (indicating existing data)
        let repository = if base_path.join("chunks").exists() {
            CASGRepository::open(&base_path)?
        } else {
            CASGRepository::init(&base_path)?
        };

        let taxonomy_manager = TaxonomyManager::new(&base_path);

        Ok(Self {
            repository,
            base_path,
            use_json_manifest: false,
            taxonomy_manager,
        })
    }

    /// Create a new CASG database manager with options
    pub fn with_options(base_dir: Option<String>, use_json_manifest: bool) -> Result<Self> {
        let base_path = if let Some(dir) = base_dir {
            PathBuf::from(dir)
        } else {
            // Use centralized path configuration
            paths::talaria_databases_dir()
        };

        // Ensure directory exists
        std::fs::create_dir_all(&base_path)?;

        // Initialize or open CASG repository
        // Always use open if chunks directory exists (indicating existing data)
        let repository = if base_path.join("chunks").exists() {
            CASGRepository::open(&base_path)?
        } else {
            CASGRepository::init(&base_path)?
        };

        let taxonomy_manager = TaxonomyManager::new(&base_path);

        Ok(Self {
            repository,
            base_path,
            use_json_manifest,
            taxonomy_manager,
        })
    }

    /// Check for updates without downloading (dry-run mode)
    pub async fn check_for_updates(
        &mut self,
        source: &DatabaseSource,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        // Check if we have a cached manifest
        let manifest_path = self.get_manifest_path(source);

        if !manifest_path.exists() {
            progress_callback("No local database found - initial download required");
            return Ok(DownloadResult::InitialDownload);
        }

        // Try to get manifest URL for update check
        if let Ok(manifest_url) = self.get_manifest_url(source) {
            progress_callback("Checking for updates...");
            self.repository.manifest.set_remote_url(manifest_url.clone());

            match self.repository.check_updates().await {
                Ok(false) => {
                    progress_callback("Database is up to date");
                    return Ok(DownloadResult::UpToDate);
                }
                Ok(true) => {
                    progress_callback("Updates available");
                    // In dry-run mode, we don't actually download, just report what would happen
                    // Try to get basic info about the update
                    if let Ok(new_manifest) = self.repository.manifest.fetch_remote().await {
                        let diff = self.repository.manifest.diff(&new_manifest)?;
                        return Ok(DownloadResult::Updated {
                            chunks_added: diff.new_chunks.len(),
                            chunks_removed: diff.removed_chunks.len(),
                        });
                    }
                }
                Err(_) => {
                    progress_callback("Cannot check for updates (manifest server unavailable)");
                    return Ok(DownloadResult::UpToDate);
                }
            }
        }

        progress_callback("No manifest server configured - cannot check for updates");
        Ok(DownloadResult::UpToDate)
    }

    /// Force download even if up-to-date
    pub async fn force_download(
        &mut self,
        source: &DatabaseSource,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        progress_callback("Force download requested - bypassing version check");

        // Set environment variable to signal force mode for taxonomy versioning
        std::env::set_var("TALARIA_FORCE_NEW_VERSION", "1");

        // Delete existing manifest to force re-download
        let manifest_path = self.get_manifest_path(source);
        if manifest_path.exists() {
            std::fs::remove_file(&manifest_path).ok();
        }

        // Now do a normal download which will treat it as initial
        let result = self.download(source, progress_callback).await;

        // Clear the force flag after download
        std::env::remove_var("TALARIA_FORCE_NEW_VERSION");

        result
    }

    /// Ensure version integrity - fix symlinks and metadata even if data is present
    pub fn ensure_version_integrity(&mut self, source: &DatabaseSource) -> Result<()> {

        let (source_name, dataset) = self.get_source_dataset_names(source);
        let versions_dir = self.base_path
            .join("versions")
            .join(&source_name)
            .join(&dataset);

        // If no versions directory, nothing to fix
        if !versions_dir.exists() {
            return Ok(());
        }

        // Find the latest version directory
        if let Some(latest_version_dir) = self.find_latest_version_dir(&versions_dir)? {
            if let Some(timestamp) = latest_version_dir.file_name().and_then(|s| s.to_str()) {
                // Ensure current symlink points to latest
                let current_link = versions_dir.join("current");

                // Check if current symlink is correct
                let needs_update = if current_link.exists() {
                    if let Ok(target) = std::fs::read_link(&current_link) {
                        target.file_name().and_then(|s| s.to_str()) != Some(timestamp)
                    } else {
                        true
                    }
                } else {
                    true
                };

                if needs_update {
                    // Update the current symlink
                    self.update_version_symlinks(source, timestamp)?;
                }

                // Ensure version.json exists
                let version_file = latest_version_dir.join("version.json");
                if !version_file.exists() {
                    // Create version metadata if missing
                    let manifest_path = latest_version_dir.join("manifest.tal");
                    if !manifest_path.exists() {
                        let manifest_path = latest_version_dir.join("manifest.json");
                        if manifest_path.exists() {
                            self.create_version_metadata(source, timestamp, &manifest_path)?;
                        }
                    } else {
                        self.create_version_metadata(source, timestamp, &manifest_path)?;
                    }
                } else {
                    // Version.json exists but temporal aliases might be missing
                    // Always ensure temporal aliases are up to date
                    self.update_version_symlinks(source, timestamp)?;
                }
            }
        }

        Ok(())
    }


    /// Get current version information
    pub fn get_current_version_info(&self, source: &DatabaseSource) -> Result<VersionInfo> {
        use crate::utils::version_detector::DatabaseVersion;

        let (source_name, dataset) = self.get_source_dataset_names(source);
        let versions_dir = self.base_path
            .join("versions")
            .join(&source_name)
            .join(&dataset);

        // Follow current symlink or find latest
        let current_link = versions_dir.join("current");
        let version_dir = if current_link.exists() && current_link.is_symlink() {
            if let Ok(target) = std::fs::read_link(&current_link) {
                if target.is_absolute() {
                    target
                } else {
                    versions_dir.join(target)
                }
            } else {
                self.find_latest_version_dir(&versions_dir)?
                    .ok_or_else(|| anyhow::anyhow!("No versions found"))?
            }
        } else {
            self.find_latest_version_dir(&versions_dir)?
                .ok_or_else(|| anyhow::anyhow!("No versions found"))?
        };

        // Read version.json if it exists
        let version_file = version_dir.join("version.json");
        if version_file.exists() {
            let content = std::fs::read_to_string(version_file)?;
            let version: DatabaseVersion = serde_json::from_str(&content)?;

            Ok(VersionInfo {
                timestamp: version.timestamp.clone(),
                upstream_version: version.upstream_version.clone(),
                aliases: version.all_aliases(),
            })
        } else {
            // Fallback to just timestamp
            let timestamp = version_dir.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            Ok(VersionInfo {
                timestamp,
                upstream_version: None,
                aliases: vec!["current".to_string()],
            })
        }
    }

    /// Download a database using CASG
    pub async fn download(
        &mut self,
        source: &DatabaseSource,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        // For taxonomy files, check if the specific file exists
        if Self::is_taxonomy_database(source) {
            if self.has_specific_taxonomy_file(source) {
                progress_callback("Taxonomy component already exists");
                return Ok(DownloadResult::UpToDate);
            }
            // Component doesn't exist yet, proceed directly to download
            // Skip CASG manifest checks for taxonomy files
            println!("  Taxonomy component not found, will download: {}", source);
            return self.handle_initial_download(source, progress_callback).await;
        }

        // Check if we have a cached manifest (for non-taxonomy databases)
        let manifest_path = self.get_manifest_path(source);
        let has_existing = manifest_path.exists();

        // If we have an existing manifest, check for updates
        if has_existing {
            // Try to get manifest URL (may not exist in dev/local mode)
            if let Ok(manifest_url) = self.get_manifest_url(source) {
                progress_callback("Checking for updates...");

                // Set remote URL in repository
                self.repository.manifest.set_remote_url(manifest_url.clone());

                // Try to check for updates, but don't fail if manifest server is unavailable
                match self.repository.check_updates().await {
                    Ok(false) => {
                        progress_callback("Database is up to date");
                        return Ok(DownloadResult::UpToDate);
                    }
                    Ok(true) => {
                        progress_callback("Updates available, downloading manifest...");
                        // Try to fetch remote manifest
                        match self.repository.manifest.fetch_remote().await {
                            Ok(new_manifest) => {
                                // Successfully got remote manifest, proceed with incremental update
                                return self.handle_incremental_update(new_manifest, progress_callback).await;
                            }
                            Err(_) => {
                                progress_callback("[!] Manifest server unavailable, keeping current version");
                                return Ok(DownloadResult::UpToDate);
                            }
                        }
                    }
                    Err(_) => {
                        // Manifest server unavailable, but we have local data
                        progress_callback("[!] Cannot check for updates (manifest server unavailable)");
                        return Ok(DownloadResult::UpToDate);
                    }
                }
            } else {
                // No manifest URL available (dev mode), just use local
                progress_callback("Using local CASG database (no remote manifest configured)");
                return Ok(DownloadResult::UpToDate);
            }
        }

        // No existing manifest - need to do initial download
        progress_callback("[NEW] Initial download required - no local CASG data found");
        progress_callback("This will download the full database and convert it to CASG format");
        progress_callback("Future updates will be incremental and much faster!");

        self.handle_initial_download(source, progress_callback).await
    }

    /// Handle incremental update when manifest is available
    async fn handle_incremental_update(
        &mut self,
        new_manifest: crate::casg::Manifest,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        use crate::casg::{OperationType, SourceInfo};

        // Get manifest data for version info
        let manifest_data = new_manifest.get_data()
            .ok_or_else(|| anyhow::anyhow!("No manifest data"))?;
        let manifest_hash = SHA256Hash::compute(&serde_json::to_vec(&manifest_data)?);
        let manifest_version = manifest_data.version.clone();

        // Compute diff to see what chunks we need
        let diff = self.repository.manifest.diff(&new_manifest)?;

        let chunks_to_download = diff.new_chunks.len();
        let chunks_to_remove = diff.removed_chunks.len();

        // Check for resumable state
        let source_info = SourceInfo {
            database: manifest_data.source_database.clone().unwrap_or_else(|| "unknown".to_string()),
            source_url: new_manifest.get_remote_url().map(|s| s.to_string()),
            etag: new_manifest.get_etag().map(|s| s.to_string()),
            total_size_bytes: None,
        };

        let resumable_state = self.repository.storage.check_resumable(
            &source_info.database,
            &OperationType::IncrementalUpdate,
            &manifest_hash,
            &manifest_version,
        )?;

        if let Some(state) = resumable_state {
            progress_callback(&format!(
                "Found resumable update: {} ({:.1}% complete)",
                state.summary(),
                state.completion_percentage()
            ));
            progress_callback(&format!(
                "Resuming with {} chunks remaining",
                state.remaining_chunks()
            ));
        } else if chunks_to_download > 0 {
            // Start new processing operation
            self.repository.storage.start_processing(
                OperationType::IncrementalUpdate,
                manifest_hash,
                manifest_version.clone(),
                chunks_to_download,
                source_info,
            )?;
        }

        progress_callback(&format!(
            "Need to download {} new chunks, remove {} old chunks",
            chunks_to_download, chunks_to_remove
        ));

        // Download only new chunks (with resume support)
        if !diff.new_chunks.is_empty() {
            progress_callback("Downloading new chunks...");
            let downloaded = self.repository.storage.fetch_chunks_with_resume(
                &diff.new_chunks,
                true  // Enable resume checking
            ).await?;

            progress_callback(&format!(
                "Downloaded {} chunks, {:.2} MB",
                downloaded.len(),
                downloaded.iter().map(|c| c.size).sum::<usize>() as f64 / 1_048_576.0
            ));
        }

        // Remove old chunks (garbage collection)
        if !diff.removed_chunks.is_empty() {
            progress_callback("Removing obsolete chunks...");

            // Get all currently referenced chunks from the new manifest
            let manifest_data = new_manifest.get_data()
                .ok_or_else(|| anyhow::anyhow!("No manifest data"))?;
            let referenced_chunks: Vec<SHA256Hash> = manifest_data.chunk_index
                .iter()
                .map(|c| c.hash.clone())
                .collect();

            // Run garbage collection
            let gc_result = self.repository.storage.gc(&referenced_chunks)?;

            if gc_result.removed_count > 0 {
                progress_callback(&format!(
                    "Removed {} obsolete chunks, freed {:.2} MB",
                    gc_result.removed_count,
                    gc_result.freed_space as f64 / 1_048_576.0
                ));
            }
        }

        // Mark operation as complete
        self.repository.storage.complete_processing()?;

        // Track version in temporal index before updating manifest
        let temporal_path = self.base_path.clone();
        let mut temporal_index = crate::casg::temporal::TemporalIndex::load(&temporal_path)?;

        // Add sequence version tracking
        if let Some(manifest_data) = new_manifest.get_data() {
            temporal_index.add_sequence_version(
                manifest_data.version.clone(),
                manifest_data.sequence_root.clone(),
                manifest_data.chunk_index.len(),
                manifest_data.chunk_index.iter()
                    .map(|c| c.sequence_count)
                    .sum(),
            )?;

            // Save the temporal index
            temporal_index.save()?;
        }

        // Update manifest
        self.repository.manifest = new_manifest;
        self.repository.manifest.save()?;

        Ok(DownloadResult::Updated {
            chunks_added: chunks_to_download,
            chunks_removed: chunks_to_remove,
        })
    }

    /// Handle initial download when no local manifest exists
    /// Check if the database being downloaded is taxonomy data itself
    fn is_taxonomy_database(source: &DatabaseSource) -> bool {
        use crate::download::{NCBIDatabase, UniProtDatabase};

        match source {
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => true,
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => true,
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => true,
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => true,
            _ => false,
        }
    }

    /// Check if the specific taxonomy file exists by checking manifest components
    fn has_specific_taxonomy_file(&self, source: &DatabaseSource) -> bool {
        use crate::download::{NCBIDatabase, UniProtDatabase};

        let taxonomy_dir = crate::core::paths::talaria_taxonomy_current_dir();
        if !taxonomy_dir.exists() {
            return false;
        }

        let manifest_path = taxonomy_dir.join("manifest.json");
        if !manifest_path.exists() {
            return false;
        }

        // Read the manifest and check if it has the component
        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&content) {
                let component_name = match source {
                    DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => "taxdump",
                    DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => "prot_accession2taxid",
                    DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => "nucl_accession2taxid",
                    DatabaseSource::UniProt(UniProtDatabase::IdMapping) => "idmapping",
                    _ => return false,
                };

                if let Some(components) = manifest.get("components").and_then(|c| c.as_object()) {
                    let exists = components.contains_key(component_name);
                    if exists {
                        println!("  Component '{}' already exists in manifest", component_name);
                    }
                    return exists;
                }
            }
        }

        false
    }

    /// Create or update a composite manifest for taxonomy files
    /// Now accepts the version directory to ensure consistency
    fn create_or_update_taxonomy_manifest(&self, source: &DatabaseSource, file_path: &Path, version_dir: &Path, version: &str) -> Result<()> {
        use chrono::Utc;
        use crate::download::{NCBIDatabase, UniProtDatabase};
        use crate::core::taxonomy_manager::{
            TaxonomyManifest, InstalledComponent,
            TaxonomyVersionPolicy, AuditEntry, TaxonomyManifestFormat
        };
        use std::collections::HashMap;

        // Determine manifest format and path
        let manifest_format = if self.use_json_manifest {
            TaxonomyManifestFormat::Json
        } else {
            TaxonomyManifestFormat::Talaria
        };
        let manifest_filename = format!("manifest.{}", manifest_format.extension());
        let manifest_path = version_dir.join(&manifest_filename);

        // Try to read existing manifest (try both formats for flexibility)
        let mut manifest: TaxonomyManifest = if manifest_path.exists() {
            TaxonomyManifest::read_from_file(&manifest_path)?
        } else {
            // Try the other format if our preferred one doesn't exist
            let alt_format = if self.use_json_manifest {
                TaxonomyManifestFormat::Talaria
            } else {
                TaxonomyManifestFormat::Json
            };
            let alt_path = version_dir.join(format!("manifest.{}", alt_format.extension()));

            if alt_path.exists() {
                TaxonomyManifest::read_from_file(&alt_path)?
            } else {
                // Create new manifest with all required fields
                TaxonomyManifest {
                    version: version.to_string(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    expected_components: crate::core::taxonomy_manager::TaxonomyManager::default_components(),
                    installed_components: HashMap::new(),
                    history: vec![],
                    policy: TaxonomyVersionPolicy::default(),
                }
            }
        };

        // Determine component name and metadata
        let (component_name, source_name) = match source {
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => ("taxdump", "NCBI: NCBI Taxonomy"),
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => ("prot_accession2taxid", "NCBI: Protein Accession to TaxID"),
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => ("nucl_accession2taxid", "NCBI: Nucleotide Accession to TaxID"),
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => ("idmapping", "UniProt: ID Mapping"),
            _ => return Err(anyhow::anyhow!("Unsupported taxonomy source")),
        };

        // Detect file format
        let file_format = crate::core::taxonomy_manager::TaxonomyManager::detect_file_format(file_path)?;

        // Create installed component
        let installed = InstalledComponent {
            source: source_name.to_string(),
            checksum: String::new(), // Could calculate if needed
            size: std::fs::metadata(file_path)?.len(),
            downloaded_at: Utc::now(),
            source_version: None,
            carried_from: None,
            file_path: file_path.to_path_buf(),
            compressed: file_path.extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "gz" || s == "tar")
                .unwrap_or(false),
            format: file_format,
        };

        // Add or update the component
        manifest.installed_components.insert(component_name.to_string(), installed);

        // Add audit entry
        manifest.history.push(AuditEntry {
            timestamp: Utc::now(),
            action: "component_added".to_string(),
            component: component_name.to_string(),
            details: format!("Added {} from {}", component_name, source_name),
        });

        // Update timestamp
        manifest.updated_at = Utc::now();

        // Write manifest in the chosen format
        manifest.write_to_file(&manifest_path, manifest_format)?;

        // Update symlinks only if not already done
        self.update_version_symlinks(source, version)?;

        println!("  Updated manifest component '{}': {}", component_name, manifest_path.display());
        println!("  Version: {}", version);
        Ok(())
    }

    /// Check if we should create a new taxonomy version or update current
    /// Determine if we should create a new taxonomy version using the new manager
    fn should_create_new_taxonomy_version(&self, source: &DatabaseSource) -> Result<VersionDecision> {
        // Map source to component name
        let component_name = match source {
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => "taxdump",
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => "prot_accession2taxid",
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => "nucl_accession2taxid",
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => "idmapping",
            _ => return Err(anyhow::anyhow!("Not a taxonomy database")),
        };

        // Check if running in non-interactive mode (e.g., CI)
        let interactive = atty::is(atty::Stream::Stdin);

        self.taxonomy_manager.should_create_new_version(component_name, interactive)
    }

    /// Create a new taxonomy version, optionally copying existing files
    fn create_new_taxonomy_version(&self) -> Result<PathBuf> {
        self.create_new_taxonomy_version_internal(false)
    }

    /// Create a new taxonomy version and copy forward secondary components
    fn create_new_taxonomy_version_with_copy_forward(&self) -> Result<PathBuf> {
        self.create_new_taxonomy_version_internal(true)
    }

    fn create_new_taxonomy_version_internal(&self, copy_forward: bool) -> Result<PathBuf> {
        // Create new version with UTC timestamp (explicit for consistency)
        let new_version = crate::core::paths::generate_utc_timestamp();
        println!("Creating new taxonomy version: {} (UTC)", new_version);

        let new_version_dir = crate::core::paths::talaria_taxonomy_version_dir(&new_version);
        std::fs::create_dir_all(&new_version_dir)?;

        // Copy existing files from current version if requested
        if copy_forward {
            let current_dir = crate::core::paths::talaria_taxonomy_current_dir();
            if current_dir.exists() {
                println!("\n⚠ Carrying forward secondary components from previous version:");

                // Only copy mappings directory (secondary components)
                // Don't copy tree directory (primary component - taxdump)
                let mappings_src = current_dir.join("mappings");
                if mappings_src.exists() {
                    let mappings_dst = new_version_dir.join("mappings");
                    std::fs::create_dir_all(&mappings_dst)?;

                    let mut carried_files = Vec::new();
                    for entry in std::fs::read_dir(&mappings_src)? {
                        let entry = entry?;
                        let src = entry.path();
                        let dst = mappings_dst.join(entry.file_name());

                        // Get file metadata for age calculation
                        if let Ok(metadata) = std::fs::metadata(&src) {
                            if let Ok(modified) = metadata.modified() {
                                let age = std::time::SystemTime::now()
                                    .duration_since(modified)
                                    .unwrap_or_default();
                                let age_days = age.as_secs() / 86400;

                                carried_files.push((entry.file_name().to_string_lossy().to_string(), age_days));
                            }
                        }

                        std::fs::copy(&src, &dst)?;
                    }

                    // Show what was carried forward
                    for (file, age_days) in carried_files {
                        println!("  • {} ({} days old)", file, age_days);
                    }

                    println!("\nConsider updating these components with:");
                    println!("  talaria database download --complete ncbi/taxonomy\n");
                }
            }
        }

        // Update current symlink
        let current_link = crate::core::paths::talaria_taxonomy_versions_dir().join("current");
        if current_link.exists() {
            std::fs::remove_file(&current_link)?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&new_version, &current_link)?;
        #[cfg(windows)]
        std::fs::write(&current_link, &new_version)?;

        Ok(new_version_dir)
    }

    /// Store taxonomy mapping files directly without FASTA processing
    fn store_taxonomy_mapping_file(&mut self, file_path: &Path, source: &DatabaseSource) -> Result<()> {
        use crate::download::{NCBIDatabase, UniProtDatabase};

        println!("Storing taxonomy mapping file...");

        // Check if we should create a new version or update current
        let version_decision = self.should_create_new_taxonomy_version(source)?;

        let (taxonomy_dir, version) = match version_decision {
            VersionDecision::CreateNew { copy_forward, reason } => {
                println!("Creating new taxonomy version: {}", reason);

                // Create new version and optionally copy existing files
                let new_dir = if copy_forward {
                    self.create_new_taxonomy_version_with_copy_forward()?
                } else {
                    self.create_new_taxonomy_version()?
                };

                let version = new_dir.file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| anyhow::anyhow!("Failed to get version from directory"))?
                    .to_string();
                println!("Created new taxonomy version: {}", version);
                (new_dir, version)
            }
            VersionDecision::AppendToCurrent => {
                // Use current version for additive updates
                let current = crate::core::paths::talaria_taxonomy_current_dir();
                if !current.exists() {
                    // First file - create initial version
                    println!("No current taxonomy version found, creating initial version...");
                    let new_dir = self.create_new_taxonomy_version()?;
                    let version = new_dir.file_name()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| anyhow::anyhow!("Failed to get version from directory"))?
                        .to_string();
                    (new_dir, version)
                } else {
                    // IMPORTANT: Always resolve symlink to get the actual directory
                    let actual_dir = if current.is_symlink() {
                        let target = std::fs::read_link(&current)?;
                        if target.is_relative() {
                            current.parent()
                                .ok_or_else(|| anyhow::anyhow!("Failed to get parent directory"))?
                                .join(target)
                        } else {
                            target
                        }
                    } else {
                        current.clone()
                    };

                    let version = actual_dir.file_name()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| anyhow::anyhow!("Failed to get version from directory"))?
                        .to_string();

                    println!("Adding to existing taxonomy version: {}", version);
                    // Return actual_dir, not current (which might be a symlink)
                    (actual_dir, version)
                }
            }
            VersionDecision::UserCancelled => {
                println!("Operation cancelled by user");
                return Ok(());
            }
        };

        // Create appropriate subdirectories
        let tree_dir = taxonomy_dir.join("tree");
        let mappings_dir = taxonomy_dir.join("mappings");

        std::fs::create_dir_all(&tree_dir)?;
        std::fs::create_dir_all(&mappings_dir)?;

        // Determine the destination based on source type
        match source {
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => {
                // Extract taxonomy dump to tree/ subdirectory
                println!("Extracting taxonomy dump to tree/ directory...");
                let tar_gz = std::fs::File::open(file_path)?;
                let tar = flate2::read::GzDecoder::new(tar_gz);
                let mut archive = tar::Archive::new(tar);
                archive.unpack(&tree_dir)?;
                println!("Taxonomy dump extracted successfully");

                // Create manifest for the extracted taxonomy files
                // Use nodes.dmp as a representative file for the whole taxonomy dump
                let nodes_file = tree_dir.join("nodes.dmp");
                if nodes_file.exists() {
                    // taxonomy_dir is already the actual version directory from above
                    self.create_or_update_taxonomy_manifest(source, &nodes_file, &taxonomy_dir, &version)?;
                }

                return Ok(());
            }
            _ => {}
        }

        // For mapping files, determine the destination filename (simplified naming)
        let dest_file = match source {
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => {
                mappings_dir.join("idmapping.dat.gz")
            }
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => {
                mappings_dir.join("prot.accession2taxid.gz")
            }
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => {
                mappings_dir.join("nucl.accession2taxid.gz")
            }
            _ => unreachable!(),
        };

        // Move the file to its destination
        println!("Moving taxonomy file to: {}", dest_file.display());

        // First try rename (atomic and fast)
        if std::fs::rename(file_path, &dest_file).is_err() {
            // If rename fails (e.g., across filesystems), copy and delete
            std::fs::copy(file_path, &dest_file)?;
            // Don't delete the source file here - let the caller handle cleanup
            // This prevents the "file not found" error when caller tries to clean up
        }

        println!("✓ Taxonomy mapping file stored successfully");
        println!("  Location: {}", dest_file.display());

        // Update the composite manifest with this component
        // taxonomy_dir is already the actual version directory from above
        self.create_or_update_taxonomy_manifest(source, &dest_file, &taxonomy_dir, &version)?;

        Ok(())
    }

    async fn handle_initial_download(
        &mut self,
        source: &DatabaseSource,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        // Skip taxonomy check if we're downloading taxonomy data itself
        if !Self::is_taxonomy_database(source) {
            // Check if taxonomy is needed and download if missing
            if !self.repository.taxonomy.has_taxonomy() {
                progress_callback("Checking for taxonomy data...");
                if let Err(e) = self.ensure_taxonomy_loaded(&progress_callback).await {
                    progress_callback(&format!("[!] Warning: Could not load taxonomy: {}", e));
                    progress_callback("Continuing without taxonomy data (will use placeholders)");
                    // Ensure at least a minimal taxonomy structure
                    self.repository.taxonomy.ensure_taxonomy()?;
                }
            }
        }

        // For initial download, fall back to traditional download
        // then chunk it into CASG format
        // Use appropriate extension based on database type
        let temp_file = if Self::is_taxonomy_database(source) {
            self.base_path.join("temp_download.tar.gz")
        } else {
            self.base_path.join("temp_download.fasta.gz")
        };

        progress_callback("Downloading full database (this may take a while)...");

        // Download full file
        self.download_full_database(source, &temp_file, &progress_callback).await?;

        // Chunk the database
        progress_callback("Processing database into CASG chunks...");
        progress_callback("This one-time conversion enables future incremental updates");
        self.chunk_database(&temp_file, source)?;

        // Clean up temp file
        if temp_file.exists() {
            std::fs::remove_file(&temp_file).ok();
        }

        progress_callback("✓ Initial CASG setup complete!");
        progress_callback("Future updates will only download changed chunks");

        Ok(DownloadResult::InitialDownload)
    }

    /// Chunk sequences directly into CASG format (unified pipeline)
    pub fn chunk_sequences_direct(&mut self, sequences: Vec<Sequence>, source: &DatabaseSource) -> Result<()> {

        // Load taxonomy mapping if available
        let taxonomy_map = self.load_taxonomy_mapping(source)?;

        // Create chunker with strategy
        let mut chunker = TaxonomicChunker::new(ChunkingStrategy::default());
        chunker.load_taxonomy_mapping(taxonomy_map);

        // If this is a custom database with user-specified taxids, enrich sequences
        let sequences = sequences;
        if let DatabaseSource::Custom(name) = source {
            // Try to extract taxids from the database name or metadata
            // For now, the taxids should have been set during fetch
            tracing::info!("Processing custom database: {}", name);
        }

        // Use the new trait-based chunking with validation
        println!("Creating taxonomy-aware chunks with validation...");
        let chunks = chunker.chunk_with_validation(sequences)?;

        println!("Created {} chunks", chunks.len());

        // Store chunks in CASG (rest of the method remains the same)
        self.store_chunks_in_casg(chunks, source)?;

        Ok(())
    }

    /// Store chunks in CASG repository
    fn store_chunks_in_casg(&mut self, chunks: Vec<TaxonomyAwareChunk>, source: &DatabaseSource) -> Result<()> {
        // Store chunks in CASG with parallel processing
        let total_chunks = chunks.len();
        let pb = ProgressBar::new(total_chunks as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("##-"),
        );
        pb.set_message("Storing chunks in CASG repository");

        // Create progress tracking with atomic counter for lock-free updates
        let progress_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let progress_counter_clone = progress_counter.clone();

        // Spawn a separate thread to update progress bar without blocking workers
        let pb_handle = {
            let pb_clone = pb.clone();
            let total = total_chunks;
            std::thread::spawn(move || {
                while progress_counter_clone.load(std::sync::atomic::Ordering::Relaxed) < total {
                    let current = progress_counter_clone.load(std::sync::atomic::Ordering::Relaxed);
                    pb_clone.set_position(current as u64);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                // Final update to ensure we show 100%
                pb_clone.set_position(total as u64);
                pb_clone.finish_and_clear();
            })
        };

        let storage = &self.repository.storage;

        // Process chunks in parallel and collect taxonomy mappings
        let results: Vec<_> = chunks
            .par_iter()
            .map(|chunk| {
                // Store chunk in storage (already thread-safe but with compressor issue)
                let store_result = storage.store_taxonomy_chunk(chunk);

                // Increment progress atomically (lock-free)
                progress_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                // Return both result and chunk for taxonomy update
                (store_result, chunk)
            })
            .collect();

        // Wait for progress thread to finish
        pb_handle.join().unwrap();

        // Now do a single bulk update of all taxonomy mappings (no contention!)
        for (result, chunk) in &results {
            if result.is_ok() {
                self.repository.taxonomy.update_chunk_mapping(chunk);
            }
        }

        // Progress bar already finished by the thread, no need to call finish again
        // Small delay to ensure terminal has finished updating
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Check for any errors
        for (result, _) in results {
            result?;
        }

        // Create and save manifest
        let manifest_spinner = create_spinner("Creating and saving manifest...");
        let mut manifest_data = self.repository.manifest.create_from_chunks(
            chunks,
            self.repository.taxonomy.get_taxonomy_root()?,
            self.repository.storage.get_sequence_root()?,
        )?;
        manifest_spinner.finish_and_clear();
        println!("✓ Manifest created");

        // Set the source database
        manifest_data.source_database = Some(match source {
            DatabaseSource::UniProt(UniProtDatabase::SwissProt) => "uniprot/swissprot".to_string(),
            DatabaseSource::UniProt(UniProtDatabase::TrEMBL) => "uniprot/trembl".to_string(),
            DatabaseSource::NCBI(NCBIDatabase::NR) => "ncbi/nr".to_string(),
            DatabaseSource::NCBI(NCBIDatabase::NT) => "ncbi/nt".to_string(),
            DatabaseSource::Custom(name) => format!("custom/{}", name),
            _ => "custom".to_string(),
        });

        // Save manifest to versioned database-specific location
        let save_spinner = create_spinner("Saving manifest to disk...");
        let (manifest_path, version) = self.create_versioned_manifest_path(source, self.use_json_manifest)?;

        if self.use_json_manifest {
            // Write JSON format if requested
            let json_content = serde_json::to_string_pretty(&manifest_data)?;
            std::fs::write(&manifest_path, json_content)?;
        } else {
            // Write binary format by default
            let msgpack_data = rmp_serde::to_vec(&manifest_data)?;

            // Create .tal format with header
            let mut tal_content = Vec::new();
            tal_content.extend_from_slice(b"TAL\x01"); // Magic + version
            tal_content.extend_from_slice(&msgpack_data);

            std::fs::write(&manifest_path, tal_content)?;
        }
        save_spinner.finish_with_message("✓ Manifest saved");

        // Create version metadata with upstream detection
        let version_spinner = create_spinner("Creating version metadata...");
        self.create_version_metadata(source, &version, &manifest_path)?;

        // Create symlinks for easy access (including upstream version if detected)
        self.update_version_symlinks(source, &version)?;
        version_spinner.finish_and_clear();
        println!("✓ Version metadata created");

        // Track version in temporal index
        let temporal_spinner = create_spinner("Updating temporal index...");
        let temporal_path = self.base_path.clone();
        let mut temporal_index = crate::casg::temporal::TemporalIndex::load(&temporal_path)?;

        // Add sequence version tracking
        temporal_index.add_sequence_version(
            manifest_data.version.clone(),
            manifest_data.sequence_root.clone(),
            manifest_data.chunk_index.len(),
            manifest_data.chunk_index.iter()
                .map(|c| c.sequence_count)
                .sum(),
        )?;

        // Save the temporal index
        temporal_index.save()?;
        temporal_spinner.finish_and_clear();
        println!("✓ Version history updated");

        // Also update the repository's manifest for immediate use
        self.repository.manifest.set_data(manifest_data);

        println!("✓ Manifest saved successfully to {}", manifest_path.display());
        println!("  Version: {}", version);

        println!("Database successfully stored in CASG format");
        Ok(())
    }

    /// Chunk a downloaded database into CASG format (legacy wrapper for FASTA files)
    pub fn chunk_database(&mut self, file_path: &Path, source: &DatabaseSource) -> Result<()> {
        // Check if this is a taxonomy mapping file (not a FASTA file)
        if Self::is_taxonomy_database(source) {
            // Store taxonomy files in their proper location
            return self.store_taxonomy_mapping_file(file_path, source);
        }

        // Read sequences from FASTA file
        println!("Reading sequences from FASTA file...");
        let sequences = self.read_fasta_sequences(file_path)?;

        // Use the unified pipeline
        self.chunk_sequences_direct(sequences, source)?;

        Ok(())
    }

    /// Original chunking logic (kept for reference but not used)

    /// Create version metadata file with upstream version detection
    fn create_version_metadata(&self, source: &DatabaseSource, timestamp: &str, manifest_path: &Path) -> Result<()> {
        use crate::utils::version_detector::{VersionDetector, DatabaseVersion};
        

        let (source_name, dataset) = self.get_source_dataset_names(source);

        // Create the version metadata
        let mut version = DatabaseVersion::new(&source_name, &dataset);
        version.timestamp = timestamp.to_string();

        // Try to detect upstream version from the manifest or database content
        let detector = VersionDetector::new();

        let mut upstream_version = None;

        // Try to detect from manifest first
        if let Ok(detected) = detector.detect_from_manifest(&manifest_path.to_string_lossy()) {
            upstream_version = detected.upstream_version;
        }

        // If no upstream version detected, generate one from the timestamp based on database type
        if upstream_version.is_none() && timestamp.len() >= 8 {
            upstream_version = match source {
                DatabaseSource::UniProt(_) => {
                    // Convert timestamp to UniProt monthly format: YYYY_MM
                    let year = &timestamp[0..4];
                    let month = &timestamp[4..6];
                    Some(format!("{}_{}", year, month))
                },
                DatabaseSource::NCBI(_) => {
                    // Convert timestamp to NCBI date format: YYYY-MM-DD
                    if timestamp.len() >= 8 {
                        let year = &timestamp[0..4];
                        let month = &timestamp[4..6];
                        let day = &timestamp[6..8];
                        Some(format!("{}-{}-{}", year, month, day))
                    } else {
                        None
                    }
                },
                _ => None,
            };
        }

        // Set upstream version and create aliases/symlinks
        if let Some(upstream) = upstream_version {
            version.upstream_version = Some(upstream.clone());
            version.aliases.upstream.push(upstream.clone());

            // Create symlink for upstream version
            let versions_dir = manifest_path.parent().unwrap().parent().unwrap();
            let upstream_link = versions_dir.join(&upstream);

            #[cfg(unix)]
            {
                use std::os::unix::fs;
                // Remove if exists
                if upstream_link.exists() {
                    std::fs::remove_file(&upstream_link).ok();
                }
                // Create symlink to timestamp directory
                fs::symlink(timestamp, &upstream_link).ok();
            }
        }

        // Save version.json in the version directory
        let version_dir = manifest_path.parent().unwrap();
        let version_file = version_dir.join("version.json");
        let json = serde_json::to_string_pretty(&version)?;
        std::fs::write(version_file, json)?;

        Ok(())
    }

    /// Update symlinks for version management
    fn update_version_symlinks(&self, source: &DatabaseSource, version: &str) -> Result<()> {
        // Use get_versions_dir for consistent path handling (including unified taxonomy)
        let versions_dir = self.get_versions_dir(source);

        // Create/update 'current' symlink
        let current_link = versions_dir.join("current");
        if current_link.exists() {
            std::fs::remove_file(&current_link)?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(version, &current_link)?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(versions_dir.join(version), &current_link)?;

        // Create temporal aliases based on the timestamp
        if version.len() >= 8 {
            let temporal_alias = match source {
                DatabaseSource::UniProt(_) => {
                    // Create monthly format alias: YYYY_MM
                    let year = &version[0..4];
                    let month = &version[4..6];
                    Some(format!("{}_{}", year, month))
                },
                DatabaseSource::NCBI(_) => {
                    // Create date format alias: YYYY-MM-DD
                    let year = &version[0..4];
                    let month = &version[4..6];
                    let day = &version[6..8];
                    Some(format!("{}-{}-{}", year, month, day))
                },
                _ => None,
            };

            // Create temporal alias symlink if applicable
            if let Some(alias) = temporal_alias {
                let alias_link = versions_dir.join(&alias);
                if alias_link.exists() {
                    std::fs::remove_file(&alias_link).ok();
                }
                #[cfg(unix)]
                std::os::unix::fs::symlink(version, &alias_link).ok();
                #[cfg(windows)]
                std::os::windows::fs::symlink_dir(versions_dir.join(version), &alias_link).ok();

                // Also update version.json with the alias if it exists
                let version_dir = versions_dir.join(version);
                let version_file = version_dir.join("version.json");
                if version_file.exists() {
                    use crate::utils::version_detector::DatabaseVersion;
                    if let Ok(content) = std::fs::read_to_string(&version_file) {
                        if let Ok(mut version_data) = serde_json::from_str::<DatabaseVersion>(&content) {
                            // Update upstream version if not set
                            if version_data.upstream_version.is_none() {
                                version_data.upstream_version = Some(alias.clone());
                            }
                            // Add to upstream aliases if not present
                            if !version_data.aliases.upstream.contains(&alias) {
                                version_data.aliases.upstream.push(alias);
                            }
                            // Save updated version.json
                            if let Ok(json) = serde_json::to_string_pretty(&version_data) {
                                std::fs::write(version_file, json).ok();
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get manifest URL for a database source
    fn get_manifest_url(&self, source: &DatabaseSource) -> Result<String> {
        // Check environment variable for manifest server
        if let Ok(manifest_server) = std::env::var("TALARIA_MANIFEST_SERVER") {
            return Ok(match source {
                DatabaseSource::UniProt(UniProtDatabase::SwissProt) =>
                    format!("{}/uniprot-swissprot.json", manifest_server),
                DatabaseSource::UniProt(UniProtDatabase::TrEMBL) =>
                    format!("{}/uniprot-trembl.json", manifest_server),
                DatabaseSource::NCBI(NCBIDatabase::NR) =>
                    format!("{}/ncbi-nr.json", manifest_server),
                DatabaseSource::NCBI(NCBIDatabase::NT) =>
                    format!("{}/ncbi-nt.json", manifest_server),
                _ => anyhow::bail!("No manifest URL for this database source"),
            });
        }

        // No manifest server configured - this is fine for local/dev use
        anyhow::bail!("No manifest server configured (set TALARIA_MANIFEST_SERVER for remote updates)")
    }

    /// Get the current manifest path for reading an existing database
    /// This looks for the 'current' symlink in the versioned directory structure
    fn get_manifest_path(&self, source: &DatabaseSource) -> PathBuf {
        let versions_dir = self.get_versions_dir(source);

        let current_link = versions_dir.join("current");
        if current_link.exists() {
            if let Ok(target) = std::fs::read_link(&current_link) {
                let manifest_path = if target.is_absolute() {
                    target.join("manifest.tal")
                } else {
                    versions_dir.join(target).join("manifest.tal")
                };
                if manifest_path.exists() {
                    return manifest_path;
                }
                // Try JSON if .tal doesn't exist
                let json_path = manifest_path.with_extension("json");
                if json_path.exists() {
                    return json_path;
                }
            }
        }

        // Return expected path even if it doesn't exist yet
        versions_dir.join("current").join("manifest.tal")
    }

    /// Create a new versioned manifest path for saving a database
    /// Returns (manifest_path, version_string)
    fn create_versioned_manifest_path(&self, source: &DatabaseSource, use_json: bool) -> Result<(PathBuf, String)> {
        // Generate timestamp version
        let version = crate::core::paths::generate_utc_timestamp();

        // Create versioned path
        let version_dir = self.get_versions_dir(source).join(&version);

        // Create directory if it doesn't exist
        std::fs::create_dir_all(&version_dir)?;

        let extension = if use_json { "json" } else { "tal" };
        let manifest_path = version_dir.join(format!("manifest.{}", extension));
        Ok((manifest_path, version))
    }

    /// Get the versions directory for a database source
    fn get_versions_dir(&self, source: &DatabaseSource) -> PathBuf {
        use crate::download::NCBIDatabase;

        // Special handling for taxonomy - use unified directory
        if matches!(source,
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) |
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) |
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) |
            DatabaseSource::UniProt(crate::download::UniProtDatabase::IdMapping)) {
            return crate::core::paths::talaria_taxonomy_versions_dir();
        }

        let (source_name, dataset) = self.get_source_dataset_names(source);
        self.base_path.join("versions").join(source_name).join(dataset)
    }

    /// Get source and dataset names for directory structure
    fn get_source_dataset_names(&self, source: &DatabaseSource) -> (String, String) {
        use crate::download::{NCBIDatabase, UniProtDatabase};

        match source {
            DatabaseSource::UniProt(UniProtDatabase::SwissProt) => ("uniprot".to_string(), "swissprot".to_string()),
            DatabaseSource::UniProt(UniProtDatabase::TrEMBL) => ("uniprot".to_string(), "trembl".to_string()),
            DatabaseSource::UniProt(UniProtDatabase::UniRef50) => ("uniprot".to_string(), "uniref50".to_string()),
            DatabaseSource::UniProt(UniProtDatabase::UniRef90) => ("uniprot".to_string(), "uniref90".to_string()),
            DatabaseSource::UniProt(UniProtDatabase::UniRef100) => ("uniprot".to_string(), "uniref100".to_string()),
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => ("uniprot".to_string(), "idmapping".to_string()),
            DatabaseSource::NCBI(NCBIDatabase::NR) => ("ncbi".to_string(), "nr".to_string()),
            DatabaseSource::NCBI(NCBIDatabase::NT) => ("ncbi".to_string(), "nt".to_string()),
            DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein) => ("ncbi".to_string(), "refseq-protein".to_string()),
            DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic) => ("ncbi".to_string(), "refseq-genomic".to_string()),
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => ("ncbi".to_string(), "taxonomy".to_string()),
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => ("ncbi".to_string(), "prot-accession2taxid".to_string()),
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => ("ncbi".to_string(), "nucl-accession2taxid".to_string()),
            DatabaseSource::Custom(name) => ("custom".to_string(), name.clone()),
        }
    }

    /// Download full database (for initial setup)
    async fn download_full_database(
        &self,
        source: &DatabaseSource,
        output_path: &Path,
        progress_callback: &impl Fn(&str),
    ) -> Result<()> {
        use crate::download::DownloadProgress;

        progress_callback("Downloading full database...");

        let mut progress = DownloadProgress::new();
        crate::download::download_database(
            source.clone(),
            output_path,
            &mut progress,
        ).await?;

        Ok(())
    }

    /// Get taxonomy mapping from CASG manifest
    /// This extracts accession-to-taxid mappings directly from the manifest's chunk metadata
    pub fn get_taxonomy_mapping_from_manifest(&self, source: &DatabaseSource) -> Result<std::collections::HashMap<String, crate::casg::TaxonId>> {
        use std::collections::HashMap;

        // Load manifest for this database
        let manifest_path = self.get_manifest_path(source);
        if !manifest_path.exists() {
            anyhow::bail!("Database manifest not found. Run download first.");
        }

        let manifest = self.read_manifest(&manifest_path)?;

        let mut mapping = HashMap::new();

        let pb = crate::utils::progress::create_progress_bar(
            manifest.chunk_index.len() as u64,
            &format!("Processing {} chunks from manifest", manifest.chunk_index.len())
        );

        let mut chunks_with_taxids = 0;
        let mut chunks_without_taxids = 0;

        // For each chunk, we need to load its sequences to get the accessions
        // and map them to the chunk's TaxIDs
        for (_idx, chunk_meta) in manifest.chunk_index.iter().enumerate() {
            pb.inc(1);

            if chunk_meta.taxon_ids.is_empty() {
                chunks_without_taxids += 1;
                continue; // Skip chunks without taxonomy
            }

            chunks_with_taxids += 1;

            // Load the chunk to get sequence headers
            let chunk_data = self.repository.storage.get_chunk(&chunk_meta.hash)?;

            // Parse sequences from chunk
            let sequences = crate::bio::fasta::parse_fasta_from_bytes(&chunk_data)?;

            // Map each sequence to the chunk's primary TaxID
            // Note: chunks are organized by taxonomy, so all sequences in a chunk
            // should have the same TaxID
            let primary_taxid = chunk_meta.taxon_ids[0];

            for seq in sequences {
                // Extract accession from sequence ID/header
                if let Some(accession) = Self::extract_accession_from_header(&seq.id) {
                    mapping.insert(accession.clone(), primary_taxid);

                    // Also store without version suffix if present
                    if let Some(dot_pos) = accession.rfind('.') {
                        mapping.insert(accession[..dot_pos].to_string(), primary_taxid);
                    }
                }
            }
        }

        pb.finish_with_message(format!(
            "Processed {} chunks ({} with taxonomy, {} without). Extracted {} mappings",
            manifest.chunk_index.len(),
            chunks_with_taxids,
            chunks_without_taxids,
            mapping.len()
        ));
        Ok(mapping)
    }

    /// Extract accession from FASTA header
    fn extract_accession_from_header(header: &str) -> Option<String> {
        // UniProt format: sp|P12345|PROT1_HUMAN or tr|Q12345|...
        if header.starts_with("sp|") || header.starts_with("tr|") {
            let parts: Vec<&str> = header.split('|').collect();
            if parts.len() >= 2 {
                return Some(parts[1].to_string());
            }
        }

        // NCBI format: might be just the accession or gi|12345|ref|NP_123456.1|
        if header.contains('|') {
            let parts: Vec<&str> = header.split('|').collect();
            // Look for ref| or gb| or similar
            for (i, part) in parts.iter().enumerate() {
                if (*part == "ref" || *part == "gb" || *part == "emb" || *part == "dbj")
                    && i + 1 < parts.len() {
                    return Some(parts[i + 1].to_string());
                }
            }
        }

        // Simple format: just accession (possibly with version)
        let first_part = header.split_whitespace().next()?;
        Some(first_part.to_string())
    }

    /// Create a temporary accession2taxid file from manifest mapping
    pub fn create_accession2taxid_from_manifest(&self, source: &DatabaseSource) -> Result<PathBuf> {
        let mapping = self.get_taxonomy_mapping_from_manifest(source)?;

        // Create temporary file with .accession2taxid extension (required by LAMBDA)
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("talaria_manifest_{}.accession2taxid",
                                              std::process::id()));

        use std::io::Write;
        let mut file = std::fs::File::create(&temp_file)?;

        // Write header (NCBI format)
        writeln!(file, "accession\taccession.version\ttaxid\tgi")?;

        // Write mappings
        for (accession, taxid) in mapping {
            // Write in NCBI prot.accession2taxid format
            // accession, accession.version, taxid, gi (we use 0 for gi)
            writeln!(file, "{}\t{}\t{}\t0", accession, accession, taxid.0)?;
        }

        println!("Created temporary accession2taxid file with manifest data: {:?}", temp_file);
        Ok(temp_file)
    }

    /// Load taxonomy mapping for a database
    fn load_taxonomy_mapping(&self, source: &DatabaseSource) -> Result<std::collections::HashMap<String, crate::casg::TaxonId>> {
        use std::collections::HashMap;
        use flate2::read::GzDecoder;
        use std::io::{BufRead, BufReader};
        use std::fs::File;

        // Load from unified taxonomy mappings directory
        let mappings_dir = crate::core::paths::talaria_taxonomy_current_dir().join("mappings");
        let mapping_file = match source {
            DatabaseSource::UniProt(_) => mappings_dir.join("uniprot_idmapping.dat.gz"),
            DatabaseSource::NCBI(_) => mappings_dir.join("prot.accession2taxid.gz"),
            _ => return Ok(HashMap::new()),
        };

        if !mapping_file.exists() {
            return Ok(HashMap::new());
        }

        eprintln!("● Loading taxonomy mapping from {}", mapping_file.display());
        let mut mappings = HashMap::new();

        let file = File::open(&mapping_file)?;
        let decoder = GzDecoder::new(file);
        let reader = BufReader::new(decoder);

        let pb = crate::utils::progress::create_spinner("Parsing taxonomy mappings");
        let mut line_count = 0;

        match source {
            DatabaseSource::UniProt(_) => {
                // UniProt idmapping format: accession<tab>type<tab>value
                // We're looking for: P12345<tab>NCBI_TaxID<tab>9606
                for line_result in reader.lines() {
                    let line = line_result?;
                    line_count += 1;

                    if line_count % 100000 == 0 {
                        pb.set_message(format!("Processed {} mappings", line_count));
                    }

                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 3 && parts[1] == "NCBI_TaxID" {
                        if let Ok(taxid) = parts[2].parse::<u32>() {
                            mappings.insert(parts[0].to_string(), crate::casg::TaxonId(taxid));
                        }
                    }
                }
            }
            DatabaseSource::NCBI(_) => {
                // NCBI prot.accession2taxid format:
                // accession.version<tab>taxid<tab>gi
                // Skip header line
                let mut lines = reader.lines();
                lines.next(); // Skip header

                for line_result in lines {
                    let line = line_result?;
                    line_count += 1;

                    if line_count % 100000 == 0 {
                        pb.set_message(format!("Processed {} mappings", line_count));
                    }

                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 2 {
                        if let Ok(taxid) = parts[1].parse::<u32>() {
                            // Store both with and without version
                            let accession = parts[0].to_string();
                            mappings.insert(accession.clone(), crate::casg::TaxonId(taxid));

                            // Also store without version suffix
                            if let Some(dot_pos) = accession.rfind('.') {
                                mappings.insert(accession[..dot_pos].to_string(), crate::casg::TaxonId(taxid));
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        pb.finish_and_clear();
        println!("Loaded {} taxonomy mappings", mappings.len());
        Ok(mappings)
    }

    /// Ensure taxonomy is loaded, downloading if necessary
    async fn ensure_taxonomy_loaded(
        &mut self,
        progress_callback: &impl Fn(&str),
    ) -> Result<()> {
        let taxonomy_dir = crate::core::paths::talaria_taxonomy_current_dir();
        let taxdump_dir = taxonomy_dir.join("tree");

        // Check if taxonomy dump files exist
        let nodes_file = taxdump_dir.join("nodes.dmp");
        let names_file = taxdump_dir.join("names.dmp");

        if !nodes_file.exists() || !names_file.exists() {
            progress_callback("Taxonomy data not found, downloading NCBI taxonomy...");

            // Create taxonomy directory
            std::fs::create_dir_all(&taxdump_dir)?;

            // Download NCBI taxonomy
            let taxdump_url = "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/taxdump.tar.gz";
            let taxdump_file = taxdump_dir.join("taxdump.tar.gz");

            progress_callback("Downloading NCBI taxonomy dump...");

            // Use reqwest to download
            let response = reqwest::get(taxdump_url).await?;
            let bytes = response.bytes().await?;
            std::fs::write(&taxdump_file, bytes)?;

            progress_callback("Extracting taxonomy files...");

            // Extract the tar.gz file
            use flate2::read::GzDecoder;
            use tar::Archive;

            let tar_gz = std::fs::File::open(&taxdump_file)?;
            let tar = GzDecoder::new(tar_gz);
            let mut archive = Archive::new(tar);
            archive.unpack(&taxdump_dir)?;

            // Clean up tar file
            std::fs::remove_file(taxdump_file).ok();

            progress_callback("Taxonomy files downloaded and extracted");
        }

        // Load the taxonomy
        progress_callback("Loading taxonomy data...");
        self.repository.taxonomy.load_ncbi_taxonomy(&taxdump_dir)?;
        progress_callback("Taxonomy loaded successfully");

        Ok(())
    }

    /// Get reduction profiles for a specific database
    pub fn get_reduction_profiles_for_database(&self, db_name: &str) -> Result<Vec<String>> {
        // Parse the database name
        let parts: Vec<&str> = db_name.split('/').collect();
        if parts.len() != 2 {
            // For backward compatibility, also check the old global profiles
            let mut matching_profiles = Vec::new();
            let profiles = self.repository.storage.list_reduction_profiles()?;
            for profile_name in &profiles {
                if let Ok(Some(manifest)) = self.repository.storage.get_reduction_by_profile(profile_name) {
                    if manifest.source_database == db_name {
                        matching_profiles.push(profile_name.clone());
                    }
                }
            }
            return Ok(matching_profiles);
        }

        let source = parts[0];
        let dataset = parts[1];

        // Get profiles from the version-specific directories
        self.repository.storage.list_database_reduction_profiles(source, dataset, None)
    }

    /// Find the latest version directory in a dataset path
    fn find_latest_version_dir(&self, dataset_path: &Path) -> Result<Option<PathBuf>> {
        use crate::utils::version_detector::is_timestamp_format;

        let mut latest_dir = None;
        let mut latest_timestamp = String::new();

        for entry in std::fs::read_dir(dataset_path)? {
            let entry = entry?;
            let path = entry.path();

            // Skip if not a directory or is a symlink
            if !path.is_dir() || path.is_symlink() {
                continue;
            }

            if let Some(dir_name) = path.file_name().and_then(|s| s.to_str()) {
                // Check if it's a timestamp directory
                if is_timestamp_format(dir_name) {
                    // Keep the latest timestamp
                    if dir_name > latest_timestamp.as_str() {
                        latest_timestamp = dir_name.to_string();
                        latest_dir = Some(path);
                    }
                }
            }
        }

        Ok(latest_dir)
    }

    /// Read a manifest file (supports both .tal and .json formats)
    fn read_manifest(&self, path: &Path) -> Result<crate::casg::TemporalManifest> {
        let content = std::fs::read(path)?;

        // Check if it's a .tal file (binary format with magic header)
        if path.extension().and_then(|s| s.to_str()) == Some("tal") {
            // Check for TALARIA_MAGIC header
            if content.len() > TALARIA_MAGIC.len() &&
               &content[..TALARIA_MAGIC.len()] == TALARIA_MAGIC {
                // Skip magic header and deserialize MessagePack data
                let manifest_bytes = &content[TALARIA_MAGIC.len()..];
                let manifest: crate::casg::TemporalManifest = rmp_serde::from_slice(manifest_bytes)?;
                return Ok(manifest);
            }
        }

        // Try to parse as JSON (works for both .json files and .tal files without magic header)
        let manifest: crate::casg::TemporalManifest = serde_json::from_slice(&content)?;
        Ok(manifest)
    }

    /// List all available databases in CASG
    pub fn list_databases(&self) -> Result<Vec<DatabaseInfo>> {
        let mut databases = Vec::new();

        // First, check if base_path exists
        if !self.base_path.exists() {
            return Ok(databases);
        }

        // Look for databases in the versions/ directory structure
        let versions_dir = self.base_path.join("versions");
        if !versions_dir.exists() {
            return Ok(databases);
        }

        // Traverse versions/{source}/{dataset}/ structure
        for source_entry in std::fs::read_dir(&versions_dir)? {
            let source_entry = source_entry?;
            let source_path = source_entry.path();

            if !source_path.is_dir() {
                continue;
            }

            let source_name = source_path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            // Iterate through datasets within each source
            for dataset_entry in std::fs::read_dir(&source_path)? {
                let dataset_entry = dataset_entry?;
                let dataset_path = dataset_entry.path();

                if !dataset_path.is_dir() {
                    continue;
                }

                let dataset_name = dataset_path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Try to find the current version via symlink first
                let current_link = dataset_path.join("current");
                let version_dir = if current_link.exists() && current_link.is_symlink() {
                    // Follow the symlink to get the actual version directory
                    if let Ok(target) = std::fs::read_link(&current_link) {
                        if target.is_absolute() {
                            Some(target)
                        } else {
                            Some(dataset_path.join(target))
                        }
                    } else {
                        // If symlink is broken, find the latest timestamp directory
                        self.find_latest_version_dir(&dataset_path)?
                    }
                } else {
                    // No current symlink, find the latest timestamp directory
                    self.find_latest_version_dir(&dataset_path)?
                };

                if version_dir.is_none() {
                    continue;
                }

                let version_dir = version_dir.unwrap();

                // Look for manifest file (.tal or .json)
                let tal_manifest = version_dir.join("manifest.tal");
                let json_manifest = version_dir.join("manifest.json");

                let manifest_path = if tal_manifest.exists() {
                    tal_manifest
                } else if json_manifest.exists() {
                    json_manifest
                } else {
                    continue;
                };

                // Read and parse the manifest
                if let Ok(manifest) = self.read_manifest(&manifest_path) {
                    let db_name = format!("{}/{}", source_name, dataset_name);

                    // Get reduction profiles for this database
                    let reduction_profiles = self.get_reduction_profiles_for_database(&db_name)
                        .unwrap_or_default();

                    databases.push(DatabaseInfo {
                        name: db_name,
                        version: manifest.version,
                        created_at: manifest.created_at,
                        chunk_count: manifest.chunk_index.len(),
                        total_size: manifest.chunk_index.iter().map(|c| c.size).sum(),
                        reduction_profiles,
                    });
                }
            }
        }

        Ok(databases)
    }

    /// Initialize temporal tracking for existing data
    pub fn init_temporal_for_existing(&mut self) -> Result<()> {
        let temporal_path = self.base_path.clone();
        let mut temporal_index = crate::casg::temporal::TemporalIndex::load(&temporal_path)?;

        // Check if temporal index is empty
        let history = temporal_index.get_version_history(1)?;
        if !history.is_empty() {
            // Already has history
            return Ok(());
        }

        // Check for existing manifest
        let root_manifest = self.base_path.join("manifest.json");
        if root_manifest.exists() {
            if let Ok(content) = std::fs::read_to_string(&root_manifest) {
                if let Ok(manifest) = serde_json::from_str::<crate::casg::TemporalManifest>(&content) {
                    // Add initial version to temporal index
                    temporal_index.add_sequence_version(
                        manifest.version.clone(),
                        manifest.sequence_root.clone(),
                        manifest.chunk_index.len(),
                        manifest.chunk_index.iter()
                            .map(|c| c.sequence_count)
                            .sum(),
                    )?;

                    // Save the temporal index
                    temporal_index.save()?;
                    println!("Initialized temporal tracking for existing database");
                }
            }
        }

        Ok(())
    }

    /// Get statistics for the CASG repository
    pub fn get_stats(&self) -> Result<CASGStats> {
        let storage_stats = self.repository.storage.get_stats();
        let databases = self.list_databases()?;

        Ok(CASGStats {
            total_chunks: storage_stats.total_chunks,
            total_size: storage_stats.total_size,
            compressed_chunks: storage_stats.compressed_chunks,
            deduplication_ratio: storage_stats.deduplication_ratio,
            database_count: databases.len(),
            databases,
        })
    }

    /// List all resumable operations
    pub fn list_resumable_operations(&self) -> Result<Vec<(String, crate::casg::ProcessingState)>> {
        self.repository.storage.list_resumable_operations()
    }

    /// Clean up expired processing states
    pub fn cleanup_expired_states(&self) -> Result<usize> {
        self.repository.storage.cleanup_expired_states()
    }

    /// Get access to the underlying storage
    pub fn get_storage(&self) -> &crate::casg::CASGStorage {
        &self.repository.storage
    }

    /// Check for taxonomy updates and download if available
    pub async fn update_taxonomy(&mut self) -> Result<TaxonomyUpdateResult> {
        let taxonomy_dir = crate::core::paths::talaria_taxonomy_current_dir();
        let taxdump_dir = taxonomy_dir.join("tree");
        let version_file = taxonomy_dir.join("version.json");

        // Read current version if it exists
        let current_version = if version_file.exists() {
            let content = std::fs::read_to_string(&version_file)?;
            let version_data: serde_json::Value = serde_json::from_str(&content)?;
            version_data["date"].as_str().map(|s| s.to_string())
        } else {
            None
        };

        // Check NCBI for latest taxonomy version
        // NCBI updates taxonomy weekly, we can check the timestamp
        let taxdump_url = "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/taxdump.tar.gz";

        // Do a HEAD request to check if there's an update
        let client = reqwest::Client::new();
        let response = client.head(taxdump_url).send().await?;

        // Get last modified date from headers
        let last_modified = response.headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Check if we need to update
        let needs_update = match (&current_version, &last_modified) {
            (Some(current), Some(latest)) => current != latest,
            (None, Some(_)) => true, // No current version, need to download
            _ => false, // Can't determine, assume no update needed
        };

        if !needs_update {
            return Ok(TaxonomyUpdateResult::UpToDate);
        }

        // Download new taxonomy
        println!("Downloading updated NCBI taxonomy...");
        let response = client.get(taxdump_url).send().await?;
        let bytes = response.bytes().await?;

        // Create a new version directory for the updated taxonomy
        if taxdump_dir.exists() {
            let new_version = crate::core::paths::generate_utc_timestamp();
            let new_version_dir = crate::core::paths::talaria_taxonomy_version_dir(&new_version);
            std::fs::create_dir_all(&new_version_dir)?;

            // Copy existing data to new version
            let _ = std::fs::create_dir_all(&new_version_dir.join("taxdump"));

            // Update current symlink to point to new version
            let current_link = crate::core::paths::talaria_taxonomy_versions_dir().join("current");
            if current_link.exists() {
                std::fs::remove_file(&current_link).ok();
            }
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&new_version, &current_link)?;
            }
            #[cfg(windows)]
            {
                std::fs::write(&current_link, new_version.as_bytes())?;
            }
        }

        // Create new taxonomy directory
        std::fs::create_dir_all(&taxdump_dir)?;

        // Extract the tar.gz file
        let taxdump_file = taxdump_dir.join("taxdump.tar.gz");
        std::fs::write(&taxdump_file, bytes)?;

        use flate2::read::GzDecoder;
        use tar::Archive;
        let tar_gz = std::fs::File::open(&taxdump_file)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        archive.unpack(&taxdump_dir)?;

        // Clean up tar file
        std::fs::remove_file(taxdump_file).ok();

        // Save version information
        let version_date = last_modified.clone().unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        let version_data = serde_json::json!({
            "date": &version_date,
            "source": "NCBI",
            "updated_at": chrono::Utc::now().to_rfc3339()
        });
        std::fs::write(&version_file, serde_json::to_string_pretty(&version_data)?)?;

        // Reload taxonomy in repository
        self.repository.taxonomy.load_ncbi_taxonomy(&taxdump_dir)?;

        Ok(TaxonomyUpdateResult::Updated {
            old_version: current_version,
            new_version: last_modified,
        })
    }

    /// Get current taxonomy version
    pub fn get_taxonomy_version(&self) -> Result<Option<String>> {
        let version_file = self.base_path.join("taxonomy/version.json");
        if !version_file.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&version_file)?;
        let version_data: serde_json::Value = serde_json::from_str(&content)?;
        Ok(version_data["date"].as_str().map(|s| s.to_string()))
    }

    /// Assemble a FASTA file from CASG for a specific database
    pub fn assemble_database(&self, source: &DatabaseSource, output_path: &Path) -> Result<()> {
        // Load manifest for this database
        let manifest_path = self.get_manifest_path(source);
        if !manifest_path.exists() {
            anyhow::bail!("Database not found in CASG. Run download first.");
        }

        let manifest = self.read_manifest(&manifest_path)?;

        // Get all chunk hashes
        let chunk_hashes: Vec<_> = manifest.chunk_index
            .iter()
            .map(|c| c.hash.clone())
            .collect();

        // Assemble to output file
        let assembler = crate::casg::FastaAssembler::new(&self.repository.storage);
        let mut output_file = std::fs::File::create(output_path)?;

        let sequence_count = assembler.stream_assembly(&chunk_hashes, &mut output_file)?;

        println!("Assembled {} sequences to {}", sequence_count, output_path.display());

        Ok(())
    }

    /// Assemble a taxonomic subset
    pub fn assemble_taxon(&self, taxon: &str, output_path: &Path) -> Result<()> {
        let sequences = self.repository.extract_taxon(taxon)?;

        // Write to FASTA
        use std::io::Write;
        let mut output = std::fs::File::create(output_path)?;

        for seq in sequences {
            writeln!(output, ">{}", seq.id)?;
            if let Some(desc) = seq.description {
                writeln!(output, " {}", desc)?;
            }
            writeln!(output, "{}", String::from_utf8_lossy(&seq.sequence))?;
        }

        Ok(())
    }

    /// Read sequences from a FASTA file
    fn read_fasta_sequences(&self, path: &Path) -> Result<Vec<Sequence>> {
        use std::io::{BufRead, BufReader};
        use std::fs::File;

        let file = File::open(path)?;
        let file_size = file.metadata()?.len();
        let reader = BufReader::new(file);

        // Create progress bar based on file size
        let progress = create_progress_bar(file_size, "Reading FASTA file");
        let mut bytes_read = 0u64;

        let mut sequences = Vec::new();
        let mut current_id = String::new();
        let mut current_desc = None;
        let mut current_seq = Vec::new();

        for line in reader.lines() {
            let line = line?;
            bytes_read += line.len() as u64 + 1; // +1 for newline
            progress.set_position(bytes_read);

            if line.starts_with('>') {
                // Save previous sequence if any
                if !current_id.is_empty() {
                    sequences.push(Sequence {
                        id: current_id.clone(),
                        description: current_desc.clone(),
                        sequence: current_seq.clone(),
                        taxon_id: None,
                        taxonomy_sources: Default::default(),
                    });
                }

                // Parse new header
                let header = &line[1..];
                let parts: Vec<&str> = header.splitn(2, ' ').collect();
                current_id = parts[0].to_string();
                current_desc = parts.get(1).map(|s| s.to_string());
                current_seq.clear();
            } else {
                // Append to sequence
                current_seq.extend(line.bytes());
            }
        }

        // Save last sequence
        if !current_id.is_empty() {
            sequences.push(Sequence {
                id: current_id,
                description: current_desc,
                sequence: current_seq,
                taxon_id: None,
                taxonomy_sources: Default::default(),
            });
        }

        progress.finish_and_clear();
        println!("Read {} sequences", sequences.len());
        Ok(sequences)
    }
}

#[derive(Debug)]
pub enum DownloadResult {
    UpToDate,
    Updated {
        chunks_added: usize,
        chunks_removed: usize,
    },
    InitialDownload,
}

#[derive(Debug)]
pub struct DatabaseInfo {
    pub name: String,
    pub version: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub chunk_count: usize,
    pub total_size: usize,
    pub reduction_profiles: Vec<String>,
}

#[derive(Debug)]
pub struct VersionInfo {
    pub timestamp: String,
    pub upstream_version: Option<String>,
    pub aliases: Vec<String>,
}

#[derive(Debug)]
pub struct CASGStats {
    pub total_chunks: usize,
    pub total_size: usize,
    pub compressed_chunks: usize,
    pub deduplication_ratio: f32,
    pub database_count: usize,
    pub databases: Vec<DatabaseInfo>,
}

#[derive(Debug)]
pub enum TaxonomyUpdateResult {
    UpToDate,
    Updated {
        old_version: Option<String>,
        new_version: Option<String>,
    },
}

impl DatabaseManager {
    /// Query database at a specific bi-temporal coordinate
    pub fn query_at_time(
        &self,
        sequence_time: chrono::DateTime<chrono::Utc>,
        taxonomy_time: chrono::DateTime<chrono::Utc>,
        taxon_ids: Option<Vec<u32>>,
    ) -> Result<Vec<crate::bio::sequence::Sequence>> {
        

        // Find manifest that matches the temporal coordinate
        let manifest = self.find_manifest_at_time(&sequence_time, &taxonomy_time)?;

        // Filter chunks by taxon IDs if specified
        let chunks = if let Some(taxa) = taxon_ids {
            manifest.chunk_index
                .iter()
                .filter(|chunk| {
                    chunk.taxon_ids.iter().any(|tid| taxa.contains(&tid.0))
                })
                .cloned()
                .collect()
        } else {
            manifest.chunk_index.clone()
        };

        // Load sequences from chunks
        self.repository.load_sequences_from_chunks(&chunks)
    }

    /// Find manifest at a specific temporal coordinate
    fn find_manifest_at_time(
        &self,
        _sequence_time: &chrono::DateTime<chrono::Utc>,
        _taxonomy_time: &chrono::DateTime<chrono::Utc>,
    ) -> Result<crate::casg::types::TemporalManifest> {
        // For now, return the current manifest
        // In a full implementation, this would search historical manifests
        self.repository.manifest.get_data()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No manifest available"))
    }

    /// Get temporal history of a sequence
    pub fn get_sequence_history(&self, sequence_id: &str) -> Result<Vec<TemporalSequenceRecord>> {
        let mut history = Vec::new();

        // Get current manifest
        if let Some(manifest) = self.repository.manifest.get_data() {
            // Search for sequence in chunks
            for chunk in &manifest.chunk_index {
                // Load chunk and check for sequence
                if let Ok(sequences) = self.repository.storage.load_sequences_from_chunk(&chunk.hash) {
                    if let Some(seq) = sequences.iter().find(|s| s.id == sequence_id) {
                        history.push(TemporalSequenceRecord {
                            sequence_id: sequence_id.to_string(),
                            version: manifest.version.clone(),
                            sequence_time: manifest.created_at,
                            taxonomy_time: manifest.created_at,
                            taxon_id: seq.taxon_id,
                            chunk_hash: chunk.hash.clone(),
                        });
                    }
                }
            }
        }

        Ok(history)
    }

    /// Verify Merkle proof for a chunk
    pub fn verify_chunk_proof(&self, chunk_hash: &crate::casg::types::SHA256Hash) -> Result<bool> {
        use crate::casg::merkle::MerkleDAG;

        let manifest = self.repository.manifest.get_data()
            .ok_or_else(|| anyhow::anyhow!("No manifest available"))?;

        // Rebuild Merkle tree from manifest chunks
        let dag = MerkleDAG::build_from_items(manifest.chunk_index.clone())?;

        // Generate and verify proof
        match dag.generate_proof(&chunk_hash.0) {
            Ok(proof) => Ok(MerkleDAG::verify_proof(&proof, &[])),
            Err(_) => Ok(false),
        }
    }

    /// Get manifest for a database by name
    pub fn get_manifest(&self, database_name: &str) -> Result<crate::casg::types::TemporalManifest> {
        // Try to find manifest file for this database
        let manifest_path = self.base_path.join(format!("{}.manifest.json", database_name));
        if manifest_path.exists() {
            return self.read_manifest(&manifest_path);
        }

        // Try the manifest in data directory
        let data_manifest_path = self.base_path.join("data").join(database_name).join("manifest.json");
        if data_manifest_path.exists() {
            return self.read_manifest(&data_manifest_path);
        }

        // If not found, check if it's a DatabaseSource
        let source = DatabaseSource::from_string(database_name)?;
        let source_manifest_path = self.get_manifest_path(&source);
        if source_manifest_path.exists() {
            return self.read_manifest(&source_manifest_path);
        }

        anyhow::bail!("Manifest not found for database: {}", database_name)
    }

    /// Load a chunk by its hash
    pub fn load_chunk(&self, hash: &crate::casg::SHA256Hash) -> Result<crate::casg::types::TaxonomyAwareChunk> {
        // Use the storage to get the chunk
        let chunk_data = self.repository.storage.get_chunk(hash)?;

        // Deserialize based on format
        // Try MessagePack first (binary format)
        if chunk_data.len() > 4 && &chunk_data[0..4] == TALARIA_MAGIC {
            let chunk: crate::casg::types::TaxonomyAwareChunk =
                rmp_serde::from_slice(&chunk_data[4..])?;
            Ok(chunk)
        } else {
            // Fall back to JSON
            let chunk: crate::casg::types::TaxonomyAwareChunk =
                serde_json::from_slice(&chunk_data)?;
            Ok(chunk)
        }
    }

    /// Load taxonomy mappings for a database
    pub fn load_taxonomy_mappings(&self, database_name: &str) -> Result<std::collections::HashMap<String, crate::casg::TaxonId>> {
        // Try to get mappings from the manifest
        let source = DatabaseSource::from_string(database_name)
            .unwrap_or(DatabaseSource::Custom(database_name.to_string()));

        self.get_taxonomy_mapping_from_manifest(&source)
    }
}

/// Temporal sequence record for history tracking
#[derive(Debug, Clone)]
pub struct TemporalSequenceRecord {
    pub sequence_id: String,
    pub version: String,
    pub sequence_time: chrono::DateTime<chrono::Utc>,
    pub taxonomy_time: chrono::DateTime<chrono::Utc>,
    pub taxon_id: Option<u32>,
    pub chunk_hash: crate::casg::SHA256Hash,
}

#[cfg(test)]
#[path = "casg_database_manager_tests.rs"]
mod tests;
