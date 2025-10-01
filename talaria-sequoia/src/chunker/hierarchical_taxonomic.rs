/// Hierarchical taxonomic chunking implementation for SEQUOIA
///
/// This module implements hierarchical organization of chunks based on taxonomy,
/// allowing efficient access patterns for taxonomic queries at different levels
/// (species, genus, family, order, class, phylum, kingdom, domain)
use crate::storage::sequence::SequenceStorage;
use crate::taxonomy::TaxonomyManager;
use crate::types::{ChunkClassification, ChunkManifest, DatabaseSource, SHA256Hash, TaxonId};
use anyhow::Result;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use talaria_bio::sequence::Sequence;
use talaria_utils::display::progress::{create_progress_bar, create_spinner};

/// Hierarchical taxonomic levels used for chunking
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaxonomicRank {
    Domain,     // e.g., Bacteria, Archaea, Eukaryota
    Kingdom,    // e.g., Animalia, Plantae, Fungi
    Phylum,     // e.g., Chordata, Arthropoda
    Class,      // e.g., Mammalia, Insecta
    Order,      // e.g., Primates, Diptera
    Family,     // e.g., Hominidae, Drosophilidae
    Genus,      // e.g., Homo, Drosophila
    Species,    // e.g., sapiens, melanogaster
    Subspecies, // e.g., subspecies or strain level
}

impl TaxonomicRank {
    /// Get all ranks from highest to lowest
    pub fn all() -> Vec<Self> {
        vec![
            Self::Domain,
            Self::Kingdom,
            Self::Phylum,
            Self::Class,
            Self::Order,
            Self::Family,
            Self::Genus,
            Self::Species,
            Self::Subspecies,
        ]
    }

    /// Get the rank name as a string
    pub fn as_str(&self) -> &str {
        match self {
            Self::Domain => "domain",
            Self::Kingdom => "kingdom",
            Self::Phylum => "phylum",
            Self::Class => "class",
            Self::Order => "order",
            Self::Family => "family",
            Self::Genus => "genus",
            Self::Species => "species",
            Self::Subspecies => "subspecies",
        }
    }

    /// Get minimum chunk size for this rank
    pub fn min_chunk_size(&self) -> usize {
        match self {
            Self::Domain => 100_000_000, // 100MB for domain level
            Self::Kingdom => 50_000_000, // 50MB for kingdom
            Self::Phylum => 20_000_000,  // 20MB for phylum
            Self::Class => 10_000_000,   // 10MB for class
            Self::Order => 5_000_000,    // 5MB for order
            Self::Family => 2_000_000,   // 2MB for family
            Self::Genus => 1_000_000,    // 1MB for genus
            Self::Species => 500_000,    // 500KB for species
            Self::Subspecies => 100_000, // 100KB for subspecies
        }
    }

    /// Get target chunk size for this rank
    pub fn target_chunk_size(&self) -> usize {
        self.min_chunk_size() * 2
    }

    /// Get maximum chunk size for this rank
    pub fn max_chunk_size(&self) -> usize {
        self.min_chunk_size() * 5
    }

    /// Create from string rank name
    pub fn from_string(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "domain" => Self::Domain,
            "kingdom" => Self::Kingdom,
            "phylum" => Self::Phylum,
            "class" => Self::Class,
            "order" => Self::Order,
            "family" => Self::Family,
            "genus" => Self::Genus,
            "species" => Self::Species,
            "subspecies" | "strain" => Self::Subspecies,
            _ => Self::Species, // Default to species if unknown
        }
    }
}

/// Hierarchical taxonomic chunker
pub struct HierarchicalTaxonomicChunker {
    _strategy: super::ChunkingStrategy,
    sequence_storage: SequenceStorage,
    database_source: DatabaseSource,
    taxonomy_manager: Option<TaxonomyManager>,
}

impl HierarchicalTaxonomicChunker {
    pub fn new(
        strategy: super::ChunkingStrategy,
        sequence_storage: SequenceStorage,
        database_source: DatabaseSource,
        taxonomy_manager: Option<TaxonomyManager>,
    ) -> Self {
        Self {
            _strategy: strategy,
            sequence_storage,
            database_source,
            taxonomy_manager,
        }
    }

    /// Chunk sequences using hierarchical taxonomic organization
    pub fn chunk_sequences_hierarchical(
        &mut self,
        sequences: Vec<Sequence>,
    ) -> Result<Vec<ChunkManifest>> {
        // Step 1: Store sequences canonically
        let storing_progress =
            create_progress_bar(sequences.len() as u64, "Storing canonical sequences");

        let mut sequence_records = Vec::new();
        let dedup_count = 0;
        let mut new_count = 0;

        // Process in batches for performance
        const BATCH_SIZE: usize = 10000;
        for chunk in sequences.chunks(BATCH_SIZE) {
            let batch_data: Vec<(String, String, DatabaseSource)> = chunk
                .iter()
                .map(|seq| {
                    let header = format!(
                        ">{}{}",
                        seq.id,
                        seq.description
                            .as_ref()
                            .map(|d| format!(" {}", d))
                            .unwrap_or_default()
                    );
                    let sequence_str = String::from_utf8_lossy(&seq.sequence).to_string();
                    (sequence_str, header, self.database_source.clone())
                })
                .collect();

            // Store batch
            let batch_results: Vec<SHA256Hash> = batch_data
                .iter()
                .map(|(seq, header, source)| {
                    self.sequence_storage
                        .store_sequence(&seq, &header, source.clone())
                })
                .collect::<Result<Vec<_>>>()?;

            // Track results
            for (seq, hash) in chunk.iter().zip(batch_results.iter()) {
                // Check if it's new by checking sequence storage
                // For simplicity, assume all are new
                new_count += 1;

                let taxon_id = seq.taxon_id.map(|t| TaxonId(t)).unwrap_or(TaxonId(0));

                sequence_records.push((hash.clone(), taxon_id, seq.id.clone()));
            }

            storing_progress.set_position(sequence_records.len() as u64);
        }

        // Save indices after all sequences are processed
        self.sequence_storage.save_indices()?;
        storing_progress.finish_and_clear();

        use talaria_utils::display::output::format_number;
        println!(
            "Stored {} sequences ({} new, {} deduplicated)",
            format_number(sequence_records.len()),
            format_number(new_count),
            format_number(dedup_count)
        );

        // Step 2: Build taxonomic hierarchy
        let hierarchy_progress = create_spinner("Building taxonomic hierarchy");
        let hierarchy = self.build_taxonomic_hierarchy(&sequence_records)?;
        hierarchy_progress.finish_and_clear();

        // Step 3: Create hierarchical chunks
        let chunking_progress =
            create_progress_bar(hierarchy.len() as u64, "Creating hierarchical chunks");

        let mut manifests = Vec::new();
        for (rank, rank_groups) in hierarchy {
            for (parent_taxon, sequences_in_group) in rank_groups {
                let rank_manifests =
                    self.create_rank_manifests(rank, parent_taxon, sequences_in_group)?;
                manifests.extend(rank_manifests);
            }
            chunking_progress.inc(1);
        }

        chunking_progress.finish_and_clear();
        println!("Created {} hierarchical chunk manifests", manifests.len());

        // Step 4: Apply cross-level optimization
        let optimization_progress = create_spinner("Optimizing chunk hierarchy");
        manifests = self.optimize_hierarchy(manifests)?;
        optimization_progress.finish_and_clear();

        Ok(manifests)
    }

    /// Build hierarchical taxonomic structure from sequences
    fn build_taxonomic_hierarchy(
        &self,
        sequence_records: &[(SHA256Hash, TaxonId, String)],
    ) -> Result<HashMap<TaxonomicRank, HashMap<TaxonId, Vec<SHA256Hash>>>> {
        let mut hierarchy: HashMap<TaxonomicRank, HashMap<TaxonId, Vec<SHA256Hash>>> =
            HashMap::new();

        // Initialize all ranks
        for rank in TaxonomicRank::all() {
            hierarchy.insert(rank, HashMap::new());
        }

        // Group sequences by their taxonomic lineage
        for (hash, taxon_id, _) in sequence_records {
            // Get lineage for this taxon
            let lineage = self.get_taxonomic_lineage(*taxon_id)?;

            // Add sequence to appropriate groups at each rank
            for (rank, parent_taxon) in lineage {
                hierarchy
                    .get_mut(&rank)
                    .unwrap()
                    .entry(parent_taxon)
                    .or_default()
                    .push(hash.clone());
            }
        }

        Ok(hierarchy)
    }

    /// Get taxonomic lineage for a taxon ID
    fn get_taxonomic_lineage(&self, taxon_id: TaxonId) -> Result<Vec<(TaxonomicRank, TaxonId)>> {
        // If we have a taxonomy manager, use it
        if let Some(tax_mgr) = &self.taxonomy_manager {
            // Query the actual taxonomy tree from RocksDB
            let nodes = tax_mgr.get_lineage(&taxon_id)?;

            // Convert TaxonomyNode to (TaxonomicRank, TaxonId)
            let lineage = nodes
                .into_iter()
                .map(|node| {
                    let rank = TaxonomicRank::from_string(&node.rank);
                    (rank, node.taxon_id)
                })
                .collect();

            return Ok(lineage);
        }

        // No taxonomy available - return minimal lineage
        Err(anyhow::anyhow!("Taxonomy data not available. Please download with: talaria database download ncbi/taxonomy"))
    }

    /// Create manifests for a specific taxonomic rank
    fn create_rank_manifests(
        &self,
        rank: TaxonomicRank,
        parent_taxon: TaxonId,
        sequence_hashes: Vec<SHA256Hash>,
    ) -> Result<Vec<ChunkManifest>> {
        let mut manifests = Vec::new();
        let mut current_refs = Vec::new();
        let mut current_size = 0;

        let min_size = rank.min_chunk_size();
        let target_size = rank.target_chunk_size();
        let max_size = rank.max_chunk_size();

        // Estimate size based on average sequence length
        const AVG_SEQUENCE_SIZE: usize = 1000;

        for hash in sequence_hashes {
            let estimated_size = AVG_SEQUENCE_SIZE;

            // Check if we should create a new chunk
            if current_size + estimated_size > max_size
                || (current_size > target_size && current_refs.len() > 100)
            {
                // Create manifest
                if current_size >= min_size || rank == TaxonomicRank::Subspecies {
                    manifests.push(self.create_hierarchical_manifest(
                        rank,
                        vec![parent_taxon],
                        current_refs,
                    )?);
                }

                // Start new manifest
                current_refs = vec![hash];
                current_size = estimated_size;
            } else {
                current_refs.push(hash);
                current_size += estimated_size;
            }
        }

        // Create final manifest if it meets minimum size
        if !current_refs.is_empty()
            && (current_size >= min_size || rank == TaxonomicRank::Subspecies)
        {
            manifests.push(self.create_hierarchical_manifest(
                rank,
                vec![parent_taxon],
                current_refs,
            )?);
        }

        Ok(manifests)
    }

    /// Create a hierarchical chunk manifest
    fn create_hierarchical_manifest(
        &self,
        rank: TaxonomicRank,
        taxon_ids: Vec<TaxonId>,
        sequence_refs: Vec<SHA256Hash>,
    ) -> Result<ChunkManifest> {
        // Compute manifest hash
        let mut sorted_refs = sequence_refs.clone();
        sorted_refs.sort();

        let manifest_data = format!(
            "{}:{}:{}",
            rank.as_str(),
            taxon_ids
                .iter()
                .map(|t| t.0.to_string())
                .collect::<Vec<_>>()
                .join(","),
            sorted_refs
                .iter()
                .map(|h| h.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        let chunk_hash = SHA256Hash::compute(manifest_data.as_bytes());

        // Classify chunk based on rank
        // For subspecies, use delta compression with reference to parent species
        let chunk_type = if rank == TaxonomicRank::Subspecies {
            // Find a reference hash (use first as reference for now)
            let reference_hash = sequence_refs
                .first()
                .cloned()
                .unwrap_or_else(|| SHA256Hash::compute(b"no_ref"));
            ChunkClassification::Delta {
                reference_hash,
                compression_ratio: 0.5, // Estimated
            }
        } else {
            ChunkClassification::Full
        };

        Ok(ChunkManifest {
            chunk_hash,
            sequence_refs: sequence_refs.clone(),
            taxon_ids,
            chunk_type,
            total_size: sequence_refs.len() * 1000, // Estimate
            sequence_count: sequence_refs.len(),
            created_at: Utc::now(),
            taxonomy_version: self.get_taxonomy_version(),
            sequence_version: self.get_sequence_version(),
        })
    }

    /// Optimize the chunk hierarchy to eliminate redundancy
    fn optimize_hierarchy(&self, manifests: Vec<ChunkManifest>) -> Result<Vec<ChunkManifest>> {
        let mut optimized = Vec::new();
        let mut sequence_coverage: HashSet<SHA256Hash> = HashSet::new();

        // Process from highest to lowest rank (domain to subspecies)
        let mut manifests_by_rank: HashMap<String, Vec<ChunkManifest>> = HashMap::new();

        for manifest in &manifests {
            // Use chunk type as proxy for rank level
            let rank_key = match &manifest.chunk_type {
                ChunkClassification::Full => "high",
                ChunkClassification::Delta { .. } => "low",
                _ => "high", // Default to high for other types
            };
            manifests_by_rank
                .entry(rank_key.to_string())
                .or_default()
                .push(manifest.clone());
        }

        // Process high-level chunks first
        for rank_key in ["high", "low"] {
            if let Some(rank_manifests) = manifests_by_rank.get(rank_key) {
                for manifest in rank_manifests {
                    // Check if this chunk adds new sequences
                    let new_sequences: Vec<SHA256Hash> = manifest
                        .sequence_refs
                        .iter()
                        .filter(|s| !sequence_coverage.contains(s))
                        .cloned()
                        .collect();

                    // Only keep chunk if it adds significant new coverage
                    if new_sequences.len() > manifest.sequence_refs.len() / 2 {
                        for seq in &manifest.sequence_refs {
                            sequence_coverage.insert(seq.clone());
                        }
                        optimized.push(manifest.clone());
                    }
                }
            }
        }

        let original_count = manifests.len();
        println!(
            "Optimized from {} to {} chunks (removed {} redundant)",
            original_count,
            optimized.len(),
            original_count - optimized.len()
        );

        Ok(optimized)
    }

    fn get_taxonomy_version(&self) -> SHA256Hash {
        if let Some(tax_mgr) = &self.taxonomy_manager {
            if tax_mgr.has_taxonomy() {
                tax_mgr
                    .get_taxonomy_root()
                    .unwrap_or_else(|_| SHA256Hash::compute(b"v1"))
            } else {
                SHA256Hash::compute(b"no_taxonomy")
            }
        } else {
            SHA256Hash::compute(b"no_taxonomy")
        }
    }

    fn get_sequence_version(&self) -> SHA256Hash {
        SHA256Hash::compute(format!("seq_v_{}", Utc::now().timestamp()).as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    #[serial_test::serial]
    fn test_taxonomic_rank_sizes() {
        assert!(TaxonomicRank::Domain.min_chunk_size() > TaxonomicRank::Species.min_chunk_size());
        assert_eq!(
            TaxonomicRank::Genus.target_chunk_size(),
            TaxonomicRank::Genus.min_chunk_size() * 2
        );
    }

    #[test]
    #[serial_test::serial]
    fn test_hierarchical_chunking() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let sequence_storage = SequenceStorage::new(temp_dir.path())?;

        let mut chunker = HierarchicalTaxonomicChunker::new(
            super::super::ChunkingStrategy::default(),
            sequence_storage,
            DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt),
            None,
        );

        // Create test sequences
        let sequences = vec![
            Sequence {
                id: "seq1".to_string(),
                description: Some("E. coli sequence".to_string()),
                sequence: b"ACGT".to_vec(),
                taxon_id: Some(511145), // E. coli K-12
                taxonomy_sources: Default::default(),
            },
            Sequence {
                id: "seq2".to_string(),
                description: Some("Human sequence".to_string()),
                sequence: b"TGCA".to_vec(),
                taxon_id: Some(9606), // Human
                taxonomy_sources: Default::default(),
            },
        ];

        let manifests = chunker.chunk_sequences_hierarchical(sequences)?;

        // Should create manifests for different taxonomic levels
        assert!(!manifests.is_empty());

        Ok(())
    }
}
