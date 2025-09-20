/// Trait for chunk indexing strategies
use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

use crate::casg::types::{ChunkMetadata, SHA256Hash, TaxonId};

/// Query options for chunk searches
#[derive(Debug, Clone, Default)]
pub struct ChunkQuery {
    /// Filter by taxon IDs
    pub taxon_ids: Option<Vec<TaxonId>>,
    /// Filter by size range
    pub size_range: Option<(usize, usize)>,
    /// Include only chunks with references
    pub has_reference: Option<bool>,
    /// Limit number of results
    pub limit: Option<usize>,
    /// Order by access count (for hot chunks)
    pub order_by_access: bool,
}

/// Statistics about the index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    /// Total number of chunks
    pub total_chunks: usize,
    /// Total size of all chunks
    pub total_size: usize,
    /// Number of unique taxons
    pub unique_taxons: usize,
    /// Average chunk size
    pub avg_chunk_size: usize,
    /// Number of delta chunks
    pub delta_chunks: usize,
    /// Number of reference chunks
    pub reference_chunks: usize,
}

/// Trait for chunk indexing
#[async_trait]
pub trait ChunkIndex: Send + Sync {
    /// Add or update chunk metadata
    async fn add_chunk(&mut self, metadata: ChunkMetadata) -> Result<()>;

    /// Remove a chunk from the index
    async fn remove_chunk(&mut self, hash: &SHA256Hash) -> Result<()>;

    /// Find chunks containing sequences for specific taxons
    async fn find_by_taxon(&self, taxon_id: TaxonId) -> Result<Vec<SHA256Hash>>;

    /// Find chunks containing any of the specified taxons
    async fn find_by_taxons(&self, taxon_ids: &[TaxonId]) -> Result<Vec<SHA256Hash>>;

    /// Get chunk metadata without loading the chunk
    async fn get_metadata(&self, hash: &SHA256Hash) -> Result<Option<ChunkMetadata>>;

    /// Query chunks with multiple filters
    async fn query(&self, query: ChunkQuery) -> Result<Vec<ChunkMetadata>>;

    /// Rebuild index from storage
    async fn rebuild(&mut self) -> Result<()>;

    /// Get index statistics
    async fn get_stats(&self) -> Result<IndexStats>;

    /// Check if a chunk exists in the index
    async fn exists(&self, hash: &SHA256Hash) -> bool {
        self.get_metadata(hash).await.unwrap_or(None).is_some()
    }

    /// Get all chunk hashes
    async fn list_all(&self) -> Result<Vec<SHA256Hash>>;

    /// Clear the entire index
    async fn clear(&mut self) -> Result<()>;
}

/// In-memory chunk index implementation using DashMap for thread-safety
pub struct InMemoryChunkIndex {
    /// Main index: hash -> metadata
    chunks: Arc<DashMap<SHA256Hash, ChunkMetadata>>,
    /// Taxon index: taxon_id -> set of chunk hashes
    taxon_index: Arc<DashMap<TaxonId, HashSet<SHA256Hash>>>,
}

impl InMemoryChunkIndex {
    pub fn new() -> Self {
        Self {
            chunks: Arc::new(DashMap::new()),
            taxon_index: Arc::new(DashMap::new()),
        }
    }
}

#[async_trait]
impl ChunkIndex for InMemoryChunkIndex {
    async fn add_chunk(&mut self, metadata: ChunkMetadata) -> Result<()> {
        let hash = metadata.hash.clone();

        // Update taxon index
        for taxon_id in &metadata.taxon_ids {
            self.taxon_index
                .entry(*taxon_id)
                .or_default()
                .insert(hash.clone());
        }

        // Add to main index
        self.chunks.insert(hash, metadata);
        Ok(())
    }

    async fn remove_chunk(&mut self, hash: &SHA256Hash) -> Result<()> {
        if let Some((_, metadata)) = self.chunks.remove(hash) {
            // Update taxon index
            for taxon_id in &metadata.taxon_ids {
                if let Some(mut hashes) = self.taxon_index.get_mut(taxon_id) {
                    hashes.remove(hash);
                }
            }
        }
        Ok(())
    }

    async fn find_by_taxon(&self, taxon_id: TaxonId) -> Result<Vec<SHA256Hash>> {
        Ok(self
            .taxon_index
            .get(&taxon_id)
            .map(|hashes| hashes.iter().cloned().collect())
            .unwrap_or_default())
    }

    async fn find_by_taxons(&self, taxon_ids: &[TaxonId]) -> Result<Vec<SHA256Hash>> {
        let mut result = HashSet::new();
        for taxon_id in taxon_ids {
            if let Some(hashes) = self.taxon_index.get(taxon_id) {
                result.extend(hashes.iter().cloned());
            }
        }
        Ok(result.into_iter().collect())
    }

    async fn get_metadata(&self, hash: &SHA256Hash) -> Result<Option<ChunkMetadata>> {
        Ok(self.chunks.get(hash).map(|entry| entry.clone()))
    }

    async fn query(&self, query: ChunkQuery) -> Result<Vec<ChunkMetadata>> {
        let mut results = Vec::new();

        // Start with all chunks or taxon-filtered chunks
        let hashes: Vec<SHA256Hash> = if let Some(taxon_ids) = query.taxon_ids {
            self.find_by_taxons(&taxon_ids).await?
        } else {
            self.chunks
                .iter()
                .map(|entry| entry.key().clone())
                .collect()
        };

        for hash in hashes {
            if let Some(metadata) = self.chunks.get(&hash) {
                // Apply size filter
                if let Some((min, max)) = query.size_range {
                    if metadata.size < min || metadata.size > max {
                        continue;
                    }
                }

                results.push(metadata.clone());

                // Apply limit
                if let Some(limit) = query.limit {
                    if results.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(results)
    }

    async fn rebuild(&mut self) -> Result<()> {
        // This would scan storage and rebuild the index
        // For in-memory index, this is typically called on startup
        Ok(())
    }

    async fn get_stats(&self) -> Result<IndexStats> {
        let total_chunks = self.chunks.len();
        let total_size: usize = self.chunks.iter().map(|entry| entry.size).sum();
        let unique_taxons = self.taxon_index.len();
        let avg_chunk_size = if total_chunks > 0 {
            total_size / total_chunks
        } else {
            0
        };

        Ok(IndexStats {
            total_chunks,
            total_size,
            unique_taxons,
            avg_chunk_size,
            delta_chunks: 0,                // Would need to track this separately
            reference_chunks: total_chunks, // Simplified for now
        })
    }

    async fn list_all(&self) -> Result<Vec<SHA256Hash>> {
        Ok(self
            .chunks
            .iter()
            .map(|entry| entry.key().clone())
            .collect())
    }

    async fn clear(&mut self) -> Result<()> {
        self.chunks.clear();
        self.taxon_index.clear();
        Ok(())
    }
}
