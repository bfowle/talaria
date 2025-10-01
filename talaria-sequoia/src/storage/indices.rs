/// Index structures for fast sequence lookups and queries
///
/// This module provides multiple index types optimized for different bioinformatics
/// query patterns, enabling O(1) lookups while maintaining compatibility with
/// existing tools that expect accession-based queries.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use talaria_storage::backend::{RocksDBBackend, RocksDBIndexOps};
use crate::types::{DatabaseSource, SHA256Hash, TaxonId};

/// Bloom filter for O(1) sequence existence checks
/// Using simple bit vector for now, can upgrade to cuckoo filter later
#[derive(Debug, Clone)]
pub struct BloomFilter {
    bits: Vec<bool>,
    size: usize,
    hash_count: usize,
}

impl BloomFilter {
    /// Create new bloom filter with target false positive rate
    pub fn new(expected_items: usize, false_positive_rate: f64) -> Self {
        // Calculate optimal size and hash count
        let size = Self::optimal_size(expected_items, false_positive_rate);
        let hash_count = Self::optimal_hash_count(size, expected_items);

        Self {
            bits: vec![false; size],
            size,
            hash_count,
        }
    }

    /// Calculate optimal bit array size
    fn optimal_size(n: usize, p: f64) -> usize {
        let ln2 = std::f64::consts::LN_2;
        ((-1.0 * n as f64 * p.ln()) / (ln2 * ln2)).ceil() as usize
    }

    /// Calculate optimal number of hash functions
    fn optimal_hash_count(m: usize, n: usize) -> usize {
        let ln2 = std::f64::consts::LN_2;
        ((m as f64 / n as f64) * ln2).round() as usize
    }

    /// Add item to filter
    pub fn insert(&mut self, hash: &SHA256Hash) {
        for i in 0..self.hash_count {
            let index = self.hash_index(hash, i);
            self.bits[index] = true;
        }
    }

    /// Check if item might be in set (may have false positives)
    pub fn contains(&self, hash: &SHA256Hash) -> bool {
        for i in 0..self.hash_count {
            let index = self.hash_index(hash, i);
            if !self.bits[index] {
                return false;
            }
        }
        true
    }

    /// Get index for hash function i
    fn hash_index(&self, hash: &SHA256Hash, i: usize) -> usize {
        // Use different portions of SHA256 hash for different hash functions
        let bytes = &hash.0;
        let hash_val = match i {
            0 => u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            1 => u64::from_le_bytes([
                bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14],
                bytes[15],
            ]),
            2 => u64::from_le_bytes([
                bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22],
                bytes[23],
            ]),
            _ => {
                // Mix bytes for additional hash functions
                let mut result = 0u64;
                for j in 0..8 {
                    result = (result << 8) | bytes[(i * 7 + j) % 32] as u64;
                }
                result
            }
        };
        (hash_val as usize) % self.size
    }

    /// Estimate number of items in filter
    pub fn estimate_count(&self) -> usize {
        let ones = self.bits.iter().filter(|&&b| b).count();
        let estimate = -1.0 * (self.size as f64) * ((1.0 - ones as f64 / self.size as f64).ln())
            / self.hash_count as f64;
        estimate.round() as usize
    }
}

// Implement Serialize and Deserialize for BloomFilter to store in RocksDB
impl Serialize for BloomFilter {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Pack bool vector into bytes for efficient storage
        let mut packed = Vec::with_capacity((self.size + 7) / 8);
        for chunk in self.bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                if bit {
                    byte |= 1 << i;
                }
            }
            packed.push(byte);
        }

        // Serialize as tuple
        use serde::ser::SerializeTuple;
        let mut tuple = serializer.serialize_tuple(3)?;
        tuple.serialize_element(&packed)?;
        tuple.serialize_element(&self.size)?;
        tuple.serialize_element(&self.hash_count)?;
        tuple.end()
    }
}

impl<'de> Deserialize<'de> for BloomFilter {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (packed, size, hash_count): (Vec<u8>, usize, usize) =
            Deserialize::deserialize(deserializer)?;

        // Unpack bytes to bool vector
        let mut bits = Vec::with_capacity(size);
        for byte in &packed {
            for i in 0..8 {
                if bits.len() < size {
                    bits.push((byte >> i) & 1 == 1);
                }
            }
        }

        Ok(Self {
            bits,
            size,
            hash_count,
        })
    }
}

/// Main index structure using RocksDB for all storage
#[derive(Clone)]
pub struct SequenceIndices {
    /// RocksDB backend for all index operations
    /// Keys in INDICES column family:
    /// - "acc:{accession}" → SHA256Hash
    /// - "tax:{taxon_id}" → Vec<SHA256Hash>
    /// - "db:{source}" → Vec<SHA256Hash>
    /// - "bloom:filter" → Serialized BloomFilter
    backend: Arc<RocksDBBackend>,

    /// Bloom filter for O(1) "do we have this sequence?" checks
    /// Keep in memory for performance, persist to RocksDB
    pub sequence_bloom: Arc<parking_lot::RwLock<BloomFilter>>,

    /// Streaming mode flag - when true, skip index updates to save memory
    /// This is critical for processing large files like UniRef50 (26GB+)
    streaming_mode: Arc<AtomicBool>,
}

impl SequenceIndices {
    /// Create new indices with RocksDB backend
    pub fn new(base_path: &Path) -> Result<Self> {
        Self::with_backend(None, base_path, None)
    }

    /// Create indices with existing RocksDB backend (for sharing)
    pub fn with_backend(
        backend: Option<Arc<RocksDBBackend>>,
        base_path: &Path,
        bloom_config: Option<crate::config::BloomFilterConfig>,
    ) -> Result<Self> {
        // Use provided backend or create new one
        let backend = if let Some(b) = backend {
            b
        } else {
            let rocksdb_path = base_path.join("rocksdb");
            Arc::new(RocksDBBackend::new(&rocksdb_path)?)
        };

        // Get bloom filter configuration
        let bloom_config = bloom_config.unwrap_or_default();

        // Try to load bloom filter from RocksDB
        let bloom_filter = if let Ok(Some(data)) = backend.get_index("bloom:filter") {
            // Deserialize bloom filter from RocksDB
            if let Ok(bloom) = bincode::deserialize::<BloomFilter>(&data) {
                tracing::info!(
                    "Loaded existing bloom filter with {} estimated sequences",
                    bloom.estimate_count()
                );
                Arc::new(parking_lot::RwLock::new(bloom))
            } else {
                tracing::info!(
                    "Creating new bloom filter for {} expected sequences (FP rate: {})",
                    bloom_config.expected_sequences,
                    bloom_config.false_positive_rate
                );
                Arc::new(parking_lot::RwLock::new(BloomFilter::new(
                    bloom_config.expected_sequences,
                    bloom_config.false_positive_rate,
                )))
            }
        } else {
            tracing::info!(
                "Creating new bloom filter for {} expected sequences (FP rate: {})",
                bloom_config.expected_sequences,
                bloom_config.false_positive_rate
            );
            Arc::new(parking_lot::RwLock::new(BloomFilter::new(
                bloom_config.expected_sequences,
                bloom_config.false_positive_rate,
            )))
        };

        Ok(Self {
            backend,
            sequence_bloom: bloom_filter,
            streaming_mode: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Enable streaming mode - disables index updates to save memory
    pub fn set_streaming_mode(&self, enabled: bool) {
        self.streaming_mode.store(enabled, Ordering::Relaxed);
        // No need to clear RocksDB indices - they're on disk
        // This is the key benefit: memory usage stays constant
    }

    /// Check if streaming mode is enabled
    pub fn is_streaming_mode(&self) -> bool {
        self.streaming_mode.load(Ordering::Relaxed)
    }

    /// Add a sequence to indices
    pub fn add_sequence(
        &self,
        hash: SHA256Hash,
        accession: Option<String>,
        taxon_id: Option<TaxonId>,
        source: Option<DatabaseSource>,
    ) -> Result<()> {
        // Always update bloom filter (it's compact and essential)
        {
            let mut bloom = self.sequence_bloom.write();
            bloom.insert(&hash);
        }

        // Skip index updates in streaming mode to save memory
        if self.is_streaming_mode() {
            return Ok(());
        }

        // Update accession index if provided
        if let Some(acc) = accession {
            let key = format!("acc:{}", acc);
            self.backend.put_index(&key, &bincode::serialize(&hash)?)?;
        }

        // Update taxonomy index if provided
        if let Some(tax_id) = taxon_id {
            let key = format!("tax:{}", tax_id.0);
            self.backend.append_to_index_list(&key, &hash)?;
        }

        // Update database index if provided
        if let Some(db_source) = source {
            let key = format!(
                "db:{}:{}",
                db_source.source_name(),
                db_source.dataset_name()
            );
            self.backend.append_to_index_list(&key, &hash)?;
        }

        Ok(())
    }

    /// Check if sequence exists (fast bloom filter check)
    pub fn sequence_exists(&self, hash: &SHA256Hash) -> bool {
        self.sequence_bloom.read().contains(hash)
    }

    /// Get sequence hash by accession
    pub fn get_by_accession(&self, accession: &str) -> Option<SHA256Hash> {
        let key = format!("acc:{}", accession);
        match self.backend.get_index(&key) {
            Ok(Some(data)) => bincode::deserialize(&data).ok(),
            _ => None,
        }
    }

    /// Get all sequences for a taxonomy ID
    pub fn get_by_taxonomy(&self, taxon_id: TaxonId) -> HashSet<SHA256Hash> {
        let key = format!("tax:{}", taxon_id.0);
        match self.backend.get_index_list(&key) {
            Ok(hashes) => hashes.into_iter().collect(),
            Err(_) => HashSet::new(),
        }
    }

    /// Get all sequences from a database source
    pub fn get_by_database(&self, source: &DatabaseSource) -> HashSet<SHA256Hash> {
        let key = format!("db:{}:{}", source.source_name(), source.dataset_name());
        match self.backend.get_index_list(&key) {
            Ok(hashes) => hashes.into_iter().collect(),
            Err(_) => HashSet::new(),
        }
    }

    /// Get sequences matching multiple taxonomy IDs
    pub fn get_by_taxonomies(&self, taxon_ids: &[TaxonId]) -> HashSet<SHA256Hash> {
        let mut result = HashSet::new();
        for tax_id in taxon_ids {
            let key = format!("tax:{}", tax_id.0);
            if let Ok(hashes) = self.backend.get_index_list(&key) {
                result.extend(hashes);
            }
        }
        result
    }

    /// Save indices to disk (bloom filter only - other indices are already in RocksDB)
    pub fn save(&self) -> Result<()> {
        // Save bloom filter to RocksDB
        let bloom_data = bincode::serialize(&*self.sequence_bloom.read())?;
        self.backend.put_index("bloom:filter", &bloom_data)?;

        // Ensure RocksDB flushes to disk
        self.backend.flush()?;

        Ok(())
    }

    /// Load indices from disk (deprecated - use with_backend instead)
    pub fn load(indices_dir: &Path) -> Result<Self> {
        // Just create new instance with RocksDB backend
        // All data is already in RocksDB
        Self::new(indices_dir.parent().unwrap_or(indices_dir))
    }

    /// Get statistics about indices
    pub fn stats(&self) -> IndexStats {
        // For RocksDB, we need to estimate counts differently
        // This is a trade-off for memory efficiency
        IndexStats {
            total_sequences: self.sequence_bloom.read().estimate_count(),
            total_accessions: 0, // Would need to iterate RocksDB to count
            total_taxa: 0,       // Would need to iterate RocksDB to count
            total_databases: 0,  // Would need to iterate RocksDB to count
        }
    }
}

/// Statistics about index contents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub total_sequences: usize,
    pub total_accessions: usize,
    pub total_taxa: usize,
    pub total_databases: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    #[serial_test::serial]
    fn test_bloom_filter() {
        let mut bloom = BloomFilter::new(100, 0.01);

        let hash1 = SHA256Hash::compute(b"sequence1");
        let hash2 = SHA256Hash::compute(b"sequence2");
        let hash3 = SHA256Hash::compute(b"sequence3");

        bloom.insert(&hash1);
        bloom.insert(&hash2);

        assert!(bloom.contains(&hash1));
        assert!(bloom.contains(&hash2));
        assert!(!bloom.contains(&hash3));
    }

    #[test]
    #[serial_test::serial]
    fn test_indices() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let indices = SequenceIndices::new(temp_dir.path())?;

        let hash = SHA256Hash::compute(b"ACGTACGT");
        let accession = "NP_123456.1".to_string();
        let taxon_id = TaxonId(9606); // Human
        let source = DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt);

        // Add sequence
        indices.add_sequence(
            hash.clone(),
            Some(accession.clone()),
            Some(taxon_id.clone()),
            Some(source.clone()),
        );

        // Test lookups
        assert!(indices.sequence_exists(&hash));
        assert_eq!(indices.get_by_accession(&accession), Some(hash.clone()));
        assert!(indices.get_by_taxonomy(taxon_id.clone()).contains(&hash));
        assert!(indices.get_by_database(&source).contains(&hash));

        // Save and reload
        indices.save()?;
        let indices2 = SequenceIndices::load(&temp_dir.path().join("indices"))?;

        // Verify persistence
        assert!(indices2.sequence_exists(&hash));
        assert_eq!(indices2.get_by_accession(&accession), Some(hash.clone()));

        Ok(())
    }
}
