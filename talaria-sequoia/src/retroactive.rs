/// Retroactive analysis engine for bi-temporal queries
///
/// This module provides the core engine for temporal queries including:
/// - Historical reproduction (exact state at a point in time)
/// - Retroactive analysis (apply modern taxonomy to past sequences)
/// - Classification evolution tracking
/// - Temporal joins for finding reclassified sequences
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};

use talaria_bio::fasta::parse_fasta_from_bytes;
use talaria_bio::sequence::Sequence;
use crate::storage::SEQUOIAStorage;
use crate::taxonomy::TaxonomyManager;
use crate::temporal::{SequenceVersion, TaxonomyVersion, TemporalIndex};
use crate::traits::temporal::*;
use crate::types::{BiTemporalCoordinate, SHA256Hash, TaxonId};
use crate::SEQUOIARepository;

/// Core engine for retroactive temporal analysis
pub struct RetroactiveAnalyzer {
    temporal_index: TemporalIndex,
    storage: SEQUOIAStorage,
    taxonomy_manager: TaxonomyManager,
}

impl RetroactiveAnalyzer {
    /// Create a new retroactive analyzer
    pub fn new(
        storage: SEQUOIAStorage,
        temporal_index: TemporalIndex,
        taxonomy_manager: TaxonomyManager,
    ) -> Self {
        Self {
            temporal_index,
            storage,
            taxonomy_manager,
        }
    }

    /// Create from SEQUOIARepository
    pub fn from_repository(repository: SEQUOIARepository) -> Self {
        Self {
            temporal_index: repository.temporal,
            storage: repository.storage,
            taxonomy_manager: repository.taxonomy,
        }
    }

    /// Initialize from a base path
    pub fn from_path(base_path: &std::path::Path) -> Result<Self> {
        let storage = SEQUOIAStorage::open(base_path)?;
        let temporal_index = TemporalIndex::load(base_path)?;
        let taxonomy_manager = TaxonomyManager::load(base_path)?;

        Ok(Self {
            temporal_index,
            storage,
            taxonomy_manager,
        })
    }

    fn count_unique_taxa(&self, sequences: &[Sequence]) -> usize {
        let taxa: HashSet<_> = sequences
            .iter()
            .filter_map(|s| s.taxon_id.map(TaxonId))
            .collect();
        taxa.len()
    }

    // Synchronous query methods for CLI

    pub fn query_snapshot(&self, query: SnapshotQuery) -> Result<TemporalSnapshot> {
        // Implementation for snapshot query
        // Get chunks at the given coordinate
        let chunks = self.temporal_index.get_chunks_at_time(&query.coordinate)?;
        let mut sequences = Vec::new();

        for chunk_meta in &chunks {
            // Apply taxon filter if provided
            if let Some(ref filter) = query.taxon_filter {
                let has_match = chunk_meta.taxon_ids.iter().any(|t| filter.contains(t));
                if !has_match {
                    continue;
                }
            }

            if let Ok(chunk_sequences) = self.storage.load_sequences_from_chunk(&chunk_meta.hash) {
                sequences.extend(chunk_sequences);
            }
        }

        // Calculate metadata before moving sequences
        let chunk_count = chunks.len();
        let sequence_count = sequences.len();
        let unique_taxa = self.count_unique_taxa(&sequences);

        // Get version information
        let sequence_version = SequenceVersion {
            version: "2024-09-15".to_string(),
            timestamp: query.coordinate.sequence_time,
            root_hash: SHA256Hash::zero(),
            chunk_count,
            sequence_count,
        };

        let taxonomy_version = TaxonomyVersion {
            version: "2024-09-15".to_string(),
            timestamp: query.coordinate.taxonomy_time,
            root_hash: SHA256Hash::zero(),
            taxa_count: unique_taxa,
            source: "NCBI Taxonomy".to_string(),
        };

        Ok(TemporalSnapshot {
            coordinate: query.coordinate,
            sequences,
            sequence_version,
            taxonomy_version,
            metadata: SnapshotMetadata {
                total_sequences: sequence_count,
                total_chunks: chunk_count,
                unique_taxa,
                snapshot_hash: SHA256Hash::zero(),
                created_at: Utc::now(),
            },
        })
    }

    pub fn query_evolution(&self, query: EvolutionQuery) -> Result<EvolutionHistory> {
        // Implementation for evolution query
        let mut events = Vec::new();

        // This would query the temporal index for changes
        // For now, return a simple example
        events.push(EvolutionEvent {
            timestamp: query.from_date,
            event_type: EventType::Created,
            description: format!("Entity {} first appeared", query.entity_id),
            metadata: serde_json::json!({}),
        });

        Ok(EvolutionHistory {
            entity_id: query.entity_id,
            events,
            from_date: query.from_date,
            to_date: query.to_date,
        })
    }

    pub fn query_join(&self, query: JoinQuery) -> Result<TemporalJoinResult> {
        // Implementation for join query
        let reference_coord = BiTemporalCoordinate::at(query.reference_date);
        let comparison_coord = if let Some(date) = query.comparison_date {
            BiTemporalCoordinate::at(date)
        } else {
            BiTemporalCoordinate::now()
        };

        let ref_snapshot = self.query_snapshot(SnapshotQuery {
            coordinate: reference_coord,
            taxon_filter: query.taxon_filter.clone(),
        })?;

        let comp_snapshot = self.query_snapshot(SnapshotQuery {
            coordinate: comparison_coord,
            taxon_filter: query.taxon_filter,
        })?;

        // Find reclassified sequences
        let mut reclassified: Vec<ReclassifiedGroup> = Vec::new();
        let mut stable = Vec::new();
        let mut taxonomies_changed = 0;

        let ref_map: HashMap<String, Option<TaxonId>> = ref_snapshot
            .sequences
            .iter()
            .map(|s| (s.id.clone(), s.taxon_id.map(TaxonId)))
            .collect();

        let comp_map: HashMap<String, Option<TaxonId>> = comp_snapshot
            .sequences
            .iter()
            .map(|s| (s.id.clone(), s.taxon_id.map(TaxonId)))
            .collect();

        for (id, ref_taxon) in &ref_map {
            if let Some(comp_taxon) = comp_map.get(id) {
                if ref_taxon != comp_taxon {
                    // Track reclassification
                    if let Some(group) = reclassified
                        .iter_mut()
                        .find(|g| g.old_taxon == *ref_taxon && g.new_taxon == *comp_taxon)
                    {
                        group.count += 1;
                        group.sequences.push(id.clone());
                    } else {
                        reclassified.push(ReclassifiedGroup {
                            old_taxon: *ref_taxon,
                            new_taxon: *comp_taxon,
                            count: 1,
                            sequences: vec![id.clone()],
                        });
                        taxonomies_changed += 1;
                    }
                } else {
                    stable.push(id.clone());
                }
            }
        }

        let total_affected = reclassified.iter().map(|g| g.count).sum();

        Ok(TemporalJoinResult {
            query: TemporalJoinQuery {
                taxon: None,
                taxon_name: None,
                reference_date: query.reference_date,
                comparison_date: query.comparison_date,
                include_descendants: false,
            },
            reclassified,
            stable,
            total_affected,
            taxonomies_changed,
            execution_time_ms: 0,
        })
    }

    pub fn query_diff(&self, query: DiffQuery) -> Result<TemporalDiff> {
        // Implementation for diff query
        let from_snapshot = self.query_snapshot(SnapshotQuery {
            coordinate: query.from.clone(),
            taxon_filter: query.taxon_filter.clone(),
        })?;

        let to_snapshot = self.query_snapshot(SnapshotQuery {
            coordinate: query.to.clone(),
            taxon_filter: query.taxon_filter,
        })?;

        let from_ids: HashSet<String> = from_snapshot
            .sequences
            .iter()
            .map(|s| s.id.clone())
            .collect();

        let to_ids: HashSet<String> = to_snapshot.sequences.iter().map(|s| s.id.clone()).collect();

        let added: Vec<String> = to_ids.difference(&from_ids).cloned().collect();
        let removed: Vec<String> = from_ids.difference(&to_ids).cloned().collect();

        // Find reclassifications
        let mut reclassifications = Vec::new();
        for seq in &from_snapshot.sequences {
            if let Some(to_seq) = to_snapshot.sequences.iter().find(|s| s.id == seq.id) {
                if seq.taxon_id != to_seq.taxon_id {
                    reclassifications.push(Reclassification {
                        sequence_id: seq.id.clone(),
                        old_taxon: seq.taxon_id.map(TaxonId),
                        new_taxon: to_seq.taxon_id.map(TaxonId),
                        reason: ReclassificationReason::TaxonomyUpdate,
                        date: query.to.sequence_time,
                    });
                }
            }
        }

        Ok(TemporalDiff {
            from: query.from,
            to: query.to,
            sequence_changes: SequenceChanges {
                added: added.clone(),
                removed: removed.clone(),
                modified: vec![],
                total_delta: added.len() as i64 - removed.len() as i64,
            },
            taxonomy_changes: TaxonomyChanges {
                added_taxa: vec![],
                removed_taxa: vec![],
                renamed_taxa: vec![],
                merged_taxa: vec![],
                split_taxa: vec![],
            },
            reclassifications,
        })
    }

    /// Historical reproduction: Get exact state at a specific date
    pub async fn historical_reproduction(
        &self,
        date: DateTime<Utc>,
        taxon_filter: Option<TaxonId>,
    ) -> Result<TemporalSnapshot> {
        let coord = BiTemporalCoordinate::at(date);
        let mut snapshot = self.query_at(coord).await?;

        // Apply taxon filter if specified
        if let Some(taxon) = taxon_filter {
            snapshot
                .sequences
                .retain(|seq| seq.taxon_id == Some(taxon.0));
        }

        Ok(snapshot)
    }

    /// Retroactive analysis: Apply current taxonomy to past sequences
    pub async fn retroactive_analysis(
        &self,
        sequences_from: DateTime<Utc>,
        taxonomy_version: &TaxonomyVersion,
    ) -> Result<RetroactiveResult> {
        // Get sequences from the past
        let seq_coord = BiTemporalCoordinate::at(sequences_from);
        let snapshot = self.query_at(seq_coord).await?;

        // Apply the new taxonomy
        let result = self.apply_taxonomy(&snapshot, taxonomy_version).await?;

        Ok(result)
    }

    /// Track classification evolution for a specific sequence
    pub async fn classification_evolution(
        &self,
        sequence_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<EvolutionHistory> {
        self.evolution_history(sequence_id, start, end).await
    }

    /// Find sequences that were reclassified between two dates
    pub async fn find_reclassified(
        &self,
        taxon: TaxonId,
        from_date: DateTime<Utc>,
        to_date: DateTime<Utc>,
    ) -> Result<TemporalJoinResult> {
        let query = TemporalJoinQuery {
            taxon: Some(taxon),
            taxon_name: None,
            reference_date: from_date,
            comparison_date: Some(to_date),
            include_descendants: true,
        };

        self.temporal_join(query).await
    }

    /// Get all sequences for a taxon at a specific time
    #[allow(dead_code)]
    async fn get_sequences_for_taxon(
        &self,
        taxon: TaxonId,
        timestamp: DateTime<Utc>,
        include_descendants: bool,
    ) -> Result<Vec<Sequence>> {
        let state = self.temporal_index.get_state_at(timestamp)?;

        if state.sequence_version.is_none() {
            return Ok(Vec::new());
        }

        // Get chunks containing this taxon
        let mut target_taxa = vec![taxon];

        if include_descendants {
            // Add descendant taxa
            if let Some(descendants) = self.get_descendant_taxa(taxon) {
                target_taxa.extend(descendants);
            }
        }

        // Collect all sequences from chunks
        let mut sequences = Vec::new();
        for chunk_hash in self.get_chunks_for_taxa(&target_taxa)? {
            let chunk_data = self.storage.get_chunk(&chunk_hash)?;

            // Parse sequences from chunk
            if let Ok(chunk_sequences) = parse_fasta_from_bytes(&chunk_data) {
                for seq in chunk_sequences {
                    if let Some(seq_taxon) = seq.taxon_id {
                        if target_taxa.iter().any(|t| t.0 == seq_taxon) {
                            sequences.push(seq);
                        }
                    }
                }
            }
        }

        Ok(sequences)
    }

    /// Get descendant taxa IDs
    #[allow(dead_code)]
    fn get_descendant_taxa(&self, _taxon: TaxonId) -> Option<Vec<TaxonId>> {
        // This would query the taxonomy tree for descendants
        // For now, return None (no descendants)
        None
    }

    /// Get chunks containing any of the specified taxa
    #[allow(dead_code)]
    fn get_chunks_for_taxa(&self, taxa: &[TaxonId]) -> Result<Vec<SHA256Hash>> {
        let chunk_hashes = HashSet::new();

        for _taxon in taxa {
            // This would query the taxonomy manager for chunks
            // For now, return empty
        }

        Ok(chunk_hashes.into_iter().collect())
    }

    /// Analyze the impact of taxonomy changes
    async fn analyze_changes(
        &self,
        old_snapshot: &TemporalSnapshot,
        new_snapshot: &TemporalSnapshot,
    ) -> Result<Vec<Reclassification>> {
        let mut reclassifications = Vec::new();

        // Build ID -> taxon maps
        let old_map: HashMap<String, TaxonId> = old_snapshot
            .sequences
            .iter()
            .filter_map(|s| s.taxon_id.map(|t| (s.id.clone(), TaxonId(t))))
            .collect();

        let new_map: HashMap<String, TaxonId> = new_snapshot
            .sequences
            .iter()
            .filter_map(|s| s.taxon_id.map(|t| (s.id.clone(), TaxonId(t))))
            .collect();

        // Find changes
        for (seq_id, old_taxon) in &old_map {
            let new_taxon = new_map.get(seq_id);

            if new_taxon != Some(old_taxon) {
                reclassifications.push(Reclassification {
                    sequence_id: seq_id.clone(),
                    old_taxon: Some(*old_taxon),
                    new_taxon: new_taxon.copied(),
                    reason: ReclassificationReason::TaxonomyUpdate,
                    date: new_snapshot.coordinate.taxonomy_time,
                });
            }
        }

        Ok(reclassifications)
    }
}

#[async_trait]
impl TemporalQueryable for RetroactiveAnalyzer {
    async fn query_at(&self, coord: BiTemporalCoordinate) -> Result<TemporalSnapshot> {
        // Get temporal state
        let state = self.temporal_index.get_state_at(coord.sequence_time)?;

        let sequence_version = state
            .sequence_version
            .ok_or_else(|| anyhow::anyhow!("No sequence version at {}", coord.sequence_time))?;

        let taxonomy_version = state
            .taxonomy_version
            .ok_or_else(|| anyhow::anyhow!("No taxonomy version at {}", coord.taxonomy_time))?;

        // Load sequences from storage
        let sequences = self.load_sequences_at(&sequence_version).await?;

        // Create metadata
        let metadata = SnapshotMetadata {
            total_sequences: sequences.len(),
            total_chunks: sequence_version.chunk_count,
            unique_taxa: self.count_unique_taxa(&sequences),
            snapshot_hash: self.compute_snapshot_hash(&sequence_version, &taxonomy_version),
            created_at: Utc::now(),
        };

        Ok(TemporalSnapshot {
            coordinate: coord,
            sequences,
            sequence_version,
            taxonomy_version,
            metadata,
        })
    }

    async fn changes_between(
        &self,
        from: BiTemporalCoordinate,
        to: BiTemporalCoordinate,
    ) -> Result<TemporalDiff> {
        let from_snapshot = self.query_at(from.clone()).await?;
        let to_snapshot = self.query_at(to.clone()).await?;

        // Analyze sequence changes
        let sequence_changes = self.analyze_sequence_changes(&from_snapshot, &to_snapshot)?;

        // Analyze taxonomy changes
        let taxonomy_changes = self.analyze_taxonomy_changes(
            &from_snapshot.taxonomy_version,
            &to_snapshot.taxonomy_version,
        )?;

        // Find reclassifications
        let reclassifications = self.analyze_changes(&from_snapshot, &to_snapshot).await?;

        Ok(TemporalDiff {
            from,
            to,
            sequence_changes,
            taxonomy_changes,
            reclassifications,
        })
    }

    async fn evolution_history(
        &self,
        entity_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<EvolutionHistory> {
        let mut events = Vec::new();
        let mut timeline = Vec::new();

        // Get all versions in the time range
        let versions = self.temporal_index.get_timeline(start, end);

        for event in versions.events {
            // Check if this event affects our entity
            if self.event_affects_entity(&event, entity_id)? {
                events.push(EvolutionEvent {
                    timestamp: event.timestamp,
                    event_type: self.map_event_type(&event.event_type),
                    description: event.description,
                    metadata: event.details,
                });

                timeline.push(TimelinePoint {
                    date: event.timestamp,
                    state: serde_json::json!({
                        "entity_id": entity_id,
                        "event": &event.event_type,
                    }),
                });
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
        let start_time = std::time::Instant::now();

        // Get sequences at reference date
        let ref_snapshot = self
            .query_at(BiTemporalCoordinate::at(query.reference_date))
            .await?;

        // Get sequences at comparison date (or current)
        let comp_date = query.comparison_date.unwrap_or_else(Utc::now);
        let comp_snapshot = self.query_at(BiTemporalCoordinate::at(comp_date)).await?;

        // Build classification maps
        let ref_classifications = self.build_classification_map(&ref_snapshot);
        let comp_classifications = self.build_classification_map(&comp_snapshot);

        // Find reclassified sequences
        let mut reclassified_groups: HashMap<(Option<TaxonId>, Option<TaxonId>), Vec<String>> =
            HashMap::new();
        let mut stable = Vec::new();

        for (seq_id, ref_taxon) in &ref_classifications {
            let comp_taxon = comp_classifications.get(seq_id);

            if comp_taxon != Some(ref_taxon) {
                let key = (Some(*ref_taxon), comp_taxon.copied());
                reclassified_groups
                    .entry(key)
                    .or_default()
                    .push(seq_id.clone());
            } else {
                stable.push(seq_id.clone());
            }
        }

        // Convert to result format
        let reclassified: Vec<ReclassifiedGroup> = reclassified_groups
            .into_iter()
            .map(|((old, new), sequences)| ReclassifiedGroup {
                old_taxon: old,
                new_taxon: new,
                count: sequences.len(),
                sequences,
            })
            .collect();

        let total_affected = reclassified.iter().map(|g| g.count).sum();
        let taxonomies_changed = reclassified.len();

        Ok(TemporalJoinResult {
            query,
            reclassified,
            stable,
            total_affected,
            taxonomies_changed,
            execution_time_ms: start_time.elapsed().as_millis() as u64,
        })
    }
}

#[async_trait]
impl RetroactiveAnalyzable for RetroactiveAnalyzer {
    async fn apply_taxonomy(
        &self,
        sequences: &TemporalSnapshot,
        taxonomy: &TaxonomyVersion,
    ) -> Result<RetroactiveResult> {
        let mut reclassifications = Vec::new();
        let conflicts = Vec::new();

        // Apply new taxonomy to each sequence
        for seq in &sequences.sequences {
            if let Some(old_taxon) = seq.taxon_id {
                // Look up new classification
                // This would use taxonomy mapping tables
                let new_taxon = self.map_to_new_taxonomy(TaxonId(old_taxon), taxonomy)?;

                if new_taxon != Some(TaxonId(old_taxon)) {
                    reclassifications.push(Reclassification {
                        sequence_id: seq.id.clone(),
                        old_taxon: Some(TaxonId(old_taxon)),
                        new_taxon,
                        reason: ReclassificationReason::TaxonomyUpdate,
                        date: Utc::now(),
                    });
                }
            }
        }

        // Calculate statistics
        let statistics = RetroactiveStatistics {
            total_sequences: sequences.sequences.len(),
            reclassified_count: reclassifications.len(),
            conflict_count: conflicts.len(),
            new_taxa_discovered: 0,   // Would calculate from taxonomy diff
            obsolete_taxa_removed: 0, // Would calculate from taxonomy diff
        };

        Ok(RetroactiveResult {
            original_coordinate: sequences.coordinate.clone(),
            applied_taxonomy: taxonomy.clone(),
            reclassifications,
            conflicts,
            statistics,
        })
    }

    async fn find_conflicts(
        &self,
        coord: BiTemporalCoordinate,
    ) -> Result<Vec<ClassificationConflict>> {
        let snapshot = self.query_at(coord).await?;
        let mut conflicts = Vec::new();

        // Check for conflicts in the snapshot
        for seq in &snapshot.sequences {
            if let Some(taxon_id) = seq.taxon_id {
                // Check if taxon exists in current taxonomy
                if !self.taxonomy_manager.taxon_exists(TaxonId(taxon_id)) {
                    conflicts.push(ClassificationConflict {
                        sequence_id: seq.id.clone(),
                        original_classification: Some(TaxonId(taxon_id)),
                        new_classification: None,
                        conflict_type: ConflictType::TaxonNoLongerExists,
                        resolution_suggestion: Some("Map to parent taxon".to_string()),
                    });
                }
            }
        }

        Ok(conflicts)
    }

    async fn analyze_taxonomy_impact(
        &self,
        old_taxonomy: &TaxonomyVersion,
        new_taxonomy: &TaxonomyVersion,
    ) -> Result<TaxonomyImpactAnalysis> {
        // This would analyze the full impact of taxonomy changes
        Ok(TaxonomyImpactAnalysis {
            old_version: old_taxonomy.version.clone(),
            new_version: new_taxonomy.version.clone(),
            sequences_affected: 0,
            taxa_affected: 0,
            major_changes: Vec::new(),
            stability_score: 0.95,
        })
    }
}

// Helper methods
impl RetroactiveAnalyzer {
    async fn load_sequences_at(&self, _version: &SequenceVersion) -> Result<Vec<Sequence>> {
        // Load sequences from chunks at this version
        // This would query the storage for the specific version
        Ok(Vec::new())
    }

    fn compute_snapshot_hash(
        &self,
        seq_version: &SequenceVersion,
        tax_version: &TaxonomyVersion,
    ) -> SHA256Hash {
        let mut data = Vec::new();
        data.extend(seq_version.root_hash.as_bytes());
        data.extend(tax_version.root_hash.as_bytes());
        SHA256Hash::compute(&data)
    }

    fn analyze_sequence_changes(
        &self,
        from: &TemporalSnapshot,
        to: &TemporalSnapshot,
    ) -> Result<SequenceChanges> {
        let from_ids: HashSet<_> = from.sequences.iter().map(|s| s.id.clone()).collect();
        let to_ids: HashSet<_> = to.sequences.iter().map(|s| s.id.clone()).collect();

        let added: Vec<_> = to_ids.difference(&from_ids).cloned().collect();
        let removed: Vec<_> = from_ids.difference(&to_ids).cloned().collect();
        let modified = Vec::new(); // Would need content comparison

        let total_delta = to.sequences.len() as i64 - from.sequences.len() as i64;

        Ok(SequenceChanges {
            added,
            removed,
            modified,
            total_delta,
        })
    }

    fn analyze_taxonomy_changes(
        &self,
        _from: &TaxonomyVersion,
        _to: &TaxonomyVersion,
    ) -> Result<TaxonomyChanges> {
        // This would compare taxonomy versions
        Ok(TaxonomyChanges {
            added_taxa: Vec::new(),
            removed_taxa: Vec::new(),
            renamed_taxa: Vec::new(),
            merged_taxa: Vec::new(),
            split_taxa: Vec::new(),
        })
    }

    fn event_affects_entity(
        &self,
        event: &crate::temporal::TimelineEvent,
        entity_id: &str,
    ) -> Result<bool> {
        // Check if the event affects the entity
        Ok(event
            .details
            .get("entity_id")
            .and_then(|v| v.as_str())
            .map(|id| id == entity_id)
            .unwrap_or(false))
    }

    fn map_event_type(&self, event_type: &crate::temporal::TimelineEventType) -> EventType {
        use crate::temporal::TimelineEventType;

        match event_type {
            TimelineEventType::SequenceUpdate => EventType::Modified,
            TimelineEventType::TaxonomyUpdate => EventType::Reclassified,
            _ => EventType::Modified,
        }
    }

    fn build_classification_map(&self, snapshot: &TemporalSnapshot) -> HashMap<String, TaxonId> {
        snapshot
            .sequences
            .iter()
            .filter_map(|s| s.taxon_id.map(|t| (s.id.clone(), TaxonId(t))))
            .collect()
    }

    fn map_to_new_taxonomy(
        &self,
        old_taxon: TaxonId,
        _taxonomy: &TaxonomyVersion,
    ) -> Result<Option<TaxonId>> {
        // This would map old taxon to new taxonomy
        // For now, return unchanged
        Ok(Some(old_taxon))
    }
}
