use crate::types::{ManifestMetadata, SHA256Hash};
use crate::Manifest;
/// Chunk indexing traits and implementations for HERALD
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

/// Trait for building and maintaining chunk indices
pub trait ChunkIndexBuilder {
    /// Build index from a manifest
    fn index_manifest(&mut self, manifest: &Manifest) -> Result<()>;

    /// Build index from a directory of manifests
    fn index_directory(&mut self, path: &Path) -> Result<()>;

    /// Persist the index to storage
    fn save(&self, path: &Path) -> Result<()>;

    /// Load index from storage
    fn load(path: &Path) -> Result<Self>
    where
        Self: Sized;
}

/// Trait for querying chunk indices
pub trait ChunkQuery {
    /// Find chunks by hash
    fn find_by_hash(&self, hash: &SHA256Hash) -> Option<&ManifestMetadata>;

    /// Find chunks by taxonomy ID
    fn find_by_taxid(&self, taxid: u32) -> Vec<&ManifestMetadata>;

    /// Find chunks by accession
    fn find_by_accession(&self, accession: &str) -> Option<&ManifestMetadata>;

    /// Get all chunks for a database
    fn find_by_database(&self, database: &str) -> Vec<&ManifestMetadata>;

    /// Get statistics about the index
    fn statistics(&self) -> IndexStatistics;
}

/// Trait for chunk access patterns and optimization
pub trait ChunkAccessTracker {
    /// Record an access to a chunk
    fn record_access(&mut self, hash: &SHA256Hash);

    /// Get access frequency for a chunk
    fn get_access_frequency(&self, hash: &SHA256Hash) -> usize;

    /// Get hot chunks (frequently accessed)
    fn get_hot_chunks(&self, threshold: usize) -> Vec<SHA256Hash>;

    /// Get cold chunks (rarely accessed)
    fn get_cold_chunks(&self, threshold: usize) -> Vec<SHA256Hash>;

    /// Suggest optimizations based on access patterns
    fn suggest_optimizations(&self) -> Vec<OptimizationSuggestion>;
}

/// Trait for chunk relationship discovery
pub trait ChunkRelationships {
    /// Find chunks that are phylogenetically related
    fn find_related_by_taxonomy(&self, hash: &SHA256Hash, distance: u32) -> Vec<SHA256Hash>;

    /// Find chunks that share sequences
    fn find_overlapping(&self, hash: &SHA256Hash) -> Vec<SHA256Hash>;

    /// Find chunks that could be merged (too small)
    fn find_merge_candidates(&self, min_size: u64) -> Vec<Vec<SHA256Hash>>;

    /// Find chunks that should be split (too large)
    fn find_split_candidates(&self, max_size: u64) -> Vec<SHA256Hash>;
}

/// Statistics about the chunk index
#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexStatistics {
    pub total_chunks: usize,
    pub total_sequences: usize,
    pub total_size: u64,
    pub total_compressed_size: u64,
    pub unique_taxa: usize,
    pub databases: Vec<String>,
    pub avg_chunk_size: f64,
    pub avg_compression_ratio: f64,
    pub size_distribution: SizeDistribution,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SizeDistribution {
    pub small_chunks: usize,  // < 10MB
    pub medium_chunks: usize, // 10-50MB
    pub large_chunks: usize,  // 50-100MB
    pub xlarge_chunks: usize, // > 100MB
}

#[derive(Debug, Clone)]
pub enum OptimizationSuggestion {
    MergeChunks {
        chunks: Vec<SHA256Hash>,
        reason: String,
        estimated_savings: u64,
    },
    SplitChunk {
        chunk: SHA256Hash,
        reason: String,
        suggested_parts: usize,
    },
    RecompressChunk {
        chunk: SHA256Hash,
        current_ratio: f32,
        expected_ratio: f32,
    },
    MoveToHotStorage {
        chunks: Vec<SHA256Hash>,
        access_frequency: usize,
    },
    MoveToColdStorage {
        chunks: Vec<SHA256Hash>,
        last_access_days_ago: u32,
    },
}

/// Default implementation of chunk index
pub struct DefaultChunkIndex {
    by_hash: HashMap<SHA256Hash, ManifestMetadata>,
    by_taxid: HashMap<u32, Vec<SHA256Hash>>,
    by_accession: HashMap<String, SHA256Hash>,
    by_database: HashMap<String, Vec<SHA256Hash>>,
    access_counts: HashMap<SHA256Hash, usize>,
}

impl Default for DefaultChunkIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultChunkIndex {
    pub fn new() -> Self {
        Self {
            by_hash: HashMap::new(),
            by_taxid: HashMap::new(),
            by_accession: HashMap::new(),
            by_database: HashMap::new(),
            access_counts: HashMap::new(),
        }
    }
}

impl ChunkQuery for DefaultChunkIndex {
    fn find_by_hash(&self, hash: &SHA256Hash) -> Option<&ManifestMetadata> {
        self.by_hash.get(hash)
    }

    fn find_by_taxid(&self, taxid: u32) -> Vec<&ManifestMetadata> {
        self.by_taxid
            .get(&taxid)
            .map(|hashes| hashes.iter().filter_map(|h| self.by_hash.get(h)).collect())
            .unwrap_or_default()
    }

    fn find_by_accession(&self, accession: &str) -> Option<&ManifestMetadata> {
        self.by_accession
            .get(accession)
            .and_then(|hash| self.by_hash.get(hash))
    }

    fn find_by_database(&self, database: &str) -> Vec<&ManifestMetadata> {
        self.by_database
            .get(database)
            .map(|hashes| hashes.iter().filter_map(|h| self.by_hash.get(h)).collect())
            .unwrap_or_default()
    }

    fn statistics(&self) -> IndexStatistics {
        let total_chunks = self.by_hash.len();
        let total_size: u64 = self.by_hash.values().map(|c| c.size as u64).sum();
        let total_compressed: u64 = self
            .by_hash
            .values()
            .map(|c| c.compressed_size.unwrap_or(c.size) as u64)
            .sum();
        let total_sequences: usize = self.by_hash.values().map(|c| c.sequence_count).sum();

        let mut small = 0;
        let mut medium = 0;
        let mut large = 0;
        let mut xlarge = 0;

        for chunk in self.by_hash.values() {
            let size_mb = chunk.size / 1_048_576;
            match size_mb {
                0..=9 => small += 1,
                10..=49 => medium += 1,
                50..=99 => large += 1,
                _ => xlarge += 1,
            }
        }

        IndexStatistics {
            total_chunks,
            total_sequences,
            total_size,
            total_compressed_size: total_compressed,
            unique_taxa: self.by_taxid.len(),
            databases: self.by_database.keys().cloned().collect(),
            avg_chunk_size: if total_chunks > 0 {
                total_size as f64 / total_chunks as f64
            } else {
                0.0
            },
            avg_compression_ratio: if total_compressed > 0 {
                total_size as f64 / total_compressed as f64
            } else {
                0.0
            },
            size_distribution: SizeDistribution {
                small_chunks: small,
                medium_chunks: medium,
                large_chunks: large,
                xlarge_chunks: xlarge,
            },
        }
    }
}

impl ChunkAccessTracker for DefaultChunkIndex {
    fn record_access(&mut self, hash: &SHA256Hash) {
        *self.access_counts.entry(hash.clone()).or_insert(0) += 1;
    }

    fn get_access_frequency(&self, hash: &SHA256Hash) -> usize {
        self.access_counts.get(hash).copied().unwrap_or(0)
    }

    fn get_hot_chunks(&self, threshold: usize) -> Vec<SHA256Hash> {
        self.access_counts
            .iter()
            .filter(|(_, count)| **count >= threshold)
            .map(|(hash, _)| hash.clone())
            .collect()
    }

    fn get_cold_chunks(&self, threshold: usize) -> Vec<SHA256Hash> {
        self.access_counts
            .iter()
            .filter(|(_, count)| **count < threshold)
            .map(|(hash, _)| hash.clone())
            .collect()
    }

    fn suggest_optimizations(&self) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Find chunks that should be in hot storage
        let hot_chunks = self.get_hot_chunks(100);
        if !hot_chunks.is_empty() {
            suggestions.push(OptimizationSuggestion::MoveToHotStorage {
                chunks: hot_chunks.clone(),
                access_frequency: 100,
            });
        }

        // Find oversized chunks
        for (hash, metadata) in &self.by_hash {
            if metadata.size > 100_000_000 {
                // 100MB
                suggestions.push(OptimizationSuggestion::SplitChunk {
                    chunk: hash.clone(),
                    reason: "Chunk exceeds 100MB".to_string(),
                    suggested_parts: (metadata.size / 50_000_000) + 1,
                });
            }

            // Check compression efficiency
            if let Some(compressed) = metadata.compressed_size {
                let ratio = metadata.size as f32 / compressed as f32;
                if ratio < 2.0 {
                    suggestions.push(OptimizationSuggestion::RecompressChunk {
                        chunk: hash.clone(),
                        current_ratio: ratio,
                        expected_ratio: 3.0, // Typical for biological data
                    });
                }
            }
        }

        suggestions
    }
}
