/// Manifest management for CASG with ETag-based update checking

use crate::casg::types::*;
use crate::utils::progress::create_progress_bar;
use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::header::{ETAG, IF_NONE_MATCH};
use reqwest::StatusCode;
use serde_json;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Manifest {
    data: Option<TemporalManifest>,
    path: PathBuf,
    remote_url: Option<String>,
    cached_etag: Option<String>,
}

impl Manifest {
    pub fn new() -> Self {
        Self {
            data: None,
            path: PathBuf::new(),
            remote_url: None,
            cached_etag: None,
        }
    }

    pub fn new_with_path(base_path: &Path) -> Self {
        Self {
            data: None,
            path: base_path.join("manifest.json"),
            remote_url: None,
            cached_etag: None,
        }
    }

    /// Load a specific manifest file
    pub fn load_file(manifest_path: &Path) -> Result<Self> {
        let content = fs::read_to_string(manifest_path)
            .context("Failed to read manifest")?;
        let data: TemporalManifest = serde_json::from_str(&content)
            .context("Failed to parse manifest")?;

        Ok(Self {
            data: Some(data),
            path: manifest_path.to_path_buf(),
            remote_url: None,
            cached_etag: None,
        })
    }

    pub fn load(base_path: &Path) -> Result<Self> {
        let manifest_path = base_path.join("manifest.json");
        let etag_path = base_path.join(".etag");

        let data = if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path)
                .context("Failed to read manifest")?;
            Some(serde_json::from_str(&content)
                .context("Failed to parse manifest")?)
        } else {
            None
        };

        let cached_etag = if etag_path.exists() {
            Some(fs::read_to_string(&etag_path)
                .context("Failed to read cached ETag")?)
        } else {
            None
        };

        // Load remote URL from config
        let config_path = base_path.join("config.json");
        let remote_url = if config_path.exists() {
            let config: serde_json::Value = serde_json::from_str(
                &fs::read_to_string(&config_path)?
            )?;
            config.get("remote_url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };

        Ok(Self {
            data,
            path: manifest_path,
            remote_url,
            cached_etag,
        })
    }

    pub fn save(&self) -> Result<()> {
        if let Some(ref manifest) = self.data {
            let content = serde_json::to_string_pretty(manifest)?;
            fs::write(&self.path, content)
                .context("Failed to write manifest")?;

            // Save ETag if present
            if let Some(ref etag) = self.cached_etag {
                let etag_path = self.path.with_extension("etag");
                fs::write(etag_path, etag)
                    .context("Failed to write ETag")?;
            }
        }
        Ok(())
    }

    /// Check if remote updates are available using ETag
    pub async fn check_remote_updates(&self) -> Result<bool> {
        let url = self.remote_url.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No remote URL configured"))?;

        let client = reqwest::Client::new();
        let mut request = client.head(url);

        // Add If-None-Match header if we have a cached ETag
        if let Some(ref etag) = self.cached_etag {
            request = request.header(IF_NONE_MATCH, etag.as_str());
        }

        let response = request.send().await
            .context("Failed to check for updates")?;

        // 304 Not Modified means no updates
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(false);
        }

        // Check if ETag is different
        if let Some(new_etag) = response.headers().get(ETAG) {
            let new_etag_str = new_etag.to_str()
                .context("Invalid ETag header")?;

            if let Some(ref cached) = self.cached_etag {
                return Ok(cached != new_etag_str);
            }
        }

        // If we get here, assume updates are available
        Ok(true)
    }

    /// Fetch the remote manifest
    pub async fn fetch_remote(&self) -> Result<Manifest> {
        let url = self.remote_url.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No remote URL configured"))?;

        let client = reqwest::Client::new();
        let response = client.get(url)
            .send()
            .await
            .context("Failed to fetch manifest")?;

        // Extract ETag for caching
        let etag = response.headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let content = response.text().await
            .context("Failed to read manifest content")?;

        let manifest_data: TemporalManifest = serde_json::from_str(&content)
            .context("Failed to parse remote manifest")?;

        Ok(Manifest {
            data: Some(manifest_data),
            path: self.path.clone(),
            remote_url: self.remote_url.clone(),
            cached_etag: etag,
        })
    }

    /// Compute diff between this manifest and another
    pub fn diff(&self, other: &Manifest) -> Result<ManifestDiff> {
        let current = self.data.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No current manifest"))?;
        let new = other.data.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No new manifest"))?;

        // Build hash sets for efficient comparison
        use std::collections::HashSet;

        let current_chunks: HashSet<_> = current.chunk_index
            .iter()
            .map(|c| c.hash.clone())
            .collect();

        let new_chunks: HashSet<_> = new.chunk_index
            .iter()
            .map(|c| c.hash.clone())
            .collect();

        let added = new_chunks.difference(&current_chunks)
            .cloned()
            .collect();
        let removed = current_chunks.difference(&new_chunks)
            .cloned()
            .collect();

        // Detect taxonomy changes
        let taxonomy_changes = self.detect_taxonomy_changes(current, new)?;

        // Detect modified chunks (same position but different hash)
        let mut modified = Vec::new();
        for chunk_info in &current.chunk_index {
            // Check if there's a chunk at the same logical position with different hash
            if let Some(new_chunk) = new.chunk_index.iter().find(|nc| {
                nc.sequence_count == chunk_info.sequence_count &&
                nc.hash != chunk_info.hash
            }) {
                // Add the new hash as a modified chunk
                modified.push(new_chunk.hash.clone());
            }
        }

        Ok(ManifestDiff {
            new_chunks: added,
            removed_chunks: removed,
            modified_chunks: modified,
            taxonomy_changes,
        })
    }

    fn detect_taxonomy_changes(
        &self,
        current: &TemporalManifest,
        new: &TemporalManifest
    ) -> Result<TaxonomyChanges> {
        // Compare taxonomy roots to detect changes
        let changed = current.taxonomy_root != new.taxonomy_root;

        if !changed {
            return Ok(TaxonomyChanges {
                reclassifications: Vec::new(),
                new_taxa: Vec::new(),
                deprecated_taxa: Vec::new(),
                merged_taxa: Vec::new(),
            });
        }

        // Load taxonomy managers for both versions
        use crate::core::paths;
        let _tax_path = paths::talaria_casg_dir().join("taxonomy");

        let mut reclassifications = Vec::new();
        let mut new_taxa = Vec::new();
        let mut deprecated_taxa = Vec::new();
        let merged_taxa = Vec::new();

        // Compare chunks to detect potential reclassifications
        for chunk_info in &current.chunk_index {
            if let Some(new_chunk) = new.chunk_index.iter().find(|nc| nc.hash == chunk_info.hash) {
                // If the same chunk has different taxon_ids, it might be reclassified
                if chunk_info.taxon_ids != new_chunk.taxon_ids {
                    // This chunk has been reclassified
                    // Note: We'd need to load chunk data to get actual taxon IDs
                    // For now, just record that there was a reclassification
                    reclassifications.push(Reclassification {
                        taxon_id: TaxonId(0), // Placeholder - would need chunk data
                        old_parent: TaxonId(0),
                        new_parent: TaxonId(0),
                        reason: "Taxonomy version change detected".to_string(),
                    });
                }
            }
        }

        // Basic detection of new/deprecated taxa based on unique taxon IDs
        let current_taxon_ids: std::collections::HashSet<_> = current.chunk_index
            .iter()
            .flat_map(|c| &c.taxon_ids)
            .collect();
        let new_taxon_ids: std::collections::HashSet<_> = new.chunk_index
            .iter()
            .flat_map(|c| &c.taxon_ids)
            .collect();

        for taxon_id in new_taxon_ids.difference(&current_taxon_ids) {
            new_taxa.push(**taxon_id);
        }

        for taxon_id in current_taxon_ids.difference(&new_taxon_ids) {
            deprecated_taxa.push(**taxon_id);
        }

        Ok(TaxonomyChanges {
            reclassifications,
            new_taxa,
            deprecated_taxa,
            merged_taxa,
        })
    }

    /// Create a manifest for a set of chunks
    pub fn create_from_chunks(
        &mut self,
        chunks: Vec<TaxonomyAwareChunk>,
        taxonomy_root: MerkleHash,
        sequence_root: MerkleHash,
    ) -> Result<TemporalManifest> {
        // Create progress bar for manifest creation
        let progress = create_progress_bar(chunks.len() as u64, "Creating manifest metadata");

        let chunk_index: Vec<ChunkMetadata> = chunks
            .iter()
            .map(|chunk| {
                progress.inc(1);
                ChunkMetadata {
                    hash: chunk.content_hash.clone(),
                    taxon_ids: chunk.taxon_ids.clone(),
                    sequence_count: chunk.sequences.len(),
                    size: chunk.size,
                    compressed_size: chunk.compressed_size,
                }
            })
            .collect();

        progress.finish_with_message("Manifest metadata created");

        // Generate ETag from content
        let etag = Self::generate_etag(&taxonomy_root, &sequence_root);

        let manifest = TemporalManifest {
            version: Utc::now().format("%Y%m%d_%H%M%S").to_string(),
            created_at: Utc::now(),
            sequence_version: Utc::now().format("%Y-%m-%d").to_string(),
            taxonomy_version: Utc::now().format("%Y-%m-%d").to_string(),
            taxonomy_root,
            sequence_root,
            taxonomy_manifest_hash: SHA256Hash::compute(b"default_taxonomy"),
            taxonomy_dump_version: Utc::now().format("%Y-%m-%d").to_string(),
            source_database: None,
            chunk_index,
            discrepancies: Vec::new(),
            etag,
            previous_version: self.data.as_ref().map(|m| m.version.clone()),
        };

        // Store in self
        self.data = Some(manifest.clone());

        Ok(manifest)
    }

    fn generate_etag(taxonomy_root: &MerkleHash, sequence_root: &MerkleHash) -> String {
        let combined = format!("{}-{}", taxonomy_root, sequence_root);
        format!("\"{}\"", &combined[0..16]) // Standard ETag format
    }

    /// Get current manifest data
    pub fn get_data(&self) -> Option<&TemporalManifest> {
        self.data.as_ref()
    }

    pub fn get_remote_url(&self) -> Option<&str> {
        self.remote_url.as_deref()
    }

    pub fn get_etag(&self) -> Option<&str> {
        self.cached_etag.as_deref()
    }

    pub fn set_remote_url(&mut self, url: String) {
        self.remote_url = Some(url);
    }

    /// Set manifest data
    pub fn set_data(&mut self, data: TemporalManifest) {
        self.data = Some(data);
    }

    /// Get summary of manifest
    pub fn summary(&self) -> Result<String> {
        let manifest = self.data.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No manifest loaded"))?;

        Ok(format!(
            "Manifest Version: {}\n\
             Created: {}\n\
             Chunks: {}\n\
             Total Sequences: {}\n\
             Taxonomy Root: {}\n\
             Sequence Root: {}\n\
             Discrepancies: {}\n\
             ETag: {}",
            manifest.version,
            manifest.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
            manifest.chunk_index.len(),
            manifest.chunk_index.iter().map(|c| c.sequence_count).sum::<usize>(),
            manifest.taxonomy_root,
            manifest.sequence_root,
            manifest.discrepancies.len(),
            manifest.etag
        ))
    }
}