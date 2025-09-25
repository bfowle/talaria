/// Bi-temporal versioning for SEQUOIA
use crate::types::*;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

// Re-export from talaria-core with same name for compatibility
pub use talaria_core::types::TemporalVersionInfo as VersionInfo;

/// Manages temporal aspects of the SEQUOIA system
pub struct TemporalIndex {
    pub base_path: PathBuf,
    sequence_timeline: BTreeMap<DateTime<Utc>, SequenceVersion>,
    taxonomy_timeline: BTreeMap<DateTime<Utc>, TaxonomyVersion>,
    cross_references: Vec<TemporalCrossReference>,
    /// Track header changes over time
    header_history: Vec<SequenceMetadataHistory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceVersion {
    pub version: String,
    pub timestamp: DateTime<Utc>,
    pub root_hash: MerkleHash,
    pub chunk_count: usize,
    pub sequence_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyVersion {
    pub version: String,
    pub timestamp: DateTime<Utc>,
    pub root_hash: MerkleHash,
    pub taxa_count: usize,
    pub source: String, // e.g., "NCBI 2024.01"
    #[serde(default)]
    pub reclassifications: HashMap<TaxonId, TaxonId>, // Old -> New taxon mappings
    #[serde(default)]
    pub active_taxa: HashSet<TaxonId>, // Taxa that are valid in this version
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalCrossReference {
    pub sequence_version: String,
    pub taxonomy_version: String,
    pub created_at: DateTime<Utc>,
    pub validity_start: DateTime<Utc>,
    pub validity_end: Option<DateTime<Utc>>,
    pub cross_hash: SHA256Hash,
}

/// Tracks the history of header changes for a sequence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceMetadataHistory {
    pub sequence_id: String,
    pub header_changes: Vec<TimestampedHeaderChange>,
}

/// A header change with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedHeaderChange {
    pub timestamp: DateTime<Utc>,
    pub old_header: String,
    pub new_header: String,
    pub change_type: HeaderChangeType,
}

/// Types of header changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HeaderChangeType {
    DescriptionUpdate,
    IdRename,
    TaxonomyUpdate,
    AnnotationChange,
}

impl TemporalIndex {
    pub fn new(base_path: &Path) -> Result<Self> {
        let temporal_dir = base_path.join("temporal");
        fs::create_dir_all(&temporal_dir)?;

        Ok(Self {
            base_path: temporal_dir,
            sequence_timeline: BTreeMap::new(),
            taxonomy_timeline: BTreeMap::new(),
            cross_references: Vec::new(),
            header_history: Vec::new(),
        })
    }

    /// Get version history for reporting
    pub fn get_version_history(&self, limit: usize) -> Result<Vec<VersionInfo>> {
        let mut versions = Vec::new();

        // Combine sequence and taxonomy versions
        for (timestamp, seq_version) in self.sequence_timeline.iter().rev().take(limit) {
            let taxonomy_version = self
                .taxonomy_timeline
                .range(..=timestamp)
                .next_back()
                .map(|(_, v)| v.clone());

            let changes = self.detect_changes(&seq_version.version);

            versions.push(VersionInfo {
                version: seq_version.version.clone(),
                timestamp: *timestamp,
                version_type: "Sequence Update".to_string(),
                sequence_root: seq_version.root_hash.to_string(),
                taxonomy_root: taxonomy_version
                    .as_ref()
                    .map(|t| t.root_hash.to_string())
                    .unwrap_or_else(|| "N/A".to_string()),
                chunk_count: seq_version.chunk_count,
                sequence_count: seq_version.sequence_count,
                changes,
                parent_version: self.get_parent_version(&seq_version.version),
            });
        }

        Ok(versions)
    }

    /// Get snapshots between two dates
    pub fn get_snapshots_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<DateTime<Utc>>> {
        let mut snapshots = Vec::new();

        // Get all unique timestamps from both timelines
        for (timestamp, _) in self.sequence_timeline.range(from..=to) {
            snapshots.push(*timestamp);
        }

        for (timestamp, _) in self.taxonomy_timeline.range(from..=to) {
            if !snapshots.contains(timestamp) {
                snapshots.push(*timestamp);
            }
        }

        snapshots.sort();
        Ok(snapshots)
    }

    /// Get chunks at a specific temporal coordinate
    pub fn get_chunks_at_time(
        &self,
        coordinate: &BiTemporalCoordinate,
    ) -> Result<Vec<ManifestMetadata>> {
        // Get the manifest at this point in time
        let state = self.get_state_at(coordinate.sequence_time)?;

        // If we have a manifest with chunks, return them
        if let Some(manifest) = state.manifest {
            return Ok(manifest.chunk_index);
        }

        // Otherwise return empty vec
        Ok(Vec::new())
    }

    /// Load manifest at a specific timestamp
    fn load_manifest_at(&self, timestamp: DateTime<Utc>) -> Result<Option<TemporalManifest>> {
        // Find the manifest file for this timestamp
        let manifest_dir = self.base_path.join("manifests");

        if !manifest_dir.exists() {
            return Ok(None);
        }

        // Look for manifest files matching the timestamp
        // Format: manifest_YYYY-MM-DD_HH-MM-SS.json
        let timestamp_str = timestamp.format("%Y-%m-%d_%H-%M-%S").to_string();
        let manifest_file = manifest_dir.join(format!("manifest_{}.json", timestamp_str));

        if manifest_file.exists() {
            let content = fs::read_to_string(&manifest_file)?;
            let manifest: TemporalManifest = serde_json::from_str(&content)?;
            return Ok(Some(manifest));
        }

        // If no exact match, find the closest earlier manifest
        let entries = fs::read_dir(&manifest_dir)?;
        let mut best_match: Option<(DateTime<Utc>, PathBuf)> = None;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.starts_with("manifest_") && filename.ends_with(".json") {
                    // Parse timestamp from filename
                    if let Some(ts_str) = filename
                        .strip_prefix("manifest_")
                        .and_then(|s| s.strip_suffix(".json"))
                    {
                        if let Ok(file_ts) =
                            DateTime::parse_from_str(&ts_str.replace('_', " "), "%Y-%m-%d %H-%M-%S")
                        {
                            let file_ts = file_ts.with_timezone(&Utc);
                            if file_ts <= timestamp
                                && (best_match.is_none() || best_match.as_ref().unwrap().0 < file_ts)
                                {
                                    best_match = Some((file_ts, path));
                                }
                        }
                    }
                }
            }
        }

        if let Some((_, path)) = best_match {
            let content = fs::read_to_string(&path)?;
            let manifest: TemporalManifest = serde_json::from_str(&content)?;
            Ok(Some(manifest))
        } else {
            Ok(None)
        }
    }

    /// Get the current version
    pub fn get_current_version(&self) -> Result<String> {
        // Get the latest sequence version
        if let Some((_, seq_version)) = self.sequence_timeline.iter().last() {
            Ok(seq_version.version.clone())
        } else {
            Ok("v0.0.0".to_string())
        }
    }

    /// Get sequence version at a specific time
    pub fn get_sequence_version_at(&self, timestamp: DateTime<Utc>) -> Result<Option<SequenceVersion>> {
        // Find the sequence version that was active at this time
        let version = self.sequence_timeline
            .range(..=timestamp)
            .next_back()
            .map(|(_, v)| v.clone());
        Ok(version)
    }

    /// Get taxonomy version at a specific time
    pub fn get_taxonomy_version_at(&self, timestamp: DateTime<Utc>) -> Result<Option<TaxonomyVersion>> {
        // Find the taxonomy version that was active at this time
        let version = self.taxonomy_timeline
            .range(..=timestamp)
            .next_back()
            .map(|(_, v)| v.clone());
        Ok(version)
    }

    /// Get all available bi-temporal coordinates
    pub fn get_all_coordinates(&self) -> Result<Vec<BiTemporalCoordinate>> {
        let mut coords = Vec::new();

        // Create coordinates for each combination of sequence and taxonomy versions
        for (seq_time, _) in &self.sequence_timeline {
            for (tax_time, _) in &self.taxonomy_timeline {
                coords.push(BiTemporalCoordinate {
                    sequence_time: *seq_time,
                    taxonomy_time: *tax_time,
                });
            }
        }

        Ok(coords)
    }

    /// Get a specific version by ID
    pub fn get_version(&self, version_id: &str) -> Result<Option<VersionInfo>> {
        // Search in sequence timeline
        for (timestamp, seq_version) in &self.sequence_timeline {
            if seq_version.version == version_id {
                let taxonomy_version = self
                    .taxonomy_timeline
                    .range(..=timestamp)
                    .next_back()
                    .map(|(_, v)| v.clone());

                let changes = self.detect_changes(&seq_version.version);

                return Ok(Some(VersionInfo {
                    version: seq_version.version.clone(),
                    timestamp: *timestamp,
                    version_type: "Sequence Update".to_string(),
                    sequence_root: seq_version.root_hash.to_string(),
                    taxonomy_root: taxonomy_version
                        .as_ref()
                        .map(|t| t.root_hash.to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
                    chunk_count: seq_version.chunk_count,
                    sequence_count: seq_version.sequence_count,
                    changes,
                    parent_version: self.get_parent_version(&seq_version.version),
                }));
            }
        }

        Ok(None)
    }

    fn detect_changes(&self, version: &str) -> Vec<String> {
        let mut changes = Vec::new();

        // Find the version in sequence timeline
        if let Some((_, seq_version)) = self
            .sequence_timeline
            .iter()
            .find(|(_, v)| v.version == version)
        {
            // Find previous version
            let prev_version = self.get_parent_version(version);

            if let Some((_, prev)) = prev_version.as_ref().and_then(|pv| {
                self.sequence_timeline
                    .iter()
                    .find(|(_, v)| v.version == *pv)
            }) {
                // Compare chunk counts
                if seq_version.chunk_count != prev.chunk_count {
                    let diff = seq_version.chunk_count as i32 - prev.chunk_count as i32;
                    if diff > 0 {
                        changes.push(format!("Added {} chunks", diff));
                    } else {
                        changes.push(format!("Removed {} chunks", -diff));
                    }
                }

                // Compare sequence counts
                if seq_version.sequence_count != prev.sequence_count {
                    let diff = seq_version.sequence_count as i32 - prev.sequence_count as i32;
                    if diff > 0 {
                        changes.push(format!("Added {} sequences", diff));
                    } else {
                        changes.push(format!("Removed {} sequences", -diff));
                    }
                }

                // Compare root hashes to detect content changes
                if seq_version.root_hash != prev.root_hash {
                    changes.push("Content modifications detected".to_string());
                }
            } else {
                // This is the first version
                changes.push(format!(
                    "Initial version with {} sequences",
                    seq_version.sequence_count
                ));
            }
        }

        // Check for taxonomy changes in the same timeframe
        if let Some(cross_ref) = self
            .cross_references
            .iter()
            .find(|cr| cr.sequence_version == version)
        {
            if let Some((_, tax_version)) = self
                .taxonomy_timeline
                .iter()
                .find(|(_, tv)| tv.version == cross_ref.taxonomy_version)
            {
                changes.push(format!("Using taxonomy: {}", tax_version.source));
            }
        }

        changes
    }

    fn get_parent_version(&self, version: &str) -> Option<String> {
        // Find the current version's index in the timeline
        let versions: Vec<_> = self.sequence_timeline.values().collect();
        let current_index = versions.iter().position(|v| v.version == version)?;

        // Parent is the previous version in the timeline
        if current_index > 0 {
            Some(versions[current_index - 1].version.clone())
        } else {
            None
        }
    }

    pub fn load(base_path: &Path) -> Result<Self> {
        let mut index = Self::new(base_path)?;

        // Load timelines
        let seq_timeline_file = index.base_path.join("sequence_timeline.json");
        if seq_timeline_file.exists() {
            let content = fs::read_to_string(&seq_timeline_file)?;
            index.sequence_timeline = serde_json::from_str(&content)?;
        }

        let tax_timeline_file = index.base_path.join("taxonomy_timeline.json");
        if tax_timeline_file.exists() {
            let content = fs::read_to_string(&tax_timeline_file)?;
            index.taxonomy_timeline = serde_json::from_str(&content)?;
        }

        // Load cross-references
        let cross_ref_file = index.base_path.join("cross_references.json");
        if cross_ref_file.exists() {
            let content = fs::read_to_string(&cross_ref_file)?;
            index.cross_references = serde_json::from_str(&content)?;
        }

        // Load header history
        let header_history_file = index.base_path.join("header_history.json");
        if header_history_file.exists() {
            let content = fs::read_to_string(&header_history_file)?;
            index.header_history = serde_json::from_str(&content)?;
        }

        Ok(index)
    }

    pub fn save(&self) -> Result<()> {
        // Save sequence timeline
        let seq_timeline_file = self.base_path.join("sequence_timeline.json");
        let content = serde_json::to_string_pretty(&self.sequence_timeline)?;
        fs::write(seq_timeline_file, content)?;

        // Save taxonomy timeline
        let tax_timeline_file = self.base_path.join("taxonomy_timeline.json");
        let content = serde_json::to_string_pretty(&self.taxonomy_timeline)?;
        fs::write(tax_timeline_file, content)?;

        // Save cross-references
        let cross_ref_file = self.base_path.join("cross_references.json");
        let content = serde_json::to_string_pretty(&self.cross_references)?;
        fs::write(cross_ref_file, content)?;

        // Save header history
        let header_history_file = self.base_path.join("header_history.json");
        let content = serde_json::to_string_pretty(&self.header_history)?;
        fs::write(header_history_file, content)?;

        Ok(())
    }

    /// Add a new sequence version
    pub fn add_sequence_version(
        &mut self,
        version: String,
        root_hash: MerkleHash,
        chunk_count: usize,
        sequence_count: usize,
    ) -> Result<()> {
        let timestamp = Utc::now();

        let seq_version = SequenceVersion {
            version,
            timestamp,
            root_hash,
            chunk_count,
            sequence_count,
        };

        self.sequence_timeline.insert(timestamp, seq_version);
        self.save()?;

        Ok(())
    }

    /// Add a new taxonomy version
    pub fn add_taxonomy_version(
        &mut self,
        version: String,
        root_hash: MerkleHash,
        taxa_count: usize,
        source: String,
    ) -> Result<()> {
        let timestamp = Utc::now();

        let tax_version = TaxonomyVersion {
            version,
            timestamp,
            root_hash,
            taxa_count,
            source,
            reclassifications: HashMap::new(),
            active_taxa: HashSet::new(),
        };

        self.taxonomy_timeline.insert(timestamp, tax_version);
        self.save()?;

        Ok(())
    }

    /// Create a cross-reference between sequence and taxonomy versions
    pub fn create_cross_reference(
        &mut self,
        sequence_version: String,
        taxonomy_version: String,
    ) -> Result<()> {
        // Find the versions
        let seq_v = self
            .sequence_timeline
            .values()
            .find(|v| v.version == sequence_version)
            .ok_or_else(|| anyhow::anyhow!("Sequence version not found"))?;

        let tax_v = self
            .taxonomy_timeline
            .values()
            .find(|v| v.version == taxonomy_version)
            .ok_or_else(|| anyhow::anyhow!("Taxonomy version not found"))?;

        // Create cross-hash
        let mut combined = Vec::new();
        combined.extend(seq_v.root_hash.as_bytes());
        combined.extend(tax_v.root_hash.as_bytes());
        let cross_hash = SHA256Hash::compute(&combined);

        // Invalidate previous cross-reference if any
        let now = Utc::now();
        for cross_ref in &mut self.cross_references {
            if cross_ref.validity_end.is_none() {
                cross_ref.validity_end = Some(now);
            }
        }

        // Add new cross-reference
        self.cross_references.push(TemporalCrossReference {
            sequence_version,
            taxonomy_version,
            created_at: now,
            validity_start: now,
            validity_end: None,
            cross_hash,
        });

        self.save()?;

        Ok(())
    }

    /// Track a header change for a sequence
    pub fn add_header_change(
        &mut self,
        sequence_id: String,
        old_header: String,
        new_header: String,
        change_type: HeaderChangeType,
    ) -> Result<()> {
        let timestamp = Utc::now();

        // Find or create history for this sequence
        let history = self
            .header_history
            .iter_mut()
            .find(|h| h.sequence_id == sequence_id);

        let change = TimestampedHeaderChange {
            timestamp,
            old_header,
            new_header,
            change_type,
        };

        if let Some(history) = history {
            history.header_changes.push(change);
        } else {
            self.header_history.push(SequenceMetadataHistory {
                sequence_id,
                header_changes: vec![change],
            });
        }

        self.save()?;
        Ok(())
    }

    /// Get header history for a specific sequence
    pub fn get_header_history(&self, sequence_id: &str) -> Option<&SequenceMetadataHistory> {
        self.header_history
            .iter()
            .find(|h| h.sequence_id == sequence_id)
    }

    /// Get the state at a specific point in time
    pub fn get_state_at(&self, timestamp: DateTime<Utc>) -> Result<TemporalState> {
        // Find sequence version
        let seq_version = self
            .sequence_timeline
            .range(..=timestamp)
            .next_back()
            .map(|(_, v)| v.clone());

        // Find taxonomy version
        let tax_version = self
            .taxonomy_timeline
            .range(..=timestamp)
            .next_back()
            .map(|(_, v)| v.clone());

        // Find applicable cross-reference
        let cross_ref = self
            .cross_references
            .iter()
            .find(|cr| {
                cr.validity_start <= timestamp
                    && cr.validity_end.is_none_or(|end| end > timestamp)
            })
            .cloned();

        // Try to load manifest for this timestamp
        let manifest = self.load_manifest_at(timestamp).ok().flatten();

        Ok(TemporalState {
            timestamp,
            sequence_version: seq_version,
            taxonomy_version: tax_version,
            cross_reference: cross_ref,
            manifest,
        })
    }

    /// Query sequences valid at a specific time with a specific taxonomy
    pub fn query_temporal(
        &self,
        sequence_time: Option<DateTime<Utc>>,
        taxonomy_time: Option<DateTime<Utc>>,
    ) -> Result<TemporalQuery> {
        let seq_time = sequence_time.unwrap_or_else(Utc::now);
        let tax_time = taxonomy_time.unwrap_or_else(Utc::now);

        let state = self.get_state_at(seq_time)?;

        Ok(TemporalQuery {
            sequence_state: state.sequence_version,
            taxonomy_state: self
                .taxonomy_timeline
                .range(..=tax_time)
                .next_back()
                .map(|(_, v)| v.clone()),
            query_time: Utc::now(),
        })
    }

    /// Get timeline of changes
    pub fn get_timeline(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Timeline {
        let mut events = Vec::new();

        // Sequence version events
        for (timestamp, version) in self.sequence_timeline.range(start..=end) {
            events.push(TimelineEvent {
                timestamp: *timestamp,
                event_type: TimelineEventType::SequenceUpdate,
                description: format!("Sequence database updated to version {}", version.version),
                details: serde_json::json!({
                    "version": version.version,
                    "chunk_count": version.chunk_count,
                    "sequence_count": version.sequence_count,
                }),
            });
        }

        // Taxonomy version events
        for (timestamp, version) in self.taxonomy_timeline.range(start..=end) {
            events.push(TimelineEvent {
                timestamp: *timestamp,
                event_type: TimelineEventType::TaxonomyUpdate,
                description: format!("Taxonomy updated to {}", version.source),
                details: serde_json::json!({
                    "version": version.version,
                    "source": version.source,
                    "taxa_count": version.taxa_count,
                }),
            });
        }

        // Cross-reference events
        for cross_ref in &self.cross_references {
            if cross_ref.created_at >= start && cross_ref.created_at <= end {
                events.push(TimelineEvent {
                    timestamp: cross_ref.created_at,
                    event_type: TimelineEventType::CrossReference,
                    description: format!(
                        "Linked sequences {} with taxonomy {}",
                        cross_ref.sequence_version, cross_ref.taxonomy_version
                    ),
                    details: serde_json::json!({
                        "sequence_version": cross_ref.sequence_version,
                        "taxonomy_version": cross_ref.taxonomy_version,
                    }),
                });
            }
        }

        // Sort by timestamp
        events.sort_by_key(|e| e.timestamp);

        Timeline { events }
    }

    /// Generate a temporal proof for a sequence at a point in time
    pub fn generate_temporal_proof(
        &self,
        sequence_id: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<TemporalProof> {
        let state = self.get_state_at(timestamp)?;

        if state.sequence_version.is_none() || state.taxonomy_version.is_none() {
            return Err(anyhow::anyhow!("No valid state at timestamp"));
        }

        let seq_v = state.sequence_version.unwrap();
        let tax_v = state.taxonomy_version.unwrap();

        // Create temporal link
        let temporal_link = CrossTimeHash {
            sequence_time: seq_v.timestamp,
            taxonomy_time: tax_v.timestamp,
            combined_hash: state
                .cross_reference
                .map(|cr| cr.cross_hash)
                .unwrap_or_else(|| {
                    let mut combined = Vec::new();
                    combined.extend(seq_v.root_hash.as_bytes());
                    combined.extend(tax_v.root_hash.as_bytes());
                    SHA256Hash::compute(&combined)
                }),
        };

        // Create placeholder proofs (would be actual Merkle proofs in full implementation)
        let sequence_proof = MerkleProof {
            leaf_hash: SHA256Hash::compute(sequence_id.as_bytes()),
            root_hash: seq_v.root_hash,
            path: Vec::new(), // Would be populated with actual proof path
        };

        let taxonomy_proof = MerkleProof {
            leaf_hash: SHA256Hash::compute(b"taxonomy"),
            root_hash: tax_v.root_hash,
            path: Vec::new(), // Would be populated with actual proof path
        };

        Ok(TemporalProof {
            sequence_proof,
            taxonomy_proof,
            temporal_link,
            timestamp,
            attestation: CryptographicSeal {
                timestamp,
                signature: Vec::new(), // Would be actual signature
                authority: "talaria-sequoia".to_string(),
            },
        })
    }

    /// Get manifest at a specific version
    pub fn get_manifest_at_version(&self, version: &str) -> Result<crate::Manifest> {
        // Try to load manifest from a version-specific file
        let manifest_path = self.base_path.join(format!("manifest_{}.json", version));
        if manifest_path.exists() {
            return crate::Manifest::load(&manifest_path);
        }

        // Otherwise, return current manifest with warning
        // TODO: Properly implement versioned manifest storage
        let current_manifest = crate::Manifest::load(&self.base_path)?;
        Ok(current_manifest)
    }

    /// List all temporal manifests
    pub fn list_all_manifests(&self) -> Result<Vec<ManifestRef>> {
        let mut manifests = Vec::new();

        // List all versions that have manifests
        for (timestamp, version) in &self.sequence_timeline {
            manifests.push(ManifestRef {
                timestamp: *timestamp,
                version: version.version.clone(),
                chunks: Vec::new(), // Would need to load manifest to get chunks
            });
        }

        Ok(manifests)
    }

    /// Rebuild the temporal index
    pub fn rebuild_index(&self) -> Result<()> {
        println!("Rebuilding temporal index...");

        // Would rebuild from stored versions
        let temporal_dir = self.base_path.join("temporal");
        if temporal_dir.exists() {
            // Scan for version files and rebuild timelines
            for entry in fs::read_dir(&temporal_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension() == Some(std::ffi::OsStr::new("json")) {
                    // Parse and re-index version file
                    println!("  Re-indexing: {}", path.display());
                }
            }
        }

        Ok(())
    }

    /// Prune versions before a certain date
    pub fn prune_before(&mut self, cutoff: DateTime<Utc>) -> Result<usize> {
        let mut pruned_count = 0;

        // Prune sequence versions
        let seq_to_remove: Vec<_> = self.sequence_timeline
            .range(..cutoff)
            .map(|(k, _)| *k)
            .collect();

        for timestamp in seq_to_remove {
            self.sequence_timeline.remove(&timestamp);
            pruned_count += 1;
        }

        // Prune taxonomy versions
        let tax_to_remove: Vec<_> = self.taxonomy_timeline
            .range(..cutoff)
            .map(|(k, _)| *k)
            .collect();

        for timestamp in tax_to_remove {
            self.taxonomy_timeline.remove(&timestamp);
            pruned_count += 1;
        }

        // Prune cross-references
        self.cross_references.retain(|cr| cr.created_at >= cutoff);

        // Save updated index
        self.save()?;

        Ok(pruned_count)
    }

    /// List versions before a certain date
    pub fn list_versions_before(&self, cutoff: DateTime<Utc>) -> Result<Vec<VersionRef>> {
        let mut versions = Vec::new();

        // Collect sequence versions
        for (timestamp, version) in self.sequence_timeline.range(..cutoff) {
            versions.push(VersionRef {
                timestamp: *timestamp,
                version_type: VersionType::Sequence,
                version: version.version.clone(),
            });
        }

        // Collect taxonomy versions
        for (timestamp, version) in self.taxonomy_timeline.range(..cutoff) {
            versions.push(VersionRef {
                timestamp: *timestamp,
                version_type: VersionType::Taxonomy,
                version: version.version.clone(),
            });
        }

        Ok(versions)
    }

    /// Get statistics about temporal storage
    pub fn get_statistics(&self) -> Result<TemporalStats> {
        let sequence_count = self.sequence_timeline.len();
        let taxonomy_count = self.taxonomy_timeline.len();
        let manifest_count = sequence_count; // Manifests correspond to sequence versions
        let cross_ref_count = self.cross_references.len();

        // Find oldest version
        let oldest_sequence = self.sequence_timeline.iter().next().map(|(t, _)| *t);
        let oldest_taxonomy = self.taxonomy_timeline.iter().next().map(|(t, _)| *t);

        let oldest = match (oldest_sequence, oldest_taxonomy) {
            (Some(s), Some(t)) => Some(if s < t { s } else { t }),
            (Some(s), None) => Some(s),
            (None, Some(t)) => Some(t),
            (None, None) => None,
        };

        let oldest_days = oldest
            .map(|t| (Utc::now() - t).num_days())
            .unwrap_or(0);

        Ok(TemporalStats {
            version_count: sequence_count + taxonomy_count,
            sequence_versions: sequence_count,
            taxonomy_versions: taxonomy_count,
            manifest_count,
            cross_ref_count,
            oldest_days: oldest_days as usize,
            total_size: 0, // Would need to calculate from files
        })
    }
}

// New structs for temporal management
#[derive(Debug, Clone)]
pub struct ManifestRef {
    pub timestamp: DateTime<Utc>,
    pub version: String,
    pub chunks: Vec<crate::SHA256Hash>,
}

#[derive(Debug, Clone)]
pub struct VersionRef {
    pub timestamp: DateTime<Utc>,
    pub version_type: VersionType,
    pub version: String,
}

#[derive(Debug, Clone)]
pub enum VersionType {
    Sequence,
    Taxonomy,
}

#[derive(Debug, Clone)]
pub struct TemporalStats {
    pub version_count: usize,
    pub sequence_versions: usize,
    pub taxonomy_versions: usize,
    pub manifest_count: usize,
    pub cross_ref_count: usize,
    pub oldest_days: usize,
    pub total_size: usize,
}

#[derive(Debug, Clone)]
pub struct TemporalState {
    pub timestamp: DateTime<Utc>,
    pub sequence_version: Option<SequenceVersion>,
    pub taxonomy_version: Option<TaxonomyVersion>,
    pub cross_reference: Option<TemporalCrossReference>,
    pub manifest: Option<TemporalManifest>,
}

#[derive(Debug)]
pub struct TemporalQuery {
    pub sequence_state: Option<SequenceVersion>,
    pub taxonomy_state: Option<TaxonomyVersion>,
    pub query_time: DateTime<Utc>,
}

#[derive(Debug)]
pub struct Timeline {
    pub events: Vec<TimelineEvent>,
}

#[derive(Debug)]
pub struct TimelineEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: TimelineEventType,
    pub description: String,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimelineEventType {
    SequenceUpdate,
    TaxonomyUpdate,
    CrossReference,
    Verification,
}

// BiTemporalCoordinate is now defined in types.rs
