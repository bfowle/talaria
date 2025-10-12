use crate::traits::temporal::*;
use crate::types::{BiTemporalCoordinate, TaxonId};
use crate::HeraldRepository;
/// Tracks evolution of taxonomic classifications over time
///
/// Provides detailed tracking of how taxonomic assignments change,
/// including merges, splits, reclassifications, and deletions.
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Tracks detailed evolution of taxonomic classifications
pub struct TaxonomyEvolutionTracker {
    repository: HeraldRepository,
    /// Cache of evolution histories by entity
    evolution_cache: HashMap<String, EvolutionHistory>,
    /// Index of reclassification events by date
    reclassification_index: BTreeMap<DateTime<Utc>, Vec<ReclassificationEvent>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are preserved for future use
struct ReclassificationEvent {
    date: DateTime<Utc>,
    sequence_id: String,
    old_taxon: Option<TaxonId>,
    new_taxon: Option<TaxonId>,
    reason: String,
}

impl TaxonomyEvolutionTracker {
    pub fn new(repository: HeraldRepository) -> Self {
        Self {
            repository,
            evolution_cache: HashMap::new(),
            reclassification_index: BTreeMap::new(),
        }
    }

    /// Track evolution of a specific entity (sequence or taxon)
    pub fn track_entity(
        &mut self,
        entity_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<EvolutionHistory> {
        // Check cache first
        let cache_key = format!("{}_{}_{}", entity_id, from.timestamp(), to.timestamp());
        if let Some(cached) = self.evolution_cache.get(&cache_key) {
            return Ok(cached.clone());
        }

        let mut events = Vec::new();
        let mut current_taxon: Option<u32> = None;
        let mut first_seen = true;

        // Get all temporal snapshots in range
        let snapshots = self.get_snapshots_in_range(from, to)?;

        for snapshot_date in snapshots {
            let coordinate = BiTemporalCoordinate::at(snapshot_date);

            // Try to find the entity in this snapshot
            if let Some(sequence) = self.find_sequence_at_time(entity_id, &coordinate)? {
                if first_seen {
                    events.push(EvolutionEvent {
                        timestamp: snapshot_date,
                        event_type: EventType::Created,
                        description: format!(
                            "First appeared with TaxID {}",
                            sequence
                                .taxon_id
                                .map(|t| t.to_string())
                                .unwrap_or_else(|| "unknown".to_string())
                        ),
                        metadata: serde_json::json!({}),
                    });
                    current_taxon = sequence.taxon_id;
                    first_seen = false;
                } else if current_taxon != sequence.taxon_id {
                    // Taxonomic reclassification
                    let old = current_taxon
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    let new = sequence
                        .taxon_id
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    events.push(EvolutionEvent {
                        timestamp: snapshot_date,
                        event_type: EventType::Reclassified,
                        description: format!("Reclassified from TaxID {} to {}", old, new),
                        metadata: serde_json::json!({
                            "old_taxon": old,
                            "new_taxon": new,
                        }),
                    });

                    // Track reclassification event
                    self.reclassification_index
                        .entry(snapshot_date)
                        .or_default()
                        .push(ReclassificationEvent {
                            date: snapshot_date,
                            sequence_id: entity_id.to_string(),
                            old_taxon: current_taxon.map(TaxonId),
                            new_taxon: sequence.taxon_id.map(TaxonId),
                            reason: "Taxonomy update".to_string(),
                        });

                    current_taxon = sequence.taxon_id;
                }
            } else if !first_seen {
                // Entity disappeared
                events.push(EvolutionEvent {
                    timestamp: snapshot_date,
                    event_type: EventType::Deleted,
                    description: "Removed from database".to_string(),
                    metadata: serde_json::json!({}),
                });
                break;
            }
        }

        let history = EvolutionHistory {
            entity_id: entity_id.to_string(),
            events,
            from_date: from,
            to_date: to,
        };

        // Cache the result
        self.evolution_cache.insert(cache_key, history.clone());

        Ok(history)
    }

    /// Find all entities that underwent a specific type of change
    pub fn find_changes_of_type(
        &mut self,
        event_type: EventType,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<String>> {
        let mut affected_entities = HashSet::new();

        // Scan through temporal index for changes
        let snapshots = self.get_snapshots_in_range(from, to)?;

        for i in 1..snapshots.len() {
            let prev_coord = BiTemporalCoordinate::at(snapshots[i - 1]);
            let curr_coord = BiTemporalCoordinate::at(snapshots[i]);

            let changes = self.compute_changes(&prev_coord, &curr_coord)?;

            match event_type {
                EventType::Created => {
                    affected_entities.extend(changes.added_sequences);
                }
                EventType::Deleted => {
                    affected_entities.extend(changes.removed_sequences);
                }
                EventType::Reclassified => {
                    affected_entities.extend(changes.reclassified_sequences);
                }
                EventType::Modified => {
                    affected_entities.extend(changes.modified_sequences);
                }
                _ => {}
            }
        }

        Ok(affected_entities.into_iter().collect())
    }

    /// Get mass reclassification events (affecting many sequences)
    pub fn find_mass_reclassifications(
        &mut self,
        threshold: usize,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<MassReclassification>> {
        let mut mass_events = Vec::new();

        for (date, events) in self.reclassification_index.range(from..=to) {
            // Group by old -> new taxon transitions
            let mut transitions: HashMap<(Option<TaxonId>, Option<TaxonId>), Vec<String>> =
                HashMap::new();

            for event in events {
                let key = (event.old_taxon, event.new_taxon);
                transitions
                    .entry(key)
                    .or_default()
                    .push(event.sequence_id.clone());
            }

            // Find transitions affecting many sequences
            for ((old, new), sequences) in transitions {
                if sequences.len() >= threshold {
                    mass_events.push(MassReclassification {
                        date: *date,
                        old_taxon: old,
                        new_taxon: new,
                        affected_sequences: sequences,
                        reason: "Taxonomic revision".to_string(),
                    });
                }
            }
        }

        Ok(mass_events)
    }

    /// Generate evolution report for a taxonomic group
    pub fn generate_taxon_report(
        &mut self,
        taxon_id: TaxonId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<TaxonEvolutionReport> {
        let mut sequences_added = Vec::new();
        let mut sequences_removed = Vec::new();
        let mut sequences_stable = Vec::new();
        let mut size_timeline = Vec::new();

        let snapshots = self.get_snapshots_in_range(from, to)?;
        let mut prev_sequences = HashSet::new();

        for snapshot_date in snapshots {
            let coordinate = BiTemporalCoordinate::at(snapshot_date);
            let sequences = self.get_taxon_sequences_at_time(taxon_id, &coordinate)?;
            let current_set: HashSet<String> = sequences.iter().map(|s| s.id.clone()).collect();

            // Track additions and removals
            let added: Vec<String> = current_set.difference(&prev_sequences).cloned().collect();
            let removed: Vec<String> = prev_sequences.difference(&current_set).cloned().collect();

            sequences_added.extend(added);
            sequences_removed.extend(removed);

            // Track size over time
            size_timeline.push((snapshot_date, current_set.len()));

            prev_sequences = current_set;
        }

        // Sequences that remained throughout
        for seq_id in &prev_sequences {
            if !sequences_removed.contains(seq_id) {
                sequences_stable.push(seq_id.clone());
            }
        }

        let total_turnover = sequences_added.len() + sequences_removed.len();

        Ok(TaxonEvolutionReport {
            taxon_id,
            from_date: from,
            to_date: to,
            sequences_added,
            sequences_removed,
            sequences_stable,
            size_timeline,
            total_turnover,
        })
    }

    // Helper methods

    fn get_snapshots_in_range(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<DateTime<Utc>>> {
        // Query temporal index for available snapshots
        self.repository.temporal.get_snapshots_between(from, to)
    }

    fn find_sequence_at_time(
        &self,
        sequence_id: &str,
        coordinate: &BiTemporalCoordinate,
    ) -> Result<Option<talaria_bio::sequence::Sequence>> {
        // Query specific sequence at temporal coordinate
        let chunks = self.repository.temporal.get_chunks_at_time(coordinate)?;

        for chunk_meta in chunks {
            if let Ok(sequences) = self
                .repository
                .storage
                .load_sequences_from_chunk(&chunk_meta.hash)
            {
                for seq in sequences {
                    if seq.id == sequence_id {
                        return Ok(Some(seq));
                    }
                }
            }
        }

        Ok(None)
    }

    fn get_taxon_sequences_at_time(
        &self,
        taxon_id: TaxonId,
        coordinate: &BiTemporalCoordinate,
    ) -> Result<Vec<talaria_bio::sequence::Sequence>> {
        let chunks = self.repository.temporal.get_chunks_at_time(coordinate)?;
        let mut sequences = Vec::new();

        for chunk_meta in chunks {
            if chunk_meta.taxon_ids.contains(&taxon_id) {
                if let Ok(chunk_sequences) = self
                    .repository
                    .storage
                    .load_sequences_from_chunk(&chunk_meta.hash)
                {
                    for seq in chunk_sequences {
                        if seq.taxon_id == Some(taxon_id.0) {
                            sequences.push(seq);
                        }
                    }
                }
            }
        }

        Ok(sequences)
    }

    fn compute_changes(
        &self,
        prev: &BiTemporalCoordinate,
        curr: &BiTemporalCoordinate,
    ) -> Result<ChangeSet> {
        let prev_chunks = self.repository.temporal.get_chunks_at_time(prev)?;
        let curr_chunks = self.repository.temporal.get_chunks_at_time(curr)?;

        let mut prev_sequences = HashMap::new();
        let mut curr_sequences = HashMap::new();

        // Load previous state
        for chunk_meta in prev_chunks {
            if let Ok(sequences) = self
                .repository
                .storage
                .load_sequences_from_chunk(&chunk_meta.hash)
            {
                for seq in sequences {
                    prev_sequences.insert(seq.id.clone(), seq.taxon_id);
                }
            }
        }

        // Load current state
        for chunk_meta in curr_chunks {
            if let Ok(sequences) = self
                .repository
                .storage
                .load_sequences_from_chunk(&chunk_meta.hash)
            {
                for seq in sequences {
                    curr_sequences.insert(seq.id.clone(), seq.taxon_id);
                }
            }
        }

        let mut changes = ChangeSet::default();

        // Find additions
        for id in curr_sequences.keys() {
            if !prev_sequences.contains_key(id) {
                changes.added_sequences.push(id.clone());
            }
        }

        // Find removals and reclassifications
        for (id, prev_taxon) in &prev_sequences {
            if let Some(curr_taxon) = curr_sequences.get(id) {
                if prev_taxon != curr_taxon {
                    changes.reclassified_sequences.push(id.clone());
                }
            } else {
                changes.removed_sequences.push(id.clone());
            }
        }

        Ok(changes)
    }
}

#[derive(Debug, Default)]
struct ChangeSet {
    added_sequences: Vec<String>,
    removed_sequences: Vec<String>,
    modified_sequences: Vec<String>,
    reclassified_sequences: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MassReclassification {
    pub date: DateTime<Utc>,
    pub old_taxon: Option<TaxonId>,
    pub new_taxon: Option<TaxonId>,
    pub affected_sequences: Vec<String>,
    pub reason: String,
}

#[derive(Debug)]
pub struct TaxonEvolutionReport {
    pub taxon_id: TaxonId,
    pub from_date: DateTime<Utc>,
    pub to_date: DateTime<Utc>,
    pub sequences_added: Vec<String>,
    pub sequences_removed: Vec<String>,
    pub sequences_stable: Vec<String>,
    pub size_timeline: Vec<(DateTime<Utc>, usize)>,
    pub total_turnover: usize,
}
