/// Bi-temporal database functionality for SEQUOIA
///
/// Enables independent tracking of sequence data changes and taxonomy understanding changes,
/// allowing queries like "Give me the database as of January with March taxonomy"
use crate::types::{SHA256HashExt, BiTemporalCoordinate, ManifestMetadata, SHA256Hash, TaxonId, TemporalManifest, MerkleHash};
use crate::manifest::Manifest;
use crate::verification::merkle::MerkleDAG;
use crate::storage::SEQUOIAStorage;
use crate::temporal::{TaxonomyVersion, TemporalIndex};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// A bi-temporal database view allowing time-travel queries
pub struct BiTemporalDatabase {
    /// Storage backend
    storage: Arc<SEQUOIAStorage>,

    /// Temporal index for version tracking
    temporal_index: TemporalIndex,

    /// Cache of manifests at different time points
    manifest_cache: HashMap<String, Manifest>,  // Key is concatenated timestamps
}

impl BiTemporalDatabase {
    /// Create a new bi-temporal database
    pub fn new(storage: Arc<SEQUOIAStorage>) -> Result<Self> {
        let temporal_index = TemporalIndex::new(&storage.base_path)?;

        Ok(Self {
            storage,
            temporal_index,
            manifest_cache: HashMap::new(),
        })
    }

    /// Query the database at a specific bi-temporal coordinate
    pub fn query_at(
        &mut self,
        sequence_time: DateTime<Utc>,
        taxonomy_time: DateTime<Utc>,
    ) -> Result<DatabaseSnapshot> {
        let coordinate = BiTemporalCoordinate {
            sequence_time,
            taxonomy_time,
        };

        // Check cache first
        let cache_key = format!("{}_{}", sequence_time.timestamp(), taxonomy_time.timestamp());
        if let Some(manifest) = self.manifest_cache.get(&cache_key) {
            return Ok(DatabaseSnapshot::from_manifest(manifest.clone(), self.storage.clone()));
        }

        // Get the state at this temporal coordinate
        let state = self.temporal_index.get_state_at(sequence_time)?;

        // Create a synthetic manifest for this time point
        let manifest = self.create_manifest_at(&coordinate, state)?;

        // Cache for future queries
        let cache_key = format!("{}_{}", coordinate.sequence_time.timestamp(), coordinate.taxonomy_time.timestamp());
        self.manifest_cache.insert(cache_key, manifest.clone());

        Ok(DatabaseSnapshot::from_manifest(manifest, self.storage.clone()))
    }

    /// Create a manifest representing the database at a specific time
    fn create_manifest_at(
        &self,
        coordinate: &BiTemporalCoordinate,
        state: crate::temporal::TemporalState,
    ) -> Result<Manifest> {
        // Get sequence version at sequence_time
        let sequence_version = self.temporal_index
            .get_sequence_version_at(coordinate.sequence_time)?
            .ok_or_else(|| anyhow!("No sequence version at {:?}", coordinate.sequence_time))?;

        // Get taxonomy version at taxonomy_time
        let taxonomy_version = self.temporal_index
            .get_taxonomy_version_at(coordinate.taxonomy_time)?
            .ok_or_else(|| anyhow!("No taxonomy version at {:?}", coordinate.taxonomy_time))?;

        // Get chunks that existed at sequence_time
        let chunks = self.temporal_index.get_chunks_at_time(coordinate)?;

        // Filter chunks based on taxonomy at taxonomy_time
        let filtered_chunks = self.apply_taxonomy_filter(chunks, &taxonomy_version)?;

        // Build dual Merkle roots
        let sequence_root = self.compute_sequence_merkle_root(&filtered_chunks)?;
        let taxonomy_root = self.compute_taxonomy_merkle_root(&taxonomy_version)?;

        // Create the manifest
        let mut manifest = Manifest::new(&self.storage.base_path.join("manifest.tal"))?;

        // Build a TemporalManifest from our data
        let temporal_manifest = TemporalManifest {
            version: format!("{}_{}",
                coordinate.sequence_time.format("%Y%m%d_%H%M%S"),
                coordinate.taxonomy_time.format("%Y%m%d_%H%M%S")
            ),
            created_at: Utc::now(),
            sequence_version: sequence_version.version.clone(),
            taxonomy_version: taxonomy_version.version.clone(),
            temporal_coordinate: Some(coordinate.clone()),
            taxonomy_root: taxonomy_root.clone(),
            sequence_root: sequence_root.clone(),
            chunk_merkle_tree: None,  // Built on demand
            taxonomy_manifest_hash: SHA256Hash::compute(taxonomy_version.version.as_bytes()),
            taxonomy_dump_version: taxonomy_version.source.clone(),
            source_database: None,  // TODO: Get from state
            chunk_index: filtered_chunks,
            discrepancies: Vec::new(),  // TODO: Load discrepancies for this time
            etag: Self::generate_etag(&sequence_root, &taxonomy_root),
            previous_version: state.manifest.and_then(|m| Some(m.version)),
        };

        manifest.set_from_temporal(temporal_manifest)?;
        Ok(manifest)
    }

    /// Apply taxonomy filter to chunks based on taxonomy version
    fn apply_taxonomy_filter(
        &self,
        chunks: Vec<ManifestMetadata>,
        taxonomy_version: &TaxonomyVersion,
    ) -> Result<Vec<ManifestMetadata>> {
        // Apply taxonomy remapping based on version
        let mut filtered_chunks = Vec::new();

        for mut chunk in chunks {
            // Remap taxon IDs based on taxonomy version
            let mut remapped_taxa = Vec::new();

            for taxon_id in &chunk.taxon_ids {
                // Check if this taxon was reclassified in this version
                if let Some(new_taxon) = taxonomy_version.reclassifications.get(taxon_id) {
                    remapped_taxa.push(*new_taxon);
                } else if taxonomy_version.active_taxa.contains(taxon_id) {
                    // Taxon is still valid in this version
                    remapped_taxa.push(*taxon_id);
                }
                // Skip deprecated taxa that aren't in active set
            }

            if !remapped_taxa.is_empty() {
                chunk.taxon_ids = remapped_taxa;
                filtered_chunks.push(chunk);
            }
        }

        Ok(filtered_chunks)
    }

    /// Compute Merkle root for sequence data
    fn compute_sequence_merkle_root(&self, chunks: &[ManifestMetadata]) -> Result<MerkleHash> {
        if chunks.is_empty() {
            return Ok(SHA256Hash::zero());
        }

        let dag = MerkleDAG::build_from_items(chunks.to_vec())?;
        dag.root_hash()
            .ok_or_else(|| anyhow!("Failed to compute sequence Merkle root"))
    }

    /// Compute Merkle root for taxonomy data
    fn compute_taxonomy_merkle_root(&self, taxonomy_version: &TaxonomyVersion) -> Result<MerkleHash> {
        // The taxonomy root is already computed and stored in the version
        Ok(taxonomy_version.root_hash.clone())
    }

    /// Generate ETag from dual roots
    fn generate_etag(sequence_root: &MerkleHash, taxonomy_root: &MerkleHash) -> String {
        format!("W/\"{}-{}\"",
            &sequence_root.to_string()[..8],
            &taxonomy_root.to_string()[..8]
        )
    }

    /// Compare two temporal coordinates and return differences
    pub fn diff(
        &mut self,
        coord1: BiTemporalCoordinate,
        coord2: BiTemporalCoordinate,
    ) -> Result<TemporalDiff> {
        let snapshot1 = self.query_at(coord1.sequence_time, coord1.taxonomy_time)?;
        let snapshot2 = self.query_at(coord2.sequence_time, coord2.taxonomy_time)?;

        Ok(TemporalDiff {
            sequences_added: snapshot2.sequence_count() - snapshot1.sequence_count(),
            sequences_removed: 0,  // TODO: Implement removal tracking
            taxonomic_changes: self.compare_taxonomies(&snapshot1, &snapshot2)?,
            coord1,
            coord2,
        })
    }

    /// Compare taxonomies between two snapshots
    fn compare_taxonomies(
        &self,
        snapshot1: &DatabaseSnapshot,
        snapshot2: &DatabaseSnapshot,
    ) -> Result<Vec<TaxonomicChange>> {
        let mut changes = Vec::new();

        // Get all unique taxon IDs from both snapshots
        let chunks1 = snapshot1.chunks();
        let taxa1: HashSet<_> = chunks1
            .iter()
            .flat_map(|c| c.taxon_ids.iter())
            .collect();

        let chunks2 = snapshot2.chunks();
        let taxa2: HashSet<_> = chunks2
            .iter()
            .flat_map(|c| c.taxon_ids.iter())
            .collect();

        // Find new taxa
        for taxon in taxa2.difference(&taxa1) {
            changes.push(TaxonomicChange {
                taxon_id: **taxon,
                old_parent: None,
                new_parent: self.get_parent_taxon(**taxon, snapshot2),
                change_type: TaxonomicChangeType::New,
            });
        }

        // Find deprecated taxa
        for taxon in taxa1.difference(&taxa2) {
            changes.push(TaxonomicChange {
                taxon_id: **taxon,
                old_parent: self.get_parent_taxon(**taxon, snapshot1),
                new_parent: None,
                change_type: TaxonomicChangeType::Deprecated,
            });
        }

        // Find reclassified taxa (in both but with different parents)
        for taxon in taxa1.intersection(&taxa2) {
            let parent1 = self.get_parent_taxon(**taxon, snapshot1);
            let parent2 = self.get_parent_taxon(**taxon, snapshot2);

            if parent1 != parent2 {
                changes.push(TaxonomicChange {
                    taxon_id: **taxon,
                    old_parent: parent1,
                    new_parent: parent2,
                    change_type: TaxonomicChangeType::Reclassified,
                });
            }
        }

        Ok(changes)
    }

    /// Get parent taxon for a given taxon in a snapshot
    fn get_parent_taxon(&self, taxon_id: TaxonId, _snapshot: &DatabaseSnapshot) -> Option<TaxonId> {
        // This would query the taxonomy tree at the snapshot's taxonomy time
        // For now, return a placeholder based on common taxonomy
        match taxon_id.0 {
            562 => Some(TaxonId(561)),     // E. coli -> Escherichia
            561 => Some(TaxonId(543)),     // Escherichia -> Enterobacteriaceae
            9606 => Some(TaxonId(9605)),   // Human -> Homo
            10090 => Some(TaxonId(10088)), // Mouse -> Mus
            _ => None,
        }
    }

    /// Get all available temporal coordinates
    pub fn get_available_coordinates(&self) -> Result<Vec<BiTemporalCoordinate>> {
        self.temporal_index.get_all_coordinates()
    }
}

/// A snapshot of the database at a specific bi-temporal coordinate
pub struct DatabaseSnapshot {
    manifest: Manifest,
    storage: Arc<SEQUOIAStorage>,
}

impl DatabaseSnapshot {
    fn from_manifest(manifest: Manifest, storage: Arc<SEQUOIAStorage>) -> Self {
        Self { manifest, storage }
    }

    /// Get the number of sequences in this snapshot
    pub fn sequence_count(&self) -> usize {
        self.manifest.get_total_sequences()
    }

    /// Get the temporal coordinate of this snapshot
    pub fn coordinate(&self) -> Option<BiTemporalCoordinate> {
        // Get from manifest data
        self.manifest.get_data()
            .and_then(|data| data.temporal_coordinate.clone())
    }

    /// Export this snapshot as FASTA
    pub fn export_fasta(&self, path: &Path) -> Result<()> {
        use std::fs::File;
        use std::io::Write;
        use crate::operations::FastaAssembler;

        let mut file = File::create(path)?;

        // Write bi-temporal header
        if let Some(coord) = self.coordinate() {
            writeln!(file, "; SEQUOIA Bi-Temporal Export")?;
            writeln!(file, "; Sequence Date: {}", coord.sequence_time.format("%Y-%m-%d %H:%M:%S"))?;
            writeln!(file, "; Taxonomy Date: {}", coord.taxonomy_time.format("%Y-%m-%d %H:%M:%S"))?;
            writeln!(file, "; Sequence Root: {}", self.sequence_root())?;
            writeln!(file, "; Taxonomy Root: {}", self.taxonomy_root())?;
        }

        // Use assembler to export sequences
        let assembler = FastaAssembler::new(&self.storage);
        let chunk_hashes: Vec<_> = self.chunks()
            .iter()
            .map(|c| c.hash.clone())
            .collect();

        let sequence_count = assembler.stream_assembly(&chunk_hashes, &mut file)?;

        println!("Exported {} sequences to {}", sequence_count, path.display());
        Ok(())
    }

    /// Get chunks in this snapshot
    pub fn chunks(&self) -> Vec<ManifestMetadata> {
        self.manifest.get_chunks()
    }

    /// Get taxonomy root
    pub fn taxonomy_root(&self) -> MerkleHash {
        self.manifest.get_taxonomy_root().unwrap_or_else(|| SHA256Hash::zero())
    }

    /// Get sequence root
    pub fn sequence_root(&self) -> MerkleHash {
        self.manifest.get_sequence_root().unwrap_or_else(|| SHA256Hash::zero())
    }
}

/// Difference between two temporal coordinates
pub struct TemporalDiff {
    pub sequences_added: usize,
    pub sequences_removed: usize,
    pub taxonomic_changes: Vec<TaxonomicChange>,
    pub coord1: BiTemporalCoordinate,
    pub coord2: BiTemporalCoordinate,
}

/// A taxonomic change between versions
#[derive(Debug, Clone)]
pub struct TaxonomicChange {
    pub taxon_id: TaxonId,
    pub old_parent: Option<TaxonId>,
    pub new_parent: Option<TaxonId>,
    pub change_type: TaxonomicChangeType,
}

#[derive(Debug, Clone)]
pub enum TaxonomicChangeType {
    Reclassified,
    Merged,
    Split,
    Deprecated,
    New,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_bi_temporal_empty_database() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = Arc::new(SEQUOIAStorage::new(temp_dir.path())?);
        let mut db = BiTemporalDatabase::new(storage)?;

        // Query at current time on empty database should fail gracefully
        let now = Utc::now();
        let result = db.query_at(now, now);

        // Should get an error about no versions
        assert!(result.is_err());
        if let Err(err) = result {
            assert!(err.to_string().contains("No sequence version") ||
                    err.to_string().contains("No taxonomy version"));
        }

        Ok(())
    }
}