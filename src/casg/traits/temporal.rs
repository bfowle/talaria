use crate::bio::sequence::Sequence;
use crate::casg::temporal::{SequenceVersion, TaxonomyVersion};
use crate::casg::types::{BiTemporalCoordinate, SHA256Hash, TaxonId};
/// Temporal query traits for bi-temporal CASG operations
///
/// These traits enable powerful temporal queries like historical reproduction,
/// retroactive analysis, and classification evolution tracking.
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Core trait for temporal queries
#[async_trait]
pub trait TemporalQueryable: Send + Sync {
    /// Query state at a specific bi-temporal coordinate
    async fn query_at(&self, coord: BiTemporalCoordinate) -> Result<TemporalSnapshot>;

    /// Find all changes between two temporal points
    async fn changes_between(
        &self,
        from: BiTemporalCoordinate,
        to: BiTemporalCoordinate,
    ) -> Result<TemporalDiff>;

    /// Get evolution history for a specific entity
    async fn evolution_history(
        &self,
        entity_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<EvolutionHistory>;

    /// Perform a temporal join to find reclassified sequences
    async fn temporal_join(&self, query: TemporalJoinQuery) -> Result<TemporalJoinResult>;
}

/// Trait for retroactive analysis capabilities
#[async_trait]
pub trait RetroactiveAnalyzable: Send + Sync {
    /// Apply different taxonomy to sequences
    async fn apply_taxonomy(
        &self,
        sequences: &TemporalSnapshot,
        taxonomy: &TaxonomyVersion,
    ) -> Result<RetroactiveResult>;

    /// Find classification conflicts between different time points
    async fn find_conflicts(
        &self,
        coord: BiTemporalCoordinate,
    ) -> Result<Vec<ClassificationConflict>>;

    /// Analyze impact of taxonomy changes
    async fn analyze_taxonomy_impact(
        &self,
        old_taxonomy: &TaxonomyVersion,
        new_taxonomy: &TaxonomyVersion,
    ) -> Result<TaxonomyImpactAnalysis>;
}

/// A snapshot of the system at a specific bi-temporal coordinate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalSnapshot {
    pub coordinate: BiTemporalCoordinate,
    pub sequences: Vec<Sequence>,
    pub sequence_version: SequenceVersion,
    pub taxonomy_version: TaxonomyVersion,
    pub metadata: SnapshotMetadata,
}

/// Metadata for a temporal snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub total_sequences: usize,
    pub total_chunks: usize,
    pub unique_taxa: usize,
    pub snapshot_hash: SHA256Hash,
    pub created_at: DateTime<Utc>,
}

/// Differences between two temporal states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDiff {
    pub from: BiTemporalCoordinate,
    pub to: BiTemporalCoordinate,
    pub sequence_changes: SequenceChanges,
    pub taxonomy_changes: TaxonomyChanges,
    pub reclassifications: Vec<Reclassification>,
}

/// Sequence changes between temporal points
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceChanges {
    pub added: Vec<String>,    // Sequence IDs
    pub removed: Vec<String>,  // Sequence IDs
    pub modified: Vec<String>, // Sequence IDs
    pub total_delta: i64,      // Net change in sequence count
}

/// Taxonomy changes between temporal points
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyChanges {
    pub added_taxa: Vec<TaxonId>,
    pub removed_taxa: Vec<TaxonId>,
    pub renamed_taxa: Vec<(TaxonId, String, String)>, // ID, old name, new name
    pub merged_taxa: Vec<(Vec<TaxonId>, TaxonId)>,    // Merged IDs -> new ID
    pub split_taxa: Vec<(TaxonId, Vec<TaxonId>)>,     // Old ID -> new IDs
}

/// A reclassification event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reclassification {
    pub sequence_id: String,
    pub old_taxon: Option<TaxonId>,
    pub new_taxon: Option<TaxonId>,
    pub reason: ReclassificationReason,
    pub date: DateTime<Utc>,
}

/// Reason for a reclassification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReclassificationReason {
    TaxonomyUpdate,
    SequenceCorrection,
    TaxonMerge,
    TaxonSplit,
    ManualOverride,
    AlgorithmicReclassification,
}

/// Evolution history for an entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionHistory {
    pub entity_id: String,
    pub events: Vec<EvolutionEvent>,
    pub from_date: DateTime<Utc>,
    pub to_date: DateTime<Utc>,
}

/// Type of entity being tracked
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntityType {
    Sequence,
    Taxon,
    Protein,
    Gene,
}

/// An event in evolution history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: EventType,
    pub description: String,
    pub metadata: serde_json::Value,
}

/// Type of evolution event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    Created,
    Modified,
    Reclassified,
    Renamed,
    Merged,
    Split,
    Deleted,
}

/// A point on the timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelinePoint {
    pub date: DateTime<Utc>,
    pub state: serde_json::Value,
}

/// Query for a temporal snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotQuery {
    pub coordinate: BiTemporalCoordinate,
    pub taxon_filter: Option<Vec<TaxonId>>,
}

/// Query for temporal evolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionQuery {
    pub entity_id: String,
    pub from_date: DateTime<Utc>,
    pub to_date: DateTime<Utc>,
}

/// Query for temporal joins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinQuery {
    pub reference_date: DateTime<Utc>,
    pub comparison_date: Option<DateTime<Utc>>,
    pub taxon_filter: Option<Vec<TaxonId>>,
    pub find_reclassified: bool,
}

/// Query for temporal diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffQuery {
    pub from: BiTemporalCoordinate,
    pub to: BiTemporalCoordinate,
    pub taxon_filter: Option<Vec<TaxonId>>,
}

/// Query for temporal joins (legacy)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalJoinQuery {
    pub taxon: Option<TaxonId>,
    pub taxon_name: Option<String>,
    pub reference_date: DateTime<Utc>,
    pub comparison_date: Option<DateTime<Utc>>,
    pub include_descendants: bool,
}

/// Result of a temporal join
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalJoinResult {
    pub query: TemporalJoinQuery,
    pub reclassified: Vec<ReclassifiedGroup>,
    pub stable: Vec<String>, // Sequence IDs that didn't change
    pub total_affected: usize,
    pub taxonomies_changed: usize,
    pub execution_time_ms: u64,
}

/// A group of reclassified sequences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReclassifiedGroup {
    pub old_taxon: Option<TaxonId>,
    pub new_taxon: Option<TaxonId>,
    pub sequences: Vec<String>,
    pub count: usize,
}

/// Result of retroactive analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetroactiveResult {
    pub original_coordinate: BiTemporalCoordinate,
    pub applied_taxonomy: TaxonomyVersion,
    pub reclassifications: Vec<Reclassification>,
    pub conflicts: Vec<ClassificationConflict>,
    pub statistics: RetroactiveStatistics,
}

/// Statistics for retroactive analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetroactiveStatistics {
    pub total_sequences: usize,
    pub reclassified_count: usize,
    pub conflict_count: usize,
    pub new_taxa_discovered: usize,
    pub obsolete_taxa_removed: usize,
}

/// A classification conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationConflict {
    pub sequence_id: String,
    pub original_classification: Option<TaxonId>,
    pub new_classification: Option<TaxonId>,
    pub conflict_type: ConflictType,
    pub resolution_suggestion: Option<String>,
}

/// Type of classification conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictType {
    TaxonNoLongerExists,
    TaxonMerged,
    TaxonSplit,
    AmbiguousMapping,
    InconsistentHierarchy,
}

/// Analysis of taxonomy change impact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyImpactAnalysis {
    pub old_version: String,
    pub new_version: String,
    pub sequences_affected: usize,
    pub taxa_affected: usize,
    pub major_changes: Vec<MajorChange>,
    pub stability_score: f64, // 0.0 to 1.0
}

/// A major taxonomic change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MajorChange {
    pub change_type: TaxonomicChangeType,
    pub description: String,
    pub affected_sequences: usize,
    pub affected_taxa: Vec<TaxonId>,
}

/// Type of taxonomic change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaxonomicChangeType {
    PhylumReorganization,
    GenusReclassification,
    SpeciesMerge,
    SpeciesSplit,
    NewTaxonDiscovery,
    TaxonObsoleted,
}
