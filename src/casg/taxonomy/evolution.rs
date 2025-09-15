/// Track taxonomy evolution and changes over time

use crate::casg::types::*;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub struct TaxonomyEvolution {
    base_path: PathBuf,
    versions: Vec<TaxonomySnapshot>,
}

#[derive(Debug, Clone)]
pub struct TaxonomySnapshot {
    pub version: String,
    pub date: DateTime<Utc>,
    pub taxa_count: usize,
    pub root_hash: MerkleHash,
    pub changes_from_previous: Option<TaxonomyChanges>,
}

impl TaxonomyEvolution {
    pub fn new(base_path: &Path) -> Result<Self> {
        let evolution_dir = base_path.join("evolution");
        fs::create_dir_all(&evolution_dir)?;

        Ok(Self {
            base_path: evolution_dir,
            versions: Vec::new(),
        })
    }

    /// Load evolution history
    pub fn load(&mut self) -> Result<()> {
        let history_file = self.base_path.join("history.json");
        if history_file.exists() {
            let content = fs::read_to_string(&history_file)?;
            self.versions = serde_json::from_str(&content)?;
        }
        Ok(())
    }

    /// Save evolution history
    pub fn save(&self) -> Result<()> {
        let history_file = self.base_path.join("history.json");
        let content = serde_json::to_string_pretty(&self.versions)?;
        fs::write(history_file, content)?;
        Ok(())
    }

    /// Add a new taxonomy version
    pub fn add_version(
        &mut self,
        version: String,
        taxonomy_data: &TaxonomyData,
    ) -> Result<()> {
        let changes = if let Some(last) = self.versions.last() {
            Some(self.compute_changes_from_data(&last.version, &version, taxonomy_data)?)
        } else {
            None
        };

        let snapshot = TaxonomySnapshot {
            version: version.clone(),
            date: Utc::now(),
            taxa_count: taxonomy_data.taxa.len(),
            root_hash: taxonomy_data.compute_hash(),
            changes_from_previous: changes,
        };

        self.versions.push(snapshot);
        self.save()?;

        // Store the actual taxonomy data
        let version_file = self.base_path.join(format!("{}.json", version));
        let content = serde_json::to_string_pretty(taxonomy_data)?;
        fs::write(version_file, content)?;

        Ok(())
    }

    /// Compute changes between two versions
    pub fn compute_changes(
        &self,
        old_version: &str,
        new_version: &str,
    ) -> Result<TaxonomyChanges> {
        let _old_data = self.load_version(old_version)?;
        let new_data = self.load_version(new_version)?;

        // Compare old and new data
        self.compute_changes_from_data(old_version, new_version, &new_data)
    }

    fn compute_changes_from_data(
        &self,
        _old_version: &str,
        _new_version: &str,
        new_data: &TaxonomyData,
    ) -> Result<TaxonomyChanges> {
        let old_data = if let Some(last) = self.versions.last() {
            self.load_version(&last.version)?
        } else {
            // No previous version
            return Ok(TaxonomyChanges {
                reclassifications: Vec::new(),
                new_taxa: new_data.taxa.keys().cloned().collect(),
                deprecated_taxa: Vec::new(),
                merged_taxa: Vec::new(),
            });
        };

        let mut reclassifications = Vec::new();
        let mut new_taxa = Vec::new();
        let mut deprecated_taxa = Vec::new();
        let mut merged_taxa = Vec::new();

        // Find new taxa
        for taxon_id in new_data.taxa.keys() {
            if !old_data.taxa.contains_key(taxon_id) {
                new_taxa.push(taxon_id.clone());
            }
        }

        // Find deprecated taxa
        for taxon_id in old_data.taxa.keys() {
            if !new_data.taxa.contains_key(taxon_id) {
                // Check if it was merged
                if let Some(merge_target) = new_data.merges.get(taxon_id) {
                    merged_taxa.push((taxon_id.clone(), merge_target.clone()));
                } else {
                    deprecated_taxa.push(taxon_id.clone());
                }
            }
        }

        // Find reclassifications
        for (taxon_id, new_info) in &new_data.taxa {
            if let Some(old_info) = old_data.taxa.get(taxon_id) {
                if old_info.parent_id != new_info.parent_id {
                    reclassifications.push(Reclassification {
                        taxon_id: taxon_id.clone(),
                        old_parent: old_info.parent_id.clone().unwrap_or(TaxonId(0)),
                        new_parent: new_info.parent_id.clone().unwrap_or(TaxonId(0)),
                        reason: self.infer_reclassification_reason(old_info, new_info),
                    });
                }
            }
        }

        Ok(TaxonomyChanges {
            reclassifications,
            new_taxa,
            deprecated_taxa,
            merged_taxa,
        })
    }

    fn infer_reclassification_reason(&self, old: &TaxonInfo, new: &TaxonInfo) -> String {
        if old.rank != new.rank {
            format!("Rank change: {} -> {}", old.rank, new.rank)
        } else if old.name != new.name {
            format!("Name change: {} -> {}", old.name, new.name)
        } else {
            "Taxonomic revision".to_string()
        }
    }

    fn load_version(&self, version: &str) -> Result<TaxonomyData> {
        let version_file = self.base_path.join(format!("{}.json", version));
        let content = fs::read_to_string(&version_file)
            .with_context(|| format!("Failed to load version {}", version))?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Track the evolution of a specific taxon
    pub fn track_taxon(&self, taxon_id: &TaxonId) -> Result<TaxonEvolution> {
        let mut history = Vec::new();

        for snapshot in &self.versions {
            let data = self.load_version(&snapshot.version)?;
            if let Some(info) = data.taxa.get(taxon_id) {
                history.push(TaxonHistoryEntry {
                    version: snapshot.version.clone(),
                    date: snapshot.date,
                    info: info.clone(),
                    status: TaxonStatus::Active,
                });
            } else if let Some(merge_target) = data.merges.get(taxon_id) {
                history.push(TaxonHistoryEntry {
                    version: snapshot.version.clone(),
                    date: snapshot.date,
                    info: TaxonInfo {
                        taxon_id: taxon_id.clone(),
                        parent_id: None,
                        name: format!("Merged into {}", merge_target),
                        rank: "merged".to_string(),
                    },
                    status: TaxonStatus::Merged(merge_target.clone()),
                });
            }
        }

        Ok(TaxonEvolution {
            taxon_id: taxon_id.clone(),
            history,
        })
    }

    /// Generate a timeline of major changes
    pub fn generate_timeline(&self) -> Timeline {
        let mut events = Vec::new();

        for snapshot in &self.versions {
            if let Some(ref changes) = snapshot.changes_from_previous {
                // Major reclassifications
                for reclassification in &changes.reclassifications {
                    events.push(TimelineEvent {
                        date: snapshot.date,
                        version: snapshot.version.clone(),
                        event_type: EventType::Reclassification,
                        description: format!(
                            "Taxon {} reclassified: {}",
                            reclassification.taxon_id,
                            reclassification.reason
                        ),
                        affected_taxa: vec![reclassification.taxon_id.clone()],
                    });
                }

                // New discoveries
                if !changes.new_taxa.is_empty() {
                    events.push(TimelineEvent {
                        date: snapshot.date,
                        version: snapshot.version.clone(),
                        event_type: EventType::NewTaxa,
                        description: format!("{} new taxa added", changes.new_taxa.len()),
                        affected_taxa: changes.new_taxa.clone(),
                    });
                }

                // Merges
                for (old, new) in &changes.merged_taxa {
                    events.push(TimelineEvent {
                        date: snapshot.date,
                        version: snapshot.version.clone(),
                        event_type: EventType::Merge,
                        description: format!("Taxon {} merged into {}", old, new),
                        affected_taxa: vec![old.clone(), new.clone()],
                    });
                }
            }
        }

        Timeline { events }
    }

    /// Find all taxa affected by changes between versions
    pub fn find_affected_taxa(
        &self,
        old_version: &str,
        new_version: &str,
    ) -> Result<HashSet<TaxonId>> {
        let changes = self.compute_changes(old_version, new_version)?;
        let mut affected = HashSet::new();

        for reclassification in changes.reclassifications {
            affected.insert(reclassification.taxon_id);
        }

        affected.extend(changes.new_taxa);
        affected.extend(changes.deprecated_taxa);

        for (old, new) in changes.merged_taxa {
            affected.insert(old);
            affected.insert(new);
        }

        Ok(affected)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaxonomyData {
    pub taxa: HashMap<TaxonId, TaxonInfo>,
    pub merges: HashMap<TaxonId, TaxonId>,
}

impl TaxonomyData {
    pub fn compute_hash(&self) -> MerkleHash {
        let serialized = serde_json::to_vec(self).unwrap();
        SHA256Hash::compute(&serialized)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaxonInfo {
    pub taxon_id: TaxonId,
    pub parent_id: Option<TaxonId>,
    pub name: String,
    pub rank: String,
}

#[derive(Debug)]
pub struct TaxonEvolution {
    pub taxon_id: TaxonId,
    pub history: Vec<TaxonHistoryEntry>,
}

#[derive(Debug)]
pub struct TaxonHistoryEntry {
    pub version: String,
    pub date: DateTime<Utc>,
    pub info: TaxonInfo,
    pub status: TaxonStatus,
}

#[derive(Debug)]
pub enum TaxonStatus {
    Active,
    Deprecated,
    Merged(TaxonId),
}

#[derive(Debug)]
pub struct Timeline {
    pub events: Vec<TimelineEvent>,
}

#[derive(Debug)]
pub struct TimelineEvent {
    pub date: DateTime<Utc>,
    pub version: String,
    pub event_type: EventType,
    pub description: String,
    pub affected_taxa: Vec<TaxonId>,
}

#[derive(Debug)]
pub enum EventType {
    Reclassification,
    NewTaxa,
    Deprecation,
    Merge,
}

// Implement serialization for TaxonomySnapshot
impl serde::Serialize for TaxonomySnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("TaxonomySnapshot", 5)?;
        state.serialize_field("version", &self.version)?;
        state.serialize_field("date", &self.date)?;
        state.serialize_field("taxa_count", &self.taxa_count)?;
        state.serialize_field("root_hash", &self.root_hash)?;
        state.serialize_field("changes_from_previous", &self.changes_from_previous)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for TaxonomySnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Helper {
            version: String,
            date: DateTime<Utc>,
            taxa_count: usize,
            root_hash: MerkleHash,
            changes_from_previous: Option<TaxonomyChanges>,
        }

        let helper = Helper::deserialize(deserializer)?;
        Ok(TaxonomySnapshot {
            version: helper.version,
            date: helper.date,
            taxa_count: helper.taxa_count,
            root_hash: helper.root_hash,
            changes_from_previous: helper.changes_from_previous,
        })
    }
}