use super::{DownloadResult, TaxonomyUpdateResult};
use crate::download::manager::{DownloadManager, DownloadOptions};
use crate::download::workspace::{find_existing_workspace_for_source, DownloadState, Stage};
use crate::download::{parse_database_source, DownloadProgress};
use crate::taxonomy::{TaxonomyManager, VersionDecision};
/// Database manager using content-addressed storage
///
/// Instead of downloading entire databases and creating dated directories,
/// this uses content-addressed storage with manifests for efficient updates.
use crate::{ChunkingStrategy, SHA256Hash, SHA256HashExt, HeraldRepository, TaxonomicChunker, TemporalManifest};
use talaria_bio::sequence::Sequence;
use talaria_core::system::paths;
use talaria_core::{DatabaseSource, NCBIDatabase, UniProtDatabase};
use talaria_utils::database::database_ref::parse_database_reference;
use tracing::{debug, info};

use anyhow::{Context, Result};
use indicatif::ProgressBar;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
// UI imports removed - using progress_callback pattern instead

/// Statistics about a database in the repository
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub sequence_count: usize,
    pub chunk_count: usize,
    pub total_size: u64,
}

pub struct DatabaseManager {
    repository: HeraldRepository,
    base_path: PathBuf,
    #[allow(dead_code)]
    use_json_manifest: bool,
    _taxonomy_manager: TaxonomyManager,
    /// Accumulate manifests during batch processing to avoid creating multiple versions
    accumulated_manifests: Vec<(crate::ChunkManifest, crate::SHA256Hash)>,
    /// Current version being processed (set once at start)
    current_version: Option<String>,
    /// Metadata cache for expensive queries
    cache: Option<std::sync::Arc<crate::database::cache::MetadataCache>>,
}

/// Structure for storing partial manifests in RocksDB
#[derive(serde::Serialize, serde::Deserialize)]
struct PartialManifest {
    batch_num: usize,
    manifests: Vec<(crate::ChunkManifest, crate::SHA256Hash)>,
    sequence_count: usize,
    /// Whether chunk manifests have been stored to RocksDB (for streaming mode)
    finalized: bool,
}

impl DatabaseManager {
    /// Get access to the repository (for extensions)
    pub fn get_repository(&self) -> &HeraldRepository {
        &self.repository
    }

    /// Get mutable access to the repository (for extensions)
    pub fn get_repository_mut(&mut self) -> &mut HeraldRepository {
        &mut self.repository
    }

    /// Create a new HERALD database manager
    pub fn new(base_dir: Option<String>) -> Result<Self> {
        let base_path = if let Some(dir) = base_dir {
            PathBuf::from(dir)
        } else {
            // Use centralized path configuration
            paths::talaria_databases_dir()
        };

        // Ensure directory exists
        std::fs::create_dir_all(&base_path)?;

        // Initialize or open HERALD repository
        // Always use open if chunks directory exists (indicating existing data)
        let repository = if base_path.join("chunks").exists() {
            HeraldRepository::open(&base_path)?
        } else {
            HeraldRepository::init(&base_path)?
        };

        let taxonomy_manager = TaxonomyManager::new(&base_path)?;

        // Initialize cache
        let cache_dir = base_path.join(".cache");
        let cache = crate::database::cache::MetadataCache::new(cache_dir)?;
        let _ = cache.load_from_disk(); // Load existing caches if available
        let cache = Some(std::sync::Arc::new(cache));

        Ok(Self {
            repository,
            base_path,
            use_json_manifest: false,
            _taxonomy_manager: taxonomy_manager,
            accumulated_manifests: Vec::new(),
            current_version: None,
            cache,
        })
    }

    /// Create a new HERALD database manager with options
    pub fn with_options(base_dir: Option<String>, use_json_manifest: bool) -> Result<Self> {
        let base_path = if let Some(dir) = base_dir {
            PathBuf::from(dir)
        } else {
            // Use centralized path configuration
            paths::talaria_databases_dir()
        };

        // Ensure directory exists
        std::fs::create_dir_all(&base_path)?;

        // Initialize or open HERALD repository
        // Always use open if chunks directory exists (indicating existing data)
        let repository = if base_path.join("chunks").exists() {
            HeraldRepository::open(&base_path)?
        } else {
            HeraldRepository::init(&base_path)?
        };

        let taxonomy_manager = TaxonomyManager::new(&base_path)?;

        // Initialize cache
        let cache_dir = base_path.join(".cache");
        let cache = crate::database::cache::MetadataCache::new(cache_dir)?;
        let _ = cache.load_from_disk(); // Load existing caches if available
        let cache = Some(std::sync::Arc::new(cache));

        Ok(Self {
            repository,
            base_path,
            use_json_manifest,
            _taxonomy_manager: taxonomy_manager,
            accumulated_manifests: Vec::new(),
            current_version: None,
            cache,
        })
    }

    /// Check if a database exists in the repository (by name string)
    pub fn has_database(&self, db_name: &str) -> Result<bool> {
        // Parse database name to get source/dataset
        let parts: Vec<&str> = db_name.split('/').collect();
        if parts.len() != 2 {
            return Ok(false);
        }

        let source = parts[0];
        let dataset = parts[1];

        // Check RocksDB for 'current' alias
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let current_alias_key = format!("alias:{}:{}:current", source, dataset);
        Ok(rocksdb.get_manifest(&current_alias_key)?.is_some())
    }

    /// Check for updates without downloading (dry-run mode)
    pub async fn check_for_updates(
        &mut self,
        source: &DatabaseSource,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        // Check if we have a cached manifest in RocksDB
        if !self.has_database_by_source(source)? {
            progress_callback("No local database found - initial download required");
            return Ok(DownloadResult::InitialDownload {
                total_chunks: 0,
                total_size: 0,
            });
        }

        // Try to get manifest URL for update check
        if let Ok(manifest_url) = self.get_manifest_url(source) {
            progress_callback("Checking for updates...");
            self.repository
                .manifest
                .set_remote_url(manifest_url.clone());

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
                            chunks_updated: 0,
                            chunks_removed: diff.removed_chunks.len(),
                            size_difference: 0,
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

        // Delete existing manifest from RocksDB to force re-download
        let (source_name, dataset) = self.get_source_dataset_names(source);
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Delete current alias
        let current_alias_key = format!("alias:{}:{}:current", source_name, dataset);
        rocksdb.delete_manifest(&current_alias_key).ok();

        // Now do a normal download which will treat it as initial
        let result = self.download(source, progress_callback).await;

        // Clear the force flag after download
        std::env::remove_var("TALARIA_FORCE_NEW_VERSION");

        result
    }

    /// Ensure version integrity - verify metadata is present in RocksDB
    /// Note: With RocksDB-based version management, this is a no-op.
    /// Version integrity is maintained atomically during database operations.
    pub fn ensure_version_integrity(&mut self, _source: &DatabaseSource) -> Result<()> {
        // All version management is now in RocksDB
        // Integrity is ensured atomically during downloads
        Ok(())
    }

    /// Get current version information
    pub fn get_current_version_info(
        &self,
        source: &DatabaseSource,
    ) -> Result<talaria_core::types::DatabaseVersionInfo> {
        let (source_name, dataset) = self.get_source_dataset_names(source);
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Look up 'current' alias in RocksDB
        let current_alias_key = format!("alias:{}:{}:current", source_name, dataset);
        let timestamp = if let Ok(Some(data)) = rocksdb.get_manifest(&current_alias_key) {
            String::from_utf8(data)
                .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in current alias: {}", e))?
        } else {
            return Err(anyhow::anyhow!(
                "No current version found for {}:{}",
                source_name,
                dataset
            ));
        };

        // Load version metadata from RocksDB manifest
        let manifest_key = format!("manifest:{}:{}:{}", source_name, dataset, timestamp);
        if let Ok(Some(data)) = rocksdb.get_manifest(&manifest_key) {
            let manifest: crate::TemporalManifest = bincode::deserialize(&data)?;
            let aliases = self.get_version_aliases(&source_name, &dataset, &timestamp)?;

            Ok(talaria_core::types::DatabaseVersionInfo {
                timestamp: timestamp.clone(),
                created_at: manifest.created_at,
                upstream_version: Some(manifest.version.clone()),
                source: source_name.clone(),
                dataset: dataset.clone(),
                aliases,
                chunk_count: manifest.chunk_index.len(),
                sequence_count: manifest.chunk_index.iter().map(|c| c.sequence_count).sum(),
                total_size: manifest.chunk_index.iter().map(|c| c.size as u64).sum(),
            })
        } else {
            // Fallback to minimal info
            use chrono::Utc;
            Ok(talaria_core::types::DatabaseVersionInfo {
                timestamp: timestamp.clone(),
                created_at: Utc::now(),
                upstream_version: None,
                source: source_name.clone(),
                dataset: dataset.clone(),
                aliases: vec!["current".to_string()],
                chunk_count: 0,
                sequence_count: 0,
                total_size: 0,
            })
        }
    }

    /// Download a database using HERALD
    pub async fn download(
        &mut self,
        source: &DatabaseSource,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        tracing::info!("Starting database download for source: {}", source);
        // For taxonomy files, check if the specific file exists
        if Self::is_taxonomy_database(source) {
            tracing::debug!("Checking for existing taxonomy file");
            if self.has_specific_taxonomy_file(source) {
                tracing::info!("Taxonomy component already exists, skipping download");
                progress_callback("Taxonomy component already exists");
                return Ok(DownloadResult::UpToDate);
            }
            // Component doesn't exist yet, proceed directly to download
            // Skip HERALD manifest checks for taxonomy files
            tracing::info!("Taxonomy component not found, initiating download");
            progress_callback(&format!(
                "Taxonomy component not found, will download: {}",
                source
            ));
            return Box::pin(self.handle_initial_download(source, progress_callback)).await;
        }

        // First check if we have an existing complete download that needs processing
        if let Ok(Some((workspace, state))) = find_existing_workspace_for_source(source) {
            if state.stage == Stage::Complete {
                if let Some(decompressed) = state.files.decompressed.as_ref() {
                    if decompressed.exists() {
                        let file_size = decompressed.metadata()?.len();
                        progress_callback(&format!(
                            "✓ Found complete download: {} ({:.2} GB)",
                            decompressed
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy(),
                            file_size as f64 / 1_073_741_824.0
                        ));

                        // CRITICAL FIX: Check if sequences were actually processed
                        if state.sequences_processed == 0 {
                            progress_callback(&format!(
                                "⚠ Download marked complete but no sequences processed (sequences_processed: {})",
                                state.sequences_processed
                            ));
                            progress_callback("Will reprocess the downloaded file...");
                            // Fall through to processing logic below instead of returning
                        } else {
                            // Check if already processed into RocksDB
                            tracing::debug!(
                                "Checking if database already exists in RocksDB for source: {:?}",
                                source
                            );
                            let db_exists = self.has_database_by_source(source)?;
                            tracing::debug!("Database exists check result: {}", db_exists);

                            if db_exists {
                                progress_callback(&format!(
                                    "Database already processed into HERALD format ({} sequences processed)",
                                    state.sequences_processed
                                ));
                                progress_callback("Cleaning up workspace...");

                                // Clean up the workspace since we're done with it
                                if let Err(e) = DownloadManager::cleanup_download_workspace(source) {
                                    tracing::warn!("Failed to cleanup workspace: {}", e);
                                }

                                progress_callback("Database is up to date");
                                return Ok(DownloadResult::UpToDate);
                            }
                        }

                        // Either sequences_processed == 0 OR not in RocksDB yet, proceed with processing
                        progress_callback(
                            "Using existing download, processing into HERALD format...",
                        );

                        // Process the existing file directly
                        progress_callback("Processing database into HERALD chunks...");
                        progress_callback(
                            "This one-time conversion enables future incremental updates",
                        );

                        // Chunk the database with workspace for resumable processing
                        if let Err(e) = self.chunk_database_with_resume(
                            &decompressed,
                            source,
                            Some(&workspace),
                            Some(&progress_callback),
                        ) {
                            progress_callback(&format!(
                                "Processing failed: {}. Downloaded file preserved in workspace for retry.",
                                e
                            ));
                            return Err(e);
                        }

                        // Clean up workspace after successful processing
                        if let Err(e) = DownloadManager::cleanup_download_workspace(source) {
                            progress_callback(&format!(
                                "Warning: Failed to clean up workspace: {}",
                                e
                            ));
                        }

                        progress_callback("Database successfully stored in HERALD format");

                        // Return success - we've processed the existing download
                        return Ok(DownloadResult::InitialDownload {
                            total_chunks: 0, // We don't have exact counts here
                            total_size: file_size,
                        });
                    }
                }
            }
        }

        // Check if we have a cached manifest in RocksDB (for non-taxonomy databases)
        let has_existing = self.has_database_by_source(source)?;

        // For taxonomy databases with existing manifest, still need to check if the specific component exists
        if has_existing && Self::is_taxonomy_database(source) {
            // Even if manifest exists, check if this specific taxonomy component's files exist
            if !self.has_specific_taxonomy_file(source) {
                progress_callback(&format!(
                    "Taxonomy manifest exists but component files missing for {}",
                    source
                ));
                // Continue to download the missing component
                return Box::pin(self.handle_initial_download(source, progress_callback)).await;
            }
        }

        // If we have an existing manifest, check for updates
        if has_existing {
            // Try to get manifest URL (may not exist in dev/local mode)
            if let Ok(manifest_url) = self.get_manifest_url(source) {
                progress_callback("Checking for updates...");

                // Set remote URL in repository
                self.repository
                    .manifest
                    .set_remote_url(manifest_url.clone());

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
                                return self
                                    .handle_incremental_update(new_manifest, progress_callback)
                                    .await;
                            }
                            Err(_) => {
                                progress_callback(
                                    "[!] Manifest server unavailable, keeping current version",
                                );
                                return Ok(DownloadResult::UpToDate);
                            }
                        }
                    }
                    Err(_) => {
                        // Manifest server unavailable, but we have local data
                        progress_callback(
                            "[!] Cannot check for updates (manifest server unavailable)",
                        );
                        return Ok(DownloadResult::UpToDate);
                    }
                }
            } else {
                // No manifest URL available (dev mode), just use local
                progress_callback("Using local HERALD database (no remote manifest configured)");
                return Ok(DownloadResult::UpToDate);
            }
        }

        // No existing manifest - need to do initial download
        progress_callback("[NEW] Initial download required - no local HERALD data found");
        progress_callback("This will download the full database and convert it to HERALD format");
        progress_callback("Future updates will be incremental and much faster!");

        Box::pin(self.handle_initial_download(source, progress_callback)).await
    }

    /// Handle incremental update when manifest is available
    async fn handle_incremental_update(
        &mut self,
        new_manifest: crate::Manifest,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        tracing::info!("Starting incremental update");
        use crate::operations::state::{OperationType, SourceInfo};

        // Get manifest data for version info
        let manifest_data = new_manifest
            .get_data()
            .ok_or_else(|| anyhow::anyhow!("No manifest data"))?;
        let manifest_hash = SHA256Hash::compute(&serde_json::to_vec(&manifest_data)?);
        let manifest_version = manifest_data.version.clone();

        // Record manifest version
        tracing::Span::current().record("manifest_version", &manifest_version.as_str());
        tracing::debug!("Processing manifest version: {}", manifest_version);

        // Compute diff to see what chunks we need
        let diff = self.repository.manifest.diff(&new_manifest)?;

        let chunks_to_download = diff.new_chunks.len();
        let chunks_to_remove = diff.removed_chunks.len();

        // Record diff metrics
        tracing::Span::current().record("new_chunks", chunks_to_download);
        tracing::Span::current().record("removed_chunks", chunks_to_remove);
        tracing::info!(
            "Computed manifest diff: {} new chunks, {} removed chunks",
            chunks_to_download,
            chunks_to_remove
        );

        // Check for resumable state
        let source_info = SourceInfo {
            database: manifest_data
                .source_database
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
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

        // Download only new chunks
        if !diff.new_chunks.is_empty() {
            progress_callback("Downloading new chunks...");

            // Check if remote chunk server is configured
            if !crate::remote::ChunkClient::is_configured() {
                // Fall back to full re-download if no chunk server is available
                progress_callback("No chunk server configured - falling back to full download");
                progress_callback("Set TALARIA_CHUNK_SERVER for incremental updates");
                let source = manifest_data
                    .source_database
                    .as_ref()
                    .map(|s| DatabaseSource::from_database_string(s))
                    .unwrap_or_else(|| DatabaseSource::Custom("unknown".to_string()));
                return Box::pin(self.handle_initial_download(&source, progress_callback)).await;
            }

            // Create chunk client and download chunks
            let chunk_client = crate::remote::ChunkClient::new(None)?;

            // Download chunks in parallel (max 8 concurrent)
            let parallel_downloads = 8;
            progress_callback(&format!(
                "Downloading {} new chunks ({}x parallel)...",
                diff.new_chunks.len(),
                parallel_downloads
            ));

            let downloaded_chunks = chunk_client
                .download_chunks(&diff.new_chunks, parallel_downloads)
                .await?;

            // Store downloaded chunks in local storage
            let storage = &self.repository.storage;
            for (hash, data) in downloaded_chunks {
                storage.store_chunk(&data, false)?; // false = don't compress again

                // Update progress
                progress_callback(&format!(
                    "Stored chunk: {}",
                    hash.to_string().chars().take(8).collect::<String>()
                ));
            }

            progress_callback(&format!(
                "✓ Downloaded and stored {} new chunks",
                diff.new_chunks.len()
            ));
        }

        // Remove old chunks (garbage collection)
        if !diff.removed_chunks.is_empty() {
            progress_callback("Removing obsolete chunks...");

            // Get all currently referenced chunks from the new manifest
            let manifest_data = new_manifest
                .get_data()
                .ok_or_else(|| anyhow::anyhow!("No manifest data"))?;
            let referenced_chunks: Vec<SHA256Hash> = manifest_data
                .chunk_index
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
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let mut temporal_index = crate::TemporalIndex::load(&temporal_path, rocksdb)?;

        // Add sequence version tracking
        if let Some(manifest_data) = new_manifest.get_data() {
            temporal_index.add_sequence_version(
                manifest_data.version.clone(),
                manifest_data.sequence_root.clone(),
                manifest_data.chunk_index.len(),
                manifest_data
                    .chunk_index
                    .iter()
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
            chunks_updated: 0,
            chunks_removed: chunks_to_remove,
            size_difference: 0,
        })
    }

    /// Handle initial download when no local manifest exists
    /// Check if the database being downloaded is taxonomy data itself
    fn is_taxonomy_database(source: &DatabaseSource) -> bool {
        matches!(
            source,
            DatabaseSource::UniProt(UniProtDatabase::IdMapping)
                | DatabaseSource::NCBI(NCBIDatabase::Taxonomy)
                | DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId)
                | DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId)
        )
    }

    /// Check if the specific taxonomy file exists by checking manifest components
    fn has_specific_taxonomy_file(&self, source: &DatabaseSource) -> bool {
        use talaria_core::{NCBIDatabase, UniProtDatabase};

        let taxonomy_dir = talaria_core::system::paths::talaria_taxonomy_current_dir();
        if !taxonomy_dir.exists() {
            debug!("Taxonomy directory does not exist: {:?}", taxonomy_dir);
            return false;
        }

        // Check for actual files directly instead of relying on manifest
        // This is more reliable since manifests can be in different formats
        let file_exists = match source {
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => {
                let path = taxonomy_dir.join("tree").join("nodes.dmp");
                let exists = path.exists();
                debug!("Checking for taxdump at {:?}: {}", path, exists);
                exists
            }
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => {
                let path = taxonomy_dir
                    .join("mappings")
                    .join("prot.accession2taxid.gz");
                let exists = path.exists();
                debug!(
                    "Checking for prot.accession2taxid at {:?}: {}",
                    path, exists
                );
                exists
            }
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => {
                let path = taxonomy_dir
                    .join("mappings")
                    .join("nucl.accession2taxid.gz");
                let exists = path.exists();
                debug!(
                    "Checking for nucl.accession2taxid at {:?}: {}",
                    path, exists
                );
                exists
            }
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => {
                let path = taxonomy_dir.join("mappings").join("idmapping.dat.gz");
                let exists = path.exists();
                debug!("Checking for idmapping at {:?}: {}", path, exists);
                exists
            }
            _ => false,
        };

        debug!("has_specific_taxonomy_file({:?}) = {}", source, file_exists);
        file_exists
    }

    /// Create or update a composite manifest for taxonomy files
    /// Now uses RocksDB instead of filesystem
    fn create_or_update_taxonomy_manifest(
        &self,
        source: &DatabaseSource,
        file_path: &Path,
        _version_dir: &Path, // No longer used - kept for API compatibility
        version: &str,
    ) -> Result<()> {
        use crate::taxonomy::{
            AuditEntry, InstalledComponent, TaxonomyManifest, TaxonomyVersionPolicy,
        };
        use chrono::Utc;
        use talaria_core::{NCBIDatabase, UniProtDatabase};

        // Get RocksDB handle
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Try to load existing manifest from RocksDB
        let mut manifest: TaxonomyManifest =
            if let Some(existing) = TaxonomyManifest::load_from_rocksdb(&rocksdb, version)? {
                existing
            } else {
                // Create new manifest
                TaxonomyManifest {
                    version: version.to_string(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    expected_components: TaxonomyManager::default_components(),
                    installed_components: Vec::new(),
                    components: Vec::new(),
                    history: vec![],
                    policy: TaxonomyVersionPolicy::default(),
                }
            };

        // Determine component name and metadata
        let (component_name, source_name) = match source {
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => ("taxdump", "NCBI: NCBI Taxonomy"),
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => {
                ("prot_accession2taxid", "NCBI: Protein Accession to TaxID")
            }
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => (
                "nucl_accession2taxid",
                "NCBI: Nucleotide Accession to TaxID",
            ),
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => {
                ("idmapping", "UniProt: ID Mapping")
            }
            _ => return Err(anyhow::anyhow!("Unsupported taxonomy source")),
        };

        // Detect file format
        let file_format = TaxonomyManager::detect_file_format(file_path)?;

        // Create installed component
        let installed = InstalledComponent {
            source: source_name.to_string(),
            checksum: String::new(), // Could calculate if needed
            size: std::fs::metadata(file_path)?.len(),
            downloaded_at: Utc::now(),
            source_version: None,
            carried_from: None,
            file_path: file_path.to_path_buf(),
            compressed: file_path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "gz" || s == "tar")
                .unwrap_or(false),
            format: file_format,
        };

        // Add or update the component
        // Remove existing if present
        manifest
            .installed_components
            .retain(|c| c.source != component_name);
        // Add new one
        manifest.installed_components.push(installed.clone());
        manifest.components.push(installed);

        // Add audit entry
        manifest.history.push(AuditEntry {
            timestamp: Utc::now(),
            action: "component_added".to_string(),
            component: component_name.to_string(),
            user: None,
            details: format!("Added {} from {}", component_name, source_name),
        });

        // Update timestamp
        manifest.updated_at = Utc::now();

        // Save manifest to RocksDB (single source of truth)
        manifest.save_as_current(&rocksdb)?;

        tracing::info!("Updated taxonomy manifest component '{}' in RocksDB", component_name);
        tracing::info!("Version: {}", version);
        Ok(())
    }

    /// Check if we should create a new taxonomy version or update current
    /// Determine if we should create a new taxonomy version using the new manager
    fn should_create_new_taxonomy_version(
        &self,
        source: &DatabaseSource,
    ) -> Result<VersionDecision> {
        // Map source to component name
        let _component_name = match source {
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => "taxdump",
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => "prot_accession2taxid",
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => "nucl_accession2taxid",
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => "idmapping",
            _ => return Err(anyhow::anyhow!("Not a taxonomy database")),
        };

        // Check if running in non-interactive mode (e.g., CI)
        let _interactive = atty::is(atty::Stream::Stdin);

        // For now, always return false as per the current implementation
        Ok(VersionDecision::AppendToCurrent)
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
        let new_version = talaria_core::system::paths::generate_utc_timestamp();
        tracing::info!("Creating new taxonomy version: {} (UTC)", new_version);

        let new_version_dir =
            talaria_core::system::paths::talaria_taxonomy_version_dir(&new_version);
        std::fs::create_dir_all(&new_version_dir)?;

        // Copy existing files from current version if requested
        if copy_forward {
            let current_dir = talaria_core::system::paths::talaria_taxonomy_current_dir();
            if current_dir.exists() {
                tracing::warn!("Carrying forward secondary components from previous version:");

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

                                carried_files.push((
                                    entry.file_name().to_string_lossy().to_string(),
                                    age_days,
                                ));
                            }
                        }

                        std::fs::copy(&src, &dst)?;
                    }

                    // Show what was carried forward
                    for (file, age_days) in carried_files {
                        tracing::info!("  • {} ({} days old)", file, age_days);
                    }

                    tracing::info!("Consider updating these components with:");
                    tracing::info!("  talaria database download --complete ncbi/taxonomy");
                }
            }
        }

        // Update current symlink
        let current_link =
            talaria_core::system::paths::talaria_taxonomy_versions_dir().join("current");
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
    fn store_taxonomy_mapping_file(
        &mut self,
        file_path: &Path,
        source: &DatabaseSource,
    ) -> Result<()> {
        use talaria_core::{NCBIDatabase, UniProtDatabase};

        tracing::info!("Storing taxonomy mapping file...");

        // Verify file exists and is accessible
        if !file_path.exists() {
            return Err(anyhow::anyhow!(
                "Downloaded file not found at: {}. This may indicate a download failure.",
                file_path.display()
            ));
        }

        // Check file size to ensure download completed
        let file_size = std::fs::metadata(file_path)?.len();
        if file_size == 0 {
            return Err(anyhow::anyhow!(
                "Downloaded file is empty. The download may have failed or been interrupted."
            ));
        }

        tracing::debug!("File size: {} bytes", file_size);

        // Check if we should create a new version or update current
        let version_decision = self.should_create_new_taxonomy_version(source)?;

        let (taxonomy_dir, version) = match version_decision {
            VersionDecision::CreateNew {
                copy_forward,
                reason,
            } => {
                tracing::info!("Creating new taxonomy version: {}", reason);

                // Create new version and optionally copy existing files
                let new_dir = if copy_forward {
                    self.create_new_taxonomy_version_with_copy_forward()?
                } else {
                    self.create_new_taxonomy_version()?
                };

                let version = new_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| anyhow::anyhow!("Failed to get version from directory"))?
                    .to_string();
                tracing::info!("Created new taxonomy version: {}", version);
                (new_dir, version)
            }
            VersionDecision::AppendToCurrent => {
                // Use current version for additive updates
                let current = talaria_core::system::paths::talaria_taxonomy_current_dir();
                if !current.exists() {
                    // First file - create initial version
                    tracing::info!("No current taxonomy version found, creating initial version...");
                    let new_dir = self.create_new_taxonomy_version()?;
                    let version = new_dir
                        .file_name()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| anyhow::anyhow!("Failed to get version from directory"))?
                        .to_string();
                    (new_dir, version)
                } else {
                    // IMPORTANT: Always resolve symlink to get the actual directory
                    let actual_dir = if current.is_symlink() {
                        let target = std::fs::read_link(&current)?;
                        if target.is_relative() {
                            current
                                .parent()
                                .ok_or_else(|| anyhow::anyhow!("Failed to get parent directory"))?
                                .join(target)
                        } else {
                            target
                        }
                    } else {
                        current.clone()
                    };

                    let version = actual_dir
                        .file_name()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| anyhow::anyhow!("Failed to get version from directory"))?
                        .to_string();

                    tracing::info!("Adding to existing taxonomy version: {}", version);
                    // Return actual_dir, not current (which might be a symlink)
                    (actual_dir, version)
                }
            }
            VersionDecision::UserCancelled => {
                tracing::warn!("Operation cancelled by user");
                return Ok(());
            }
        };

        // Create appropriate subdirectories
        let tree_dir = taxonomy_dir.join("tree");
        let mappings_dir = taxonomy_dir.join("mappings");

        std::fs::create_dir_all(&tree_dir)?;
        std::fs::create_dir_all(&mappings_dir)?;

        // Determine the destination based on source type
        if let DatabaseSource::NCBI(NCBIDatabase::Taxonomy) = source {
            // Extract taxonomy dump to tree/ subdirectory
            tracing::info!("Extracting taxonomy dump to tree/ directory...");

            // The file has already been decompressed from .tar.gz to .tar
            // by the download manager, so we just need to extract the tar archive
            let tar_file = std::fs::File::open(file_path)
                .with_context(|| format!("Failed to open tar file: {}", file_path.display()))?;
            let mut archive = tar::Archive::new(tar_file);

            // Extract with better error handling
            archive.unpack(&tree_dir).with_context(|| {
                format!(
                    "Failed to extract taxonomy archive to {}",
                    tree_dir.display()
                )
            })?;
            tracing::info!("Taxonomy dump extracted successfully");

            // Create manifest for the extracted taxonomy files
            // Use nodes.dmp as a representative file for the whole taxonomy dump
            let nodes_file = tree_dir.join("nodes.dmp");
            if nodes_file.exists() {
                // taxonomy_dir is already the actual version directory from above
                self.create_or_update_taxonomy_manifest(
                    source,
                    &nodes_file,
                    &taxonomy_dir,
                    &version,
                )?;
            }

            return Ok(());
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
        tracing::info!("Moving taxonomy file to: {}", dest_file.display());

        // First try rename (atomic and fast)
        if std::fs::rename(file_path, &dest_file).is_err() {
            // If rename fails (e.g., across filesystems), copy and delete
            std::fs::copy(file_path, &dest_file)?;
            // Don't delete the source file here - let the caller handle cleanup
            // This prevents the "file not found" error when caller tries to clean up
        }

        tracing::info!("✓ Taxonomy mapping file stored successfully");
        tracing::info!("  Location: {}", dest_file.display());

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
        tracing::info!("Starting initial download for {}", source);
        // First check if HERALD manifest already exists in RocksDB for this database
        if self.has_database_by_source(source)? {
            // For taxonomy databases, check if the specific component files exist
            // even if a shared manifest exists
            if Self::is_taxonomy_database(source) {
                if !self.has_specific_taxonomy_file(source) {
                    progress_callback(&format!(
                        "Taxonomy manifest exists but component {} not found - proceeding with download",
                        source
                    ));
                    // Continue with download - don't return UpToDate
                } else {
                    progress_callback(
                        "HERALD manifest and taxonomy component files already exist",
                    );
                    return Ok(DownloadResult::UpToDate);
                }
            } else {
                // For non-taxonomy databases, if manifest exists we're done
                progress_callback("HERALD manifest already exists for this database");
                progress_callback(
                    "Database is already in HERALD format - skipping download and processing",
                );

                // Try to clean up any lingering download workspace
                if let Err(e) = DownloadManager::cleanup_download_workspace(source) {
                    progress_callback(&format!(
                        "Note: Failed to clean up old download workspace: {}",
                        e
                    ));
                }

                return Ok(DownloadResult::UpToDate);
            }
        }

        // Skip taxonomy check if we're downloading taxonomy data itself
        if !Self::is_taxonomy_database(source) {
            // Check if taxonomy is needed and download if missing
            progress_callback("Checking for taxonomy data...");

            // Check specific paths
            let taxonomy_dir = talaria_core::system::paths::talaria_taxonomy_current_dir();
            let tree_file = taxonomy_dir.join("taxonomy_tree.json");
            let nodes_file = taxonomy_dir.join("tree/nodes.dmp");
            let names_file = taxonomy_dir.join("tree/names.dmp");

            if !self.repository.taxonomy.has_taxonomy() {
                // Try to load taxonomy silently first
                let mut loaded = false;

                if tree_file.exists() {
                    // Try loading from cached JSON
                    if let Ok(tax_mgr) = crate::taxonomy::TaxonomyManager::load(&self.base_path) {
                        if tax_mgr.has_taxonomy() {
                            self.repository.taxonomy = tax_mgr;
                            loaded = true;
                            progress_callback("✓ Loaded existing taxonomy cache");
                        }
                    }
                }

                if !loaded && nodes_file.exists() && names_file.exists() {
                    // Load from NCBI dump files (quietly to avoid progress bar interference)
                    progress_callback("  Loading taxonomy from NCBI dump files...");
                    if let Err(e) = self
                        .repository
                        .taxonomy
                        .load_ncbi_taxonomy_quiet(&taxonomy_dir.join("tree"))
                    {
                        progress_callback(&format!("    ✗ Failed to load: {}", e));
                    } else {
                        loaded = true;
                        progress_callback("  ✓ Taxonomy loaded successfully");
                    }
                }

                // Final check if we still don't have taxonomy
                if !loaded && !self.repository.taxonomy.has_taxonomy() {
                    progress_callback("  ⚠ WARNING: No taxonomy data available");
                    progress_callback(
                        "    Without taxonomy, sequences cannot be properly classified",
                    );
                    progress_callback(
                        "    This reduces chunking efficiency and search performance",
                    );
                    progress_callback("");
                    progress_callback("    To download taxonomy data, run:");
                    progress_callback("    talaria database download ncbi/taxonomy");
                    progress_callback("");

                    // Download taxonomy automatically if possible
                    progress_callback("    Attempting to download taxonomy data...");
                    match self.download_taxonomy_if_needed().await {
                        Ok(true) => {
                            progress_callback("    ✓ Taxonomy data downloaded successfully");
                            // Reload taxonomy manager with new data
                            self.repository.taxonomy = TaxonomyManager::new(
                                &talaria_core::system::paths::talaria_home()
                                    .join("databases")
                                    .join("taxonomy"),
                            )?;
                        }
                        Ok(false) => {
                            progress_callback("    Taxonomy data already up to date");
                        }
                        Err(e) => {
                            progress_callback(&format!("    ✗ Failed to download taxonomy: {}", e));
                            progress_callback(
                                "    Using minimal taxonomy structure for basic operation",
                            );
                            // Ensure at least a minimal taxonomy structure for fallback
                            self.repository.taxonomy.ensure_taxonomy()?;
                        }
                    }
                }
            } else {
                progress_callback("Taxonomy data is loaded and ready");
            }
        }

        // Use the new workspace-isolated download system
        // First check if there's an existing complete download

        // Create download manager with database manager for processing
        let mut download_manager = DownloadManager::new()?;

        let options = DownloadOptions {
            skip_verify: false,
            resume: true,              // Always enable resume
            preserve_on_failure: true, // Keep files if processing fails
            preserve_always: std::env::var("TALARIA_PRESERVE_DOWNLOADS").is_ok(),
            force: false,
        };

        let mut progress = DownloadProgress::new();

        // Check if there's already a complete download before downloading
        let existing_download = if options.resume {
            match find_existing_workspace_for_source(source)? {
                Some((_workspace, state)) if state.stage == Stage::Complete => {
                    if let Some(decompressed) = state.files.decompressed.as_ref() {
                        if decompressed.exists() {
                            let file_size = decompressed.metadata()?.len();
                            progress_callback(&format!(
                                "✓ Found complete download: {} ({:.2} GB)",
                                decompressed
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy(),
                                file_size as f64 / 1_073_741_824.0
                            ));
                            progress_callback(
                                "Skipping download, proceeding directly to processing...",
                            );
                            Some(decompressed.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            }
        } else {
            None
        };

        // Use existing download or download new
        let output_path = if let Some(existing_path) = existing_download {
            // We have a complete download, use it directly
            existing_path
        } else {
            // Need to download (either fresh or resume incomplete)
            progress_callback("Downloading database file...");
            let result = download_manager
                .download_with_state(source.clone(), options, &mut progress)
                .await?;
            progress_callback("Download complete");
            result
        };

        // Process the downloaded/existing file
        progress_callback("Processing database into HERALD chunks...");
        progress_callback("This one-time conversion enables future incremental updates");

        // Chunk the database (only if not a taxonomy file)
        if !Self::is_taxonomy_database(source) {
            if let Err(e) = self.chunk_database(&output_path, source, Some(&progress_callback)) {
                // Don't delete the file - it's preserved in the workspace
                progress_callback(&format!(
                    "Processing failed: {}. Downloaded file preserved in workspace for retry.",
                    e
                ));
                progress_callback("Run with --resume to retry processing without re-downloading");
                return Err(e);
            }
        } else {
            // For taxonomy files, store them directly
            if let Err(e) = self.store_taxonomy_mapping_file(&output_path, source) {
                progress_callback(&format!(
                    "Failed to store taxonomy file: {}. File preserved for retry.",
                    e
                ));
                return Err(e);
            }
        }

        // Now that processing is complete, clean up the download workspace
        // unless preservation is enabled
        if let Err(e) = DownloadManager::cleanup_download_workspace(source) {
            progress_callback(&format!("Warning: Failed to clean up workspace: {}", e));
            // Don't fail the whole operation if cleanup fails
        }

        progress_callback("✓ Initial HERALD setup complete!");
        progress_callback("Future updates will only download changed chunks");

        Ok(DownloadResult::InitialDownload {
            total_chunks: 0,
            total_size: 0,
        })
    }

    /// Chunk sequences directly into HERALD format (unified pipeline)
    pub fn chunk_sequences_direct(
        &mut self,
        sequences: Vec<Sequence>,
        source: &DatabaseSource,
    ) -> Result<()> {
        let _span = tracing::info_span!(
            "chunk_sequences_direct",
            source = %source,
            sequence_count = sequences.len()
        ).entered();

        // Since this is called for non-streaming mode, it's the final (and only) batch
        self.chunk_sequences_direct_with_progress_final(sequences, source, None, true)
    }

    /// Quiet version of chunk_sequences_direct for use in streaming mode
    #[allow(dead_code)]
    fn chunk_sequences_direct_quiet(
        &mut self,
        sequences: Vec<Sequence>,
        source: &DatabaseSource,
    ) -> Result<()> {
        // Call the version with progress callback, passing None
        self.chunk_sequences_direct_with_progress(sequences, source, None)
    }

    /// Process sequences with optional progress callback
    #[allow(dead_code)]
    fn chunk_sequences_direct_with_progress(
        &mut self,
        sequences: Vec<Sequence>,
        source: &DatabaseSource,
        progress_callback: Option<Box<dyn Fn(usize, &str) + Send>>,
    ) -> Result<()> {
        // Default to not being the final batch
        self.chunk_sequences_direct_with_progress_final(sequences, source, progress_callback, false)
    }

    /// Process sequences with optional progress callback and final batch indicator
    pub fn chunk_sequences_direct_with_progress_final(
        &mut self,
        sequences: Vec<Sequence>,
        source: &DatabaseSource,
        progress_callback: Option<Box<dyn Fn(usize, &str) + Send>>,
        is_final_batch: bool,
    ) -> Result<()> {
        self.chunk_sequences_direct_with_progress_final_batch(
            sequences,
            source,
            progress_callback,
            is_final_batch,
            0,
        )
    }

    /// Process sequences with batch tracking for partial manifest saving
    pub fn chunk_sequences_direct_with_progress_final_batch(
        &mut self,
        sequences: Vec<Sequence>,
        source: &DatabaseSource,
        progress_callback: Option<Box<dyn Fn(usize, &str) + Send>>,
        is_final_batch: bool,
        batch_num: usize,
    ) -> Result<()> {
        use std::sync::Arc;

        // Use the existing SequenceStorage from the repository
        // This avoids trying to open RocksDB twice
        let sequence_storage = Arc::clone(&self.get_repository().storage.sequence_storage);

        // Create chunker with canonical storage
        let mut chunker = TaxonomicChunker::new(
            ChunkingStrategy::default(),
            sequence_storage,
            source.clone(),
        );

        // Process sequences - for now run quietly since the callback type conversion is complex
        // TODO: Thread progress callbacks through properly
        let chunk_manifests = chunker.chunk_sequences_canonical_quiet_final(sequences, is_final_batch)?;

        // Store the manifests quietly as well
        let manifests_with_hashes = self.store_chunk_manifests_internal(
            chunk_manifests,
            source,
            None
        )?;

        // Report progress if callback exists
        if let Some(cb) = progress_callback {
            cb(batch_num, &format!("Processed batch {} with {} manifests", batch_num, manifests_with_hashes.len()));
        }

        // Get or create version for this operation
        if self.current_version.is_none() {
            self.current_version = Some(talaria_core::system::paths::generate_utc_timestamp());
        }
        let version = self.current_version.as_ref().unwrap().clone();

        // Save partial manifest for this batch (no memory accumulation!)
        if !manifests_with_hashes.is_empty() {
            self.save_partial_manifest(batch_num, manifests_with_hashes, source, &version)?;
            // Removed tracing to prevent progress bar interference
        }

        // On final batch, merge all partial manifests and create final database manifest
        if is_final_batch {
            // Final batch reached - merging partial manifests

            // Merge all partial manifests
            let all_manifests = self.merge_partial_manifests(source, &version)?;

            if !all_manifests.is_empty() {
                // Save final merged manifest
                // Check if quiet mode is enabled
                let verbose = !std::env::var("TALARIA_LOG")
                    .map(|v| v == "error")
                    .unwrap_or(false);
                self.save_database_manifest_internal_with_version(all_manifests, source, &version, verbose)?;

                // Flush RocksDB to ensure manifest is persisted to disk
                // This ensures the database is immediately visible in database list
                self.get_repository()
                    .storage
                    .sequence_storage
                    .get_rocksdb()
                    .flush()?;

                // Compact database to compress uncompressed L0/L1 data
                // This is critical to reduce storage bloat after bulk writes
                tracing::info!("Compacting database to compress sequences...");
                self.get_repository()
                    .storage
                    .sequence_storage
                    .get_rocksdb()
                    .compact()?;
                tracing::info!("Database compacted successfully");

                // Final database manifest saved and persisted
            }

            // Clear version for next operation
            self.current_version = None;
            self.accumulated_manifests.clear(); // Just in case
        }

        Ok(())
    }

    /// Save database manifest in versions directory structure (quiet version)
    #[allow(dead_code)]
    fn save_database_manifest_quiet(
        &mut self,
        manifests_with_hashes: Vec<(crate::ChunkManifest, crate::SHA256Hash)>,
        source: &DatabaseSource,
    ) -> Result<()> {
        self.save_database_manifest_internal(manifests_with_hashes, source, false)
    }

    /// Save database manifest with specific version (quiet version)
    #[allow(dead_code)]
    fn save_database_manifest_quiet_with_version(
        &mut self,
        manifests_with_hashes: Vec<(crate::ChunkManifest, crate::SHA256Hash)>,
        source: &DatabaseSource,
        version: &str,
    ) -> Result<()> {
        self.save_database_manifest_internal_with_version(
            manifests_with_hashes,
            source,
            version,
            false,
        )
    }

    /// Save partial manifest for a batch (static version for use in threads)
    /// In streaming mode, also stores the chunk manifests immediately to RocksDB
    fn save_partial_manifest_static(
        rocksdb: &Arc<talaria_storage::backend::RocksDBBackend>,
        chunk_storage: Option<&Arc<talaria_storage::backend::RocksDBBackend>>,
        batch_num: usize,
        manifests_with_hashes: Vec<(crate::ChunkManifest, crate::SHA256Hash)>,
        source: &DatabaseSource,
        version: &str,
    ) -> Result<()> {
        // Store in RocksDB with structured key
        let (source_name, dataset_name) = match source {
            DatabaseSource::UniProt(db) => ("uniprot", format!("{:?}", db).to_lowercase()),
            DatabaseSource::NCBI(db) => ("ncbi", format!("{:?}", db).to_lowercase()),
            DatabaseSource::Custom(name) => ("custom", name.clone()),
        };

        // Create key for partial manifest in RocksDB
        let key = format!(
            "partial:{}:{}:{}:{:06}",
            source_name, dataset_name, version, batch_num
        );

        // DEBUG: Show what key we're saving to
        if batch_num == 0 || batch_num % 1000 == 0 {
            tracing::debug!(" Saving partial manifest to key: {}", key);
        }

        // STREAMING MODE: Store chunk manifests immediately if chunk_storage is provided
        let finalized = if let Some(chunk_storage_ref) = chunk_storage {
            // Serialize each manifest and store to RocksDB immediately
            let batch_data: Vec<(crate::SHA256Hash, Vec<u8>)> = manifests_with_hashes
                .iter()
                .map(|(manifest, hash)| {
                    let data = rmp_serde::to_vec(manifest)?;
                    Ok((hash.clone(), data))
                })
                .collect::<Result<Vec<_>>>()?;

            // Store all manifests in this batch directly to RocksDB MANIFESTS column family
            chunk_storage_ref.store_chunks_batch(&batch_data)?;

            true // Mark as finalized
        } else {
            false // Not finalized (old behavior for compatibility)
        };

        // Create partial manifest data structure
        let partial_manifest = PartialManifest {
            batch_num,
            manifests: manifests_with_hashes.clone(),
            sequence_count: manifests_with_hashes
                .iter()
                .map(|(m, _)| m.sequence_count)
                .sum::<usize>(),
            finalized,
        };

        // Serialize and store in RocksDB MANIFESTS column family
        let data = bincode::serialize(&partial_manifest)?;
        rocksdb.put_manifest(&key, &data)?;

        // Removed tracing to prevent progress bar interference during streaming
        Ok(())
    }

    /// Save partial manifest for a batch (instance method wrapper)
    fn save_partial_manifest(
        &self,
        batch_num: usize,
        manifests_with_hashes: Vec<(crate::ChunkManifest, crate::SHA256Hash)>,
        source: &DatabaseSource,
        version: &str,
    ) -> Result<()> {
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let chunk_storage = self.get_repository().storage.chunk_storage();
        Self::save_partial_manifest_static(
            &rocksdb,
            Some(&chunk_storage),
            batch_num,
            manifests_with_hashes,
            source,
            version,
        )
    }

    /// Build manifest index from partial manifests (streaming mode - chunks already stored)
    /// This is lightweight - just reads the manifest metadata, not the actual chunk data
    fn build_manifest_index_from_partials(
        &mut self,
        source: &DatabaseSource,
        version: &str,
    ) -> Result<Vec<(crate::ChunkManifest, crate::SHA256Hash)>> {
        // Use same structure as save_partial_manifest
        let (source_name, dataset_name) = match source {
            DatabaseSource::UniProt(db) => ("uniprot", format!("{:?}", db).to_lowercase()),
            DatabaseSource::NCBI(db) => ("ncbi", format!("{:?}", db).to_lowercase()),
            DatabaseSource::Custom(name) => ("custom", name.clone()),
        };

        let mut all_manifests = Vec::new();

        // Get RocksDB backend
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Read partial manifests directly by batch number
        let prefix = format!("partial:{}:{}:{}:", source_name, dataset_name, version);

        tracing::info!("Looking for partials with prefix: {}", prefix);

        // DEBUG: Also list ALL partial manifests to see what's actually stored
        let all_partials_prefix = format!("partial:{}:{}:", source_name, dataset_name);
        let existing_keys = rocksdb.list_manifest_keys_with_prefix(&all_partials_prefix)?;
        if !existing_keys.is_empty() {
            tracing::warn!(
                "Found {} existing partial manifests with broader prefix '{}'. First few keys:",
                existing_keys.len(),
                all_partials_prefix
            );
            for (i, key) in existing_keys.iter().take(5).enumerate() {
                tracing::warn!("  [{}] {}", i, key);
            }
            if existing_keys.len() > 5 {
                tracing::warn!("  ... and {} more", existing_keys.len() - 5);
            }
        } else {
            tracing::error!(
                "NO partial manifests found with prefix '{}' - this is the problem!",
                all_partials_prefix
            );
        }

        let mut processed = 0;
        let mut consecutive_misses = 0;
        const MAX_BATCHES: usize = 10000; // Safety limit
        const MAX_CONSECUTIVE_MISSES: usize = 10; // Allow gaps but stop after 10 consecutive misses

        // Building manifest index from partials (tracing removed to prevent progress bar interference)

        for batch_num in 0..=MAX_BATCHES {
            let key = format!("{}{:06}", prefix, batch_num);

            if let Some(data) = rocksdb.get_manifest(&key)? {
                // Deserialize the partial manifest
                let partial: PartialManifest = bincode::deserialize(&data).map_err(|e| {
                    anyhow::anyhow!("Failed to deserialize partial manifest {}: {}", key, e)
                })?;

                // Add manifests to index
                all_manifests.extend(partial.manifests);

                processed += 1;
                consecutive_misses = 0; // Reset on successful read
                // Progress tracking removed to prevent terminal interference
            } else {
                // Track consecutive misses to allow gaps in batch numbering
                // (batch 0 might be empty and not saved, for example)
                consecutive_misses += 1;
                if consecutive_misses >= MAX_CONSECUTIVE_MISSES {
                    // No more partials after reasonable gap
                    break;
                }
            }
        }

        tracing::debug!(" Processed {} partial manifests", processed);

        // Built index from partial manifests (tracing removed to prevent progress bar interference)

        // Clean up partial manifests from RocksDB after indexing
        for batch_num in 0..processed {
            let key = format!("{}{:06}", prefix, batch_num);
            rocksdb.delete_manifest(&key)?;
        }

        // Cleaned up partial manifests (tracing removed to prevent progress bar interference)

        Ok(all_manifests)
    }

    /// Load chunk_index (ManifestMetadata) from partial manifests
    /// This is used for lazy-loading streaming manifests
    fn load_chunk_index_from_partials(
        &self,
        source_name: &str,
        dataset_name: &str,
        version: &str,
    ) -> Result<Vec<crate::ManifestMetadata>> {
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let prefix = format!("partial:{}:{}:{}:", source_name, dataset_name, version);

        let mut chunk_index = Vec::new();
        let mut processed = 0;
        let mut consecutive_misses = 0;
        const MAX_BATCHES: usize = 100000; // Safety limit (increased for large DBs)
        const MAX_CONSECUTIVE_MISSES: usize = 10;

        for batch_num in 0..=MAX_BATCHES {
            let key = format!("{}{:06}", prefix, batch_num);

            if let Some(data) = rocksdb.get_manifest(&key)? {
                let partial: PartialManifest = bincode::deserialize(&data).map_err(|e| {
                    anyhow::anyhow!("Failed to deserialize partial manifest {}: {}", key, e)
                })?;

                // Convert ChunkManifest to ManifestMetadata
                for (chunk_manifest, chunk_hash) in partial.manifests {
                    chunk_index.push(crate::ManifestMetadata {
                        hash: chunk_hash,
                        taxon_ids: chunk_manifest.taxon_ids.clone(),
                        sequence_count: chunk_manifest.sequence_count,
                        size: chunk_manifest.total_size,
                        compressed_size: None, // Not tracked in ChunkManifest
                    });
                }

                processed += 1;
                consecutive_misses = 0;
            } else {
                consecutive_misses += 1;
                if consecutive_misses >= MAX_CONSECUTIVE_MISSES {
                    break;
                }
            }
        }

        tracing::debug!(
            "Loaded chunk_index with {} entries from {} partial manifests",
            chunk_index.len(),
            processed
        );

        Ok(chunk_index)
    }

    /// Get total size from partials without loading full chunk metadata into memory
    /// This is much more memory-efficient than load_chunk_index_from_partials for size-only queries
    fn get_total_size_from_partials(
        &self,
        source_name: &str,
        dataset_name: &str,
        version: &str,
    ) -> Result<usize> {
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let prefix = format!("partial:{}:{}:{}:", source_name, dataset_name, version);

        let mut total_size = 0usize;
        let mut processed = 0;
        let mut consecutive_misses = 0;
        const MAX_BATCHES: usize = 100000; // Safety limit
        const MAX_CONSECUTIVE_MISSES: usize = 10;

        for batch_num in 0..=MAX_BATCHES {
            let key = format!("{}{:06}", prefix, batch_num);

            if let Some(data) = rocksdb.get_manifest(&key)? {
                let partial: PartialManifest = bincode::deserialize(&data).map_err(|e| {
                    anyhow::anyhow!("Failed to deserialize partial manifest {}: {}", key, e)
                })?;

                // Sum sizes without creating full ManifestMetadata structs
                for (chunk_manifest, _chunk_hash) in partial.manifests {
                    total_size += chunk_manifest.total_size;
                }

                processed += 1;
                consecutive_misses = 0;
            } else {
                consecutive_misses += 1;
                if consecutive_misses >= MAX_CONSECUTIVE_MISSES {
                    break;
                }
            }
        }

        tracing::debug!(
            "Calculated total size {} bytes from {} partial manifests without loading full chunk metadata",
            total_size,
            processed
        );

        Ok(total_size)
    }

    /// Build and save manifest index from partials using streaming to avoid OOM
    /// Returns total number of manifests processed
    fn build_and_save_manifest_streaming(
        &mut self,
        source: &DatabaseSource,
        version: &str,
        progress_callback: Option<&dyn Fn(&str)>,
    ) -> Result<usize> {
        use crate::{BiTemporalCoordinate, TemporalManifest, SHA256Hash, MerkleHash};
        use chrono::Utc;

        let (source_name, dataset_name) = match source {
            DatabaseSource::UniProt(db) => ("uniprot", format!("{:?}", db).to_lowercase()),
            DatabaseSource::NCBI(db) => ("ncbi", format!("{:?}", db).to_lowercase()),
            DatabaseSource::Custom(name) => ("custom", name.clone()),
        };

        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let prefix = format!("partial:{}:{}:{}:", source_name, dataset_name, version);

        // Count manifests and sequences without loading them all into memory
        let mut total_manifests = 0usize;
        let mut total_sequences = 0usize;
        let mut processed_partials = 0;
        let mut consecutive_misses = 0;
        const MAX_BATCHES: usize = 100000; // Increased for very large databases
        const MAX_CONSECUTIVE_MISSES: usize = 10;

        // First pass: just count manifests and sequences to avoid OOM
        tracing::info!("Counting manifests and sequences from partials...");
        for batch_num in 0..=MAX_BATCHES {
            let key = format!("{}{:06}", prefix, batch_num);

            if let Some(data) = rocksdb.get_manifest(&key)? {
                let partial: PartialManifest = bincode::deserialize(&data)?;
                total_manifests += partial.manifests.len();
                total_sequences += partial.sequence_count;
                processed_partials += 1;
                consecutive_misses = 0;

                // Progress update every 1000 partials
                if processed_partials % 1000 == 0 {
                    if let Some(cb) = progress_callback {
                        cb(&format!(
                            "Counting: {} batches, {} chunks, {} sequences...",
                            processed_partials,
                            total_manifests,
                            total_sequences
                        ));
                    }
                }
            } else {
                consecutive_misses += 1;
                if consecutive_misses >= MAX_CONSECUTIVE_MISSES {
                    break;
                }
            }
        }

        if total_manifests == 0 {
            return Ok(0);
        }

        tracing::info!("Counted {} total manifests from {} partial batches", total_manifests, processed_partials);

        // For very large manifest counts, create a lightweight manifest that references partials
        // instead of loading everything into memory
        // The chunks themselves are already stored in chunk_storage, partials are just references
        let now = Utc::now();
        let manifest_data = TemporalManifest {
            version: version.to_string(),
            created_at: now,
            sequence_version: version.to_string(),
            taxonomy_version: "current".to_string(), // Will be updated when taxonomy is loaded
            temporal_coordinate: Some(BiTemporalCoordinate {
                sequence_time: now,
                taxonomy_time: now,
            }),
            taxonomy_root: MerkleHash::default(),
            sequence_root: MerkleHash::default(),
            chunk_merkle_tree: None,
            taxonomy_manifest_hash: SHA256Hash::default(),
            taxonomy_dump_version: "unknown".to_string(),
            source_database: Some(format!("{}/{}", source_name, dataset_name)),
            chunk_index: Vec::new(), // Empty - data is in partials which will be lazy-loaded
            discrepancies: Vec::new(),
            etag: format!("streaming-{}", version),
            previous_version: None,
        };

        // Store a marker manifest indicating partials exist
        let manifest_json = serde_json::to_vec(&manifest_data)?;
        let manifest_key = format!("manifest:{}:{}:{}", source_name, dataset_name, version);
        rocksdb.put_manifest(&manifest_key, &manifest_json)?;

        // Store manifest count separately for queries
        let count_key = format!("manifest_count:{}:{}:{}", source_name, dataset_name, version);
        rocksdb.put_manifest(&count_key, &total_manifests.to_le_bytes())?;

        // Store sequence count separately for queries
        let seq_count_key = format!("sequence_count:{}:{}:{}", source_name, dataset_name, version);
        rocksdb.put_manifest(&seq_count_key, &total_sequences.to_le_bytes())?;

        // Use shared manifest saving function
        self.save_manifest_to_repository(
            source_name,
            &dataset_name,
            version,
            &manifest_data,
            total_manifests,
            total_sequences,
            0, // total_size not easily available for streaming
        )?;

        // DON'T delete partial manifests - keep them for lazy loading
        // This allows queries to access chunks without loading all 72M manifests into memory
        // Partials will be cleaned up when the database is deleted

        if let Some(cb) = progress_callback {
            cb(&format!("✓ Manifest created ({} chunks stored in {} partials)", total_manifests, processed_partials));
        }

        tracing::info!("Streaming manifest build complete: {} manifests", total_manifests);

        Ok(total_manifests)
    }

    /// Merge all partial manifests into final manifest (old non-streaming mode)
    /// DEPRECATED: Use build_manifest_index_from_partials for streaming mode
    fn merge_partial_manifests(
        &mut self,
        source: &DatabaseSource,
        version: &str,
    ) -> Result<Vec<(crate::ChunkManifest, crate::SHA256Hash)>> {
        // Use same structure as save_partial_manifest
        let (source_name, dataset_name) = match source {
            DatabaseSource::UniProt(db) => ("uniprot", format!("{:?}", db).to_lowercase()),
            DatabaseSource::NCBI(db) => ("ncbi", format!("{:?}", db).to_lowercase()),
            DatabaseSource::Custom(name) => ("custom", name.clone()),
        };

        let mut all_manifests = Vec::new();

        // Get RocksDB backend
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Read partial manifests directly by batch number
        // Keys are in format: "partial:{source}:{dataset}:{version}:{batch_num:06}"
        let prefix = format!("partial:{}:{}:{}:", source_name, dataset_name, version);

        // For UniRef50, we know there are 7020 batches
        // Read them directly without listing first
        let mut processed = 0;
        const MAX_BATCHES: usize = 10000; // Safety limit

        // Merging partial manifests (tracing removed to prevent progress bar interference)

        for batch_num in 0..=MAX_BATCHES {
            let key = format!("{}{:06}", prefix, batch_num);

            if let Some(data) = rocksdb.get_manifest(&key)? {
                // Deserialize the partial manifest
                let partial: PartialManifest = bincode::deserialize(&data).map_err(|e| {
                    anyhow::anyhow!("Failed to deserialize partial manifest {}: {}", key, e)
                })?;
                all_manifests.extend(partial.manifests);

                processed += 1;
                // Progress tracking removed to prevent terminal interference
            } else {
                // No more partials
                break;
            }
        }

        // Merged partial manifests (tracing removed to prevent progress bar interference)

        // Clean up partial manifests from RocksDB after merging
        // Delete the same keys we just processed
        for batch_num in 0..processed {
            let key = format!("{}{:06}", prefix, batch_num);
            rocksdb.delete_manifest(&key)?;
        }

        // Cleaned up partial manifests (tracing removed to prevent progress bar interference)

        Ok(all_manifests)
    }

    /// Internal implementation of save_database_manifest
    fn save_database_manifest_internal(
        &mut self,
        manifests_with_hashes: Vec<(crate::ChunkManifest, crate::SHA256Hash)>,
        source: &DatabaseSource,
        verbose: bool,
    ) -> Result<()> {
        use chrono::Utc;
        // Generate new version timestamp
        let version = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        self.save_database_manifest_internal_with_version(
            manifests_with_hashes,
            source,
            &version,
            verbose,
        )
    }

    /// Internal implementation with specific version
    fn save_database_manifest_internal_with_version(
        &mut self,
        manifests_with_hashes: Vec<(crate::ChunkManifest, crate::SHA256Hash)>,
        source: &DatabaseSource,
        version: &str,
        verbose: bool,
    ) -> Result<()> {
        use crate::{BiTemporalCoordinate, ManifestMetadata, TemporalManifest};
        use chrono::Utc;

        // Get source and dataset names for RocksDB keys
        let (source_name, dataset_name) = match source {
            DatabaseSource::UniProt(db) => ("uniprot", format!("{:?}", db).to_lowercase()),
            DatabaseSource::NCBI(db) => ("ncbi", format!("{:?}", db).to_lowercase()),
            DatabaseSource::Custom(name) => ("custom", name.clone()),
        };

        // Convert chunk manifests to metadata using the stored hashes
        let chunk_metadata: Vec<ManifestMetadata> = if manifests_with_hashes.len() > 100000 {
            // Use parallel processing for large datasets
            use rayon::prelude::*;
            if verbose {
                info!(
                    "Creating metadata for {} manifests in parallel...",
                    manifests_with_hashes.len()
                );
            }
            manifests_with_hashes
                .par_iter()
                .map(|(manifest, stored_hash)| {
                    ManifestMetadata {
                        hash: stored_hash.clone(), // Use the actual stored hash, not the manifest's internal hash
                        taxon_ids: manifest.taxon_ids.clone(),
                        sequence_count: manifest.sequence_count,
                        size: manifest.total_size,
                        compressed_size: Some(manifest.total_size / 10), // Estimate
                    }
                })
                .collect()
        } else {
            // Use sequential processing for smaller datasets
            manifests_with_hashes
                .iter()
                .map(|(manifest, stored_hash)| {
                    ManifestMetadata {
                        hash: stored_hash.clone(), // Use the actual stored hash, not the manifest's internal hash
                        taxon_ids: manifest.taxon_ids.clone(),
                        sequence_count: manifest.sequence_count,
                        size: manifest.total_size,
                        compressed_size: Some(manifest.total_size / 10), // Estimate
                    }
                })
                .collect()
        };

        // Create temporal manifest
        let manifest = TemporalManifest {
            version: version.to_string(),
            created_at: Utc::now(),
            sequence_version: version.to_string(),
            taxonomy_version: "current".to_string(),
            temporal_coordinate: Some(BiTemporalCoordinate {
                sequence_time: Utc::now(),
                taxonomy_time: Utc::now(),
            }),
            taxonomy_root: crate::SHA256Hash::zero(),
            sequence_root: crate::SHA256Hash::zero(),
            chunk_merkle_tree: None,
            taxonomy_manifest_hash: crate::SHA256Hash::zero(),
            taxonomy_dump_version: "current".to_string(),
            source_database: Some(format!("{}/{}", source_name, dataset_name)),
            chunk_index: chunk_metadata,
            discrepancies: Vec::new(),
            etag: format!("{}-{}", source_name, version),
            previous_version: None,
        };

        // Check for duplicate manifests before saving
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let current_alias_key = format!("alias:{}:{}:current", source_name, dataset_name);

        let final_version = if let Ok(Some(current_version_bytes)) =
            rocksdb.get_manifest(&current_alias_key)
        {
            let current_version =
                String::from_utf8(current_version_bytes).unwrap_or_else(|_| version.to_string());
            let current_manifest_key = format!(
                "manifest:{}:{}:{}",
                source_name, dataset_name, current_version
            );

            if let Ok(Some(current_manifest_data)) = rocksdb.get_manifest(&current_manifest_key) {
                if let Ok(current_manifest) =
                    bincode::deserialize::<TemporalManifest>(&current_manifest_data)
                {
                    // Compare manifests by chunk content
                    if Self::manifests_are_identical(&manifest, &current_manifest) {
                        if verbose {
                            tracing::info!(
                                "✓ Content identical to version {}, reusing existing version",
                                current_version
                            );
                        }
                        current_version // Reuse existing version
                    } else {
                        version.to_string() // Different content, use new version
                    }
                } else {
                    version.to_string()
                }
            } else {
                version.to_string()
            }
        } else {
            version.to_string() // No current version exists
        };

        // Use shared manifest saving function
        let chunk_count = manifest.chunk_index.len();
        let sequence_count = manifest.chunk_index.iter().map(|c| c.sequence_count).sum();
        let total_size = manifest.chunk_index.iter().map(|c| c.size).sum();

        self.save_manifest_to_repository(
            source_name,
            &dataset_name,
            &final_version,
            &manifest,
            chunk_count,
            sequence_count,
            total_size,
        )?;

        tracing::info!("✓ Successfully saved manifest for {}/{} version {}", source_name, dataset_name, final_version);

        // Verify the alias was actually set
        let current_alias_key = format!("alias:{}:{}:current", source_name, dataset_name);
        if let Ok(Some(stored)) = self.get_repository().storage.sequence_storage.get_rocksdb().get_manifest(&current_alias_key) {
            let stored_version = String::from_utf8_lossy(&stored);
            tracing::debug!("Verified current alias points to: {}", stored_version);
        } else {
            tracing::warn!("Could not verify current alias was set correctly");
        }

        // Create version metadata with upstream version detection
        self.create_version_metadata(source, &final_version, std::path::Path::new(""))?;

        // Invalidate caches since database was modified
        if let Some(cache) = &self.cache {
            cache.invalidate_database(source_name, &dataset_name);
        }

        if verbose {
            tracing::info!(
                "✓ Database manifest saved to RocksDB ({}:{}:{})",
                source_name, dataset_name, final_version
            );
        }

        Ok(())
    }

    /// Store chunk manifests quietly (no progress bar)
    #[allow(dead_code)]
    fn store_chunk_manifests_quiet(
        &mut self,
        manifests: Vec<crate::ChunkManifest>,
        source: &DatabaseSource,
    ) -> Result<Vec<(crate::ChunkManifest, crate::SHA256Hash)>> {
        self.store_chunk_manifests_internal(manifests, source, None)
    }

    /// Internal implementation of store_chunk_manifests
    fn store_chunk_manifests_internal(
        &mut self,
        manifests: Vec<crate::ChunkManifest>,
        _source: &DatabaseSource,
        progress_callback: Option<&dyn Fn(&str)>,
    ) -> Result<Vec<(crate::ChunkManifest, crate::SHA256Hash)>> {
        use crate::ManifestMetadata;

        let total = manifests.len();
        if let Some(cb) = progress_callback {
            cb(&format!("Storing {} chunk manifests", total));
        }

        let mut manifest_with_hashes = Vec::new();

        // Batch manifests to reduce I/O operations
        const BATCH_SIZE: usize = 100;
        let manifest_chunks: Vec<_> = manifests.chunks(BATCH_SIZE).collect();

        for (batch_idx, chunk) in manifest_chunks.iter().enumerate() {
            // Process a batch of manifests
            let mut batch_data = Vec::new();
            let mut batch_metadata = Vec::new();

            for manifest in *chunk {
                // Serialize manifest
                let manifest_data = rmp_serde::to_vec(&manifest)?;
                let hash = SHA256Hash::compute(&manifest_data);

                // Prepare for batch storage
                batch_data.push((hash.clone(), manifest_data.clone()));

                // Create metadata
                let metadata = ManifestMetadata {
                    hash: hash.clone(),
                    taxon_ids: manifest.taxon_ids.clone(),
                    sequence_count: manifest.sequence_count,
                    size: manifest.total_size,
                    compressed_size: Some(manifest_data.len()),
                };

                batch_metadata.push(metadata);
                manifest_with_hashes.push((manifest.clone(), hash));
            }

            // Store batch of manifests at once
            for (hash, data) in batch_data {
                // Check if already exists before storing (deduplication)
                if !self.repository.storage.has_chunk(&hash) {
                    self.repository.storage.store_chunk(&data, true)?;
                }
            }

            // Add all metadata to repository index
            for metadata in batch_metadata {
                self.repository.manifest.add_chunk(metadata);
            }

            // Report progress every batch
            if let Some(cb) = progress_callback {
                let stored = (batch_idx + 1) * BATCH_SIZE.min(total - batch_idx * BATCH_SIZE);
                cb(&format!("Stored {}/{} manifests", stored, total));
            }
        }

        if let Some(cb) = progress_callback {
            cb(&format!("All {} manifests stored", total));
        }

        // Save and persist the repository state
        self.repository.save()?;

        Ok(manifest_with_hashes)
    }

    /// Chunk database with resumable processing support
    pub fn chunk_database_with_resume(
        &mut self,
        file_path: &Path,
        source: &DatabaseSource,
        workspace_path: Option<&Path>,
        progress_callback: Option<&dyn Fn(&str)>,
    ) -> Result<()> {
        let _span = tracing::info_span!(
            "chunk_database_with_resume",
            source = %source,
            file_path = %file_path.display(),
            workspace_path = ?workspace_path.as_ref().map(|p| p.display().to_string())
        ).entered();

        // Load or create download state for tracking progress
        let mut download_state = if let Some(workspace) = workspace_path {
            let state_path = workspace.join("state.json");
            if state_path.exists() {
                DownloadState::load(&state_path)?
            } else {
                // Create new state for tracking
                let mut state = DownloadState::new(source.clone(), workspace.to_path_buf());
                state.files.decompressed = Some(file_path.to_path_buf());
                state
            }
        } else {
            // No workspace, create temporary state
            let mut state = DownloadState::new(
                source.clone(),
                std::env::temp_dir().join(format!(
                    "talaria_chunk_{}",
                    chrono::Utc::now().timestamp_millis()
                )),
            );
            state.files.decompressed = Some(file_path.to_path_buf());
            state
        };

        // Check if we need to resume from a previous position
        if download_state.sequences_processed > 0 {
            if let Some(callback) = progress_callback {
                callback(&format!(
                    "Resuming processing from sequence {} (offset: {})",
                    download_state.sequences_processed,
                    download_state.file_offset
                ));
            }

            // CRITICAL: Check if processing already completed but final manifest wasn't saved
            // This prevents the "skip all sequences → 0 chunks saved" bug
            let (source_name, dataset_name) = match source {
                DatabaseSource::UniProt(db) => ("uniprot", format!("{:?}", db).to_lowercase()),
                DatabaseSource::NCBI(db) => ("ncbi", format!("{:?}", db).to_lowercase()),
                DatabaseSource::Custom(name) => ("custom", name.clone()),
            };

            // Try to find existing version from partial manifests
            let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
            let prefix = format!("partial:{}:{}:", source_name, dataset_name);
            let existing_keys = rocksdb.list_manifest_keys_with_prefix(&prefix).unwrap_or_default();

            if !existing_keys.is_empty() {
                // Extract version from first key
                let first_key = &existing_keys[0];
                let parts: Vec<&str> = first_key.split(':').collect();
                if parts.len() >= 4 {
                    let version = parts[3].to_string();

                    if let Some(callback) = progress_callback {
                        callback(&format!(
                            "Found {} existing partial manifests from version {}",
                            existing_keys.len(),
                            version
                        ));
                        callback("Attempting to build final manifest from existing data...");
                    }

                    // Try to build and save final manifest from partials
                    match self.build_manifest_index_from_partials(source, &version) {
                        Ok(all_manifests) if !all_manifests.is_empty() => {
                            if let Some(callback) = progress_callback {
                                callback(&format!(
                                    "✓ Successfully loaded {} chunk manifests",
                                    all_manifests.len()
                                ));
                                callback("Building final database manifest...");
                            }

                            // Save the final manifest
                            if let Some(callback) = progress_callback {
                                callback("Saving database manifest...");
                            }
                            self.save_database_manifest_internal_with_version(
                                all_manifests,
                                source,
                                &version,
                                true,
                            )?;

                            // Flush to disk
                            if let Some(callback) = progress_callback {
                                callback("Flushing to disk...");
                            }
                            rocksdb.flush()?;

                            if let Some(callback) = progress_callback {
                                callback("✓ Database manifest saved successfully");
                                callback(&format!(
                                    "✓ Resume completed - processed {} sequences total",
                                    download_state.sequences_processed
                                ));
                            }

                            // Early return - we're done!
                            return Ok(());
                        }
                        _ => {
                            // Couldn't build from partials - will need to re-process
                            if let Some(callback) = progress_callback {
                                callback(
                                    "Warning: Could not rebuild from partials, will re-process file",
                                );
                            }

                            // Reset resume position since we're starting over
                            tracing::warn!(
                                "Partial manifests exist but couldn't be used - resetting resume position"
                            );
                            download_state.sequences_processed = 0;
                            download_state.file_offset = 0;

                            // Clean up unusable partials
                            tracing::info!("Cleaning up {} unusable partial manifests", existing_keys.len());
                            for key in &existing_keys {
                                if let Err(e) = rocksdb.delete_manifest(key) {
                                    tracing::warn!("Failed to delete partial {}: {}", key, e);
                                }
                            }
                        }
                    }
                } else {
                    // No partials found at all - if sequences_processed > 0, this means
                    // the previous run crashed before persisting ANY chunk data
                    if download_state.sequences_processed > 0 {
                        tracing::warn!(
                            "Resume state shows {} sequences processed but no partial manifests found in RocksDB",
                            download_state.sequences_processed
                        );
                        tracing::warn!(
                            "Previous run likely crashed before persisting chunk data - resetting to reprocess"
                        );

                        if let Some(callback) = progress_callback {
                            callback(&format!(
                                "⚠ Previous chunking crashed before saving - reprocessing {} sequences from downloaded file",
                                download_state.sequences_processed
                            ));
                        }

                        // Reset to force full reprocessing (but keep the downloaded file!)
                        download_state.sequences_processed = 0;
                        download_state.file_offset = 0;

                        // Clean up orphaned partial manifests from the failed run
                        // This ensures we start with a fresh version instead of mixing old/new partials
                        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
                        let prefix = format!("partial:{}:{}:", source_name, dataset_name);
                        let orphaned_keys = rocksdb.list_manifest_keys_with_prefix(&prefix).unwrap_or_default();

                        if !orphaned_keys.is_empty() {
                            tracing::info!("Cleaning up {} orphaned partial manifests from failed run", orphaned_keys.len());
                            for key in &orphaned_keys {
                                if let Err(e) = rocksdb.delete_manifest(key) {
                                    tracing::warn!("Failed to delete orphaned partial {}: {}", key, e);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Call the actual chunking with state tracking
        let result = self.chunk_database_internal(
            file_path,
            source,
            &mut download_state,
            workspace_path,
            progress_callback,
        );

        // Save final state
        if let Some(workspace) = workspace_path {
            let state_path = workspace.join("state.json");
            download_state.save(&state_path)?;
        }

        result
    }

    /// Original chunk_database method for backwards compatibility
    pub fn chunk_database(
        &mut self,
        file_path: &Path,
        source: &DatabaseSource,
        progress_callback: Option<&dyn Fn(&str)>,
    ) -> Result<()> {
        self.chunk_database_with_resume(file_path, source, None, progress_callback)
    }

    fn chunk_database_internal(
        &mut self,
        file_path: &Path,
        source: &DatabaseSource,
        download_state: &mut DownloadState,
        workspace_path: Option<&Path>,
        progress_callback: Option<&dyn Fn(&str)>,
    ) -> Result<()> {
        let _span = tracing::info_span!(
            "chunk_database_internal",
            source = %source,
            file_path = %file_path.display(),
            workspace_path = ?workspace_path.as_ref().map(|p| p.display().to_string())
        ).entered();

        // Check if this is a taxonomy mapping file (not a FASTA file)
        if Self::is_taxonomy_database(source) {
            info!("Processing taxonomy database file");
            // Store taxonomy files in their proper location
            return self.store_taxonomy_mapping_file(file_path, source);
        }

        // Check file size to determine whether to use streaming
        let file_size = file_path.metadata()?.len();
        tracing::Span::current().record("file_size", file_size);

        const STREAMING_THRESHOLD: u64 = 1_000_000_000; // Use streaming for files > 1GB

        if file_size > STREAMING_THRESHOLD {
            info!(
                file_size_gb = file_size as f64 / 1_073_741_824.0,
                "Large file detected, using streaming mode"
            );
            let msg = format!(
                "Large file detected ({:.2} GB), using streaming mode...",
                file_size as f64 / 1_073_741_824.0
            );
            if let Some(cb) = progress_callback {
                cb(&msg);
            } else {
                tracing::info!("{}", msg);
            }
            self.chunk_database_streaming(
                file_path,
                source,
                download_state,
                workspace_path,
                progress_callback,
            )?;
        } else {
            // Read sequences from FASTA file
            // Note: read_fasta_sequences handles its own progress display
            let sequences = self.read_fasta_sequences(file_path, progress_callback)?;

            info!(sequence_count = sequences.len(), "Sequences loaded");

            // Use the unified pipeline
            self.chunk_sequences_direct(sequences, source)?;
        }

        info!("Database chunking completed successfully");
        Ok(())
    }

    /// Original chunking logic (kept for reference but not used)

    /// Create version metadata file with upstream version detection
    fn create_version_metadata(
        &self,
        source: &DatabaseSource,
        timestamp: &str,
        manifest_path: &Path,
    ) -> Result<()> {
        use talaria_utils::database::version_detector::{DatabaseVersion, VersionDetector};

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
                }
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
                }
                _ => None,
            };
        }

        // Set upstream version and store in RocksDB
        if let Some(upstream) = upstream_version {
            version.upstream_version = Some(upstream.clone());
            version.aliases.upstream.push(upstream.clone());
        }

        // Save version info to RocksDB
        let (source_name, dataset_name) = match source {
            DatabaseSource::UniProt(db) => ("uniprot", format!("{:?}", db).to_lowercase()),
            DatabaseSource::NCBI(db) => ("ncbi", format!("{:?}", db).to_lowercase()),
            DatabaseSource::Custom(name) => ("custom", name.clone()),
        };

        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Store version metadata
        let version_key = format!(
            "version:{}:{}:{}",
            source_name, dataset_name, version.timestamp
        );
        let version_data = bincode::serialize(&version)?;
        rocksdb.put_manifest(&version_key, &version_data)?;

        // Store alias for upstream version if exists
        if let Some(ref upstream) = version.upstream_version {
            let alias_key = format!("alias:{}:{}:{}", source_name, dataset_name, upstream);
            rocksdb.put_manifest(&alias_key, timestamp.as_bytes())?;
        }

        Ok(())
    }

    /// Update version aliases in RocksDB (no filesystem operations)
    #[allow(dead_code)]
    fn update_version_symlinks(&self, source: &DatabaseSource, version: &str) -> Result<()> {
        let (source_name, dataset_name) = match source {
            DatabaseSource::UniProt(db) => ("uniprot", format!("{:?}", db).to_lowercase()),
            DatabaseSource::NCBI(db) => ("ncbi", format!("{:?}", db).to_lowercase()),
            DatabaseSource::Custom(name) => ("custom", name.clone()),
        };

        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Store 'current' alias pointing to this version
        let current_alias_key = format!("alias:{}:{}:current", source_name, dataset_name);
        rocksdb.put_manifest(&current_alias_key, version.as_bytes())?;

        // Create temporal aliases based on the timestamp
        if version.len() >= 8 {
            let temporal_alias = match source {
                DatabaseSource::UniProt(_) => {
                    // Create monthly format alias: YYYY_MM
                    let year = &version[0..4];
                    let month = &version[4..6];
                    Some(format!("{}_{}", year, month))
                }
                DatabaseSource::NCBI(_) => {
                    // Create date format alias: YYYY-MM-DD
                    let year = &version[0..4];
                    let month = &version[4..6];
                    let day = &version[6..8];
                    Some(format!("{}-{}-{}", year, month, day))
                }
                _ => None,
            };

            // Store temporal alias if applicable
            if let Some(alias) = temporal_alias {
                let alias_key = format!("alias:{}:{}:{}", source_name, dataset_name, alias);
                rocksdb.put_manifest(&alias_key, version.as_bytes())?;

                // Also update version data in RocksDB with the alias
                let version_key = format!("version:{}:{}:{}", source_name, dataset_name, version);

                // Load version from RocksDB
                if let Ok(Some(data)) = rocksdb.get_manifest(&version_key) {
                    use talaria_utils::database::version_detector::DatabaseVersion;
                    if let Ok(mut version_data) = bincode::deserialize::<DatabaseVersion>(&data) {
                        // Update upstream version if not set
                        if version_data.upstream_version.is_none() {
                            version_data.upstream_version = Some(alias.clone());
                        }
                        // Add to upstream aliases if not present
                        if !version_data.aliases.upstream.contains(&alias) {
                            version_data.aliases.upstream.push(alias);
                        }
                        // Save updated version back to RocksDB
                        if let Ok(updated_data) = bincode::serialize(&version_data) {
                            rocksdb.put_manifest(&version_key, &updated_data).ok();
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
                DatabaseSource::UniProt(UniProtDatabase::SwissProt) => {
                    format!("{}/uniprot-swissprot.json", manifest_server)
                }
                DatabaseSource::UniProt(UniProtDatabase::TrEMBL) => {
                    format!("{}/uniprot-trembl.json", manifest_server)
                }
                DatabaseSource::NCBI(NCBIDatabase::NR) => {
                    format!("{}/ncbi-nr.json", manifest_server)
                }
                DatabaseSource::NCBI(NCBIDatabase::NT) => {
                    format!("{}/ncbi-nt.json", manifest_server)
                }
                _ => anyhow::bail!("No manifest URL for this database source"),
            });
        }

        // No manifest server configured - this is fine for local/dev use
        anyhow::bail!(
            "No manifest server configured (set TALARIA_MANIFEST_SERVER for remote updates)"
        )
    }

    /// Get database statistics (sequence count, chunk count, etc.)
    pub fn get_database_stats(&self, source: &DatabaseSource) -> Result<DatabaseStats> {
        let (source_name, dataset) = self.get_source_dataset_names(source);
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Get the manifest
        let current_alias_key = format!("alias:{}:{}:current", source_name, dataset);
        let manifest_bytes = rocksdb
            .get_manifest(&current_alias_key)?
            .ok_or_else(|| anyhow::anyhow!("No manifest found for {}", source))?;

        // Deserialize the manifest
        let manifest: crate::types::TemporalManifest = rmp_serde::from_slice(&manifest_bytes)?;

        // Count sequences and chunks
        let sequence_count = manifest.chunk_index.iter()
            .map(|chunk| chunk.sequence_count)
            .sum();

        let chunk_count = manifest.chunk_index.len();

        Ok(DatabaseStats {
            sequence_count,
            chunk_count,
            total_size: 0, // Would need to calculate from chunks
        })
    }

    /// Check if a database manifest exists in RocksDB (by DatabaseSource)
    ///
    /// Also performs automatic repair if database versions exist but current alias is missing
    fn has_database_by_source(&self, source: &DatabaseSource) -> Result<bool> {
        let (source_name, dataset) = self.get_source_dataset_names(source);
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Check if 'current' alias exists
        let current_alias_key = format!("alias:{}:{}:current", source_name, dataset);
        tracing::debug!("Looking up alias key: {}", current_alias_key);

        let result = rocksdb.get_manifest(&current_alias_key)?;
        tracing::debug!(
            "Alias lookup result: {:?}",
            result.as_ref().map(|_| "Some(manifest)")
        );

        if result.is_some() {
            return Ok(true);
        }

        // Alias doesn't exist - check if we have any versions for this database
        // This can happen if:
        // - Previous processing was interrupted before alias was set
        // - Database was created with older version of code
        // - Manual database corruption/cleanup
        tracing::debug!("Current alias not found, checking for existing versions...");

        // Quick check: list all manifest keys with this prefix (faster than full version info)
        let manifest_prefix = format!("manifest:{}:{}:", source_name, dataset);
        let manifest_keys = rocksdb.list_manifest_keys_with_prefix(&manifest_prefix)?;

        if !manifest_keys.is_empty() {
            // We have manifests but no current alias - find the latest timestamp
            // Extract timestamps from keys like "manifest:uniprot:uniref50:20251001_012052"
            let mut timestamps: Vec<String> = manifest_keys
                .iter()
                .filter_map(|key| key.strip_prefix(&manifest_prefix).map(|s| s.to_string()))
                .collect();

            if !timestamps.is_empty() {
                // Sort descending (latest first)
                timestamps.sort_by(|a, b| b.cmp(a));
                let latest_timestamp = &timestamps[0];

                tracing::warn!(
                    "Found {} versions for {}/{} but no current alias - repairing by setting current -> {}",
                    timestamps.len(),
                    source_name,
                    dataset,
                    latest_timestamp
                );

                // Set the current alias to point to the latest version
                if let Err(e) =
                    rocksdb.put_manifest(&current_alias_key, latest_timestamp.as_bytes())
                {
                    tracing::error!("Failed to repair current alias: {}", e);
                    return Err(e.into());
                }

                tracing::info!(
                    "✓ Repaired current alias: {} -> {}",
                    current_alias_key,
                    latest_timestamp
                );
                return Ok(true);
            }
        }

        // No versions exist at all
        Ok(false)
    }

    /// Get source and dataset names for directory structure
    fn get_source_dataset_names(&self, source: &DatabaseSource) -> (String, String) {
        use talaria_core::{NCBIDatabase, UniProtDatabase};

        match source {
            DatabaseSource::UniProt(UniProtDatabase::SwissProt) => {
                ("uniprot".to_string(), "swissprot".to_string())
            }
            DatabaseSource::UniProt(UniProtDatabase::TrEMBL) => {
                ("uniprot".to_string(), "trembl".to_string())
            }
            DatabaseSource::UniProt(UniProtDatabase::UniRef50) => {
                ("uniprot".to_string(), "uniref50".to_string())
            }
            DatabaseSource::UniProt(UniProtDatabase::UniRef90) => {
                ("uniprot".to_string(), "uniref90".to_string())
            }
            DatabaseSource::UniProt(UniProtDatabase::UniRef100) => {
                ("uniprot".to_string(), "uniref100".to_string())
            }
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => {
                ("uniprot".to_string(), "idmapping".to_string())
            }
            DatabaseSource::NCBI(NCBIDatabase::NR) => ("ncbi".to_string(), "nr".to_string()),
            DatabaseSource::NCBI(NCBIDatabase::NT) => ("ncbi".to_string(), "nt".to_string()),
            DatabaseSource::NCBI(NCBIDatabase::RefSeq) => {
                ("ncbi".to_string(), "refseq".to_string())
            }
            DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein) => {
                ("ncbi".to_string(), "refseq-protein".to_string())
            }
            DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic) => {
                ("ncbi".to_string(), "refseq-genomic".to_string())
            }
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => {
                ("ncbi".to_string(), "taxonomy".to_string())
            }
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => {
                ("ncbi".to_string(), "prot-accession2taxid".to_string())
            }
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => {
                ("ncbi".to_string(), "nucl-accession2taxid".to_string())
            }
            DatabaseSource::NCBI(NCBIDatabase::GenBank) => {
                ("ncbi".to_string(), "genbank".to_string())
            }
            DatabaseSource::Custom(name) => ("custom".to_string(), name.clone()),
        }
    }

    /// Get taxonomy mapping from HERALD manifest
    /// This extracts accession-to-taxid mappings directly from the manifest's chunk metadata
    pub fn get_taxonomy_mapping_from_manifest(
        &self,
        source: &DatabaseSource,
        progress_callback: Option<&dyn Fn(&str)>,
    ) -> Result<std::collections::HashMap<String, crate::TaxonId>> {
        // Check if this is a streaming database first
        let (source_name, dataset_name) = self.get_source_dataset_names(source);
        let database_name = format!("{}/{}", source_name, dataset_name);

        let manifest_lightweight = self.get_manifest_lightweight(&database_name)?;
        let is_streaming = manifest_lightweight.etag.starts_with("streaming-");

        if is_streaming {
            // Use streaming version to avoid OOM
            return self.get_taxonomy_mapping_from_partials_streaming(
                &source_name,
                &dataset_name,
                &manifest_lightweight.version,
                progress_callback
            );
        }

        // Non-streaming path (small databases)
        use std::collections::HashMap;

        let manifest = self.get_manifest(&database_name)?;

        let mut mapping = HashMap::new();
        let total_chunks = manifest.chunk_index.len();

        if let Some(cb) = progress_callback {
            cb(&format!("Processing {} chunks from manifest", total_chunks));
        }

        let mut chunks_with_taxids = 0;
        let mut chunks_without_taxids = 0;

        // For each chunk, we need to load its sequences to get the accessions
        // and map them to the chunk's TaxIDs
        for (idx, chunk_meta) in manifest.chunk_index.iter().enumerate() {
            if idx % 100 == 0 {
                if let Some(cb) = progress_callback {
                    cb(&format!("Processed {}/{} chunks", idx, total_chunks));
                }
            }

            if chunk_meta.taxon_ids.is_empty() {
                chunks_without_taxids += 1;
                continue; // Skip chunks without taxonomy
            }

            chunks_with_taxids += 1;

            // Load the chunk to get sequence headers
            let chunk_data = self.repository.storage.get_chunk(&chunk_meta.hash)?;

            // Parse sequences from chunk
            let sequences = talaria_bio::parse_fasta_from_bytes(&chunk_data)?;

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

        if let Some(cb) = progress_callback {
            cb(&format!(
                "Processed {} chunks ({} with taxonomy, {} without). Extracted {} mappings",
                total_chunks,
                chunks_with_taxids,
                chunks_without_taxids,
                mapping.len()
            ));
        }
        Ok(mapping)
    }

    /// Get taxonomy mapping from partials (streaming mode for massive databases)
    /// Processes batches in windows to avoid loading all chunks into memory
    fn get_taxonomy_mapping_from_partials_streaming(
        &self,
        source_name: &str,
        dataset_name: &str,
        version: &str,
        progress_callback: Option<&dyn Fn(&str)>,
    ) -> Result<std::collections::HashMap<String, crate::TaxonId>> {
        use std::collections::HashMap;
        use rayon::prelude::*;

        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let prefix = format!("partial:{}:{}:{}:", source_name, dataset_name, version);

        let mut mapping = HashMap::new();
        let mut processed_batches = 0;
        let mut consecutive_misses = 0;
        const MAX_BATCHES: usize = 100000;
        const MAX_CONSECUTIVE_MISSES: usize = 10;
        const BATCH_WINDOW: usize = 50;

        tracing::info!("Starting streaming taxonomy mapping extraction for {}/{} v{}", source_name, dataset_name, version);

        let mut current_batch = 0;

        while current_batch <= MAX_BATCHES {
            let window_start = current_batch;
            let window_end = (current_batch + BATCH_WINDOW).min(MAX_BATCHES + 1);

            // Collect chunk metadata from all batches in this window
            let mut window_chunks = Vec::new();
            let mut batches_in_window = 0;

            for batch_num in window_start..window_end {
                let key = format!("{}{:06}", prefix, batch_num);

                if let Some(data) = rocksdb.get_manifest(&key)? {
                    let partial: PartialManifest = bincode::deserialize(&data).map_err(|e| {
                        anyhow::anyhow!("Failed to deserialize partial manifest {}: {}", key, e)
                    })?;

                    for (chunk_manifest, chunk_hash) in partial.manifests {
                        if !chunk_manifest.taxon_ids.is_empty() {
                            window_chunks.push((chunk_hash, chunk_manifest.taxon_ids[0]));
                        }
                    }

                    batches_in_window += 1;
                    consecutive_misses = 0;
                } else {
                    consecutive_misses += 1;
                    if consecutive_misses >= MAX_CONSECUTIVE_MISSES {
                        break;
                    }
                }
            }

            if batches_in_window > 0 {
                // Process all chunks in this window in parallel
                let window_mappings: Vec<HashMap<String, crate::TaxonId>> = window_chunks
                    .par_iter()
                    .filter_map(|(chunk_hash, taxid)| {
                        let mut chunk_mapping = HashMap::new();

                        // Load chunk and extract accessions
                        if let Ok(chunk_data) = self.repository.storage.get_chunk(chunk_hash) {
                            if let Ok(sequences) = talaria_bio::parse_fasta_from_bytes(&chunk_data) {
                                for seq in sequences {
                                    if let Some(accession) = Self::extract_accession_from_header(&seq.id) {
                                        chunk_mapping.insert(accession.clone(), *taxid);

                                        // Also store without version suffix
                                        if let Some(dot_pos) = accession.rfind('.') {
                                            chunk_mapping.insert(accession[..dot_pos].to_string(), *taxid);
                                        }
                                    }
                                }
                            }
                        }

                        Some(chunk_mapping)
                    })
                    .collect();

                // Merge all window mappings into main mapping
                for chunk_mapping in window_mappings {
                    mapping.extend(chunk_mapping);
                }

                processed_batches += batches_in_window;

                if let Some(cb) = progress_callback {
                    cb(&format!(
                        "Processed {} batches, extracted {} mappings",
                        processed_batches,
                        mapping.len()
                    ));
                }

                tracing::info!(
                    "Taxonomy mapping progress: {} batches processed, {} mappings extracted",
                    processed_batches,
                    mapping.len()
                );
            }

            if consecutive_misses >= MAX_CONSECUTIVE_MISSES {
                break;
            }

            current_batch = window_end;
        }

        tracing::info!(
            "Streaming taxonomy mapping complete: {} batches processed, {} mappings extracted",
            processed_batches,
            mapping.len()
        );

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
                    && i + 1 < parts.len()
                {
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
        let mapping = self.get_taxonomy_mapping_from_manifest(source, None)?;

        // Create temporary file with .accession2taxid extension (required by LAMBDA)
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!(
            "talaria_manifest_{}.accession2taxid",
            std::process::id()
        ));

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

        tracing::info!(
            "Created temporary accession2taxid file with manifest data: {:?}",
            temp_file
        );
        Ok(temp_file)
    }

    /// Load taxonomy mapping for a database
    #[allow(dead_code)]
    fn load_taxonomy_mapping(
        &self,
        source: &DatabaseSource,
    ) -> Result<std::collections::HashMap<String, crate::TaxonId>> {
        use flate2::read::GzDecoder;
        use std::collections::HashMap;
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        // Load from unified taxonomy mappings directory
        let mappings_dir =
            talaria_core::system::paths::talaria_taxonomy_current_dir().join("mappings");
        let mapping_file = match source {
            DatabaseSource::UniProt(_) => mappings_dir.join("uniprot_idmapping.dat.gz"),
            DatabaseSource::NCBI(_) => mappings_dir.join("prot.accession2taxid.gz"),
            _ => return Ok(HashMap::new()),
        };

        if !mapping_file.exists() {
            return Ok(HashMap::new());
        }

        tracing::info!("● Loading taxonomy mapping from {}", mapping_file.display());
        let mut mappings = HashMap::new();

        let file = File::open(&mapping_file)?;
        let decoder = GzDecoder::new(file);
        let reader = BufReader::new(decoder);

        let mut line_count = 0;

        match source {
            DatabaseSource::UniProt(_) => {
                // UniProt idmapping format: accession<tab>type<tab>value
                // We're looking for: P12345<tab>NCBI_TaxID<tab>9606
                for line_result in reader.lines() {
                    let line = line_result?;
                    line_count += 1;

                    if line_count % 100000 == 0 {
                        tracing::info!("  Processed {} mappings", line_count);
                    }

                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 3 && parts[1] == "NCBI_TaxID" {
                        if let Ok(taxid) = parts[2].parse::<u32>() {
                            mappings.insert(parts[0].to_string(), crate::TaxonId(taxid));
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
                        tracing::info!("  Processed {} mappings", line_count);
                    }

                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 2 {
                        if let Ok(taxid) = parts[1].parse::<u32>() {
                            // Store both with and without version
                            let accession = parts[0].to_string();
                            mappings.insert(accession.clone(), crate::TaxonId(taxid));

                            // Also store without version suffix
                            if let Some(dot_pos) = accession.rfind('.') {
                                mappings.insert(
                                    accession[..dot_pos].to_string(),
                                    crate::TaxonId(taxid),
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        tracing::info!("✓ Loaded {} taxonomy mappings", mappings.len());
        Ok(mappings)
    }

    /// Ensure taxonomy is loaded, downloading if necessary
    #[allow(dead_code)]
    async fn ensure_taxonomy_loaded(&mut self, progress_callback: &impl Fn(&str)) -> Result<()> {
        let taxonomy_dir = talaria_core::system::paths::talaria_taxonomy_current_dir();
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
    pub fn get_reduction_profiles_for_database(&self, db_name: &str, version: &str) -> Result<Vec<String>> {
        // Parse the database name
        let parts: Vec<&str> = db_name.split('/').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!("Invalid database name format: {}", db_name));
        }

        let source = parts[0];
        let dataset = parts[1];

        // Get profiles from the version-specific directories
        self.repository
            .storage
            .list_database_reduction_profiles(source, dataset, version)
    }

    /// List all available databases in HERALD
    pub fn list_databases(&self) -> Result<Vec<DatabaseInfo>> {
        let _span = tracing::debug_span!("list_databases").entered();

        // Try to get from cache first
        if let Some(cache) = &self.cache {
            if let Some(mut cached_databases) = cache.get_database_list() {
                // Fix zero-size streaming manifests in cached data
                let mut updated = false;
                for db_info in &mut cached_databases {
                    if db_info.total_size == 0 && db_info.chunk_count > 0 {
                        // Recalculate size - use streaming-safe method to avoid OOM
                        let parts: Vec<&str> = db_info.name.split('/').collect();
                        if parts.len() == 2 {
                            if let Ok(total_size) = self.get_total_size_from_partials(parts[0], parts[1], &db_info.version) {
                                db_info.total_size = total_size;
                                debug!("Fixed zero-size cached metadata for {}: {} bytes (from partials)", db_info.name, db_info.total_size);
                                updated = true;
                            }
                        }
                    }
                }

                // Update cache if we fixed anything
                if updated {
                    let _ = cache.set_database_list(cached_databases.clone());
                }

                debug!(
                    "Returning cached database list ({} entries)",
                    cached_databases.len()
                );
                return Ok(cached_databases);
            }
        }

        debug!("Cache miss - querying RocksDB for database metadata...");
        let mut databases = Vec::new();

        // Get databases from RocksDB (the single source of truth)
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Step 1: Load lightweight metadata (fast path - avoids OOM)
        let metadata_list = rocksdb.list_database_metadata()?;
        debug!("Found {} database metadata entries", metadata_list.len());

        // Track which databases we've seen
        use std::collections::HashSet;
        let mut seen_databases: HashSet<String> = HashSet::new();

        // Load databases from metadata
        for (_key, data) in metadata_list {
            if let Ok(mut db_info) = bincode::deserialize::<DatabaseInfo>(&data) {
                // Fix zero-size streaming manifests (cached before fix was applied)
                if db_info.total_size == 0 && db_info.chunk_count > 0 {
                    // Recalculate size - use streaming-safe method to avoid OOM
                    let parts: Vec<&str> = db_info.name.split('/').collect();
                    if parts.len() == 2 {
                        let source_name = parts[0];
                        let dataset_name = parts[1];

                        // Use get_total_size_from_partials which doesn't load full chunk_index
                        if let Ok(total_size) = self.get_total_size_from_partials(source_name, dataset_name, &db_info.version) {
                            db_info.total_size = total_size;
                            debug!("Fixed zero-size metadata for {}: {} bytes (from partials)", db_info.name, db_info.total_size);

                            // Update the cached metadata with correct size
                            let metadata_serialized = bincode::serialize(&db_info).ok();
                            if let Some(data) = metadata_serialized {
                                let _ = rocksdb.put_database_metadata(source_name, dataset_name, &data);
                            }
                        }
                    }
                }

                // Populate reduction profiles for this specific version
                let reduction_profiles = self
                    .get_reduction_profiles_for_database(&db_info.name, &db_info.version)
                    .unwrap_or_default();
                db_info.reduction_profiles = reduction_profiles;

                seen_databases.insert(db_info.name.clone());
                databases.push(db_info);
            }
        }

        // Step 2: Check manifest keys for databases missing from metadata
        // This catches databases where metadata was deleted (e.g., after streaming import)
        let manifest_keys = rocksdb.list_manifest_keys_with_prefix("manifest:")?;
        debug!("Found {} manifest keys in RocksDB", manifest_keys.len());

        let mut missing_count = 0;
        for key in manifest_keys {
            // Parse key format: "manifest:{source}:{dataset}:{version}"
            let parts: Vec<&str> = key.split(':').collect();
            if parts.len() >= 4 && parts[0] == "manifest" {
                let source_name = parts[1].to_string();
                let dataset_name = parts[2].to_string();
                let db_name = format!("{}/{}", source_name, dataset_name);

                // Only process if not already in metadata
                if seen_databases.contains(&db_name) {
                    continue;
                }
                seen_databases.insert(db_name.clone());

                // Load manifest to get metadata
                if let Ok(Some(data)) = rocksdb.get_manifest(&key) {
                    if let Ok(manifest_data) =
                        bincode::deserialize::<crate::TemporalManifest>(&data)
                    {
                        missing_count += 1;
                        debug!("Found database {} with manifest but no metadata - regenerating", db_name);

                        // Get reduction profiles for this version
                        let reduction_profiles = self
                            .get_reduction_profiles_for_database(&db_name, &manifest_data.version)
                            .unwrap_or_default();

                        // Calculate counts - check if manifest has chunk_index or if we need to load from stored keys
                        let (chunk_count, sequence_count, total_size) = if !manifest_data.chunk_index.is_empty() {
                            // Non-streaming: calculate from chunk_index
                            (
                                manifest_data.chunk_index.len(),
                                manifest_data.chunk_index.iter().map(|c| c.sequence_count).sum(),
                                manifest_data.chunk_index.iter().map(|c| c.size).sum(),
                            )
                        } else {
                            // Streaming manifest: load from stored keys
                            let count_key = format!("manifest_count:{}:{}:{}", source_name, dataset_name, &manifest_data.version);
                            let chunk_count = if let Ok(Some(data)) = rocksdb.get_manifest(&count_key) {
                                if data.len() == 8 {
                                    usize::from_le_bytes(data.try_into().unwrap_or([0u8; 8]))
                                } else {
                                    0
                                }
                            } else {
                                0
                            };

                            let seq_count_key = format!("sequence_count:{}:{}:{}", source_name, dataset_name, &manifest_data.version);
                            let sequence_count = if let Ok(Some(data)) = rocksdb.get_manifest(&seq_count_key) {
                                if data.len() == 8 {
                                    usize::from_le_bytes(data.try_into().unwrap_or([0u8; 8]))
                                } else {
                                    0
                                }
                            } else {
                                0
                            };

                            // For streaming manifests, calculate total_size from partials
                            // Use optimized method that only sums sizes without loading full metadata
                            let total_size = self.get_total_size_from_partials(&source_name, &dataset_name, &manifest_data.version)
                                .unwrap_or_else(|_| {
                                    // Fallback: use chunk count * average estimate
                                    // This happens if partials aren't available yet
                                    chunk_count * 1000
                                });

                            (chunk_count, sequence_count, total_size)
                        };

                        let db_info = DatabaseInfo {
                            name: db_name.clone(),
                            version: manifest_data.version.clone(),
                            created_at: manifest_data.created_at,
                            chunk_count,
                            sequence_count,
                            total_size,
                            reduction_profiles,
                        };

                        databases.push(db_info.clone());

                        // Save metadata for next time
                        let metadata_serialized = bincode::serialize(&db_info)?;
                        rocksdb
                            .put_database_metadata(
                                &source_name,
                                &dataset_name,
                                &metadata_serialized,
                            )
                            .ok();
                        debug!(
                            "Regenerated and saved database metadata for future fast listing: {}/{}",
                            source_name, dataset_name
                        );
                    }
                }
            }
        }

        if missing_count > 0 {
            debug!("Regenerated metadata for {} databases", missing_count);
        }

        // Update cache
        if let Some(cache) = &self.cache {
            let _ = cache.set_database_list(databases.clone());
        }

        Ok(databases)
    }

    /// Rebuild database metadata entries from existing manifests in RocksDB
    /// This repairs missing db_meta:* entries that prevent databases from showing in list
    pub fn rebuild_database_metadata(&mut self) -> Result<usize> {
        use std::collections::HashSet;

        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let manifest_keys = rocksdb.list_manifest_keys_with_prefix("manifest:")?;

        tracing::info!("Found {} manifest keys, rebuilding metadata...", manifest_keys.len());

        let mut seen_databases: HashSet<(String, String)> = HashSet::new();
        let mut rebuilt_count = 0;

        for key in manifest_keys {
            // Parse key format: "manifest:{source}:{dataset}:{version}"
            let parts: Vec<&str> = key.split(':').collect();
            if parts.len() >= 4 && parts[0] == "manifest" {
                let source_name = parts[1].to_string();
                let dataset_name = parts[2].to_string();

                // Only process each database once
                if seen_databases.contains(&(source_name.clone(), dataset_name.clone())) {
                    continue;
                }
                seen_databases.insert((source_name.clone(), dataset_name.clone()));

                // Load manifest to get counts and metadata
                if let Ok(Some(data)) = rocksdb.get_manifest(&key) {
                    if let Ok(manifest_data) = bincode::deserialize::<crate::TemporalManifest>(&data) {
                        let db_name = format!("{}/{}", source_name, dataset_name);

                        // Calculate counts from manifest
                        let (chunk_count, sequence_count, total_size) = if !manifest_data.chunk_index.is_empty() {
                            // Non-streaming: calculate from chunk_index
                            (
                                manifest_data.chunk_index.len(),
                                manifest_data.chunk_index.iter().map(|c| c.sequence_count).sum(),
                                manifest_data.chunk_index.iter().map(|c| c.size).sum(),
                            )
                        } else {
                            // Streaming manifest: load from stored keys
                            let count_key = format!("manifest_count:{}:{}:{}", source_name, dataset_name, &manifest_data.version);
                            let chunk_count = if let Ok(Some(data)) = rocksdb.get_manifest(&count_key) {
                                if data.len() == 8 {
                                    usize::from_le_bytes(data.try_into().unwrap_or([0u8; 8]))
                                } else {
                                    0
                                }
                            } else {
                                0
                            };

                            let seq_count_key = format!("sequence_count:{}:{}:{}", source_name, dataset_name, &manifest_data.version);
                            let sequence_count = if let Ok(Some(data)) = rocksdb.get_manifest(&seq_count_key) {
                                if data.len() == 8 {
                                    usize::from_le_bytes(data.try_into().unwrap_or([0u8; 8]))
                                } else {
                                    0
                                }
                            } else {
                                0
                            };

                            (chunk_count, sequence_count, 0)
                        };

                        // Create and save metadata
                        let db_metadata = DatabaseInfo {
                            name: db_name.clone(),
                            version: manifest_data.version.clone(),
                            created_at: manifest_data.created_at,
                            chunk_count,
                            sequence_count,
                            total_size,
                            reduction_profiles: Vec::new(), // Will be populated by list_databases
                        };

                        let metadata_serialized = bincode::serialize(&db_metadata)?;
                        rocksdb.put_database_metadata(&source_name, &dataset_name, &metadata_serialized)?;

                        tracing::debug!("Rebuilt metadata for {}/{}", source_name, dataset_name);
                        rebuilt_count += 1;
                    }
                }
            }
        }

        tracing::info!("Rebuilt metadata for {} databases", rebuilt_count);
        Ok(rebuilt_count)
    }

    /// Save manifest to RocksDB repository (shared by add and download)
    /// This is the ONLY place manifest saving should happen to prevent divergence
    pub fn save_manifest_to_repository(
        &self,
        source_name: &str,
        dataset_name: &str,
        version: &str,
        manifest: &TemporalManifest,
        chunk_count: usize,
        sequence_count: usize,
        total_size: usize,
    ) -> Result<()> {
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Save manifest to RocksDB (single source of truth)
        let manifest_key = format!("manifest:{}:{}:{}", source_name, dataset_name, version);
        let manifest_serialized = bincode::serialize(manifest)?;
        rocksdb.put_manifest(&manifest_key, &manifest_serialized)?;
        debug!("Saved manifest to RocksDB with key: {}", manifest_key);

        // Save lightweight database metadata for fast listing
        let db_metadata = DatabaseInfo {
            name: format!("{}/{}", source_name, dataset_name),
            version: version.to_string(),
            created_at: manifest.created_at,
            chunk_count,
            sequence_count,
            total_size,
            reduction_profiles: Vec::new(),
        };
        let metadata_serialized = bincode::serialize(&db_metadata)?;
        rocksdb.put_database_metadata(source_name, dataset_name, &metadata_serialized)?;
        debug!("Saved database metadata for fast listing: {}/{}", source_name, dataset_name);

        // Update 'current' alias
        let current_alias_key = format!("alias:{}:{}:current", source_name, dataset_name);
        rocksdb.put_manifest(&current_alias_key, version.as_bytes())?;
        debug!("Set current alias: {} -> {}", current_alias_key, version);

        Ok(())
    }

    /// Update database metadata cache with a new reduction profile
    /// This ensures the profile shows up in database list/info immediately
    pub fn add_reduction_profile_to_metadata(
        &self,
        source_name: &str,
        dataset_name: &str,
        profile_name: &str,
    ) -> Result<()> {
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Try to load existing metadata
        if let Some(metadata_bytes) = rocksdb.get_database_metadata(source_name, dataset_name)? {
            let mut db_info: DatabaseInfo = bincode::deserialize(&metadata_bytes)?;

            // Add profile if not already present
            if !db_info.reduction_profiles.contains(&profile_name.to_string()) {
                db_info.reduction_profiles.push(profile_name.to_string());
                db_info.reduction_profiles.sort(); // Keep sorted for consistent output

                // Save updated metadata
                let metadata_serialized = bincode::serialize(&db_info)?;
                rocksdb.put_database_metadata(source_name, dataset_name, &metadata_serialized)?;
                debug!(
                    "Added reduction profile '{}' to metadata cache for {}/{}",
                    profile_name, source_name, dataset_name
                );
            }
        } else {
            debug!(
                "No metadata found for {}/{} - will be populated on next list operation",
                source_name, dataset_name
            );
        }

        Ok(())
    }

    /// Initialize temporal tracking for existing data
    pub fn init_temporal_for_existing(&mut self) -> Result<()> {
        let temporal_path = self.base_path.clone();
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let mut temporal_index = crate::TemporalIndex::load(&temporal_path, rocksdb)?;

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
                if let Ok(manifest) = serde_json::from_str::<crate::TemporalManifest>(&content) {
                    // Add initial version to temporal index
                    temporal_index.add_sequence_version(
                        manifest.version.clone(),
                        manifest.sequence_root.clone(),
                        manifest.chunk_index.len(),
                        manifest.chunk_index.iter().map(|c| c.sequence_count).sum(),
                    )?;

                    // Save the temporal index
                    temporal_index.save()?;
                    tracing::info!("Initialized temporal tracking for existing database");
                }
            }
        }

        Ok(())
    }

    /// Calculate deduplication ratio from RocksDB manifests
    ///
    /// This computes how many times chunks are referenced across all databases.
    /// A ratio of 1.0 means no deduplication (each chunk used once).
    /// A ratio > 1.0 means chunks are shared across databases.
    ///
    /// Example: If 3 databases share 100 unique chunks, and each database
    /// references 80 of those chunks, total references = 240, unique = 100,
    /// ratio = 2.4x (each chunk referenced 2.4 times on average).
    fn calculate_deduplication_ratio(&self) -> Result<f32> {
        use std::collections::HashMap;

        // Get all manifests from RocksDB
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let manifests = rocksdb.list_manifests()?;

        if manifests.is_empty() {
            return Ok(1.0); // No data = no deduplication
        }

        // Track how many times each chunk hash is referenced
        let mut chunk_reference_counts: HashMap<crate::SHA256Hash, usize> = HashMap::new();

        for (_key, data) in manifests {
            // Deserialize manifest
            if let Ok(manifest_data) = bincode::deserialize::<crate::TemporalManifest>(&data) {
                // Count chunk references from this manifest
                for chunk_metadata in &manifest_data.chunk_index {
                    *chunk_reference_counts
                        .entry(chunk_metadata.hash.clone())
                        .or_insert(0) += 1;
                }
            }
        }

        if chunk_reference_counts.is_empty() {
            return Ok(1.0);
        }

        // Calculate deduplication ratio
        let total_references: usize = chunk_reference_counts.values().sum();
        let unique_chunks = chunk_reference_counts.len();

        let ratio = total_references as f32 / unique_chunks as f32;

        debug!(
            "Deduplication ratio: {:.2}x ({} total references / {} unique chunks)",
            ratio, total_references, unique_chunks
        );

        Ok(ratio)
    }

    /// Get statistics for the HERALD repository
    pub fn get_stats(&self) -> Result<HeraldStats> {
        // Try to get from cache first
        if let Some(cache) = &self.cache {
            if let Some(cached_stats) = cache.get_stats() {
                debug!("Returning cached stats");
                return Ok(cached_stats);
            }
        }

        debug!("Cache miss - computing repository stats");
        let storage_stats = self.repository.storage.get_stats();
        let databases = self.list_databases()?;

        // Calculate total chunks from all databases since storage backend doesn't track chunks
        let total_chunks: usize = databases.iter().map(|db| db.chunk_count).sum();

        // Calculate total size from all databases for accuracy
        let total_size: usize = databases.iter().map(|db| db.total_size).sum();

        // Calculate actual deduplication ratio from RocksDB data
        let deduplication_ratio = self.calculate_deduplication_ratio().unwrap_or(1.0);

        let stats = HeraldStats {
            total_chunks,
            total_size,
            compressed_chunks: storage_stats.compressed_chunks,
            deduplication_ratio,
            database_count: databases.len(),
            databases,
        };

        // Update cache
        if let Some(cache) = &self.cache {
            let _ = cache.set_stats(stats.clone());
        }

        Ok(stats)
    }

    /// List all resumable operations
    pub fn list_resumable_operations(&self) -> Result<Vec<(String, crate::ProcessingState)>> {
        self.repository.storage.list_resumable_operations()
    }

    /// Clean up expired processing states
    ///
    /// This method performs periodic maintenance of the HERALD storage
    /// to remove old/expired processing states and free up disk space.
    pub fn cleanup_expired_states(&self) -> Result<usize> {
        self.repository.storage.cleanup_expired_states()
    }

    /// Schedule automatic cleanup of expired states
    pub fn schedule_cleanup(&self, interval_hours: u64) -> Result<()> {
        use std::thread;
        use std::time::Duration;

        let storage = self.repository.storage.clone();
        let interval = Duration::from_secs(interval_hours * 3600);

        thread::spawn(move || {
            loop {
                thread::sleep(interval);

                // Perform cleanup
                match storage.cleanup_expired_states() {
                    Ok(removed) if removed > 0 => {
                        tracing::info!("Cleaned up {} expired processing states", removed);
                    }
                    Ok(_) => {
                        tracing::debug!("No expired states to clean up");
                    }
                    Err(e) => {
                        tracing::error!("Failed to clean up expired states: {}", e);
                    }
                }

                // Also perform garbage collection on old chunks
                match storage.garbage_collect_deltas() {
                    Ok(stats) if stats.chunks_deleted > 0 => {
                        tracing::info!(
                            "Garbage collected {} chunks, freed {} bytes",
                            stats.chunks_deleted,
                            stats.bytes_freed
                        );
                    }
                    Ok(_) => {
                        tracing::debug!("No chunks to garbage collect");
                    }
                    Err(e) => {
                        tracing::error!("Failed to garbage collect: {}", e);
                    }
                }
            }
        });

        Ok(())
    }

    /// Get access to the underlying storage
    pub fn get_storage(&self) -> &crate::HeraldStorage {
        &self.repository.storage
    }

    /// Download taxonomy if needed
    async fn download_taxonomy_if_needed(&mut self) -> Result<bool> {
        use talaria_core::{DatabaseSource, NCBIDatabase};

        let source = DatabaseSource::NCBI(NCBIDatabase::Taxonomy);
        let progress = |msg: &str| {
            tracing::info!("{}", msg);
        };

        // Check if taxonomy exists
        let taxonomy_dir = talaria_core::system::paths::talaria_taxonomy_current_dir();
        if taxonomy_dir.exists() && taxonomy_dir.join("tree/nodes.dmp").exists() {
            // Check for updates
            match self.check_for_updates(&source, progress).await {
                Ok(DownloadResult::UpToDate) => Ok(false),
                Ok(DownloadResult::Updated { .. }) => {
                    // Download the update
                    self.download_with_resume(&source, false, progress).await?;
                    Ok(true)
                }
                _ => {
                    // Need initial download
                    self.download_with_resume(&source, false, progress).await?;
                    Ok(true)
                }
            }
        } else {
            // Download taxonomy for the first time
            self.download_with_resume(&source, false, progress).await?;
            Ok(true)
        }
    }

    /// Check for taxonomy updates and download if available
    pub async fn update_taxonomy(&mut self) -> Result<TaxonomyUpdateResult> {
        let taxonomy_dir = talaria_core::system::paths::talaria_taxonomy_current_dir();
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
        let last_modified = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Check if we need to update
        let needs_update = match (&current_version, &last_modified) {
            (Some(current), Some(latest)) => current != latest,
            (None, Some(_)) => true, // No current version, need to download
            _ => false,              // Can't determine, assume no update needed
        };

        if !needs_update {
            return Ok(TaxonomyUpdateResult::UpToDate);
        }

        // Download new taxonomy
        tracing::info!("Downloading updated NCBI taxonomy...");
        let response = client.get(taxdump_url).send().await?;
        let bytes = response.bytes().await?;

        // Generate new version timestamp
        let new_version = talaria_core::system::paths::generate_utc_timestamp();

        // Create a new version directory for the updated taxonomy
        if taxdump_dir.exists() {
            let new_version_dir =
                talaria_core::system::paths::talaria_taxonomy_version_dir(&new_version);
            std::fs::create_dir_all(&new_version_dir)?;

            // Copy existing data to new version
            let _ = std::fs::create_dir_all(new_version_dir.join("tree"));

            // Update current symlink to point to new version
            let current_link =
                talaria_core::system::paths::talaria_taxonomy_versions_dir().join("current");
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

        // Save version information to RocksDB
        let version_date = last_modified
            .clone()
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        let version_data = serde_json::json!({
            "date": &version_date,
            "source": "NCBI",
            "updated_at": chrono::Utc::now().to_rfc3339()
        });

        // Store in RocksDB
        let taxonomy_version_key = format!("taxonomy:version:{}", new_version);
        let version_serialized = bincode::serialize(&version_data)?;
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        rocksdb.put_manifest(&taxonomy_version_key, &version_serialized)?;

        // Reload taxonomy in repository
        self.repository.taxonomy.load_ncbi_taxonomy(&taxdump_dir)?;

        Ok(TaxonomyUpdateResult::Updated {
            nodes_updated: true,
            names_updated: true,
            merged_updated: false,
            deleted_updated: false,
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

    /// Assemble a FASTA file from HERALD for a specific database
    pub fn assemble_database(&self, source: &DatabaseSource, output_path: &Path) -> Result<()> {
        // Load manifest for this database from RocksDB
        let (source_name, dataset_name) = self.get_source_dataset_names(source);
        let database_name = format!("{}/{}", source_name, dataset_name);

        let manifest = self.get_manifest(&database_name)?;

        // Get all chunk hashes
        let chunk_hashes: Vec<_> = manifest
            .chunk_index
            .iter()
            .map(|c| c.hash.clone())
            .collect();

        // Assemble to output file
        let assembler = crate::FastaAssembler::new(&self.repository.storage);

        // Use scope to ensure file is properly closed and flushed
        let sequence_count = {
            use std::io::Write;
            let mut output_file = std::fs::File::create(output_path)?;
            let count = assembler.stream_assembly(&chunk_hashes, &mut output_file)?;
            // Explicitly flush before closing
            output_file.flush()?;
            count
        }; // File handle dropped and closed here

        tracing::info!(
            "Assembled {} sequences to {}",
            sequence_count,
            output_path.display()
        );

        Ok(())
    }

    /// Assemble database from partials in streaming mode (memory-efficient for massive databases)
    ///
    /// This method assembles a database by iterating through partial manifests one batch at a time,
    /// streaming sequences directly to the output file without loading all chunk metadata into memory.
    /// Essential for massive databases like UniRef100 with 36M+ chunks that would cause OOM.
    ///
    /// The progress_callback is called after each batch with (batches_processed, total_sequences_so_far)
    pub fn assemble_from_partials_streaming(
        &self,
        source_name: &str,
        dataset_name: &str,
        version: &str,
        output_path: &Path,
        progress_callback: Option<Box<dyn Fn(usize, usize) + Send>>,
    ) -> Result<usize> {
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let prefix = format!("partial:{}:{}:{}:", source_name, dataset_name, version);

        let assembler = crate::FastaAssembler::new(&self.repository.storage);
        let mut output_file = std::fs::File::create(output_path)?;
        let mut total_sequences = 0usize;
        let mut processed_batches = 0;
        const MAX_BATCHES: usize = 100000;
        const MAX_CONSECUTIVE_MISSES: usize = 10;
        const BATCH_WINDOW: usize = 10; // Process 10 batches at once (reduced for memory safety)

        tracing::info!("Starting streaming assembly from partials for {}/{} v{} (windowed batching with memory safety)", source_name, dataset_name, version);

        // Process batches in windows for better parallelism while avoiding OOM
        // Window of 10 batches (~18,700 chunks) processed in sub-batches of 1000 by assembler
        let mut current_batch = 0;
        let mut consecutive_misses = 0;

        while current_batch <= MAX_BATCHES {
            // Determine window boundaries
            let window_start = current_batch;
            let window_end = (current_batch + BATCH_WINDOW).min(MAX_BATCHES + 1);

            // Collect chunk hashes from all batches in this window
            let mut window_chunk_hashes = Vec::new();
            let mut batches_in_window = 0;

            for batch_num in window_start..window_end {
                let key = format!("{}{:06}", prefix, batch_num);

                if let Some(data) = rocksdb.get_manifest(&key)? {
                    let partial: PartialManifest = bincode::deserialize(&data).map_err(|e| {
                        anyhow::anyhow!("Failed to deserialize partial manifest {}: {}", key, e)
                    })?;

                    // Extract chunk hashes from this partial
                    for (_, hash) in partial.manifests {
                        window_chunk_hashes.push(hash);
                    }

                    batches_in_window += 1;
                    consecutive_misses = 0;
                } else {
                    consecutive_misses += 1;
                    if consecutive_misses >= MAX_CONSECUTIVE_MISSES {
                        tracing::debug!("Reached end of partials at batch {}", batch_num);
                        break;
                    }
                }
            }

            // If we found batches in this window, process them all at once
            if batches_in_window > 0 {
                tracing::debug!(
                    "Processing window {}-{}: {} batches, {} chunks",
                    window_start,
                    window_start + batches_in_window - 1,
                    batches_in_window,
                    window_chunk_hashes.len()
                );

                // Stream sequences from ALL chunks in this window in parallel
                let window_sequences = assembler.stream_assembly(&window_chunk_hashes, &mut output_file)?;
                total_sequences += window_sequences;
                processed_batches += batches_in_window;

                // Call progress callback after each window
                if let Some(ref callback) = progress_callback {
                    callback(processed_batches, total_sequences);
                }

                // Log progress every few windows
                tracing::info!(
                    "Progress: {} batches processed, {} sequences assembled",
                    processed_batches,
                    total_sequences
                );
            }

            // Exit if we've hit too many consecutive misses
            if consecutive_misses >= MAX_CONSECUTIVE_MISSES {
                break;
            }

            // Move to next window
            current_batch = window_end;
        }

        // Ensure all data is written
        use std::io::Write;
        output_file.flush()?;
        drop(output_file);

        tracing::info!(
            "Streaming assembly complete: {} sequences from {} batches written to {}",
            total_sequences,
            processed_batches,
            output_path.display()
        );

        Ok(total_sequences)
    }

    /// Assemble a taxonomic subset
    ///
    /// Extracts all sequences belonging to a specific taxon and writes them to a FASTA file.
    /// Used by the `talaria extract --taxon` command to allow users to extract
    /// specific taxonomic groups from the database.
    pub fn extract_taxon(
        &self,
        taxon: &str,
        output_path: &Path,
        include_descendants: bool,
    ) -> Result<usize> {
        // Parse taxon (could be name or TaxID)
        let taxon_id = if let Ok(id) = taxon.parse::<u32>() {
            crate::TaxonId(id)
        } else {
            // Look up taxon by name
            let taxonomy_path = talaria_core::system::paths::talaria_taxonomy_current_dir();
            let names_path = taxonomy_path.join("tree/names.dmp");
            if names_path.exists() {
                // Search for taxon name in taxonomy
                use std::io::{BufRead, BufReader};
                let file = std::fs::File::open(&names_path)?;
                let reader = BufReader::new(file);
                let mut found_id = None;

                for line in reader.lines() {
                    let line = line?;
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() > 2 && parts[2].contains(taxon) {
                        if let Ok(id) = parts[0].parse::<u32>() {
                            found_id = Some(crate::TaxonId(id));
                            break;
                        }
                    }
                }

                found_id
                    .ok_or_else(|| anyhow::anyhow!("Taxon '{}' not found in taxonomy", taxon))?
            } else {
                return Err(anyhow::anyhow!("Taxonomy database not installed"));
            }
        };

        // Get sequences for this taxon (and optionally descendants)
        let sequences = if include_descendants {
            self.repository.extract_taxon_with_descendants(taxon_id)?
        } else {
            self.repository.extract_taxon_exact(taxon_id)?
        };

        // Write to FASTA
        use std::io::Write;
        let mut output = std::fs::File::create(output_path)?;
        let count = sequences.len();

        for seq in sequences {
            write!(output, ">{}", seq.id)?;
            if let Some(desc) = seq.description {
                write!(output, " {}", desc)?;
            }
            writeln!(output)?;
            writeln!(output, "{}", String::from_utf8_lossy(&seq.sequence))?;
        }

        Ok(count)
    }

    /// Read sequences from a FASTA file (uses shared parser with .gz support)
    fn read_fasta_sequences(&self, path: &Path, progress_callback: Option<&dyn Fn(&str)>) -> Result<Vec<Sequence>> {
        // Use the shared FASTA parser from talaria-bio which handles .gz files automatically
        if let Some(cb) = progress_callback {
            let file_size = path.metadata()?.len();
            cb(&format!("Reading FASTA file ({} bytes)", file_size));

            // Parse using shared parser (handles .gz via extension detection)
            let sequences = talaria_bio::formats::fasta::parse_fasta(path)
                .map_err(|e| anyhow::anyhow!("Failed to parse FASTA: {}", e))?;

            cb(&format!("Read {} sequences", sequences.len()));

            Ok(sequences)
        } else {
            // Quiet mode - just parse without progress
            talaria_bio::formats::fasta::parse_fasta(path)
                .map_err(|e| anyhow::anyhow!("Failed to parse FASTA: {}", e))
        }
    }

    /// Stream-process FASTA file with true parallel pipeline
    fn chunk_database_streaming(
        &mut self,
        file_path: &Path,
        source: &DatabaseSource,
        download_state: &mut DownloadState,
        workspace_path: Option<&Path>,
        progress_callback: Option<&dyn Fn(&str)>,
    ) -> Result<()> {
        use flate2::read::GzDecoder;
        use indicatif::{MultiProgress, ProgressStyle};
        use std::fs::File;
        use std::io::{BufRead, BufReader};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{mpsc, Arc, Mutex};
        use std::thread;
        use talaria_bio::sequence::Sequence;
        use talaria_utils::display::{format_bytes, format_number};

        let file = File::open(file_path)?;
        let file_size = file.metadata()?.len();

        // Detect .gz files and wrap with decompressor
        let is_compressed = file_path.extension().and_then(|s| s.to_str()) == Some("gz");
        let reader: Box<dyn BufRead> = if is_compressed {
            Box::new(BufReader::new(GzDecoder::new(file)))
        } else {
            Box::new(BufReader::new(file))
        };

        let msg = format!(
            "Processing {} file in streaming mode...",
            format_bytes(file_size)
        );
        if let Some(cb) = progress_callback {
            cb(&msg);
        } else {
            tracing::info!("{}", msg);
        }

        // Get storage reference first - we need it for version lookup
        let sequence_storage = Arc::clone(&self.get_repository().storage.sequence_storage);

        // Create version once at start - but check for existing partials first
        let version = if download_state.sequences_processed > 0 {
            // Resuming - try to find existing version from partial manifests
            let (source_name, dataset_name) = match source {
                DatabaseSource::UniProt(db) => ("uniprot", format!("{:?}", db).to_lowercase()),
                DatabaseSource::NCBI(db) => ("ncbi", format!("{:?}", db).to_lowercase()),
                DatabaseSource::Custom(name) => ("custom", name.clone()),
            };

            // Look for any existing partial manifests to get the version
            let prefix = format!("partial:{}:{}:", source_name, dataset_name);
            let existing_keys = sequence_storage.get_rocksdb().list_manifest_keys_with_prefix(&prefix).unwrap_or_default();

            if !existing_keys.is_empty() {
                // Extract version from first key: partial:uniprot:uniref90:VERSION:000000
                let first_key = &existing_keys[0];
                let parts: Vec<&str> = first_key.split(':').collect();
                if parts.len() >= 4 {
                    let existing_version = parts[3].to_string();
                    tracing::info!("Resuming with existing version: {}", existing_version);
                    self.current_version = Some(existing_version.clone());
                    existing_version
                } else {
                    // Fallback to new version
                    let new_version = talaria_core::system::paths::generate_utc_timestamp();
                    self.current_version = Some(new_version.clone());
                    new_version
                }
            } else {
                // No existing partials, create new version
                let new_version = talaria_core::system::paths::generate_utc_timestamp();
                self.current_version = Some(new_version.clone());
                new_version
            }
        } else {
            // Fresh start - create new version
            if self.current_version.is_none() {
                self.current_version = Some(talaria_core::system::paths::generate_utc_timestamp());
            }
            self.current_version.as_ref().unwrap().clone()
        };

        // Enable streaming mode to prevent memory accumulation in indices
        sequence_storage.set_streaming_mode(true);

        // Check if quiet mode is enabled (CLI sets TALARIA_LOG=error when --quiet is used)
        let quiet_mode = std::env::var("TALARIA_LOG")
            .map(|v| v == "error")
            .unwrap_or(false);

        // Create multi-progress (hidden if quiet mode)
        use indicatif::ProgressDrawTarget;
        let multi_progress = if quiet_mode {
            Arc::new(MultiProgress::with_draw_target(ProgressDrawTarget::hidden()))
        } else {
            Arc::new(MultiProgress::new())
        };

        // Reading progress: spinner for compressed, progress bar for uncompressed
        let reading_progress = if is_compressed {
            // Spinner for compressed files (can't predict decompressed size)
            let spinner = Arc::new(multi_progress.add(ProgressBar::new_spinner()));
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .template("Reading FASTA {spinner:.cyan} ({bytes} read)")
                    .unwrap()
                    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
            );
            spinner.enable_steady_tick(std::time::Duration::from_millis(80));
            spinner
        } else {
            // Progress bar for uncompressed files (known size)
            let bar = Arc::new(multi_progress.add(ProgressBar::new(file_size)));
            bar.set_style(
                ProgressStyle::default_bar()
                    .template(
                        "Reading FASTA [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta_precise})",
                    )
                    .unwrap()
                    .progress_chars("━━─"),
            );
            bar
        };

        // Processing progress bar (sequences)
        // Start with 0 length - will update dynamically as we discover the true count
        let processing_progress = Arc::new(multi_progress.add(ProgressBar::new(0)));
        processing_progress.set_style(
            ProgressStyle::default_bar()
                .template("   Processing [{bar:40.cyan/blue}] {human_pos}/{human_len} sequences [{elapsed_precise}]")
                .unwrap()
                .progress_chars("━━─")
        );
        processing_progress.enable_steady_tick(std::time::Duration::from_millis(100));

        // Chunking progress bar
        let chunking_progress = Arc::new(multi_progress.add(ProgressBar::new(0)));
        chunking_progress.set_style(
            ProgressStyle::default_bar()
                .template("     Chunking [{bar:40.cyan/blue}] {human_pos}/{human_len} sequences [{elapsed_precise}]")
                .unwrap()
                .progress_chars("━━─"),
        );
        chunking_progress.enable_steady_tick(std::time::Duration::from_millis(100));

        // Atomic counters for thread-safe tracking
        // When resuming, start counters from saved position
        let resume_sequences = download_state.sequences_processed;
        let resume_batches = resume_sequences / BATCH_SIZE;  // Calculate which batch we were on

        let total_sequences = Arc::new(AtomicUsize::new(resume_sequences));
        let batch_counter = Arc::new(AtomicUsize::new(resume_batches));
        let processed_counter = Arc::new(AtomicUsize::new(resume_batches)); // Start batch numbering from resume point
        let sequences_processed = Arc::new(AtomicUsize::new(resume_sequences));

        // Channel for sending batches from reader to workers
        let (batch_sender, batch_receiver) = mpsc::sync_channel::<Vec<Sequence>>(8); // Buffer up to 8 batches
        let batch_receiver = Arc::new(Mutex::new(batch_receiver));

        // Channel for collecting results
        let (result_sender, result_receiver) =
            mpsc::channel::<(usize, Vec<(crate::ChunkManifest, crate::SHA256Hash)>)>();

        // Clone things for the worker threads
        let num_workers = num_cpus::get();
        let storage_for_workers = Arc::clone(&sequence_storage);
        let source_for_workers = source.clone();
        let _version_for_workers = version.clone();
        let _base_path_for_workers = self.base_path.clone();
        let processed_for_workers = Arc::clone(&processed_counter);
        let total_for_progress = Arc::clone(&total_sequences);
        let sequences_processed_for_workers = Arc::clone(&sequences_processed);
        let processing_progress_for_workers = Arc::clone(&processing_progress);
        let chunking_progress_for_workers = Arc::clone(&chunking_progress);

        // Spawn worker threads pool
        let mut workers = vec![];
        for _worker_id in 0..num_workers {
            let receiver = Arc::clone(&batch_receiver);
            let storage = Arc::clone(&storage_for_workers);
            let source = source_for_workers.clone();
            let result_tx = result_sender.clone();
            let processed = Arc::clone(&processed_for_workers);
            let total_seq = Arc::clone(&total_for_progress);
            let seq_processed = Arc::clone(&sequences_processed_for_workers);
            let proc_progress = Arc::clone(&processing_progress_for_workers);
            let chunk_progress = Arc::clone(&chunking_progress_for_workers);

            let worker = thread::spawn(move || {
                // Each worker gets its own chunker
                let mut chunker =
                    TaxonomicChunker::new(ChunkingStrategy::default(), storage, source.clone());
                chunker.set_quiet_mode(true); // Quiet for parallel processing

                loop {
                    // Get next batch to process
                    let batch = {
                        let rx = receiver.lock().unwrap();
                        match rx.recv() {
                            Ok(batch) => batch,
                            Err(_) => break, // Channel closed, we're done
                        }
                    };

                    let batch_size = batch.len();
                    let batch_num = processed.fetch_add(1, Ordering::Relaxed) + 1;

                    // Update processing progress
                    proc_progress.inc(batch_size as u64);

                    // Update progress bar length to match actual total discovered so far
                    let current_total = total_seq.load(Ordering::Relaxed);
                    if proc_progress.length().unwrap_or(0) < current_total as u64 {
                        proc_progress.set_length(current_total as u64);
                        chunk_progress.set_length(current_total as u64);
                    }

                    // Process this batch with proper error handling
                    let manifests = match chunker.chunk_sequences_canonical_quiet_final(batch, false) {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::error!(" in worker thread: Failed to chunk batch {}: {}", batch_num, e);
                            tracing::error!("Worker is exiting gracefully. This will cause the download to fail.");
                            return; // Exit worker thread gracefully
                        }
                    };

                    // Update chunking progress
                    let processed_count =
                        seq_processed.fetch_add(batch_size, Ordering::Relaxed) + batch_size;
                    chunk_progress.set_position(processed_count as u64);

                    // Store manifests with error handling
                    let manifests_with_hashes: Vec<_> = manifests
                        .into_iter()
                        .filter_map(|manifest| {
                            match rmp_serde::to_vec(&manifest) {
                                Ok(manifest_data) => {
                                    let hash = SHA256Hash::compute(&manifest_data);
                                    Some((manifest, hash))
                                },
                                Err(e) => {
                                    tracing::error!(": Failed to serialize manifest in batch {}: {}", batch_num, e);
                                    None
                                }
                            }
                        })
                        .collect();

                    // Send results back (with error logging if send fails)
                    if !manifests_with_hashes.is_empty() {
                        if let Err(e) = result_tx.send((batch_num, manifests_with_hashes)) {
                            tracing::error!(": Failed to send results for batch {}: {}", batch_num, e);
                            return; // Exit worker if we can't communicate
                        }
                    }
                }
            });
            workers.push(worker);
        }
        drop(result_sender); // Close original sender

        // Clone for results collector thread
        let rocksdb_for_collector = sequence_storage.get_rocksdb();
        let chunk_storage_for_collector = self.repository.storage.chunk_storage();
        let source_for_collector = source.clone();
        let version_for_collector = version.clone();

        // Spawn results collector thread that saves partials to RocksDB immediately
        // In streaming mode, this also stores chunk manifests directly to RocksDB
        let collector = thread::spawn(move || -> Result<(usize, usize)> {
            let mut batch_count = 0usize;
            let mut total_chunks = 0usize;

            while let Ok((batch_num, manifests_with_hashes)) = result_receiver.recv() {
                // Save partial manifest to RocksDB immediately
                if !manifests_with_hashes.is_empty() {
                    total_chunks += manifests_with_hashes.len();

                    // Save partial manifest directly to RocksDB
                    // Passing chunk_storage enables streaming mode (stores manifests immediately)
                    // Propagate errors instead of just logging them
                    Self::save_partial_manifest_static(
                        &rocksdb_for_collector,
                        Some(&chunk_storage_for_collector),
                        batch_num,
                        manifests_with_hashes,
                        &source_for_collector,
                        &version_for_collector,
                    )?;

                    batch_count += 1;
                }
            }
            Ok((batch_count, total_chunks))
        });

        // Main reading thread - feed the pipeline
        const BATCH_SIZE: usize = 10_000; // Balance between efficiency and progress updates
        let mut sequences_batch = Vec::with_capacity(BATCH_SIZE);

        // Resume from saved position if available
        let mut bytes_read = 0u64; // Always start from 0 for byte counting
        let mut sequences_to_skip = 0usize;

        // If resuming, we need to skip already processed sequences
        if download_state.sequences_processed > 0 {
            sequences_to_skip = download_state.sequences_processed;

            // NOTE: Early completion detection already happened in chunk_database_with_resume()
            // If we reach here, either no partials exist OR they couldn't be used
            // Proceed with normal resume (skip already-processed sequences)

            if let Some(cb) = progress_callback {
                cb(&format!(
                    "Resuming from sequence {} (will skip already processed sequences)",
                    download_state.sequences_processed
                ));
            }

            // For uncompressed files, we could seek directly
            // For compressed files, we'll have to read through and skip
            if !is_compressed && download_state.file_offset > 0 {
                // This path is never taken since we're dealing with .gz files
                // but keeping for future uncompressed support
                bytes_read = download_state.file_offset;
            }
        }

        let checkpoint_interval = 500_000; // Save checkpoint every 500k sequences
        let mut last_checkpoint = download_state.sequences_processed;
        let mut current_id = String::new();
        let mut current_desc = None;
        let mut current_seq = Vec::new();
        let mut current_taxon_id: Option<u32> = None;

        let mut sequences_seen = 0usize; // Track total sequences encountered (including skipped)

        for line in reader.lines() {
            let line = line?;
            bytes_read += line.len() as u64 + 1;
            reading_progress.set_position(bytes_read);

            if let Some(header) = line.strip_prefix('>') {
                // Save previous sequence if any
                if !current_id.is_empty() {
                    sequences_seen += 1;

                    // Skip if we're still catching up to resume position
                    if sequences_seen <= sequences_to_skip {
                        // Just skip this sequence
                        if sequences_seen % 100000 == 0 {
                            if let Some(cb) = progress_callback {
                                cb(&format!("Skipping already processed sequences... {}/{}",
                                    sequences_seen, sequences_to_skip));
                            }
                        }
                    } else {
                        // Actually process this sequence
                        sequences_batch.push(Sequence {
                            id: current_id.clone(),
                            description: current_desc.clone(),
                            sequence: current_seq.clone(),
                            taxon_id: current_taxon_id,
                            taxonomy_sources: Default::default(),
                        });
                        total_sequences.fetch_add(1, Ordering::Relaxed);

                        // Send batch to workers when full
                        if sequences_batch.len() >= BATCH_SIZE {
                            batch_counter.fetch_add(1, Ordering::Relaxed);
                            let batch_to_send =
                                std::mem::replace(&mut sequences_batch, Vec::with_capacity(BATCH_SIZE));
                            batch_sender.send(batch_to_send).unwrap();

                            let total = total_sequences.load(Ordering::Relaxed);

                            // Save checkpoint periodically
                            if total - last_checkpoint >= checkpoint_interval {
                                download_state.sequences_processed = total;
                                download_state.file_offset = bytes_read;
                                download_state.last_sequence_id = Some(current_id.clone());
                                download_state.total_file_size = Some(file_size);

                                if let Some(workspace) = workspace_path {
                                    let state_path = workspace.join("state.json");
                                    // Silent save - don't disrupt progress bars with tracing output
                                    let _ = download_state.save(&state_path);
                                }
                                last_checkpoint = total;
                            }
                        }
                    }
                }

                // Parse new header
                let parts: Vec<&str> = header.splitn(2, ' ').collect();
                current_id = parts[0].to_string();
                current_desc = parts.get(1).map(|s| s.to_string());
                current_seq.clear();
                current_taxon_id = current_desc
                    .as_ref()
                    .and_then(|desc| talaria_bio::formats::fasta::extract_taxon_id(desc));
            } else {
                // Append to sequence
                current_seq.extend(line.bytes());
            }
        }

        // Save last sequence
        if !current_id.is_empty() {
            sequences_seen += 1;

            // Only process if we're past the resume point
            if sequences_seen > sequences_to_skip {
                sequences_batch.push(Sequence {
                    id: current_id,
                    description: current_desc,
                    sequence: current_seq,
                    taxon_id: current_taxon_id,
                    taxonomy_sources: Default::default(),
                });
                total_sequences.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Send final batch if any
        if !sequences_batch.is_empty() {
            batch_counter.fetch_add(1, Ordering::Relaxed);
            batch_sender.send(sequences_batch).unwrap();
        }

        // Close channel to signal workers we're done reading
        drop(batch_sender);

        // Update final progress bar lengths
        let final_total = total_sequences.load(Ordering::Relaxed);
        let _final_batches = batch_counter.load(Ordering::Relaxed);
        processing_progress.set_length(final_total as u64);
        chunking_progress.set_length(final_total as u64);

        // Finish reading progress
        reading_progress
            .finish_with_message(format!("Read {} sequences", format_number(final_total)));

        // Wait for all workers to finish
        for worker in workers {
            worker.join().unwrap();
        }

        // Finish processing and chunking progress bars
        processing_progress.finish_with_message(format!(
            "Processed {} sequences",
            format_number(final_total)
        ));
        chunking_progress
            .finish_with_message(format!("Chunked {} sequences", format_number(final_total)));

        // Get collector results (batch count and total chunks)
        // Propagate any errors that occurred during manifest saving
        let (batch_count, total_chunks) = collector
            .join()
            .map_err(|e| anyhow::anyhow!("Collector thread panicked: {:?}", e))??;

        // CRITICAL VALIDATION: Ensure work was actually done
        // This catches the "skip all sequences → 0 chunks" resume bug
        if batch_count == 0 && total_chunks == 0 && final_total > 0 {
            // We "processed" sequences but created no chunks - this is a bug!
            if sequences_to_skip > 0 {
                // This is the resume bug - all sequences were skipped
                anyhow::bail!(
                    "Resume logic error: Processed {} sequences but created 0 chunks.\n\
                     All sequences were skipped (resume position: {}), indicating the download\n\
                     was already complete but partial manifests were not found in RocksDB.\n\
                     \n\
                     This can happen if:\n\
                     1. A previous run crashed during final manifest building\n\
                     2. RocksDB was corrupted or cleared\n\
                     \n\
                     To fix:\n\
                     1. Delete the workspace: rm -rf {}\n\
                     2. Restart the download: talaria database download [source]",
                    format_number(final_total),
                    format_number(sequences_to_skip),
                    workspace_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "workspace".to_string())
                );
            } else {
                // Not a resume issue - chunking failed for some other reason
                anyhow::bail!(
                    "Fatal error: Processed {} sequences but created 0 chunks.\n\
                     The chunking process failed to generate any manifests.\n\
                     Please check logs for errors.",
                    format_number(final_total)
                );
            }
        }

        tracing::info!(
            "Saved {} batches with {} total chunks to disk",
            format_number(batch_count),
            format_number(total_chunks)
        );

        // Update final state
        download_state.sequences_processed = final_total;
        download_state.file_offset = bytes_read;
        if let Some(workspace) = workspace_path {
            let state_path = workspace.join("state.json");
            download_state.save(&state_path)?;
        }

        // STREAMING MODE: Chunks are already stored by collector thread!
        // Just need to flush and build the database manifest index
        if batch_count > 0 {
            // Flush to ensure all data is persisted
            let use_bulk_mode = std::env::var("TALARIA_BULK_IMPORT_MODE")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false);

            if use_bulk_mode {
                if let Some(cb) = progress_callback {
                    cb("Flushing all data to disk (bulk mode)...");
                }
                self.repository.storage.chunk_storage().flush()?;
                if let Some(cb) = progress_callback {
                    cb(&format!(
                        "✓ All {} chunk manifests safely persisted",
                        format_number(total_chunks)
                    ));
                }
            } else {
                if let Some(cb) = progress_callback {
                    cb(&format!(
                        "✓ All {} chunk manifests stored",
                        format_number(total_chunks)
                    ));
                }
            }

            // Build lightweight manifest index from partials (streaming - avoids loading all chunks into memory)
            if let Some(cb) = progress_callback {
                cb("Building database manifest index...");
            }

            // Use streaming version to avoid OOM with large manifest counts (e.g., 72M for UniRef90)
            let manifest_count = self.build_and_save_manifest_streaming(source, &version, progress_callback)?;

            if let Some(cb) = progress_callback {
                cb(&format!("Found {} manifests from partials", format_number(manifest_count)));
            }

            // CRITICAL ERROR CHECK: Ensure manifests were created
            if manifest_count == 0 {
                // This should NEVER happen in a successful download
                anyhow::bail!(
                    "FATAL ERROR: Processing completed but no chunk manifests were created!\n\
                     \n\
                     This indicates a critical failure in the download/processing pipeline.\n\
                     Processed {} sequences but created {} manifests.\n\
                     \n\
                     Possible causes:\n\
                     1. All sequences were skipped (resume bug)\n\
                     2. Worker threads failed silently\n\
                     3. Manifest collection failed\n\
                     \n\
                     To recover:\n\
                     1. Delete workspace: rm -rf {}\n\
                     2. Delete partial manifests from RocksDB (if any)\n\
                     3. Restart download from scratch\n\
                     \n\
                     If this persists, please report this bug with full logs.",
                    format_number(final_total),
                    manifest_count,
                    workspace_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "workspace".to_string())
                );
            }

            // Manifest already saved by build_and_save_manifest_streaming()
            // (streaming version saves incrementally to avoid OOM)

            // CRITICAL: Flush RocksDB to ensure manifest is persisted to disk
            // Without this, the manifest may remain in write buffers and not be visible
            // in subsequent operations or after process restart
            if let Some(cb) = progress_callback {
                cb("Flushing manifest to disk...");
            }
            self.get_repository()
                .storage
                .sequence_storage
                .get_rocksdb()
                .flush()?;

            if let Some(cb) = progress_callback {
                cb("✓ Database manifest saved and persisted");
            }
        }

        // Disable streaming mode now that we're done
        sequence_storage.set_streaming_mode(false);

        // Compact database to compress uncompressed L0/L1 data
        // This is critical to reduce storage bloat after bulk writes
        if let Some(cb) = progress_callback {
            cb("Compacting database to compress sequences...");
        }
        self.get_repository()
            .storage
            .sequence_storage
            .get_rocksdb()
            .compact()?;
        if let Some(cb) = progress_callback {
            cb("✓ Database compacted");
        }

        // Rebuild indices from stored data (they were skipped during streaming)
        if let Some(cb) = progress_callback {
            cb("Building indices from stored sequences...");
        }
        // Note: In a full implementation, we'd scan RocksDB and rebuild indices here
        // For now, indices will be empty but the sequences are safely stored
        sequence_storage.save_indices()?;
        if let Some(cb) = progress_callback {
            cb("✓ Indices saved (will be rebuilt on next access)");
        }

        if let Some(cb) = progress_callback {
            cb(&format!(
                "✓ Processed {} sequences using {} CPU cores",
                format_number(final_total),
                num_workers
            ));
        }

        // Invalidate cached metadata so it's refreshed with correct size from partials
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let (source_name, dataset_name) = match source {
            DatabaseSource::UniProt(db) => ("uniprot", format!("{:?}", db).to_lowercase()),
            DatabaseSource::NCBI(db) => ("ncbi", format!("{:?}", db).to_lowercase()),
            DatabaseSource::Custom(name) => ("custom", name.clone()),
        };
        let _ = rocksdb.delete_database_metadata(&source_name, &dataset_name);
        tracing::debug!("Invalidated cached metadata for {}/{} - will be regenerated with correct size", source_name, dataset_name);

        // CRITICAL: Invalidate in-memory cache to force regeneration on next list_databases() call
        // Without this, stale cache prevents the regeneration logic from running
        if let Some(cache) = &self.cache {
            cache.invalidate_database_list();
            tracing::debug!("Invalidated in-memory database list cache");
        }

        // Clear version for next operation
        self.current_version = None;

        Ok(())
    }
}

// Use DownloadResult from parent module (database/mod.rs)

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatabaseInfo {
    pub name: String,
    pub version: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub chunk_count: usize,
    pub sequence_count: usize,
    pub total_size: usize,
    pub reduction_profiles: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HeraldStats {
    pub total_chunks: usize,
    pub total_size: usize,
    pub compressed_chunks: usize,
    pub deduplication_ratio: f32,
    pub database_count: usize,
    pub databases: Vec<DatabaseInfo>,
}

// Use TaxonomyUpdateResult from parent module

impl DatabaseManager {
    /// Query database at a specific bi-temporal coordinate
    ///
    /// This enables temporal queries to retrieve the database state at any point in time.
    /// Query the database at a specific temporal coordinate
    ///
    /// Supports bi-temporal queries for reproducible analyses at specific time points:
    /// - Historical sequence versions
    /// - Taxonomy updates over time
    /// - Reproducible analyses at specific time points
    pub fn query_at_time(
        &self,
        sequence_time: chrono::DateTime<chrono::Utc>,
        taxonomy_time: chrono::DateTime<chrono::Utc>,
        taxon_ids: Option<Vec<u32>>,
    ) -> Result<Vec<talaria_bio::sequence::Sequence>> {
        // Use bi-temporal index to query at specific time
        let bi_temporal =
            crate::temporal::BiTemporalDatabase::new(Arc::new(self.repository.storage.clone()))?;

        // Query at the specified temporal coordinate
        let coordinate = crate::types::BiTemporalCoordinate {
            sequence_time: sequence_time,
            taxonomy_time: taxonomy_time,
        };

        // Note: bi_temporal needs to be mutable for query_at
        let mut bi_temporal = bi_temporal;
        let snapshot = bi_temporal.query_at(sequence_time, taxonomy_time)?;

        // Get the manifest from the snapshot
        let manifest = &snapshot.manifest;

        // Store bi-temporal index in RocksDB for future queries
        let backend = self.repository.storage.chunk_storage();
        let index_key = format!(
            "bitemporal:{}:{}",
            sequence_time.timestamp(),
            taxonomy_time.timestamp()
        );

        if let Some(cf) = backend.db.cf_handle("temporal") {
            // Serialize and store the manifest data for fast retrieval
            if let Some(temporal_manifest) = snapshot.manifest.data() {
                let manifest_data = rmp_serde::to_vec(temporal_manifest)?;
                backend
                    .db
                    .put_cf(&cf, index_key.as_bytes(), &manifest_data)?;
            }

            // Also store in a sorted index for range queries
            let time_index_key = format!(
                "bitemporal_index:{:020}:{:020}",
                sequence_time.timestamp(),
                taxonomy_time.timestamp()
            );
            let index_value = rmp_serde::to_vec(&coordinate)?;
            backend
                .db
                .put_cf(&cf, time_index_key.as_bytes(), &index_value)?;
        }

        // Filter chunks by taxon IDs if specified
        let chunks = if let Some(taxa) = taxon_ids {
            manifest
                .chunk_index()
                .map(|index| {
                    index
                        .iter()
                        .filter(|chunk| chunk.taxon_ids.iter().any(|tid| taxa.contains(&tid.0)))
                        .cloned()
                        .collect()
                })
                .unwrap_or_else(Vec::new)
        } else {
            manifest.chunk_index().cloned().unwrap_or_else(Vec::new)
        };

        // Load sequences from chunks
        let chunk_hashes: Vec<_> = chunks.iter().map(|c| c.hash.clone()).collect();
        self.repository.load_sequences_from_chunks(&chunk_hashes)
    }

    /// Find manifest at a specific temporal coordinate
    ///
    /// Internal helper for bi-temporal queries.
    /// Will search through historical manifests when full temporal support is implemented.
    #[allow(dead_code)] // Used by query_at_time
    fn find_manifest_at_time(
        &self,
        _sequence_time: &chrono::DateTime<chrono::Utc>,
        _taxonomy_time: &chrono::DateTime<chrono::Utc>,
    ) -> Result<crate::TemporalManifest> {
        // For now, return the current manifest
        // In a full implementation, this would search historical manifests
        self.repository
            .manifest
            .get_data()
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
                if let Ok(sequences) = self
                    .repository
                    .storage
                    .load_sequences_from_chunk(&chunk.hash)
                {
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
    pub fn verify_chunk_proof(&self, chunk_hash: &crate::SHA256Hash) -> Result<bool> {
        use crate::MerkleDAG;

        let manifest = self
            .repository
            .manifest
            .get_data()
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
    pub fn get_manifest(&self, database_name: &str) -> Result<crate::TemporalManifest> {
        // Parse database reference to handle database[@version][:profile]
        let db_ref = parse_database_reference(database_name)?;

        // Get RocksDB backend
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Build the manifest key
        // If version is specified, use it; otherwise try to find any version
        let key_prefix = format!("manifest:{}:{}", db_ref.source, db_ref.dataset);

        let mut manifest: crate::TemporalManifest;
        let version: String;

        // If a specific version is requested
        if let Some(v) = &db_ref.version {
            let key = format!("{}:{}", key_prefix, v);
            if let Some(data) = rocksdb.get_manifest(&key)? {
                manifest = bincode::deserialize::<crate::TemporalManifest>(&data)
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize manifest: {}", e))?;
                version = v.clone();
            } else {
                anyhow::bail!("Manifest not found for database: {}", database_name);
            }
        } else {
            // Otherwise, find the latest version or any version
            let manifests = rocksdb.list_manifest_keys_with_prefix(&key_prefix)?;
            if !manifests.is_empty() {
                // Get the latest one (they should be sorted by timestamp)
                let latest_key = manifests.last().unwrap();
                if let Some(data) = rocksdb.get_manifest(latest_key)? {
                    manifest = bincode::deserialize::<crate::TemporalManifest>(&data)
                        .map_err(|e| anyhow::anyhow!("Failed to deserialize manifest: {}", e))?;
                    // Extract version from key (format: manifest:source:dataset:version)
                    version = latest_key.split(':').nth(3).unwrap_or("unknown").to_string();
                } else {
                    anyhow::bail!("Manifest not found for database: {}", database_name);
                }
            } else {
                anyhow::bail!("Manifest not found for database: {}", database_name);
            }
        }

        // Check if this is a streaming manifest (empty chunk_index with etag starting with "streaming-")
        if manifest.chunk_index.is_empty() && manifest.etag.starts_with("streaming-") {
            tracing::debug!(
                "Detected streaming manifest for {}/{} version {}, lazy-loading chunk_index from partials",
                db_ref.source, db_ref.dataset, version
            );

            // Lazy-load chunk_index from partials
            manifest.chunk_index = self.load_chunk_index_from_partials(&db_ref.source, &db_ref.dataset, &version)?;

            tracing::debug!(
                "Loaded {} chunks from partials for {}/{}",
                manifest.chunk_index.len(),
                db_ref.source,
                db_ref.dataset
            );
        }

        Ok(manifest)
    }

    /// Get manifest without loading chunk_index (for operations that don't need it like diff)
    pub fn get_manifest_lightweight(&self, database_name: &str) -> Result<crate::TemporalManifest> {
        // Parse database reference to handle database[@version][:profile]
        let db_ref = parse_database_reference(database_name)?;

        // Get RocksDB backend
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Build the manifest key
        let key_prefix = format!("manifest:{}:{}", db_ref.source, db_ref.dataset);

        let manifest: crate::TemporalManifest;

        // If a specific version is requested
        if let Some(v) = &db_ref.version {
            let key = format!("{}:{}", key_prefix, v);
            if let Some(data) = rocksdb.get_manifest(&key)? {
                manifest = bincode::deserialize::<crate::TemporalManifest>(&data)
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize manifest: {}", e))?;
            } else {
                anyhow::bail!("Manifest not found for database: {}", database_name);
            }
        } else {
            // Otherwise, find the latest version
            let manifests = rocksdb.list_manifest_keys_with_prefix(&key_prefix)?;
            if !manifests.is_empty() {
                let latest_key = manifests.last().unwrap();
                if let Some(data) = rocksdb.get_manifest(latest_key)? {
                    manifest = bincode::deserialize::<crate::TemporalManifest>(&data)
                        .map_err(|e| anyhow::anyhow!("Failed to deserialize manifest: {}", e))?;
                } else {
                    anyhow::bail!("Manifest not found for database: {}", database_name);
                }
            } else {
                anyhow::bail!("Manifest not found for database: {}", database_name);
            }
        }

        // DON'T load chunk_index - leave it empty for streaming manifests
        // Callers can check etag.starts_with("streaming-") to determine if it's streaming

        Ok(manifest)
    }

    /// Load a chunk by its hash
    /// Load a chunk manifest (new approach - manifests only)
    pub fn load_manifest(&self, hash: &crate::SHA256Hash) -> Result<crate::ChunkManifest> {
        let chunk_data = self.repository.storage.get_chunk(hash)?;

        // Try to deserialize as ChunkManifest
        if let Ok(manifest) = rmp_serde::from_slice::<crate::ChunkManifest>(&chunk_data) {
            return Ok(manifest);
        }

        // Try JSON format as fallback
        if let Ok(manifest) = serde_json::from_slice::<crate::ChunkManifest>(&chunk_data) {
            return Ok(manifest);
        }

        Err(anyhow::anyhow!(
            "Chunk is not a manifest - may be old format"
        ))
    }

    /// Load sequences from a manifest using canonical storage
    pub fn load_sequences_from_manifest(
        &self,
        manifest: &crate::ChunkManifest,
        filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(String, String)>> {
        // Returns (id, fasta_data)
        use crate::storage::SequenceStorage;

        let sequences_path = talaria_core::system::paths::talaria_databases_dir().join("sequences");
        let sequence_storage = SequenceStorage::new(&sequences_path)?;

        let mut results = Vec::new();

        for seq_hash in &manifest.sequence_refs {
            if results.len() >= limit {
                break;
            }

            // Get the sequence as FASTA
            let fasta_data = sequence_storage.get_sequence_as_fasta(seq_hash, None)?;

            // Extract ID from header
            let seq_id = fasta_data
                .lines()
                .next()
                .and_then(|line| line.strip_prefix('>'))
                .and_then(|header| header.split_whitespace().next())
                .unwrap_or("unknown")
                .to_string();

            // Apply filter if specified
            if let Some(f) = filter {
                if !seq_id.contains(f) {
                    continue;
                }
            }

            results.push((seq_id, fasta_data));
        }

        Ok(results)
    }

    /// Load taxonomy mappings for a database
    pub fn load_taxonomy_mappings(
        &self,
        database_name: &str,
    ) -> Result<std::collections::HashMap<String, crate::TaxonId>> {
        // Try to get mappings from the manifest
        let source = parse_database_source(database_name)
            .unwrap_or(DatabaseSource::Custom(database_name.to_string()));

        self.get_taxonomy_mapping_from_manifest(&source, None)
    }
}

/// Temporal sequence record for history tracking
///
/// Tracks the bi-temporal history of sequences in the database.
/// Provides:
/// - Complete sequence revision history
/// - Taxonomy assignment changes over time
/// - Provenance tracking for scientific reproducibility
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TemporalSequenceRecord {
    pub sequence_id: String,
    pub version: String,
    pub sequence_time: chrono::DateTime<chrono::Utc>,
    pub taxonomy_time: chrono::DateTime<chrono::Utc>,
    pub taxon_id: Option<u32>,
    pub chunk_hash: crate::SHA256Hash,
}

impl DatabaseManager {
    /// Store temporal history record for a sequence
    pub fn store_temporal_history(&self, record: &TemporalSequenceRecord) -> Result<()> {
        let backend = self.repository.storage.chunk_storage();

        // Create composite key for the history record
        let history_key = format!(
            "history:{}:{}:{}",
            record.sequence_id,
            record.sequence_time.timestamp(),
            record.taxonomy_time.timestamp()
        );

        if let Some(cf) = backend.db.cf_handle("temporal") {
            // Serialize and store the record
            let record_data = rmp_serde::to_vec(record)?;
            backend
                .db
                .put_cf(&cf, history_key.as_bytes(), &record_data)?;

            // Also maintain an index by sequence ID for fast retrieval
            let index_key = format!("history_index:{}", record.sequence_id);
            if let Ok(Some(existing)) = backend.db.get_cf(&cf, index_key.as_bytes()) {
                // Append to existing history
                let mut history: Vec<(i64, i64)> = rmp_serde::from_slice(&existing)?;
                history.push((
                    record.sequence_time.timestamp(),
                    record.taxonomy_time.timestamp(),
                ));
                let updated = rmp_serde::to_vec(&history)?;
                backend.db.put_cf(&cf, index_key.as_bytes(), &updated)?;
            } else {
                // Create new history index
                let history = vec![(
                    record.sequence_time.timestamp(),
                    record.taxonomy_time.timestamp(),
                )];
                let data = rmp_serde::to_vec(&history)?;
                backend.db.put_cf(&cf, index_key.as_bytes(), &data)?;
            }
        }

        Ok(())
    }

    /// Retrieve temporal history for a sequence
    pub fn get_temporal_history(&self, sequence_id: &str) -> Result<Vec<TemporalSequenceRecord>> {
        let backend = self.repository.storage.chunk_storage();
        let mut records = Vec::new();

        if let Some(cf) = backend.db.cf_handle("temporal") {
            // Get the history index for this sequence
            let index_key = format!("history_index:{}", sequence_id);
            if let Ok(Some(index_data)) = backend.db.get_cf(&cf, index_key.as_bytes()) {
                let history: Vec<(i64, i64)> = rmp_serde::from_slice(&index_data)?;

                // Retrieve each historical record
                for (seq_time, tax_time) in history {
                    let history_key = format!("history:{}:{}:{}", sequence_id, seq_time, tax_time);

                    if let Ok(Some(record_data)) = backend.db.get_cf(&cf, history_key.as_bytes()) {
                        let record: TemporalSequenceRecord = rmp_serde::from_slice(&record_data)?;
                        records.push(record);
                    }
                }
            }
        }

        // Sort by sequence time
        records.sort_by_key(|r| r.sequence_time);
        Ok(records)
    }

    /// Query temporal history within a time range
    pub fn query_temporal_history(
        &self,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<TemporalSequenceRecord>> {
        let backend = self.repository.storage.chunk_storage();
        let mut records = Vec::new();

        if let Some(cf) = backend.db.cf_handle("temporal") {
            // Use prefix iterator to scan history records
            let prefix = b"history:";
            let iter = backend.db.prefix_iterator_cf(&cf, prefix);

            for result in iter {
                if let Ok((key, value)) = result {
                    // Parse the key to extract timestamps
                    let key_str = String::from_utf8_lossy(key.as_ref());
                    let parts: Vec<&str> = key_str.split(':').collect();
                    if parts.len() >= 4 {
                        if let Ok(seq_time) = parts[2].parse::<i64>() {
                            let record_time = chrono::DateTime::from_timestamp(seq_time, 0)
                                .unwrap_or(chrono::Utc::now());

                            // Check if within time range
                            if record_time >= start_time && record_time <= end_time {
                                if let Ok(record) =
                                    rmp_serde::from_slice::<TemporalSequenceRecord>(value.as_ref())
                                {
                                    records.push(record);
                                }
                            }
                        }
                    }
                }
            }
        }

        records.sort_by_key(|r| r.sequence_time);
        Ok(records)
    }
}

// Implement TaxonomyProvider trait for DatabaseManager
impl talaria_utils::taxonomy::TaxonomyProvider for DatabaseManager {
    fn has_taxonomy(&self) -> bool {
        talaria_utils::taxonomy::has_taxonomy()
    }

    fn require_taxonomy(&self) -> Result<()> {
        talaria_utils::taxonomy::require_taxonomy()
    }

    fn get_taxonomy_tree_path(&self) -> Result<PathBuf> {
        self.require_taxonomy()?;
        Ok(talaria_utils::taxonomy::get_taxonomy_tree_path())
    }

    fn get_taxonomy_mappings_dir(&self) -> Result<PathBuf> {
        Ok(talaria_utils::taxonomy::get_taxonomy_mappings_dir())
    }
}

impl DatabaseManager {
    /// Get taxonomy mappings for a specific database source
    ///
    /// This is a convenience method that uses the unified TaxonomyProvider
    /// to load mappings in a consistent way across all commands.
    pub fn get_taxonomy_mappings_for_source(
        &self,
        source: &DatabaseSource,
    ) -> Result<HashMap<String, crate::TaxonId>> {
        use talaria_utils::taxonomy::{
            load_taxonomy_mappings_with_fallback, TaxonomyMappingSource,
        };

        let preferred_source = match source {
            DatabaseSource::UniProt(_) => TaxonomyMappingSource::UniProt,
            DatabaseSource::NCBI(_) => TaxonomyMappingSource::NCBI,
            _ => return Ok(HashMap::new()),
        };

        // Use the unified loading function with automatic fallback
        // This allows UniProt databases to use NCBI mappings when UniProt idmapping isn't available
        let mappings: HashMap<String, u32> = load_taxonomy_mappings_with_fallback(preferred_source)?;

        // Convert to our TaxonId type
        Ok(mappings
            .into_iter()
            .map(|(k, v)| (k, crate::TaxonId(v)))
            .collect())
    }

    /// Compare two manifests to determine if they contain identical content
    ///
    /// Compares chunk hashes and sequence counts to detect duplicate databases.
    /// This prevents creating redundant versions when force-downloading identical data.
    fn manifests_are_identical(a: &crate::TemporalManifest, b: &crate::TemporalManifest) -> bool {
        use std::collections::HashSet;

        // Quick size check first
        if a.chunk_index.len() != b.chunk_index.len() {
            return false;
        }

        // Compare chunk hashes and sequence counts
        let a_chunks: HashSet<_> = a
            .chunk_index
            .iter()
            .map(|c| (&c.hash, c.sequence_count))
            .collect();

        let b_chunks: HashSet<_> = b
            .chunk_index
            .iter()
            .map(|c| (&c.hash, c.sequence_count))
            .collect();

        a_chunks == b_chunks
    }

    // ========== Version Management (RocksDB-based) ==========

    /// List all versions for a database from RocksDB
    ///
    /// Returns versions sorted by timestamp (newest first)
    pub fn list_database_versions(
        &self,
        source: &str,
        dataset: &str,
    ) -> Result<Vec<talaria_core::types::DatabaseVersionInfo>> {
        // Try to get from cache first
        if let Some(cache) = &self.cache {
            if let Some(cached_versions) = cache.get_version_list(source, dataset) {
                debug!(
                    "Returning cached version list for {}/{} ({} entries)",
                    source,
                    dataset,
                    cached_versions.len()
                );
                return Ok(cached_versions);
            }
        }

        debug!(
            "Cache miss - querying RocksDB for versions of {}/{}",
            source, dataset
        );
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let mut versions = Vec::new();

        // Scan RocksDB for all manifest keys matching this database
        let prefix = format!("manifest:{}:{}:", source, dataset);
        let items = rocksdb.iterate_manifest_prefix(&prefix)?;

        for (key, value) in items {
            // Extract timestamp from key: manifest:{source}:{dataset}:{timestamp}
            if let Some(timestamp) = key.strip_prefix(&prefix) {
                // Load manifest to get metadata
                let manifest: crate::TemporalManifest = bincode::deserialize(&value)?;

                // Get aliases for this version
                let aliases = self.get_version_aliases(source, dataset, timestamp)?;

                let version_info = talaria_core::types::DatabaseVersionInfo {
                    timestamp: timestamp.to_string(),
                    created_at: manifest.created_at,
                    upstream_version: Some(manifest.version.clone()),
                    source: source.to_string(),
                    dataset: dataset.to_string(),
                    aliases,
                    chunk_count: manifest.chunk_index.len(),
                    sequence_count: manifest.chunk_index.iter().map(|c| c.sequence_count).sum(),
                    total_size: manifest.chunk_index.iter().map(|c| c.size as u64).sum(),
                };

                versions.push(version_info);
            }
        }

        // Sort by timestamp (newest first)
        versions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Update cache
        if let Some(cache) = &self.cache {
            let _ = cache.set_version_list(source, dataset, versions.clone());
        }

        Ok(versions)
    }

    /// Get all aliases for a specific version
    pub fn get_version_aliases(
        &self,
        source: &str,
        dataset: &str,
        timestamp: &str,
    ) -> Result<Vec<String>> {
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let mut aliases = Vec::new();

        // Check for standard aliases (current, latest, stable)
        for alias in &["current", "latest", "stable"] {
            let alias_key = format!("alias:{}:{}:{}", source, dataset, alias);
            if let Ok(Some(target_bytes)) = rocksdb.get_manifest(&alias_key) {
                if let Ok(target) = String::from_utf8(target_bytes) {
                    if target == timestamp {
                        aliases.push(alias.to_string());
                    }
                }
            }
        }

        // Scan for custom aliases
        let custom_prefix = format!("alias:{}:{}:custom:", source, dataset);
        let custom_items = rocksdb.iterate_manifest_prefix(&custom_prefix)?;

        for (key, value) in custom_items {
            if let Some(alias_name) = key.strip_prefix(&custom_prefix) {
                if let Ok(target) = String::from_utf8(value) {
                    if target == timestamp {
                        aliases.push(alias_name.to_string());
                    }
                }
            }
        }

        Ok(aliases)
    }

    /// Set a version alias (current, latest, stable, or custom)
    pub fn set_version_alias(
        &self,
        source: &str,
        dataset: &str,
        timestamp: &str,
        alias: &str,
    ) -> Result<()> {
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Verify the version exists
        let manifest_key = format!("manifest:{}:{}:{}", source, dataset, timestamp);
        if rocksdb.get_manifest(&manifest_key)?.is_none() {
            anyhow::bail!(
                "Version {} does not exist for {}/{}",
                timestamp,
                source,
                dataset
            );
        }

        // Determine alias key based on type
        let alias_key = if matches!(alias, "current" | "latest" | "stable") {
            format!("alias:{}:{}:{}", source, dataset, alias)
        } else {
            format!("alias:{}:{}:custom:{}", source, dataset, alias)
        };

        // Store alias pointing to timestamp
        rocksdb.put_manifest(&alias_key, timestamp.as_bytes())?;

        Ok(())
    }

    /// Resolve a version reference (alias or timestamp) to a timestamp
    pub fn resolve_version_reference(
        &self,
        source: &str,
        dataset: &str,
        reference: &str,
    ) -> Result<String> {
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // First check if it's already a direct timestamp
        let manifest_key = format!("manifest:{}:{}:{}", source, dataset, reference);
        if rocksdb.get_manifest(&manifest_key)?.is_some() {
            return Ok(reference.to_string());
        }

        // Check standard aliases
        let standard_alias_key = format!("alias:{}:{}:{}", source, dataset, reference);
        if let Ok(Some(target_bytes)) = rocksdb.get_manifest(&standard_alias_key) {
            return Ok(String::from_utf8(target_bytes)?);
        }

        // Check custom aliases
        let custom_alias_key = format!("alias:{}:{}:custom:{}", source, dataset, reference);
        if let Ok(Some(target_bytes)) = rocksdb.get_manifest(&custom_alias_key) {
            return Ok(String::from_utf8(target_bytes)?);
        }

        anyhow::bail!(
            "Version reference '{}' not found for {}/{}",
            reference,
            source,
            dataset
        )
    }

    /// Get a specific version's manifest
    pub fn get_version_manifest(
        &self,
        source: &str,
        dataset: &str,
        version_ref: &str,
    ) -> Result<crate::TemporalManifest> {
        let timestamp = self.resolve_version_reference(source, dataset, version_ref)?;
        let manifest_key = format!("manifest:{}:{}:{}", source, dataset, timestamp);

        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();
        let manifest_bytes = rocksdb
            .get_manifest(&manifest_key)?
            .ok_or_else(|| anyhow::anyhow!("Manifest not found for version {}", timestamp))?;

        let manifest: crate::TemporalManifest = bincode::deserialize(&manifest_bytes)?;
        Ok(manifest)
    }

    /// Delete a version alias
    pub fn delete_version_alias(&self, source: &str, dataset: &str, alias: &str) -> Result<()> {
        // Prevent deletion of protected aliases
        if matches!(alias, "current" | "latest") {
            anyhow::bail!("Cannot delete protected alias '{}'", alias);
        }

        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Try standard alias first
        let standard_key = format!("alias:{}:{}:{}", source, dataset, alias);
        if rocksdb.get_manifest(&standard_key)?.is_some() {
            rocksdb.delete_manifest(&standard_key)?;
            return Ok(());
        }

        // Try custom alias
        let custom_key = format!("alias:{}:{}:custom:{}", source, dataset, alias);
        if rocksdb.get_manifest(&custom_key)?.is_some() {
            rocksdb.delete_manifest(&custom_key)?;
            return Ok(());
        }

        anyhow::bail!("Alias '{}' not found for {}/{}", alias, source, dataset)
    }

    /// Delete a specific database version
    ///
    /// This removes the version manifest from RocksDB and updates aliases if needed.
    /// Note: Does not remove chunks, as they may be shared with other versions.
    pub fn delete_database_version(
        &self,
        source: &str,
        dataset: &str,
        version: &str,
    ) -> Result<()> {
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Resolve version reference to timestamp
        let timestamp = self.resolve_version_reference(source, dataset, version)?;

        // Check if this is the current version
        let is_current = if let Ok(aliases) = self.get_version_aliases(source, dataset, &timestamp)
        {
            aliases.contains(&"current".to_string())
        } else {
            false
        };

        // Delete the manifest
        let manifest_key = format!("manifest:{}:{}:{}", source, dataset, timestamp);
        rocksdb.delete_manifest(&manifest_key)?;

        // Remove all aliases pointing to this version
        self.cleanup_version_aliases(source, dataset, &timestamp)?;

        // Invalidate caches
        if let Some(cache) = &self.cache {
            cache.invalidate_database(source, dataset);
        }

        // If we deleted the current version, warn the user
        if is_current {
            tracing::warn!("Deleted current version of {}/{}", source, dataset);
            tracing::info!("Run 'talaria database versions set-current {}/{} <version>' to set a new current version", source, dataset);
        }

        Ok(())
    }

    /// Delete all versions of a database
    ///
    /// This removes all version manifests and aliases for a database.
    /// Note: Does not remove chunks, use 'database clean' afterwards.
    pub fn delete_entire_database(&self, source: &str, dataset: &str) -> Result<Vec<String>> {
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Get all versions
        let versions = self.list_database_versions(source, dataset)?;
        let mut deleted_versions = Vec::new();

        for version in versions {
            // Delete manifest
            let manifest_key = format!("manifest:{}:{}:{}", source, dataset, version.timestamp);
            rocksdb.delete_manifest(&manifest_key)?;

            // Remove aliases
            self.cleanup_version_aliases(source, dataset, &version.timestamp)?;

            deleted_versions.push(version.timestamp);
        }

        // Invalidate caches
        if let Some(cache) = &self.cache {
            cache.invalidate_database(source, dataset);
        }

        Ok(deleted_versions)
    }

    /// Remove all aliases pointing to a specific version
    fn cleanup_version_aliases(&self, source: &str, dataset: &str, timestamp: &str) -> Result<()> {
        let rocksdb = self.get_repository().storage.sequence_storage.get_rocksdb();

        // Check and remove standard aliases
        for alias in &["current", "latest", "stable"] {
            let alias_key = format!("alias:{}:{}:{}", source, dataset, alias);
            if let Ok(Some(target_bytes)) = rocksdb.get_manifest(&alias_key) {
                if let Ok(target) = String::from_utf8(target_bytes) {
                    if target == timestamp {
                        rocksdb.delete_manifest(&alias_key)?;
                    }
                }
            }
        }

        // Check and remove custom aliases
        let custom_prefix = format!("alias:{}:{}:custom:", source, dataset);
        let custom_items = rocksdb.iterate_manifest_prefix(&custom_prefix)?;

        for (alias_key, target_bytes) in custom_items {
            if let Ok(target) = String::from_utf8(target_bytes) {
                if target == timestamp {
                    rocksdb.delete_manifest(&alias_key)?;
                }
            }
        }

        Ok(())
    }

    /// Get information about a specific version for deletion preview
    pub fn get_version_info(
        &self,
        source: &str,
        dataset: &str,
        version: &str,
    ) -> Result<talaria_core::types::DatabaseVersionInfo> {
        let timestamp = self.resolve_version_reference(source, dataset, version)?;
        let versions = self.list_database_versions(source, dataset)?;

        versions
            .into_iter()
            .find(|v| v.timestamp == timestamp)
            .ok_or_else(|| anyhow::anyhow!("Version not found: {}", version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // DatabaseSource types already imported from talaria_core at the top

    #[allow(dead_code)]
    /// Helper to create a test HERALD manager with test environment
    fn create_test_manager() -> (DatabaseManager, talaria_test::TestEnvironment) {
        let env = talaria_test::TestEnvironment::new().unwrap();
        // Set the environment variables before creating the manager
        let manager =
            DatabaseManager::new(Some(env.databases_dir().to_string_lossy().to_string())).unwrap();
        (manager, env)
    }

    /// Helper to create a fake manifest
    fn create_fake_manifest() -> crate::TemporalManifest {
        use crate::{ManifestMetadata, SHA256Hash, TaxonId, TemporalManifest};
        use chrono::Utc;

        TemporalManifest {
            version: "test_v1".to_string(),
            created_at: Utc::now(),
            sequence_version: "2024-01-01".to_string(),
            taxonomy_version: "2024-01-01".to_string(),
            temporal_coordinate: None,
            taxonomy_root: SHA256Hash::compute(b"test_taxonomy"),
            sequence_root: SHA256Hash::compute(b"test_sequence"),
            chunk_merkle_tree: None,
            taxonomy_manifest_hash: SHA256Hash::compute(b"test_tax_manifest"),
            taxonomy_dump_version: "2024-01-01".to_string(),
            source_database: Some("uniprot-swissprot".to_string()),
            chunk_index: vec![
                ManifestMetadata {
                    hash: SHA256Hash::compute(b"chunk1"),
                    taxon_ids: vec![TaxonId(9606)], // Human
                    sequence_count: 100,
                    size: 1024,
                    compressed_size: Some(512),
                },
                ManifestMetadata {
                    hash: SHA256Hash::compute(b"chunk2"),
                    taxon_ids: vec![TaxonId(10090)], // Mouse
                    sequence_count: 50,
                    size: 512,
                    compressed_size: Some(256),
                },
            ],
            discrepancies: vec![],
            etag: "test_etag_123".to_string(),
            previous_version: None,
        }
    }

    // Note: Removed filesystem-based tests as version management now uses RocksDB exclusively
    // Previous tests: test_manifest_path_for_different_databases, test_manifest_saved_to_correct_location,
    // test_subsequent_download_finds_existing_manifest, test_multiple_database_manifests_coexist,
    // test_manifest_directory_creation, test_download_detection_flow

    #[test]
    #[serial_test::serial]
    fn test_manifest_content_has_source_database() {
        let manifest = create_fake_manifest();

        // Verify source_database is set
        assert_eq!(
            manifest.source_database,
            Some("uniprot-swissprot".to_string())
        );

        // Serialize and verify it's in JSON
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        assert!(json.contains("\"source_database\""));
        assert!(json.contains("uniprot-swissprot"));
    }
}
