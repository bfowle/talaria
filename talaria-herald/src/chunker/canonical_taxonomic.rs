/// Taxonomic chunker that creates manifests referencing canonical sequences
use crate::storage::sequence::SequenceStorage;
use crate::types::{ChunkClassification, ChunkManifest, DatabaseSource, SHA256Hash, TaxonId};
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use talaria_bio::sequence::Sequence;

/// Taxonomic chunker that works with canonical sequences
pub struct TaxonomicChunker {
    strategy: super::ChunkingStrategy,
    pub sequence_storage: Arc<SequenceStorage>,
    database_source: DatabaseSource,
    quiet_mode: bool,
}

impl TaxonomicChunker {
    pub fn new(
        strategy: super::ChunkingStrategy,
        sequence_storage: Arc<SequenceStorage>,
        database_source: DatabaseSource,
    ) -> Self {
        Self {
            strategy,
            sequence_storage,
            database_source,
            quiet_mode: false,
        }
    }

    /// Set quiet mode (suppress progress bars)
    pub fn set_quiet_mode(&mut self, quiet: bool) {
        self.quiet_mode = quiet;
    }

    /// Chunk sequences by storing them canonically and creating manifests (quiet version)
    pub fn chunk_sequences_canonical_quiet(
        &mut self,
        sequences: Vec<Sequence>,
    ) -> Result<Vec<ChunkManifest>> {
        // Default to not being the final batch
        self.chunk_sequences_canonical_quiet_final(sequences, false)
    }

    /// Chunk sequences by storing them canonically and creating manifests
    pub fn chunk_sequences_canonical(
        &mut self,
        sequences: Vec<Sequence>,
    ) -> Result<Vec<ChunkManifest>> {
        self.chunk_sequences_canonical_internal(sequences, None, false)
    }

    /// Chunk sequences with progress callback
    pub fn chunk_sequences_canonical_with_progress(
        &mut self,
        sequences: Vec<Sequence>,
        progress_callback: Option<Box<dyn Fn(usize, &str) + Send>>,
    ) -> Result<Vec<ChunkManifest>> {
        // Default to not being the final batch
        self.chunk_sequences_canonical_with_progress_final(sequences, progress_callback, false)
    }

    /// Chunk sequences with progress callback and final batch indicator
    pub fn chunk_sequences_canonical_with_progress_final(
        &mut self,
        sequences: Vec<Sequence>,
        progress_callback: Option<Box<dyn Fn(usize, &str) + Send>>,
        is_final_batch: bool,
    ) -> Result<Vec<ChunkManifest>> {
        // Set quiet mode and use progress callback
        let was_quiet = self.quiet_mode;
        self.quiet_mode = true;
        let result =
            self.chunk_sequences_canonical_internal(sequences, progress_callback, is_final_batch);
        self.quiet_mode = was_quiet;
        result
    }

    /// Quiet version with final batch indicator
    pub fn chunk_sequences_canonical_quiet_final(
        &mut self,
        sequences: Vec<Sequence>,
        is_final_batch: bool,
    ) -> Result<Vec<ChunkManifest>> {
        // Set quiet mode temporarily
        let was_quiet = self.quiet_mode;
        self.quiet_mode = true;
        let result = self.chunk_sequences_canonical_internal(sequences, None, is_final_batch);
        self.quiet_mode = was_quiet;
        result
    }

    /// Internal implementation of chunk_sequences_canonical
    fn chunk_sequences_canonical_internal(
        &mut self,
        sequences: Vec<Sequence>,
        progress_callback: Option<Box<dyn Fn(usize, &str) + Send>>,
        is_final_batch: bool,
    ) -> Result<Vec<ChunkManifest>> {
        use rayon::prelude::*;

        // Step 1: Pre-process sequences in parallel to prepare data
        tracing::debug!("Processing {} sequences", sequences.len());

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
                let sequence_str =
                    String::from_utf8(seq.sequence.clone()).unwrap_or_else(|_| String::new());

                // Get taxonomic classification
                let taxon_id = seq.taxon_id.map(|t| TaxonId(t)).unwrap_or(TaxonId(0));

                (seq.id.clone(), header, sequence_str, taxon_id)
            })
            .collect();

        // Notify progress callback about preparation completion
        if let Some(ref callback) = progress_callback {
            callback(prepared_sequences.len(), "Prepared sequences");
        }

        // Step 2: Store sequences in parallel batches (optimized for performance)
        tracing::debug!("Storing {} canonical sequences", prepared_sequences.len());

        // Optimized batch sizes for high-throughput processing
        const BATCH_SIZE: usize = 200_000; // Process 200k sequences at once (increased from 50k)
        const MINI_BATCH_SIZE: usize = 50_000; // Larger mini-batches for better throughput (increased from 10k)
        let mut all_results = Vec::with_capacity(prepared_sequences.len());
        let mut dedup_count = 0;
        let mut new_count = 0;
        let mut mini_batch_count = 0;

        // Process sequences in batches for optimal performance
        for chunk in prepared_sequences.chunks(BATCH_SIZE) {
            // Process in smaller mini-batches for more frequent progress updates
            for mini_chunk in chunk.chunks(MINI_BATCH_SIZE) {
                // Prepare mini-batch for parallel storage
                let batch_data: Vec<(&str, &str, crate::types::DatabaseSource)> = mini_chunk
                    .iter()
                    .map(|(_, header, sequence_str, _)| {
                        (
                            sequence_str.as_str(),
                            header.as_str(),
                            self.database_source.clone(),
                        )
                    })
                    .collect();

                // Store mini-batch in parallel
                let batch_results = self.sequence_storage.store_sequences_batch(batch_data)?;

                // Track results
                for ((id, _, _, taxon_id), (hash, is_new)) in
                    mini_chunk.iter().zip(batch_results.iter())
                {
                    if *is_new {
                        new_count += 1;
                    } else {
                        dedup_count += 1;
                    }
                    all_results.push((hash.clone(), *taxon_id, id.clone()));
                }

                // Update progress callback less frequently to reduce overhead
                mini_batch_count += 1;
                if mini_batch_count % 5 == 0 {
                    // Update every 50k sequences
                    if let Some(ref callback) = progress_callback {
                        callback(
                            all_results.len(),
                            &format!(
                                "Storing sequences ({} new, {} dedup)",
                                new_count, dedup_count
                            ),
                        );
                    }
                }
            }
        }

        // Save indices only on the final batch to avoid blocking
        if is_final_batch {
            if let Some(ref callback) = progress_callback {
                callback(all_results.len(), "Saving indices...");
            }
            self.sequence_storage.save_indices()?;
        }

        if !self.quiet_mode {
            tracing::info!(
                "Stored {} sequences ({} new, {} deduplicated)",
                all_results.len(),
                new_count,
                dedup_count
            );
        }

        let sequence_records = all_results;

        // Step 2: Group sequences by taxonomy
        tracing::debug!("Grouping {} sequences by taxonomy", sequence_records.len());

        let mut taxon_groups: HashMap<TaxonId, Vec<SHA256Hash>> = HashMap::new();
        for (hash, taxon_id, _) in &sequence_records {
            taxon_groups
                .entry(*taxon_id)
                .or_default()
                .push(hash.clone());
        }

        // Notify progress callback about grouping completion
        if let Some(ref callback) = progress_callback {
            callback(
                sequence_records.len(),
                &format!("Grouped into {} taxa", taxon_groups.len()),
            );
        }
        if !self.quiet_mode {
            // Show sample of taxon IDs for debugging
            let sample_taxids: Vec<String> = taxon_groups
                .keys()
                .take(3)
                .map(|tid| format!("{}", tid.0))
                .collect();
            let taxid_info = if taxon_groups.len() == 1 {
                format!(" (taxon_id: {})", sample_taxids.join(", "))
            } else if taxon_groups.len() > 3 {
                format!(" (sample taxon_ids: {}, ...)", sample_taxids.join(", "))
            } else {
                format!(" (taxon_ids: {})", sample_taxids.join(", "))
            };
            tracing::info!(
                "Grouped into {} taxonomic groups{}",
                taxon_groups.len(),
                taxid_info
            );
        }

        // Step 3: Create chunk manifests
        tracing::debug!("Creating chunk manifests for {} taxonomic groups", taxon_groups.len());

        // Compute versions ONCE before parallel processing to avoid file system contention
        let taxonomy_version = self.get_taxonomy_version();
        let sequence_version = self.get_sequence_version();

        // Process taxonomic groups in chunks to avoid thread pool saturation
        // IMPORTANT: Processing all 10,000+ groups in parallel causes deadlock

        // Convert HashMap to Vec for chunking
        let taxa_vec: Vec<_> = taxon_groups.into_iter().collect();

        // Use adaptive chunk size based on number of taxonomic groups
        let num_taxa = taxa_vec.len();
        let parallel_chunk_size = if num_taxa > 5000 {
            // Very large number of taxa - use smaller chunks to avoid overwhelming the system
            10
        } else if num_taxa > 1000 {
            // Large number of taxa - use moderate chunks
            25
        } else if num_taxa > 100 {
            // Moderate number - can handle larger chunks
            50
        } else {
            // Small number - process more in parallel
            100
        };

        if !self.quiet_mode && num_taxa > 1000 {
            tracing::info!(
                "Processing {} taxonomic groups in chunks of {} (large dataset mode)",
                num_taxa, parallel_chunk_size
            );
        }

        let mut all_manifest_results = Vec::new();
        let total_chunks = (taxa_vec.len() + parallel_chunk_size - 1) / parallel_chunk_size;

        for (chunk_idx, chunk) in taxa_vec.chunks(parallel_chunk_size).enumerate() {
            // Add progress reporting for large datasets
            if !self.quiet_mode {
                if num_taxa > 5000 && chunk_idx % 100 == 0 {
                    tracing::info!(
                        "  Processing taxonomic chunk {}/{}",
                        chunk_idx + 1,
                        total_chunks
                    );
                } else if num_taxa > 1000 && chunk_idx % 10 == 0 {
                    tracing::info!(
                        "DEBUG: Processing chunk {}/{} ({} taxa in this chunk)",
                        chunk_idx + 1,
                        total_chunks,
                        chunk.len()
                    );
                }
            }

            // Process this chunk in parallel
            let chunk_results: Result<Vec<_>> = chunk
                .par_iter()
                .map(|(taxon_id, sequence_hashes)| {
                    self.create_manifests_for_group(
                        *taxon_id,
                        sequence_hashes.clone(),
                        taxonomy_version.clone(),
                        sequence_version.clone(),
                    )
                })
                .collect();

            // Collect results from this chunk
            match chunk_results {
                Ok(results) => all_manifest_results.extend(results),
                Err(e) => return Err(e),
            }

            // Log progress for large datasets
            if !self.quiet_mode && num_taxa > 1000 {
                tracing::debug!("Processed chunk {}/{}", chunk_idx + 1, total_chunks);
            }
        }

        // Flatten all results
        let mut manifests: Vec<ChunkManifest> =
            all_manifest_results.into_iter().flatten().collect();
        if !self.quiet_mode {
            tracing::info!("Created {} chunk manifests", manifests.len());
        }

        // Step 4: Apply special taxa rules
        tracing::debug!("Applying special taxa rules");
        manifests =
            self.apply_special_taxa_rules(manifests, taxonomy_version, sequence_version)?;

        Ok(manifests)
    }

    /// Create manifests for a taxonomic group
    fn create_manifests_for_group(
        &self,
        taxon_id: TaxonId,
        sequence_hashes: Vec<SHA256Hash>,
        taxonomy_version: SHA256Hash,
        sequence_version: SHA256Hash,
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
                    manifests.push(self.create_manifest(
                        vec![taxon_id],
                        current_refs,
                        taxonomy_version.clone(),
                        sequence_version.clone(),
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

        // Create final manifest
        if !current_refs.is_empty() {
            manifests.push(self.create_manifest(
                vec![taxon_id],
                current_refs,
                taxonomy_version,
                sequence_version,
            )?);
        }

        Ok(manifests)
    }

    /// Create a chunk manifest
    fn create_manifest(
        &self,
        taxon_ids: Vec<TaxonId>,
        sequence_refs: Vec<SHA256Hash>,
        taxonomy_version: SHA256Hash,
        sequence_version: SHA256Hash,
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

        // Versions are now passed as parameters to avoid file system contention
        // No longer calling self.get_taxonomy_version() from parallel context

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
        taxonomy_version: SHA256Hash,
        sequence_version: SHA256Hash,
    ) -> Result<Vec<ChunkManifest>> {
        // Skip special taxa rules for very large datasets to avoid performance issues
        if manifests.len() > 1000 {
            return Ok(manifests);
        }

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
            TaxonId(1773), // Mycobacterium tuberculosis
            TaxonId(210),  // Helicobacter pylori
            TaxonId(573),  // Klebsiella pneumoniae
            TaxonId(1280), // Staphylococcus aureus
            TaxonId(1313), // Streptococcus pneumoniae
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

                if all_refs.len() < 1000 {
                    // Max sequences per chunk
                    // Create single manifest for all pathogens
                    let pathogen_manifest = self.create_manifest(
                        pathogens.clone(),
                        all_refs,
                        taxonomy_version,
                        sequence_version,
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

        if let Ok(tax_mgr) = crate::taxonomy::TaxonomyManager::new(&paths::talaria_databases_dir())
        {
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
