/// Comprehensive database comparison functionality
use crate::{
    SEQUOIARepository, SHA256Hash, TaxonId, ChunkManifest,
};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use serde::{Serialize, Deserialize};

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
    repo_a: SEQUOIARepository,
    repo_b: SEQUOIARepository,
}

impl DatabaseDiffer {
    /// Create a new database differ
    pub fn new(path_a: &Path, path_b: &Path) -> Result<Self> {
        let repo_a = SEQUOIARepository::open(path_a)?;
        let repo_b = SEQUOIARepository::open(path_b)?;

        Ok(Self { repo_a, repo_b })
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
        let sample_shared_ids: Vec<String> = shared.iter()
            .take(10)
            .filter_map(|hash| {
                seq_id_map_a.get(hash)
                    .or_else(|| seq_id_map_b.get(hash))
                    .cloned()
            })
            .collect();

        let sample_unique_a_ids: Vec<String> = unique_to_a_set.iter()
            .take(5)
            .filter_map(|hash| seq_id_map_a.get(hash).cloned())
            .collect();

        let sample_unique_b_ids: Vec<String> = unique_to_b_set.iter()
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
        let mut top_shared: Vec<TaxonDistribution> = shared.iter()
            .filter_map(|taxon_id| {
                let count_a = taxa_counts_a.get(taxon_id)?;
                let count_b = taxa_counts_b.get(taxon_id)?;

                // Try to get taxon name from taxonomy manager
                let taxon_name = self.get_taxon_name(&self.repo_a, *taxon_id)
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
    fn get_all_chunk_hashes(&self, repo: &SEQUOIARepository) -> Result<Vec<SHA256Hash>> {
        // Get chunks from storage directly
        repo.storage.list_chunks()
    }

    /// Get all sequence hashes and their IDs from a repository
    fn get_all_sequence_hashes(&self, repo: &SEQUOIARepository)
        -> Result<(Vec<SHA256Hash>, HashMap<SHA256Hash, String>)>
    {
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
    fn get_taxonomy_distribution(&self, repo: &SEQUOIARepository)
        -> Result<(Vec<TaxonId>, HashMap<TaxonId, usize>)>
    {
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
    fn get_taxon_name(&self, _repo: &SEQUOIARepository, taxon_id: TaxonId) -> Option<String> {
        // Try to get taxon info from taxonomy manager
        // For now, just return the taxon ID as string since get_taxon_info doesn't exist
        Some(format!("TaxID {}", taxon_id.0))
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

