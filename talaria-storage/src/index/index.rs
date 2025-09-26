/// Trait for chunk indexing strategies
use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

// TODO: Update when talaria-sequoia is extracted
// use talaria_sequoia::types::{ChunkMetadata, SHA256Hash, TaxonId};
use crate::core::types::{ChunkMetadata, SHA256Hash, TaxonId};

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

impl Default for InMemoryChunkIndex {
    fn default() -> Self {
        Self::new()
    }
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
        let hash = metadata.hash;

        // Update taxon index
        for taxon_id in &metadata.taxon_ids {
            self.taxon_index
                .entry(*taxon_id)
                .or_default()
                .insert(hash);
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
                .map(|entry| *entry.key())
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
            .map(|entry| *entry.key())
            .collect())
    }

    async fn clear(&mut self) -> Result<()> {
        self.chunks.clear();
        self.taxon_index.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    fn create_test_metadata(hash: SHA256Hash, size: usize, taxon_ids: Vec<TaxonId>) -> ChunkMetadata {
        ChunkMetadata {
            hash,
            size,
            sequence_count: 100,
            taxon_ids,
            compressed_size: Some(size / 2),
            compression_ratio: Some(0.5),
        }
    }

    #[tokio::test]
    async fn test_add_and_retrieve_chunk() {
        let mut index = InMemoryChunkIndex::new();
        let hash = SHA256Hash::compute(b"test");
        let metadata = create_test_metadata(hash, 1000, vec![TaxonId(1), TaxonId(2)]);

        // Add chunk
        index.add_chunk(metadata.clone()).await.unwrap();

        // Retrieve metadata
        let retrieved = index.get_metadata(&hash).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.hash, hash);
        assert_eq!(retrieved.size, 1000);
        assert_eq!(retrieved.taxon_ids, vec![TaxonId(1), TaxonId(2)]);
    }

    #[tokio::test]
    async fn test_remove_chunk() {
        let mut index = InMemoryChunkIndex::new();
        let hash = SHA256Hash::compute(b"test");
        let metadata = create_test_metadata(hash, 1000, vec![TaxonId(1)]);

        // Add and then remove
        index.add_chunk(metadata).await.unwrap();
        assert!(index.exists(&hash).await);

        index.remove_chunk(&hash).await.unwrap();
        assert!(!index.exists(&hash).await);

        // Verify taxon index is also updated
        let taxon_results = index.find_by_taxon(TaxonId(1)).await.unwrap();
        assert!(taxon_results.is_empty());
    }

    #[tokio::test]
    async fn test_find_by_taxon() {
        let mut index = InMemoryChunkIndex::new();

        // Add chunks with different taxons
        let hash1 = SHA256Hash::compute(b"chunk1");
        let hash2 = SHA256Hash::compute(b"chunk2");
        let hash3 = SHA256Hash::compute(b"chunk3");

        index.add_chunk(create_test_metadata(hash1, 100, vec![TaxonId(1), TaxonId(2)])).await.unwrap();
        index.add_chunk(create_test_metadata(hash2, 200, vec![TaxonId(2), TaxonId(3)])).await.unwrap();
        index.add_chunk(create_test_metadata(hash3, 300, vec![TaxonId(3)])).await.unwrap();

        // Find by single taxon
        let results = index.find_by_taxon(TaxonId(1)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results.contains(&hash1));

        let results = index.find_by_taxon(TaxonId(2)).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&hash1));
        assert!(results.contains(&hash2));

        let results = index.find_by_taxon(TaxonId(99)).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_find_by_multiple_taxons() {
        let mut index = InMemoryChunkIndex::new();

        let hash1 = SHA256Hash::compute(b"chunk1");
        let hash2 = SHA256Hash::compute(b"chunk2");
        let hash3 = SHA256Hash::compute(b"chunk3");

        index.add_chunk(create_test_metadata(hash1, 100, vec![TaxonId(1)])).await.unwrap();
        index.add_chunk(create_test_metadata(hash2, 200, vec![TaxonId(2)])).await.unwrap();
        index.add_chunk(create_test_metadata(hash3, 300, vec![TaxonId(3)])).await.unwrap();

        // Find by multiple taxons
        let results = index.find_by_taxons(&[TaxonId(1), TaxonId(3)]).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&hash1));
        assert!(results.contains(&hash3));
    }

    #[tokio::test]
    async fn test_query_with_filters() {
        let mut index = InMemoryChunkIndex::new();

        // Add various chunks
        let hash1 = SHA256Hash::compute(b"small");
        let hash2 = SHA256Hash::compute(b"medium");
        let hash3 = SHA256Hash::compute(b"large");

        index.add_chunk(create_test_metadata(hash1, 100, vec![TaxonId(1)])).await.unwrap();
        index.add_chunk(create_test_metadata(hash2, 500, vec![TaxonId(2)])).await.unwrap();
        index.add_chunk(create_test_metadata(hash3, 1000, vec![TaxonId(1), TaxonId(2)])).await.unwrap();

        // Query with size filter
        let query = ChunkQuery {
            size_range: Some((200, 600)),
            ..Default::default()
        };
        let results = index.query(query).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].size, 500);

        // Query with taxon filter
        let query = ChunkQuery {
            taxon_ids: Some(vec![TaxonId(1)]),
            ..Default::default()
        };
        let results = index.query(query).await.unwrap();
        assert_eq!(results.len(), 2);

        // Query with limit
        let query = ChunkQuery {
            limit: Some(2),
            ..Default::default()
        };
        let results = index.query(query).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_get_stats() {
        let mut index = InMemoryChunkIndex::new();

        // Add chunks
        let hash1 = SHA256Hash::compute(b"chunk1");
        let hash2 = SHA256Hash::compute(b"chunk2");

        index.add_chunk(create_test_metadata(hash1, 100, vec![TaxonId(1), TaxonId(2)])).await.unwrap();
        index.add_chunk(create_test_metadata(hash2, 200, vec![TaxonId(2), TaxonId(3)])).await.unwrap();

        let stats = index.get_stats().await.unwrap();
        assert_eq!(stats.total_chunks, 2);
        assert_eq!(stats.total_size, 300);
        assert_eq!(stats.unique_taxons, 3);
        assert_eq!(stats.avg_chunk_size, 150);
    }

    #[tokio::test]
    async fn test_list_all() {
        let mut index = InMemoryChunkIndex::new();

        let hash1 = SHA256Hash::compute(b"chunk1");
        let hash2 = SHA256Hash::compute(b"chunk2");
        let hash3 = SHA256Hash::compute(b"chunk3");

        index.add_chunk(create_test_metadata(hash1, 100, vec![])).await.unwrap();
        index.add_chunk(create_test_metadata(hash2, 200, vec![])).await.unwrap();
        index.add_chunk(create_test_metadata(hash3, 300, vec![])).await.unwrap();

        let all = index.list_all().await.unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&hash1));
        assert!(all.contains(&hash2));
        assert!(all.contains(&hash3));
    }

    #[tokio::test]
    async fn test_clear() {
        let mut index = InMemoryChunkIndex::new();

        // Add some data
        let hash = SHA256Hash::compute(b"test");
        index.add_chunk(create_test_metadata(hash, 100, vec![TaxonId(1)])).await.unwrap();

        // Clear
        index.clear().await.unwrap();

        // Verify everything is gone
        assert!(!index.exists(&hash).await);
        let all = index.list_all().await.unwrap();
        assert!(all.is_empty());
        let taxon_results = index.find_by_taxon(TaxonId(1)).await.unwrap();
        assert!(taxon_results.is_empty());
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        use tokio::task;
        use std::sync::Arc;

        let index = Arc::new(tokio::sync::RwLock::new(InMemoryChunkIndex::new()));
        let mut handles = vec![];

        // Spawn concurrent add operations
        for i in 0..10 {
            let index_clone = Arc::clone(&index);
            let handle = task::spawn(async move {
                let hash = SHA256Hash::compute(format!("chunk{}", i).as_bytes());
                let metadata = create_test_metadata(hash, i * 100, vec![TaxonId(i as u32)]);
                index_clone.write().await.add_chunk(metadata).await.unwrap();
                hash
            });
            handles.push(handle);
        }

        // Wait for all operations
        let hashes: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // Verify all chunks are present
        let index = index.read().await;
        for hash in &hashes {
            assert!(index.exists(hash).await);
        }

        let all = index.list_all().await.unwrap();
        assert_eq!(all.len(), 10);
    }
}
