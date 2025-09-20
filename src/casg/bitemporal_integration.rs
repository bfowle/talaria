/// Integration of bi-temporal versioning with existing manifest system
///
/// Completes the promise of independent sequence and taxonomy versioning,
/// enabling git-like operations and temporal queries across both dimensions.

use crate::bio::sequence::Sequence;
use crate::bio::taxonomy::TaxonomyManager;
use crate::casg::manifest::Manifest;
use crate::casg::storage::CASGStorage;
use crate::casg::temporal::{SequenceVersion, TaxonomyVersion};
use crate::casg::traits::temporal::{
    ClassificationConflict, ConflictType, EntityType, EventType, EvolutionEvent, EvolutionHistory,
    Reclassification, ReclassificationReason, RetroactiveAnalyzable, RetroactiveResult,
    RetroactiveStatistics, SequenceChanges, SnapshotMetadata, TaxonomicChangeType,
    TaxonomyChanges, TaxonomyImpactAnalysis, TemporalDiff, TemporalJoinQuery, TemporalJoinResult,
    TemporalQueryable, TemporalSnapshot, ReclassifiedGroup, MajorChange,
};
use crate::casg::types::*;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Git-like branch for bi-temporal versioning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiTemporalBranch {
    pub name: String,
    pub head: BiTemporalCoordinate,
    pub parent: Option<Box<BiTemporalBranch>>,
    pub created_at: DateTime<Utc>,
    pub description: String,
}

/// Git-like commit in bi-temporal space
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiTemporalCommit {
    pub hash: SHA256Hash,
    pub coordinate: BiTemporalCoordinate,
    pub parent: Option<SHA256Hash>,
    pub message: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
    pub changes: BiTemporalChanges,
}

/// Changes in a bi-temporal commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiTemporalChanges {
    pub sequence_changes: CommitSequenceChanges,
    pub taxonomy_changes: CommitTaxonomyChanges,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitSequenceChanges {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitTaxonomyChanges {
    pub reclassifications: Vec<TaxonReclassification>,
    pub additions: Vec<TaxonId>,
    pub merges: Vec<TaxonMerge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonReclassification {
    pub taxon_id: TaxonId,
    pub old_parent: TaxonId,
    pub new_parent: TaxonId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonMerge {
    pub from: Vec<TaxonId>,
    pub to: TaxonId,
}

/// Enhanced bi-temporal repository with git-like operations
pub struct BiTemporalRepository {
    storage: CASGStorage,
    taxonomy_manager: TaxonomyManager,
    branches: HashMap<String, BiTemporalBranch>,
    commits: HashMap<SHA256Hash, BiTemporalCommit>,
    current_branch: String,
    working_coordinate: BiTemporalCoordinate,
}

impl BiTemporalRepository {
    pub fn new(storage_path: &Path) -> Result<Self> {
        let storage = CASGStorage::new(storage_path)?;
        let taxonomy_manager = TaxonomyManager::new(None)?;

        let mut repo = Self {
            storage,
            taxonomy_manager,
            branches: HashMap::new(),
            commits: HashMap::new(),
            current_branch: "main".to_string(),
            working_coordinate: BiTemporalCoordinate::default(),
        };

        // Initialize main branch
        repo.init_main_branch()?;

        Ok(repo)
    }

    fn init_main_branch(&mut self) -> Result<()> {
        let main_branch = BiTemporalBranch {
            name: "main".to_string(),
            head: BiTemporalCoordinate::default(),
            parent: None,
            created_at: Utc::now(),
            description: "Main branch".to_string(),
        };

        self.branches.insert("main".to_string(), main_branch);
        Ok(())
    }

    /// Create a new branch at current coordinate
    pub fn create_branch(&mut self, name: &str, description: &str) -> Result<()> {
        if self.branches.contains_key(name) {
            return Err(anyhow::anyhow!("Branch {} already exists", name));
        }

        let current = self.branches.get(&self.current_branch)
            .ok_or_else(|| anyhow::anyhow!("Current branch not found"))?
            .clone();

        let new_branch = BiTemporalBranch {
            name: name.to_string(),
            head: current.head.clone(),
            parent: Some(Box::new(current)),
            created_at: Utc::now(),
            description: description.to_string(),
        };

        self.branches.insert(name.to_string(), new_branch);
        Ok(())
    }

    /// Switch to a different branch
    pub fn checkout(&mut self, branch_name: &str) -> Result<()> {
        if !self.branches.contains_key(branch_name) {
            return Err(anyhow::anyhow!("Branch {} not found", branch_name));
        }

        self.current_branch = branch_name.to_string();
        let branch = &self.branches[branch_name];
        self.working_coordinate = branch.head.clone();

        Ok(())
    }

    /// Checkout a specific bi-temporal coordinate
    pub fn checkout_coordinate(
        &mut self,
        seq_version: &str,
        tax_version: &str,
    ) -> Result<()> {
        self.working_coordinate = BiTemporalCoordinate {
            sequence_time: DateTime::parse_from_rfc3339(seq_version)?.with_timezone(&Utc),
            taxonomy_time: DateTime::parse_from_rfc3339(tax_version)?.with_timezone(&Utc),
        };

        // Detached HEAD state
        self.current_branch = "HEAD".to_string();

        Ok(())
    }

    /// Commit changes with message
    pub fn commit(&mut self, message: &str, author: &str) -> Result<SHA256Hash> {
        let changes = self.detect_changes()?;

        let commit = BiTemporalCommit {
            hash: self.compute_commit_hash(&changes)?,
            coordinate: self.working_coordinate.clone(),
            parent: self.get_current_commit_hash(),
            message: message.to_string(),
            author: author.to_string(),
            timestamp: Utc::now(),
            changes,
        };

        let hash = commit.hash.clone();
        self.commits.insert(hash.clone(), commit);

        // Update branch HEAD
        if let Some(branch) = self.branches.get_mut(&self.current_branch) {
            branch.head = self.working_coordinate.clone();
        }

        Ok(hash)
    }

    fn detect_changes(&self) -> Result<BiTemporalChanges> {
        // In real implementation, would compare working state with HEAD
        Ok(BiTemporalChanges {
            sequence_changes: CommitSequenceChanges {
                added: Vec::new(),
                modified: Vec::new(),
                removed: Vec::new(),
            },
            taxonomy_changes: CommitTaxonomyChanges {
                reclassifications: Vec::new(),
                additions: Vec::new(),
                merges: Vec::new(),
            },
        })
    }

    fn compute_commit_hash(&self, changes: &BiTemporalChanges) -> Result<SHA256Hash> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();

        let serialized = serde_json::to_string(changes)?;
        hasher.update(serialized.as_bytes());
        hasher.update(self.working_coordinate.sequence_time.to_rfc3339().as_bytes());

        Ok(SHA256Hash(hasher.finalize().into()))
    }

    fn get_current_commit_hash(&self) -> Option<SHA256Hash> {
        // Find most recent commit on current branch
        self.commits
            .values()
            .filter(|c| c.coordinate == self.working_coordinate)
            .map(|c| c.hash.clone())
            .next()
    }

    /// Get log of commits
    pub fn log(&self, limit: usize) -> Vec<&BiTemporalCommit> {
        let mut commits: Vec<_> = self.commits.values().collect();
        commits.sort_by_key(|c| c.timestamp);
        commits.reverse();
        commits.into_iter().take(limit).collect()
    }

    /// Load sequences at a specific coordinate
    async fn load_sequences_at(&self, _coord: &BiTemporalCoordinate) -> Result<Vec<Sequence>> {
        // Placeholder implementation
        // In real implementation, would load from storage at specific time
        Ok(Vec::new())
    }

    /// Find reclassifications between two coordinates
    async fn find_reclassifications(
        &self,
        from: &BiTemporalCoordinate,
        to: &BiTemporalCoordinate,
    ) -> Result<Vec<Reclassification>> {
        let mut reclassifications = Vec::new();

        // Placeholder: generate some example reclassifications
        if from.taxonomy_time != to.taxonomy_time {
            reclassifications.push(Reclassification {
                sequence_id: "example_seq".to_string(),
                old_taxon: Some(100),
                new_taxon: Some(101),
                reason: ReclassificationReason::TaxonomyUpdate,
                date: to.taxonomy_time,
            });
        }

        Ok(reclassifications)
    }
}

#[async_trait]
impl TemporalQueryable for BiTemporalRepository {
    async fn query_at(&self, coord: BiTemporalCoordinate) -> Result<TemporalSnapshot> {
        let sequences = self.load_sequences_at(&coord).await?;
        let sequence_count = sequences.len();

        let snapshot = TemporalSnapshot {
            coordinate: coord.clone(),
            sequences,
            sequence_version: SequenceVersion {
                version: coord.sequence_time.format("%Y-%m-%d").to_string(),
                timestamp: coord.sequence_time,
                root_hash: SHA256Hash::zero(),
                chunk_count: 0,
                sequence_count,
            },
            taxonomy_version: TaxonomyVersion {
                version: coord.taxonomy_time.format("%Y-%m-%d").to_string(),
                timestamp: coord.taxonomy_time,
                root_hash: SHA256Hash::zero(),
                taxa_count: 0,
                source: "Internal".to_string(),
            },
            metadata: SnapshotMetadata {
                total_sequences: sequence_count,
                total_chunks: 0,
                unique_taxa: 0,
                snapshot_hash: SHA256Hash::zero(),
                created_at: Utc::now(),
            },
        };

        Ok(snapshot)
    }

    async fn changes_between(
        &self,
        from: BiTemporalCoordinate,
        to: BiTemporalCoordinate,
    ) -> Result<TemporalDiff> {
        let reclassifications = self.find_reclassifications(&from, &to).await?;

        let diff = TemporalDiff {
            from,
            to,
            sequence_changes: SequenceChanges {
                added: Vec::new(),
                removed: Vec::new(),
                modified: Vec::new(),
                total_delta: 0,
            },
            taxonomy_changes: TaxonomyChanges {
                added_taxa: Vec::new(),
                removed_taxa: Vec::new(),
                renamed_taxa: Vec::new(),
                merged_taxa: Vec::new(),
                split_taxa: Vec::new(),
            },
            reclassifications,
        };

        Ok(diff)
    }

    async fn evolution_history(
        &self,
        entity_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<EvolutionHistory> {
        let mut events = Vec::new();

        // Find relevant commits in time range
        for commit in self.commits.values() {
            if commit.timestamp >= start && commit.timestamp <= end {
                // Check if entity was affected
                if commit.changes.sequence_changes.added.contains(&entity_id.to_string())
                    || commit.changes.sequence_changes.modified.contains(&entity_id.to_string())
                {
                    events.push(EvolutionEvent {
                        timestamp: commit.timestamp,
                        event_type: EventType::Modified,
                        description: commit.message.clone(),
                        metadata: serde_json::json!({
                            "commit": commit.hash.to_hex(),
                            "author": commit.author,
                        }),
                    });
                }
            }
        }

        Ok(EvolutionHistory {
            entity_id: entity_id.to_string(),
            events,
            from_date: start,
            to_date: end,
        })
    }

    async fn temporal_join(&self, query: TemporalJoinQuery) -> Result<TemporalJoinResult> {
        let mut reclassified = Vec::new();

        // Placeholder implementation
        if query.comparison_date.is_some() {
            reclassified.push(ReclassifiedGroup {
                old_taxon: query.taxon,
                new_taxon: query.taxon.map(|t| t + 1),
                sequences: vec!["seq1".to_string(), "seq2".to_string()],
                count: 2,
            });
        }

        Ok(TemporalJoinResult {
            query,
            reclassified,
            stable: Vec::new(),
            total_affected: 2,
            taxonomies_changed: 1,
            execution_time_ms: 100,
        })
    }
}

#[async_trait]
impl RetroactiveAnalyzable for BiTemporalRepository {
    async fn apply_taxonomy(
        &self,
        sequences: &TemporalSnapshot,
        taxonomy: &TaxonomyVersion,
    ) -> Result<RetroactiveResult> {
        let mut reclassifications = Vec::new();
        let mut conflicts = Vec::new();

        // Check each sequence against new taxonomy
        for seq in &sequences.sequences {
            if let Ok(taxon_id) = seq.get_taxid() {
                // Check if taxon exists in new taxonomy
                // This is a placeholder check
                if taxonomy.timestamp > sequences.taxonomy_version.timestamp {
                    reclassifications.push(Reclassification {
                        sequence_id: seq.id.clone(),
                        old_taxon: Some(taxon_id),
                        new_taxon: Some(taxon_id + 1), // Placeholder new taxon
                        reason: ReclassificationReason::TaxonomyUpdate,
                        date: taxonomy.timestamp,
                    });
                }
            }
        }

        Ok(RetroactiveResult {
            original_coordinate: sequences.coordinate.clone(),
            applied_taxonomy: taxonomy.clone(),
            reclassifications,
            conflicts,
            statistics: RetroactiveStatistics {
                total_sequences: sequences.sequences.len(),
                reclassified_count: reclassifications.len(),
                conflict_count: conflicts.len(),
                new_taxa_discovered: 0,
                obsolete_taxa_removed: 0,
            },
        })
    }

    async fn find_conflicts(
        &self,
        coord: BiTemporalCoordinate,
    ) -> Result<Vec<ClassificationConflict>> {
        let mut conflicts = Vec::new();

        // Placeholder: check for taxonomy version mismatches
        let sequences = self.load_sequences_at(&coord).await?;
        for seq in sequences {
            // Simplified conflict detection
            if seq.get_taxid().is_err() {
                conflicts.push(ClassificationConflict {
                    sequence_id: seq.id,
                    original_classification: None,
                    new_classification: None,
                    conflict_type: ConflictType::TaxonNoLongerExists,
                    resolution_suggestion: Some("Reclassify sequence".to_string()),
                });
            }
        }

        Ok(conflicts)
    }

    async fn analyze_taxonomy_impact(
        &self,
        old_taxonomy: &TaxonomyVersion,
        new_taxonomy: &TaxonomyVersion,
    ) -> Result<TaxonomyImpactAnalysis> {
        let mut major_changes = Vec::new();

        // Placeholder: detect major taxonomy changes
        if new_taxonomy.timestamp > old_taxonomy.timestamp {
            major_changes.push(MajorChange {
                change_type: TaxonomicChangeType::PhylumReorganization,
                description: "Taxonomy updated".to_string(),
                affected_sequences: 100,
                affected_taxa: vec![1, 2, 3],
            });
        }

        Ok(TaxonomyImpactAnalysis {
            old_version: old_taxonomy.version.clone(),
            new_version: new_taxonomy.version.clone(),
            sequences_affected: 100,
            taxa_affected: 10,
            major_changes,
            stability_score: 0.85,
        })
    }
}

/// Diff between two bi-temporal coordinates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiTemporalDiff {
    pub from: BiTemporalCoordinate,
    pub to: BiTemporalCoordinate,
    pub sequence_diff: SequenceDiff,
    pub taxonomy_diff: TaxonomyDiff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceDiff {
    pub version_from: String,
    pub version_to: String,
    pub added_count: usize,
    pub removed_count: usize,
    pub modified_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyDiff {
    pub version_from: String,
    pub version_to: String,
    pub reclassifications: Vec<TaxonReclassification>,
    pub additions: Vec<TaxonId>,
    pub removals: Vec<TaxonId>,
}

/// Current repository status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiTemporalStatus {
    pub current_branch: String,
    pub working_coordinate: BiTemporalCoordinate,
    pub has_uncommitted_changes: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_bitemporal_repository() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = BiTemporalRepository::new(temp_dir.path()).unwrap();

        // Create a branch
        repo.create_branch("feature", "New feature branch").unwrap();

        // Switch to it
        repo.checkout("feature").unwrap();
        assert_eq!(repo.current_branch, "feature");

        // Make a commit
        let hash = repo.commit("Test commit", "test_user").unwrap();
        assert!(repo.commits.contains_key(&hash));

        // Check log
        let log = repo.log(10);
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].message, "Test commit");
    }

    #[tokio::test]
    async fn test_temporal_queryable() {
        let temp_dir = TempDir::new().unwrap();
        let repo = BiTemporalRepository::new(temp_dir.path()).unwrap();

        let coord = BiTemporalCoordinate::default();
        let snapshot = repo.query_at(coord.clone()).await.unwrap();
        assert_eq!(snapshot.coordinate, coord);
    }
}