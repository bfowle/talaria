/// Comprehensive database comparison functionality
use crate::{ChunkManifest, HeraldRepository, SHA256Hash, TaxonId};
use anyhow::Result;
use dashmap::DashSet;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Comprehensive database comparison result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseComparison {
    /// Chunk-level analysis
    pub chunk_analysis: ChunkAnalysis,
    /// Sequence-level analysis
    pub sequence_analysis: SequenceAnalysis,
    /// Taxonomy distribution analysis
    pub taxonomy_analysis: TaxonomyAnalysis,
    /// Storage metrics
    pub storage_metrics: StorageMetrics,
}

/// Chunk-level comparison results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkAnalysis {
    /// Total chunks in first database
    pub total_chunks_a: usize,
    /// Total chunks in second database
    pub total_chunks_b: usize,
    /// Chunks present in both databases
    pub shared_chunks: Vec<SHA256Hash>,
    /// Chunks only in first database
    pub unique_to_a: Vec<SHA256Hash>,
    /// Chunks only in second database
    pub unique_to_b: Vec<SHA256Hash>,
    /// Percentage of chunks from A that are shared
    pub shared_percentage_a: f64,
    /// Percentage of chunks from B that are shared
    pub shared_percentage_b: f64,
}

/// Sequence-level comparison results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceAnalysis {
    /// Total sequences in first database
    pub total_sequences_a: usize,
    /// Total sequences in second database
    pub total_sequences_b: usize,
    /// Sequence hashes present in both databases
    pub shared_sequences: usize,
    /// Sequences only in first database
    pub unique_to_a: usize,
    /// Sequences only in second database
    pub unique_to_b: usize,
    /// Sample shared sequence IDs (for display)
    pub sample_shared_ids: Vec<String>,
    /// Sample unique to A sequence IDs
    pub sample_unique_a_ids: Vec<String>,
    /// Sample unique to B sequence IDs
    pub sample_unique_b_ids: Vec<String>,
    /// Percentage of sequences from A that are shared
    pub shared_percentage_a: f64,
    /// Percentage of sequences from B that are shared
    pub shared_percentage_b: f64,
}

/// Taxonomy distribution comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyAnalysis {
    /// Total unique taxa in first database
    pub total_taxa_a: usize,
    /// Total unique taxa in second database
    pub total_taxa_b: usize,
    /// Taxa present in both databases
    pub shared_taxa: Vec<TaxonId>,
    /// Taxa only in first database
    pub unique_to_a: Vec<TaxonId>,
    /// Taxa only in second database
    pub unique_to_b: Vec<TaxonId>,
    /// Top shared taxa by sequence count
    pub top_shared_taxa: Vec<TaxonDistribution>,
    /// Percentage of taxa from A that are shared
    pub shared_percentage_a: f64,
    /// Percentage of taxa from B that are shared
    pub shared_percentage_b: f64,
}

/// Taxonomy distribution entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonDistribution {
    pub taxon_id: TaxonId,
    pub taxon_name: String,
    pub count_in_a: usize,
    pub count_in_b: usize,
}

/// Storage metrics comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMetrics {
    /// Total size of first database
    pub size_a_bytes: usize,
    /// Total size of second database
    pub size_b_bytes: usize,
    /// Estimated deduplication savings (shared content)
    pub dedup_savings_bytes: usize,
    /// Deduplication ratio for first database
    pub dedup_ratio_a: f32,
    /// Deduplication ratio for second database
    pub dedup_ratio_b: f32,
}

/// Database comparison engine
pub struct DatabaseDiffer {
    repo_a: HeraldRepository,
    repo_b: HeraldRepository,
}

impl DatabaseDiffer {
    /// Create a new database differ
    pub fn new(path_a: &Path, path_b: &Path) -> Result<Self> {
        let repo_a = HeraldRepository::open(path_a)?;
        let repo_b = HeraldRepository::open(path_b)?;

        Ok(Self { repo_a, repo_b })
    }

    /// Compare two database manifests directly (for databases in shared RocksDB)
    pub fn compare_manifests(
        manifest_a: &crate::TemporalManifest,
        manifest_b: &crate::TemporalManifest,
        storage: Option<&crate::storage::HeraldStorage>,
        taxonomy_manager: Option<&crate::taxonomy::TaxonomyManager>,
    ) -> Result<DatabaseComparison> {
        let chunk_analysis = Self::compare_chunks_from_manifests(manifest_a, manifest_b, storage);
        let sequence_analysis =
            Self::compare_sequences_from_manifests(manifest_a, manifest_b, storage);
        let taxonomy_analysis = Self::compare_taxonomies_from_manifests(
            manifest_a,
            manifest_b,
            taxonomy_manager,
            storage,
        );
        let storage_metrics =
            Self::calculate_storage_from_manifests(manifest_a, manifest_b, storage);

        Ok(DatabaseComparison {
            chunk_analysis,
            sequence_analysis,
            taxonomy_analysis,
            storage_metrics,
        })
    }

    /// Perform comprehensive database comparison
    pub fn compare(&self) -> Result<DatabaseComparison> {
        let chunk_analysis = self.compare_chunks()?;
        let sequence_analysis = self.compare_sequences()?;
        let taxonomy_analysis = self.compare_taxonomies()?;
        let storage_metrics = self.calculate_storage_metrics()?;

        Ok(DatabaseComparison {
            chunk_analysis,
            sequence_analysis,
            taxonomy_analysis,
            storage_metrics,
        })
    }

    /// Compare chunks between databases
    fn compare_chunks(&self) -> Result<ChunkAnalysis> {
        // Get all chunk hashes from both repositories
        let chunks_a = self.get_all_chunk_hashes(&self.repo_a)?;
        let chunks_b = self.get_all_chunk_hashes(&self.repo_b)?;

        let set_a: HashSet<_> = chunks_a.iter().cloned().collect();
        let set_b: HashSet<_> = chunks_b.iter().cloned().collect();

        let shared: Vec<_> = set_a.intersection(&set_b).cloned().collect();
        let unique_to_a: Vec<_> = set_a.difference(&set_b).cloned().collect();
        let unique_to_b: Vec<_> = set_b.difference(&set_a).cloned().collect();

        let total_a = chunks_a.len();
        let total_b = chunks_b.len();
        let shared_count = shared.len();

        Ok(ChunkAnalysis {
            total_chunks_a: total_a,
            total_chunks_b: total_b,
            shared_chunks: shared,
            unique_to_a,
            unique_to_b,
            shared_percentage_a: if total_a > 0 {
                (shared_count as f64 / total_a as f64) * 100.0
            } else {
                0.0
            },
            shared_percentage_b: if total_b > 0 {
                (shared_count as f64 / total_b as f64) * 100.0
            } else {
                0.0
            },
        })
    }

    /// Compare sequences between databases
    fn compare_sequences(&self) -> Result<SequenceAnalysis> {
        // Get all sequence hashes from chunk manifests
        let (seqs_a, seq_id_map_a) = self.get_all_sequence_hashes(&self.repo_a)?;
        let (seqs_b, seq_id_map_b) = self.get_all_sequence_hashes(&self.repo_b)?;

        let set_a: HashSet<_> = seqs_a.iter().cloned().collect();
        let set_b: HashSet<_> = seqs_b.iter().cloned().collect();

        let shared: HashSet<_> = set_a.intersection(&set_b).cloned().collect();
        let unique_to_a_set: HashSet<_> = set_a.difference(&set_b).cloned().collect();
        let unique_to_b_set: HashSet<_> = set_b.difference(&set_a).cloned().collect();

        // Get sample IDs for display
        let sample_shared_ids: Vec<String> = shared
            .iter()
            .take(10)
            .filter_map(|hash| {
                seq_id_map_a
                    .get(hash)
                    .or_else(|| seq_id_map_b.get(hash))
                    .cloned()
            })
            .collect();

        let sample_unique_a_ids: Vec<String> = unique_to_a_set
            .iter()
            .take(5)
            .filter_map(|hash| seq_id_map_a.get(hash).cloned())
            .collect();

        let sample_unique_b_ids: Vec<String> = unique_to_b_set
            .iter()
            .take(5)
            .filter_map(|hash| seq_id_map_b.get(hash).cloned())
            .collect();

        let total_a = seqs_a.len();
        let total_b = seqs_b.len();
        let shared_count = shared.len();

        Ok(SequenceAnalysis {
            total_sequences_a: total_a,
            total_sequences_b: total_b,
            shared_sequences: shared_count,
            unique_to_a: unique_to_a_set.len(),
            unique_to_b: unique_to_b_set.len(),
            sample_shared_ids,
            sample_unique_a_ids,
            sample_unique_b_ids,
            shared_percentage_a: if total_a > 0 {
                (shared_count as f64 / total_a as f64) * 100.0
            } else {
                0.0
            },
            shared_percentage_b: if total_b > 0 {
                (shared_count as f64 / total_b as f64) * 100.0
            } else {
                0.0
            },
        })
    }

    /// Compare taxonomy distributions
    fn compare_taxonomies(&self) -> Result<TaxonomyAnalysis> {
        // Get taxonomy distributions from both databases
        let (taxa_a, taxa_counts_a) = self.get_taxonomy_distribution(&self.repo_a)?;
        let (taxa_b, taxa_counts_b) = self.get_taxonomy_distribution(&self.repo_b)?;

        let set_a: HashSet<_> = taxa_a.iter().cloned().collect();
        let set_b: HashSet<_> = taxa_b.iter().cloned().collect();

        let shared: Vec<_> = set_a.intersection(&set_b).cloned().collect();
        let unique_to_a: Vec<_> = set_a.difference(&set_b).cloned().collect();
        let unique_to_b: Vec<_> = set_b.difference(&set_a).cloned().collect();

        // Get top shared taxa by total sequence count
        let mut top_shared: Vec<TaxonDistribution> = shared
            .iter()
            .filter_map(|taxon_id| {
                let count_a = taxa_counts_a.get(taxon_id)?;
                let count_b = taxa_counts_b.get(taxon_id)?;

                // Try to get taxon name from taxonomy manager
                let taxon_name = self
                    .get_taxon_name(&self.repo_a, *taxon_id)
                    .unwrap_or_else(|| format!("TaxID {}", taxon_id.0));

                Some(TaxonDistribution {
                    taxon_id: *taxon_id,
                    taxon_name,
                    count_in_a: *count_a,
                    count_in_b: *count_b,
                })
            })
            .collect();

        // Sort by total count across both databases
        top_shared.sort_by_key(|t| std::cmp::Reverse(t.count_in_a + t.count_in_b));
        top_shared.truncate(10); // Keep top 10

        let total_a = taxa_a.len();
        let total_b = taxa_b.len();
        let shared_count = shared.len();

        Ok(TaxonomyAnalysis {
            total_taxa_a: total_a,
            total_taxa_b: total_b,
            shared_taxa: shared,
            unique_to_a,
            unique_to_b,
            top_shared_taxa: top_shared,
            shared_percentage_a: if total_a > 0 {
                (shared_count as f64 / total_a as f64) * 100.0
            } else {
                0.0
            },
            shared_percentage_b: if total_b > 0 {
                (shared_count as f64 / total_b as f64) * 100.0
            } else {
                0.0
            },
        })
    }

    /// Calculate storage metrics
    fn calculate_storage_metrics(&self) -> Result<StorageMetrics> {
        let stats_a = self.repo_a.storage.get_stats();
        let stats_b = self.repo_b.storage.get_stats();

        // Estimate deduplication savings based on shared chunks
        let chunk_analysis = self.compare_chunks()?;
        let avg_chunk_size = 100_000; // Estimate 100KB average chunk size
        let dedup_savings = chunk_analysis.shared_chunks.len() * avg_chunk_size;

        Ok(StorageMetrics {
            size_a_bytes: stats_a.total_size,
            size_b_bytes: stats_b.total_size,
            dedup_savings_bytes: dedup_savings,
            dedup_ratio_a: stats_a.deduplication_ratio,
            dedup_ratio_b: stats_b.deduplication_ratio,
        })
    }

    /// Get all chunk hashes from a repository
    fn get_all_chunk_hashes(&self, repo: &HeraldRepository) -> Result<Vec<SHA256Hash>> {
        // Get chunks from storage directly
        repo.storage.list_chunks()
    }

    /// Get all sequence hashes and their IDs from a repository
    fn get_all_sequence_hashes(
        &self,
        repo: &HeraldRepository,
    ) -> Result<(Vec<SHA256Hash>, HashMap<SHA256Hash, String>)> {
        let mut all_sequences = Vec::new();
        let mut id_map = HashMap::new();

        // Get all chunks from storage
        let chunks = repo.storage.list_chunks()?;

        for chunk_hash in chunks {
            // Load chunk manifest to get sequence references
            if let Ok(chunk_data) = repo.storage.get_chunk(&chunk_hash) {
                if let Ok(chunk_manifest) = bincode::deserialize::<ChunkManifest>(&chunk_data) {
                    for seq_hash in &chunk_manifest.sequence_refs {
                        all_sequences.push(seq_hash.clone());

                        // Try to get sequence ID for display purposes
                        // For now, we'll use the hash as ID
                        // In a real implementation, we'd load the canonical sequence
                        id_map.insert(seq_hash.clone(), seq_hash.to_hex()[..12].to_string());
                    }
                }
            }
        }

        Ok((all_sequences, id_map))
    }

    /// Get taxonomy distribution from a repository
    fn get_taxonomy_distribution(
        &self,
        repo: &HeraldRepository,
    ) -> Result<(Vec<TaxonId>, HashMap<TaxonId, usize>)> {
        let mut taxa_counts: HashMap<TaxonId, usize> = HashMap::new();

        // Get all chunks from storage
        let chunks = repo.storage.list_chunks()?;

        for chunk_hash in chunks {
            // Load chunk manifest to get taxonomy info
            if let Ok(chunk_data) = repo.storage.get_chunk(&chunk_hash) {
                if let Ok(chunk_manifest) = bincode::deserialize::<ChunkManifest>(&chunk_data) {
                    for taxon_id in &chunk_manifest.taxon_ids {
                        *taxa_counts.entry(*taxon_id).or_insert(0) += chunk_manifest.sequence_count;
                    }
                }
            }
        }

        let taxa: Vec<TaxonId> = taxa_counts.keys().cloned().collect();
        Ok((taxa, taxa_counts))
    }

    /// Get taxon name from taxonomy manager
    fn get_taxon_name(&self, _repo: &HeraldRepository, taxon_id: TaxonId) -> Option<String> {
        // Try to get taxon info from taxonomy manager
        // For now, just return the taxon ID as string since get_taxon_info doesn't exist
        Some(format!("TaxID {}", taxon_id.0))
    }

    /// Load chunk count from RocksDB for streaming manifest
    fn load_chunk_count_for_streaming(
        source_name: &str,
        dataset_name: &str,
        version: &str,
        storage: &crate::storage::HeraldStorage,
    ) -> Result<usize> {
        let rocksdb = storage.sequence_storage.get_rocksdb();
        let count_key = format!(
            "manifest_count:{}:{}:{}",
            source_name, dataset_name, version
        );

        if let Some(data) = rocksdb.get_manifest(&count_key)? {
            if data.len() == 8 {
                return Ok(usize::from_le_bytes(data.try_into().unwrap_or([0u8; 8])));
            }
        }
        Ok(0)
    }

    /// Compare chunks from manifests (static method for shared RocksDB)
    fn compare_chunks_from_manifests(
        manifest_a: &crate::TemporalManifest,
        manifest_b: &crate::TemporalManifest,
        storage: Option<&crate::storage::HeraldStorage>,
    ) -> ChunkAnalysis {
        // Check if manifests are streaming (empty chunk_index)
        let is_streaming_a = manifest_a.etag.starts_with("streaming-");
        let is_streaming_b = manifest_b.etag.starts_with("streaming-");

        let total_a = if is_streaming_a && storage.is_some() {
            // Load count from RocksDB
            if let Some(ref source_db) = manifest_a.source_database {
                let parts: Vec<&str> = source_db.split('/').collect();
                if parts.len() == 2 {
                    Self::load_chunk_count_for_streaming(
                        parts[0],
                        parts[1],
                        &manifest_a.version,
                        storage.unwrap(),
                    )
                    .unwrap_or(0)
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            manifest_a.chunk_index.len()
        };

        let total_b = if is_streaming_b && storage.is_some() {
            // Load count from RocksDB
            if let Some(ref source_db) = manifest_b.source_database {
                let parts: Vec<&str> = source_db.split('/').collect();
                if parts.len() == 2 {
                    Self::load_chunk_count_for_streaming(
                        parts[0],
                        parts[1],
                        &manifest_b.version,
                        storage.unwrap(),
                    )
                    .unwrap_or(0)
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            manifest_b.chunk_index.len()
        };

        // For streaming manifests, extract chunk hashes from partials and compare
        if is_streaming_a || is_streaming_b {
            tracing::info!("Loading chunk hashes for comparison (this may take a few minutes)...");

            // Extract chunks from database A
            let chunks_a = if is_streaming_a && storage.is_some() {
                if let Some(ref source_db) = manifest_a.source_database {
                    let parts: Vec<&str> = source_db.split('/').collect();
                    if parts.len() == 2 {
                        Self::extract_chunk_hashes_from_partials(
                            parts[0],
                            parts[1],
                            &manifest_a.version,
                            storage.unwrap(),
                        )
                        .unwrap_or_else(|e| {
                            tracing::warn!("Failed to extract chunks from {}: {}", source_db, e);
                            HashSet::new()
                        })
                    } else {
                        HashSet::new()
                    }
                } else {
                    HashSet::new()
                }
            } else {
                // Non-streaming: use chunk_index
                manifest_a.chunk_index.iter().map(|m| m.hash).collect()
            };

            // Extract chunks from database B
            let chunks_b = if is_streaming_b && storage.is_some() {
                if let Some(ref source_db) = manifest_b.source_database {
                    let parts: Vec<&str> = source_db.split('/').collect();
                    if parts.len() == 2 {
                        Self::extract_chunk_hashes_from_partials(
                            parts[0],
                            parts[1],
                            &manifest_b.version,
                            storage.unwrap(),
                        )
                        .unwrap_or_else(|e| {
                            tracing::warn!("Failed to extract chunks from {}: {}", source_db, e);
                            HashSet::new()
                        })
                    } else {
                        HashSet::new()
                    }
                } else {
                    HashSet::new()
                }
            } else {
                // Non-streaming: use chunk_index
                manifest_b.chunk_index.iter().map(|m| m.hash).collect()
            };

            tracing::info!(
                "Comparing {} chunks from A vs {} chunks from B",
                chunks_a.len(),
                chunks_b.len()
            );

            // Perform set operations to find shared and unique chunks
            let shared: Vec<_> = chunks_a.intersection(&chunks_b).cloned().collect();
            let unique_to_a: Vec<_> = chunks_a.difference(&chunks_b).cloned().collect();
            let unique_to_b: Vec<_> = chunks_b.difference(&chunks_a).cloned().collect();

            let shared_count = shared.len();

            return ChunkAnalysis {
                total_chunks_a: chunks_a.len(),
                total_chunks_b: chunks_b.len(),
                shared_chunks: shared,
                unique_to_a,
                unique_to_b,
                shared_percentage_a: if !chunks_a.is_empty() {
                    (shared_count as f64 / chunks_a.len() as f64) * 100.0
                } else {
                    0.0
                },
                shared_percentage_b: if !chunks_b.is_empty() {
                    (shared_count as f64 / chunks_b.len() as f64) * 100.0
                } else {
                    0.0
                },
            };
        }

        // Non-streaming: can compare chunks normally
        let chunks_a: Vec<_> = manifest_a.chunk_index.iter().map(|m| m.hash).collect();
        let chunks_b: Vec<_> = manifest_b.chunk_index.iter().map(|m| m.hash).collect();

        let set_a: HashSet<_> = chunks_a.iter().cloned().collect();
        let set_b: HashSet<_> = chunks_b.iter().cloned().collect();

        let shared: Vec<_> = set_a.intersection(&set_b).cloned().collect();
        let unique_to_a: Vec<_> = set_a.difference(&set_b).cloned().collect();
        let unique_to_b: Vec<_> = set_b.difference(&set_a).cloned().collect();

        let shared_count = shared.len();

        ChunkAnalysis {
            total_chunks_a: total_a,
            total_chunks_b: total_b,
            shared_chunks: shared,
            unique_to_a,
            unique_to_b,
            shared_percentage_a: if total_a > 0 {
                (shared_count as f64 / total_a as f64) * 100.0
            } else {
                0.0
            },
            shared_percentage_b: if total_b > 0 {
                (shared_count as f64 / total_b as f64) * 100.0
            } else {
                0.0
            },
        }
    }

    /// Extract all sequence hashes from a manifest by loading chunk manifests from storage
    fn extract_sequence_hashes(
        manifest: &crate::TemporalManifest,
        storage: &crate::storage::HeraldStorage,
    ) -> Result<HashSet<SHA256Hash>> {
        let mut all_sequences = HashSet::new();

        // Check if this is a streaming manifest (etag starts with "streaming-")
        // Note: chunk_index might be populated by get_manifest, so check etag instead
        if manifest.etag.starts_with("streaming-") {
            // Streaming mode: load from partials (more efficient than chunk_index)
            // Extract source/dataset/version from manifest
            if let Some(ref source_db) = manifest.source_database {
                let parts: Vec<&str> = source_db.split('/').collect();
                if parts.len() == 2 {
                    return Self::extract_sequence_hashes_from_partials(
                        parts[0],
                        parts[1],
                        &manifest.version,
                        storage,
                    );
                }
            }
            tracing::warn!("Streaming manifest without source_database - cannot extract sequences");
            return Ok(all_sequences);
        }

        // Non-streaming mode: load chunks from storage (stored as MessagePack)
        for chunk_metadata in &manifest.chunk_index {
            // Load the actual ChunkManifest from storage
            match storage.get_chunk(&chunk_metadata.hash) {
                Ok(chunk_data) => {
                    // Deserialize the ChunkManifest (stored as MessagePack by streaming import)
                    match rmp_serde::from_slice::<ChunkManifest>(&chunk_data) {
                        Ok(chunk_manifest) => {
                            // Extract actual sequence hashes
                            all_sequences.extend(chunk_manifest.sequence_refs.iter().cloned());
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to deserialize chunk manifest {}: {}",
                                chunk_metadata.hash.truncated(12),
                                e
                            );
                            // Continue processing other chunks
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to load chunk {}: {}",
                        chunk_metadata.hash.truncated(12),
                        e
                    );
                    // Continue processing other chunks
                }
            }
        }

        Ok(all_sequences)
    }

    /// Extract chunk hashes from streaming database partials (parallel)
    fn extract_chunk_hashes_from_partials(
        source_name: &str,
        dataset_name: &str,
        version: &str,
        storage: &crate::storage::HeraldStorage,
    ) -> Result<HashSet<SHA256Hash>> {
        #[derive(serde::Deserialize)]
        struct PartialManifest {
            #[allow(dead_code)]
            batch_num: usize,
            manifests: Vec<(ChunkManifest, SHA256Hash)>,
            #[allow(dead_code)]
            sequence_count: usize,
            #[allow(dead_code)]
            finalized: bool,
        }

        let prefix = format!("partial:{}:{}:{}:", source_name, dataset_name, version);
        let rocksdb = storage.sequence_storage.get_rocksdb();

        // Generate all possible batch keys
        const MAX_BATCHES: usize = 100000;
        let batch_keys: Vec<_> = (0..=MAX_BATCHES)
            .map(|n| format!("{}{:06}", prefix, n))
            .collect();

        // Use DashSet for concurrent insertion
        let chunk_hashes = Arc::new(DashSet::new());
        let processed = Arc::new(AtomicUsize::new(0));
        let consecutive_misses = Arc::new(AtomicUsize::new(0));
        const MAX_CONSECUTIVE_MISSES: usize = 10;

        // Process batches in parallel
        batch_keys.par_iter().for_each(|key| {
            // Stop if we've had too many consecutive misses
            if consecutive_misses.load(Ordering::Relaxed) >= MAX_CONSECUTIVE_MISSES {
                return;
            }

            if let Some(data) = rocksdb.get_manifest(key).ok().flatten() {
                if let Ok(partial) = bincode::deserialize::<PartialManifest>(&data) {
                    // Extract chunk hashes (second element of tuple in manifests)
                    for (_chunk_manifest, chunk_hash) in partial.manifests {
                        chunk_hashes.insert(chunk_hash);
                    }

                    let count = processed.fetch_add(1, Ordering::Relaxed);
                    consecutive_misses.store(0, Ordering::Relaxed);

                    if count.is_multiple_of(1000) {
                        tracing::debug!(
                            "Loaded {} partials, {} unique chunks",
                            count,
                            chunk_hashes.len()
                        );
                    }
                }
            } else {
                consecutive_misses.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Unwrap Arc and convert DashSet to HashSet
        let chunk_hashes = Arc::try_unwrap(chunk_hashes).unwrap_or_else(|arc| (*arc).clone());
        let result: HashSet<SHA256Hash> = chunk_hashes.into_iter().collect();
        tracing::info!(
            "Extracted {} unique chunks from {} partials ({})",
            result.len(),
            processed.load(Ordering::Relaxed),
            format!("{}:{}", source_name, dataset_name)
        );

        Ok(result)
    }

    /// Extract sequence hashes from streaming database partials (parallel)
    fn extract_sequence_hashes_from_partials(
        source_name: &str,
        dataset_name: &str,
        version: &str,
        storage: &crate::storage::HeraldStorage,
    ) -> Result<HashSet<SHA256Hash>> {
        #[derive(serde::Deserialize)]
        struct PartialManifest {
            #[allow(dead_code)]
            batch_num: usize,
            manifests: Vec<(ChunkManifest, SHA256Hash)>,
            #[allow(dead_code)]
            sequence_count: usize,
            #[allow(dead_code)]
            finalized: bool,
        }

        let prefix = format!("partial:{}:{}:{}:", source_name, dataset_name, version);
        let rocksdb = storage.sequence_storage.get_rocksdb();

        // Generate all possible batch keys
        const MAX_BATCHES: usize = 100000;
        let batch_keys: Vec<_> = (0..=MAX_BATCHES)
            .map(|n| format!("{}{:06}", prefix, n))
            .collect();

        // Use DashSet for concurrent insertion
        let all_sequences = Arc::new(DashSet::new());
        let processed = Arc::new(AtomicUsize::new(0));
        let consecutive_misses = Arc::new(AtomicUsize::new(0));
        const MAX_CONSECUTIVE_MISSES: usize = 10;

        // Process batches in parallel
        batch_keys.par_iter().for_each(|key| {
            // Stop if we've had too many consecutive misses
            if consecutive_misses.load(Ordering::Relaxed) >= MAX_CONSECUTIVE_MISSES {
                return;
            }

            if let Some(data) = rocksdb.get_manifest(key).ok().flatten() {
                if let Ok(partial) = bincode::deserialize::<PartialManifest>(&data) {
                    // Extract sequence hashes from each ChunkManifest
                    for (chunk_manifest, _hash) in partial.manifests {
                        for seq_hash in chunk_manifest.sequence_refs {
                            all_sequences.insert(seq_hash);
                        }
                    }

                    let count = processed.fetch_add(1, Ordering::Relaxed);
                    consecutive_misses.store(0, Ordering::Relaxed);

                    if count.is_multiple_of(1000) {
                        tracing::debug!(
                            "Loaded {} partials, {} unique sequences",
                            count,
                            all_sequences.len()
                        );
                    }
                }
            } else {
                consecutive_misses.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Unwrap Arc and convert DashSet to HashSet
        let all_sequences = Arc::try_unwrap(all_sequences).unwrap_or_else(|arc| (*arc).clone());
        let result: HashSet<SHA256Hash> = all_sequences.into_iter().collect();
        tracing::info!(
            "Extracted {} unique sequences from {} partials ({})",
            result.len(),
            processed.load(Ordering::Relaxed),
            format!("{}:{}", source_name, dataset_name)
        );

        Ok(result)
    }

    /// Compare sequences using streaming to avoid OOM (for large databases)
    fn compare_sequences_streaming(
        source_a: &str,
        dataset_a: &str,
        version_a: &str,
        source_b: &str,
        dataset_b: &str,
        version_b: &str,
        storage: &crate::storage::HeraldStorage,
    ) -> Result<(usize, usize, usize, usize, Vec<String>)> {
        // Returns: (total_a, total_b, shared_count, unique_a_count, sample_shared_ids)

        #[derive(serde::Deserialize)]
        struct PartialManifest {
            #[allow(dead_code)]
            batch_num: usize,
            manifests: Vec<(ChunkManifest, SHA256Hash)>,
            #[allow(dead_code)]
            sequence_count: usize,
            #[allow(dead_code)]
            finalized: bool,
        }

        // Load smaller database into memory
        tracing::info!("Loading smaller database (A) into memory for comparison...");
        let seqs_a =
            Self::extract_sequence_hashes_from_partials(source_a, dataset_a, version_a, storage)?;
        let total_a = seqs_a.len();

        // Stream through larger database, comparing in parallel
        tracing::info!("Streaming through larger database (B) for comparison (parallel)...");
        let rocksdb = storage.sequence_storage.get_rocksdb();
        let prefix_b = format!("partial:{}:{}:{}:", source_b, dataset_b, version_b);

        // Generate all possible batch keys
        const MAX_BATCHES: usize = 100000;
        let batch_keys: Vec<_> = (0..=MAX_BATCHES)
            .map(|n| format!("{}{:06}", prefix_b, n))
            .collect();

        // Use atomic counters for parallel processing
        let total_b = Arc::new(AtomicUsize::new(0));
        let shared_count = Arc::new(AtomicUsize::new(0));
        let sample_shared = Arc::new(Mutex::new(Vec::new()));
        let processed = Arc::new(AtomicUsize::new(0));
        let consecutive_misses = Arc::new(AtomicUsize::new(0));
        const MAX_CONSECUTIVE_MISSES: usize = 10;

        // Process batches in parallel
        batch_keys.par_iter().for_each(|key| {
            // Stop if we've had too many consecutive misses
            if consecutive_misses.load(Ordering::Relaxed) >= MAX_CONSECUTIVE_MISSES {
                return;
            }

            if let Some(data) = rocksdb.get_manifest(key).ok().flatten() {
                if let Ok(partial) = bincode::deserialize::<PartialManifest>(&data) {
                    for (chunk_manifest, _hash) in partial.manifests {
                        for seq_hash in chunk_manifest.sequence_refs {
                            total_b.fetch_add(1, Ordering::Relaxed);
                            if seqs_a.contains(&seq_hash) {
                                let count = shared_count.fetch_add(1, Ordering::Relaxed);
                                // Only collect first 10 samples
                                if count < 10 {
                                    if let Ok(mut samples) = sample_shared.lock() {
                                        if samples.len() < 10 {
                                            samples.push(seq_hash);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    let count = processed.fetch_add(1, Ordering::Relaxed);
                    consecutive_misses.store(0, Ordering::Relaxed);

                    if count.is_multiple_of(1000) {
                        tracing::debug!(
                            "Processed {} partials from B: {} total, {} shared",
                            count,
                            total_b.load(Ordering::Relaxed),
                            shared_count.load(Ordering::Relaxed)
                        );
                    }
                }
            } else {
                consecutive_misses.fetch_add(1, Ordering::Relaxed);
            }
        });

        let total_b_final = total_b.load(Ordering::Relaxed);
        let shared_count_final = shared_count.load(Ordering::Relaxed);
        let unique_a_count = total_a - shared_count_final;

        let sample_shared_ids: Vec<String> = sample_shared
            .lock()
            .unwrap()
            .iter()
            .map(|h| h.truncated(16))
            .collect();

        tracing::info!(
            "Comparison complete: {} total in A, {} total in B, {} shared",
            total_a,
            total_b_final,
            shared_count_final
        );

        Ok((
            total_a,
            total_b_final,
            shared_count_final,
            unique_a_count,
            sample_shared_ids,
        ))
    }

    /// Compare sequences from manifests using actual sequence hashes
    fn compare_sequences_from_manifests(
        manifest_a: &crate::TemporalManifest,
        manifest_b: &crate::TemporalManifest,
        storage: Option<&crate::storage::HeraldStorage>,
    ) -> SequenceAnalysis {
        // If no storage provided, fall back to old chunk-based counting
        let Some(storage) = storage else {
            return Self::compare_sequences_from_manifests_legacy(manifest_a, manifest_b);
        };

        // Check if both are streaming manifests
        let is_streaming_a = manifest_a.etag.starts_with("streaming-");
        let is_streaming_b = manifest_b.etag.starts_with("streaming-");

        // Use streaming comparison for large databases
        if is_streaming_a && is_streaming_b {
            let (source_a, dataset_a) = if let Some(ref source_db) = manifest_a.source_database {
                let parts: Vec<&str> = source_db.split('/').collect();
                if parts.len() == 2 {
                    (parts[0], parts[1])
                } else {
                    return Self::compare_sequences_from_manifests_legacy(manifest_a, manifest_b);
                }
            } else {
                return Self::compare_sequences_from_manifests_legacy(manifest_a, manifest_b);
            };

            let (source_b, dataset_b) = if let Some(ref source_db) = manifest_b.source_database {
                let parts: Vec<&str> = source_db.split('/').collect();
                if parts.len() == 2 {
                    (parts[0], parts[1])
                } else {
                    return Self::compare_sequences_from_manifests_legacy(manifest_a, manifest_b);
                }
            } else {
                return Self::compare_sequences_from_manifests_legacy(manifest_a, manifest_b);
            };

            match Self::compare_sequences_streaming(
                source_a,
                dataset_a,
                &manifest_a.version,
                source_b,
                dataset_b,
                &manifest_b.version,
                storage,
            ) {
                Ok((total_a, total_b, shared_count, unique_a_count, sample_shared_ids)) => {
                    let unique_b_count = total_b - shared_count;

                    let shared_pct_a = if total_a > 0 {
                        (shared_count as f64 / total_a as f64) * 100.0
                    } else {
                        0.0
                    };

                    let shared_pct_b = if total_b > 0 {
                        (shared_count as f64 / total_b as f64) * 100.0
                    } else {
                        0.0
                    };

                    return SequenceAnalysis {
                        total_sequences_a: total_a,
                        total_sequences_b: total_b,
                        shared_sequences: shared_count,
                        unique_to_a: unique_a_count,
                        unique_to_b: unique_b_count,
                        shared_percentage_a: shared_pct_a,
                        shared_percentage_b: shared_pct_b,
                        sample_shared_ids,
                        sample_unique_a_ids: Vec::new(),
                        sample_unique_b_ids: Vec::new(),
                    };
                }
                Err(e) => {
                    tracing::error!("Streaming comparison failed: {}", e);
                    return Self::compare_sequences_from_manifests_legacy(manifest_a, manifest_b);
                }
            }
        }

        // Non-streaming: load both into memory (for smaller databases)
        let seqs_a = match Self::extract_sequence_hashes(manifest_a, storage) {
            Ok(seqs) => seqs,
            Err(e) => {
                tracing::error!("Failed to extract sequences from manifest A: {}", e);
                return Self::compare_sequences_from_manifests_legacy(manifest_a, manifest_b);
            }
        };

        let seqs_b = match Self::extract_sequence_hashes(manifest_b, storage) {
            Ok(seqs) => seqs,
            Err(e) => {
                tracing::error!("Failed to extract sequences from manifest B: {}", e);
                return Self::compare_sequences_from_manifests_legacy(manifest_a, manifest_b);
            }
        };

        // Compute set operations on actual sequence hashes
        let shared: HashSet<_> = seqs_a.intersection(&seqs_b).cloned().collect();
        let unique_a: HashSet<_> = seqs_a.difference(&seqs_b).cloned().collect();
        let unique_b: HashSet<_> = seqs_b.difference(&seqs_a).cloned().collect();

        let seq_count_a = seqs_a.len();
        let seq_count_b = seqs_b.len();
        let shared_count = shared.len();

        // Get sample sequence IDs for display
        let sample_shared_ids: Vec<String> = shared
            .iter()
            .take(10)
            .map(|hash| hash.truncated(16))
            .collect();

        let sample_unique_a_ids: Vec<String> = unique_a
            .iter()
            .take(5)
            .map(|hash| hash.truncated(16))
            .collect();

        let sample_unique_b_ids: Vec<String> = unique_b
            .iter()
            .take(5)
            .map(|hash| hash.truncated(16))
            .collect();

        let shared_pct_a = if seq_count_a > 0 {
            (shared_count as f64 / seq_count_a as f64) * 100.0
        } else {
            0.0
        };

        let shared_pct_b = if seq_count_b > 0 {
            (shared_count as f64 / seq_count_b as f64) * 100.0
        } else {
            0.0
        };

        SequenceAnalysis {
            total_sequences_a: seq_count_a,
            total_sequences_b: seq_count_b,
            shared_sequences: shared_count,
            unique_to_a: unique_a.len(),
            unique_to_b: unique_b.len(),
            sample_shared_ids,
            sample_unique_a_ids,
            sample_unique_b_ids,
            shared_percentage_a: shared_pct_a,
            shared_percentage_b: shared_pct_b,
        }
    }

    /// Legacy method: Compare sequences based on shared chunks (inaccurate)
    fn compare_sequences_from_manifests_legacy(
        manifest_a: &crate::TemporalManifest,
        manifest_b: &crate::TemporalManifest,
    ) -> SequenceAnalysis {
        let seq_count_a: usize = manifest_a
            .chunk_index
            .iter()
            .map(|m| m.sequence_count)
            .sum();
        let seq_count_b: usize = manifest_b
            .chunk_index
            .iter()
            .map(|m| m.sequence_count)
            .sum();

        // Calculate shared sequences based on shared chunks (INACCURATE)
        let chunks_a: HashSet<_> = manifest_a.chunk_index.iter().map(|m| m.hash).collect();
        let chunks_b: HashSet<_> = manifest_b.chunk_index.iter().map(|m| m.hash).collect();
        let shared_chunk_hashes: HashSet<_> = chunks_a.intersection(&chunks_b).cloned().collect();

        // Count sequences in shared chunks
        let shared_seq_count: usize = manifest_a
            .chunk_index
            .iter()
            .filter(|m| shared_chunk_hashes.contains(&m.hash))
            .map(|m| m.sequence_count)
            .sum();

        let unique_to_a = seq_count_a.saturating_sub(shared_seq_count);
        let unique_to_b = seq_count_b.saturating_sub(shared_seq_count);

        let shared_pct_a = if seq_count_a > 0 {
            (shared_seq_count as f64 / seq_count_a as f64) * 100.0
        } else {
            0.0
        };

        let shared_pct_b = if seq_count_b > 0 {
            (shared_seq_count as f64 / seq_count_b as f64) * 100.0
        } else {
            0.0
        };

        SequenceAnalysis {
            total_sequences_a: seq_count_a,
            total_sequences_b: seq_count_b,
            shared_sequences: shared_seq_count,
            unique_to_a,
            unique_to_b,
            sample_shared_ids: vec![],
            sample_unique_a_ids: vec![],
            sample_unique_b_ids: vec![],
            shared_percentage_a: shared_pct_a,
            shared_percentage_b: shared_pct_b,
        }
    }

    /// Compare taxonomies from manifests
    fn compare_taxonomies_from_manifests(
        manifest_a: &crate::TemporalManifest,
        manifest_b: &crate::TemporalManifest,
        taxonomy_manager: Option<&crate::taxonomy::TaxonomyManager>,
        storage: Option<&crate::storage::HeraldStorage>,
    ) -> TaxonomyAnalysis {
        let is_streaming_a = manifest_a.etag.starts_with("streaming-");
        let is_streaming_b = manifest_b.etag.starts_with("streaming-");

        // For streaming manifests, skip taxonomy analysis (would require loading all partials)
        if (is_streaming_a || is_streaming_b) && storage.is_some() {
            tracing::debug!(
                "Skipping taxonomy analysis for streaming manifests (requires full chunk index)"
            );
            return TaxonomyAnalysis {
                total_taxa_a: 0,
                total_taxa_b: 0,
                shared_taxa: Vec::new(),
                unique_to_a: Vec::new(),
                unique_to_b: Vec::new(),
                shared_percentage_a: 0.0,
                shared_percentage_b: 0.0,
                top_shared_taxa: Vec::new(),
            };
        }

        // Collect all unique taxon IDs from each manifest
        let mut taxa_a = HashSet::new();
        let mut taxa_b = HashSet::new();
        let mut taxa_counts_a: HashMap<TaxonId, usize> = HashMap::new();
        let mut taxa_counts_b: HashMap<TaxonId, usize> = HashMap::new();

        for chunk_meta in &manifest_a.chunk_index {
            for taxon_id in &chunk_meta.taxon_ids {
                taxa_a.insert(*taxon_id);
                *taxa_counts_a.entry(*taxon_id).or_insert(0) += chunk_meta.sequence_count;
            }
        }

        for chunk_meta in &manifest_b.chunk_index {
            for taxon_id in &chunk_meta.taxon_ids {
                taxa_b.insert(*taxon_id);
                *taxa_counts_b.entry(*taxon_id).or_insert(0) += chunk_meta.sequence_count;
            }
        }

        let shared: Vec<_> = taxa_a.intersection(&taxa_b).cloned().collect();
        let unique_to_a: Vec<_> = taxa_a.difference(&taxa_b).cloned().collect();
        let unique_to_b: Vec<_> = taxa_b.difference(&taxa_a).cloned().collect();

        // Get top shared taxa
        let mut top_shared: Vec<TaxonDistribution> = shared
            .iter()
            .filter_map(|taxon_id| {
                let count_a = taxa_counts_a.get(taxon_id)?;
                let count_b = taxa_counts_b.get(taxon_id)?;

                // Try to get scientific name from taxonomy manager
                let taxon_name = if let Some(tax_mgr) = taxonomy_manager {
                    if let Some(node) = tax_mgr.get_node(taxon_id) {
                        node.name.clone()
                    } else {
                        format!("TaxID {}", taxon_id.0)
                    }
                } else {
                    format!("TaxID {}", taxon_id.0)
                };

                Some(TaxonDistribution {
                    taxon_id: *taxon_id,
                    taxon_name,
                    count_in_a: *count_a,
                    count_in_b: *count_b,
                })
            })
            .collect();

        top_shared.sort_by_key(|d| std::cmp::Reverse(d.count_in_a + d.count_in_b));
        top_shared.truncate(10);

        let total_a = taxa_a.len();
        let total_b = taxa_b.len();
        let shared_count = shared.len();

        TaxonomyAnalysis {
            total_taxa_a: total_a,
            total_taxa_b: total_b,
            shared_taxa: shared,
            unique_to_a,
            unique_to_b,
            top_shared_taxa: top_shared,
            shared_percentage_a: if total_a > 0 {
                (shared_count as f64 / total_a as f64) * 100.0
            } else {
                0.0
            },
            shared_percentage_b: if total_b > 0 {
                (shared_count as f64 / total_b as f64) * 100.0
            } else {
                0.0
            },
        }
    }

    /// Calculate storage metrics from manifests
    /// Estimate storage size for streaming manifest
    fn estimate_storage_size_for_streaming(
        source_name: &str,
        dataset_name: &str,
        version: &str,
        storage: &crate::storage::HeraldStorage,
    ) -> Result<usize> {
        let rocksdb = storage.sequence_storage.get_rocksdb();
        let seq_count_key = format!(
            "sequence_count:{}:{}:{}",
            source_name, dataset_name, version
        );

        let sequence_count = if let Some(data) = rocksdb.get_manifest(&seq_count_key)? {
            if data.len() == 8 {
                usize::from_le_bytes(data.try_into().unwrap_or([0u8; 8]))
            } else {
                0
            }
        } else {
            0
        };

        // Estimate: average protein sequence is ~400 amino acids
        // Each amino acid stored as 1 byte, plus overhead
        const AVG_SEQUENCE_SIZE: usize = 500;
        Ok(sequence_count * AVG_SEQUENCE_SIZE)
    }

    fn calculate_storage_from_manifests(
        manifest_a: &crate::TemporalManifest,
        manifest_b: &crate::TemporalManifest,
        storage: Option<&crate::storage::HeraldStorage>,
    ) -> StorageMetrics {
        let is_streaming_a = manifest_a.etag.starts_with("streaming-");
        let is_streaming_b = manifest_b.etag.starts_with("streaming-");

        let size_a = if is_streaming_a && storage.is_some() {
            // Estimate size for streaming
            if let Some(ref source_db) = manifest_a.source_database {
                let parts: Vec<&str> = source_db.split('/').collect();
                if parts.len() == 2 {
                    Self::estimate_storage_size_for_streaming(
                        parts[0],
                        parts[1],
                        &manifest_a.version,
                        storage.unwrap(),
                    )
                    .unwrap_or(0)
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            manifest_a.chunk_index.iter().map(|m| m.size).sum()
        };

        let size_b = if is_streaming_b && storage.is_some() {
            // Estimate size for streaming
            if let Some(ref source_db) = manifest_b.source_database {
                let parts: Vec<&str> = source_db.split('/').collect();
                if parts.len() == 2 {
                    Self::estimate_storage_size_for_streaming(
                        parts[0],
                        parts[1],
                        &manifest_b.version,
                        storage.unwrap(),
                    )
                    .unwrap_or(0)
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            manifest_b.chunk_index.iter().map(|m| m.size).sum()
        };

        // For streaming, we can't calculate dedup savings (would need to compare chunks)
        let dedup_savings = if is_streaming_a || is_streaming_b {
            0
        } else {
            // Calculate shared chunks for dedup savings
            let chunks_a: HashSet<_> = manifest_a.chunk_index.iter().map(|m| &m.hash).collect();
            let chunks_b: HashSet<_> = manifest_b.chunk_index.iter().map(|m| &m.hash).collect();
            let shared_hashes: HashSet<_> = chunks_a.intersection(&chunks_b).cloned().collect();

            manifest_a
                .chunk_index
                .iter()
                .filter(|m| shared_hashes.contains(&m.hash))
                .map(|m| m.size)
                .sum()
        };

        // Calculate actual deduplication ratios
        let dedup_ratio_a = Self::calculate_dedup_ratio(manifest_a, size_a);
        let dedup_ratio_b = Self::calculate_dedup_ratio(manifest_b, size_b);

        StorageMetrics {
            size_a_bytes: size_a,
            size_b_bytes: size_b,
            dedup_savings_bytes: dedup_savings,
            dedup_ratio_a,
            dedup_ratio_b,
        }
    }

    /// Calculate deduplication ratio for a manifest
    /// Formula: estimated_original_size / actual_storage_size
    fn calculate_dedup_ratio(manifest: &crate::TemporalManifest, actual_storage: usize) -> f32 {
        if actual_storage == 0 {
            return 1.0;
        }

        // For streaming manifests, we need to load sequence count from RocksDB
        let sequence_count = if manifest.etag.starts_with("streaming-") {
            // Try to get from metadata
            if let Some(ref source_db) = manifest.source_database {
                let parts: Vec<&str> = source_db.split('/').collect();
                if parts.len() == 2 {
                    // This would need storage to load, but we don't have it here
                    // For now, estimate from chunk count (rough estimate)
                    manifest.chunk_index.len() * 30 // Assume ~30 sequences per chunk
                } else {
                    manifest.chunk_index.len() * 30
                }
            } else {
                manifest.chunk_index.len() * 30
            }
        } else {
            // Non-streaming: can count sequences from chunk_index
            manifest
                .chunk_index
                .iter()
                .map(|c| c.sequence_count)
                .sum::<usize>()
        };

        if sequence_count == 0 {
            return 1.0;
        }

        // Estimate original size: average ~200 bytes per sequence (FASTA format)
        // This includes header (~50 bytes) + sequence data (~150 bytes average for proteins)
        let estimated_original_bytes = sequence_count * 200;

        // Deduplication ratio = original / compressed
        let ratio = estimated_original_bytes as f32 / actual_storage as f32;

        // Clamp to reasonable range [1.0, 100.0]
        ratio.max(1.0).min(100.0)
    }
}

/// Format bytes into human-readable size
pub fn format_bytes(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}
