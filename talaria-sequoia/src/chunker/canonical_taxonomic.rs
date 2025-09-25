/// Taxonomic chunker that creates manifests referencing canonical sequences
use crate::storage::sequence::SequenceStorage;
use crate::types::{
    ChunkManifest, ChunkClassification, DatabaseSource, SHA256Hash, TaxonId,
};
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use talaria_bio::sequence::Sequence;
use talaria_utils::display::progress::{create_progress_bar, create_spinner};

/// Taxonomic chunker that works with canonical sequences
pub struct TaxonomicChunker {
    strategy: super::ChunkingStrategy,
    pub sequence_storage: SequenceStorage,
    database_source: DatabaseSource,
}

impl TaxonomicChunker {
    pub fn new(
        strategy: super::ChunkingStrategy,
        sequence_storage: SequenceStorage,
        database_source: DatabaseSource,
    ) -> Self {
        Self {
            strategy,
            sequence_storage,
            database_source,
        }
    }

    /// Chunk sequences by storing them canonically and creating manifests
    pub fn chunk_sequences_canonical(
        &mut self,
        sequences: Vec<Sequence>,
    ) -> Result<Vec<ChunkManifest>> {
        use rayon::prelude::*;

        // Step 1: Pre-process sequences in parallel to prepare data
        let storing_progress = create_progress_bar(
            sequences.len() as u64,
            "Processing sequences",
        );

        // Process sequences in parallel to prepare headers and convert to strings
        let prepared_sequences: Vec<_> = sequences
            .par_iter()
            .map(|seq| {
                // Extract header (for representation)
                let header = format!(
                    ">{}{}",
                    seq.id,
                    seq.description
                        .as_ref()
                        .map(|d| format!(" {}", d))
                        .unwrap_or_default()
                );

                // Convert sequence to string
                let sequence_str = String::from_utf8(seq.sequence.clone())
                    .unwrap_or_else(|_| String::new());

                // Get taxonomic classification
                let taxon_id = seq.taxon_id
                    .map(|t| TaxonId(t))
                    .unwrap_or(TaxonId(0));

                (seq.id.clone(), header, sequence_str, taxon_id)
            })
            .collect();

        storing_progress.finish_and_clear();

        // Step 2: Store sequences in parallel batches (optimized for performance)
        let storing_progress = create_progress_bar(
            prepared_sequences.len() as u64,
            "Storing canonical sequences",
        );

        const BATCH_SIZE: usize = 10000;
        let mut all_results = Vec::with_capacity(prepared_sequences.len());
        let mut dedup_count = 0;
        let mut new_count = 0;

        // Process sequences in batches for optimal performance
        for chunk in prepared_sequences.chunks(BATCH_SIZE) {
            // Prepare batch for parallel storage
            let batch_data: Vec<(&str, &str, crate::types::DatabaseSource)> = chunk
                .iter()
                .map(|(_, header, sequence_str, _)| {
                    (sequence_str.as_str(), header.as_str(), self.database_source.clone())
                })
                .collect();

            // Store batch in parallel
            let batch_results = self.sequence_storage.store_sequences_batch(batch_data)?;

            // Track results
            for ((id, _, _, taxon_id), (hash, is_new)) in chunk.iter().zip(batch_results.iter()) {
                if *is_new {
                    new_count += 1;
                } else {
                    dedup_count += 1;
                }
                all_results.push((hash.clone(), *taxon_id, id.clone()));
            }

            // Update progress
            storing_progress.set_position(all_results.len() as u64);
        }

        // Save indices ONCE after all sequences are processed
        self.sequence_storage.save_indices()?;

        storing_progress.finish_and_clear();
        println!(
            "Stored {} sequences ({} new, {} deduplicated)",
            all_results.len(),
            new_count,
            dedup_count
        );

        let sequence_records = all_results;

        // Step 2: Group sequences by taxonomy
        let grouping_progress = create_progress_bar(
            sequence_records.len() as u64,
            "Grouping by taxonomy",
        );

        let mut taxon_groups: HashMap<TaxonId, Vec<SHA256Hash>> = HashMap::new();
        for (hash, taxon_id, _) in &sequence_records {
            taxon_groups.entry(*taxon_id)
                .or_default()
                .push(hash.clone());
            grouping_progress.inc(1);
        }

        grouping_progress.finish_and_clear();
        println!("Grouped into {} taxonomic groups", taxon_groups.len());

        // Step 3: Create chunk manifests
        let chunking_progress = create_progress_bar(
            taxon_groups.len() as u64,
            "Creating chunk manifests",
        );

        let mut manifests = Vec::new();
        for (taxon_id, sequence_hashes) in taxon_groups {
            let group_manifests = self.create_manifests_for_group(taxon_id, sequence_hashes)?;
            manifests.extend(group_manifests);
            chunking_progress.inc(1);
        }

        chunking_progress.finish_and_clear();
        println!("Created {} chunk manifests", manifests.len());

        // Step 4: Apply special taxa rules
        let special_progress = create_spinner("Applying special taxa rules");
        manifests = self.apply_special_taxa_rules(manifests)?;
        special_progress.finish_and_clear();

        Ok(manifests)
    }

    /// Create manifests for a taxonomic group
    fn create_manifests_for_group(
        &self,
        taxon_id: TaxonId,
        sequence_hashes: Vec<SHA256Hash>,
    ) -> Result<Vec<ChunkManifest>> {
        let mut manifests = Vec::new();
        let mut current_refs = Vec::new();
        let mut current_size = 0;

        // Estimate size based on average sequence length (1000 bytes typical)
        const AVG_SEQUENCE_SIZE: usize = 1000;

        for hash in sequence_hashes {
            let estimated_size = AVG_SEQUENCE_SIZE; // In production, load and check actual size

            // Check if adding this sequence would exceed limits
            if current_size + estimated_size > self.strategy.max_chunk_size
                || (current_size > self.strategy.target_chunk_size
                    && current_refs.len() >= self.strategy.min_sequences_per_chunk)
            {
                // Create manifest
                if !current_refs.is_empty() {
                    manifests.push(self.create_manifest(vec![taxon_id], current_refs)?);
                }

                // Start new manifest
                current_refs = vec![hash];
                current_size = estimated_size;
            } else {
                current_refs.push(hash);
                current_size += estimated_size;
            }
        }

        // Create final manifest
        if !current_refs.is_empty() {
            manifests.push(self.create_manifest(vec![taxon_id], current_refs)?);
        }

        Ok(manifests)
    }

    /// Create a chunk manifest
    fn create_manifest(
        &self,
        taxon_ids: Vec<TaxonId>,
        sequence_refs: Vec<SHA256Hash>,
    ) -> Result<ChunkManifest> {
        // Compute manifest hash from sorted references
        let mut sorted_refs = sequence_refs.clone();
        sorted_refs.sort();
        let manifest_data: Vec<u8> = sorted_refs
            .iter()
            .flat_map(|h| h.as_bytes().iter())
            .copied()
            .collect();
        let chunk_hash = SHA256Hash::compute(&manifest_data);

        // Get version hashes
        let taxonomy_version = self.get_taxonomy_version();
        let sequence_version = self.get_sequence_version();

        Ok(ChunkManifest {
            chunk_hash,
            sequence_refs: sequence_refs.clone(),
            taxon_ids,
            chunk_type: ChunkClassification::Full,
            total_size: sequence_refs.len() * 1000, // Estimate
            sequence_count: sequence_refs.len(),
            created_at: Utc::now(),
            taxonomy_version,
            sequence_version,
        })
    }

    /// Apply special handling for important taxa
    fn apply_special_taxa_rules(
        &self,
        manifests: Vec<ChunkManifest>,
    ) -> Result<Vec<ChunkManifest>> {
        let mut final_manifests = Vec::new();
        let mut special_taxa_manifests: HashMap<TaxonId, Vec<ChunkManifest>> = HashMap::new();

        // Model organisms (always get dedicated chunks)
        let model_organisms = vec![
            TaxonId(9606),   // Human
            TaxonId(10090),  // Mouse
            TaxonId(7227),   // Drosophila
            TaxonId(6239),   // C. elegans
            TaxonId(559292), // S. cerevisiae
            TaxonId(511145), // E. coli K-12
        ];

        // Pathogenic organisms (grouped together for efficiency)
        let pathogens = vec![
            TaxonId(1773),   // Mycobacterium tuberculosis
            TaxonId(210),    // Helicobacter pylori
            TaxonId(573),    // Klebsiella pneumoniae
            TaxonId(1280),   // Staphylococcus aureus
            TaxonId(1313),   // Streptococcus pneumoniae
        ];

        for manifest in manifests {
            let mut is_special = false;

            // Check for model organisms
            for &model_taxon in &model_organisms {
                if manifest.taxon_ids.contains(&model_taxon) {
                    special_taxa_manifests
                        .entry(model_taxon)
                        .or_default()
                        .push(manifest.clone());
                    is_special = true;
                    break;
                }
            }

            // Check for pathogens (group them)
            if !is_special {
                for &pathogen_taxon in &pathogens {
                    if manifest.taxon_ids.contains(&pathogen_taxon) {
                        // Group all pathogens under a special ID
                        special_taxa_manifests
                            .entry(TaxonId(999999)) // Special pathogen group ID
                            .or_default()
                            .push(manifest.clone());
                        is_special = true;
                        break;
                    }
                }
            }

            if !is_special {
                final_manifests.push(manifest);
            }
        }

        // Process special taxa
        for (taxon_id, mut manifests) in special_taxa_manifests {
            if taxon_id == TaxonId(999999) {
                // Merge pathogen manifests if they're small
                let all_refs: Vec<SHA256Hash> = manifests
                    .iter()
                    .flat_map(|m| m.sequence_refs.clone())
                    .collect();

                if all_refs.len() < 1000 {  // Max sequences per chunk
                    // Create single manifest for all pathogens
                    let pathogen_manifest = self.create_manifest(
                        pathogens.clone(),
                        all_refs,
                    )?;
                    final_manifests.push(pathogen_manifest);
                } else {
                    // Keep separate
                    final_manifests.append(&mut manifests);
                }
            } else {
                // Model organisms - keep dedicated chunks
                final_manifests.append(&mut manifests);
            }
        }

        Ok(final_manifests)
    }

    fn get_taxonomy_version(&self) -> SHA256Hash {
        // Get actual taxonomy version from taxonomy manager
        use talaria_core::system::paths;

        if let Ok(tax_mgr) = crate::taxonomy::TaxonomyManager::new(&paths::talaria_databases_dir()) {
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
        // In production, this would come from the manifest's sequence root
        SHA256Hash::compute(format!("seq_v_{}", Utc::now().timestamp()).as_bytes())
    }
}

// Benefits of this approach:
// 1. True deduplication - sequences stored once across all databases
// 2. Manifests are lightweight - just references to sequences
// 3. Cross-database efficiency - same sequence in UniProt and NCBI stored once
// 4. Maintains taxonomic organization for efficient access
// 5. Special taxa handling ensures important organisms are easily accessible