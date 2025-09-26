/// Index structures for fast sequence lookups and queries
///
/// This module provides multiple index types optimized for different bioinformatics
/// query patterns, enabling O(1) lookups while maintaining compatibility with
/// existing tools that expect accession-based queries.

use anyhow::Result;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::format::{TalariaFormat, serialize, deserialize};
use crate::types::{SHA256Hash, TaxonId, DatabaseSource};

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
            0 => u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]),
            1 => u64::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]]),
            2 => u64::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23]]),
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
        let estimate = -1.0 * (self.size as f64) * ((1.0 - ones as f64 / self.size as f64).ln()) / self.hash_count as f64;
        estimate.round() as usize
    }
}

/// Serializable version of BloomFilter
#[derive(Debug, Serialize, Deserialize)]
struct BloomFilterData {
    bits: Vec<u8>,  // Packed bit vector
    size: usize,
    hash_count: usize,
}

impl From<&BloomFilter> for BloomFilterData {
    fn from(bf: &BloomFilter) -> Self {
        // Pack bool vector into bytes
        let mut packed = Vec::with_capacity((bf.size + 7) / 8);
        for chunk in bf.bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                if bit {
                    byte |= 1 << i;
                }
            }
            packed.push(byte);
        }

        Self {
            bits: packed,
            size: bf.size,
            hash_count: bf.hash_count,
        }
    }
}

impl From<BloomFilterData> for BloomFilter {
    fn from(data: BloomFilterData) -> Self {
        // Unpack bytes to bool vector
        let mut bits = Vec::with_capacity(data.size);
        for byte in &data.bits {
            for i in 0..8 {
                if bits.len() < data.size {
                    bits.push((byte >> i) & 1 == 1);
                }
            }
        }

        Self {
            bits,
            size: data.size,
            hash_count: data.hash_count,
        }
    }
}

/// Cache entry for query results
#[derive(Clone, Debug)]
struct CachedQueryResult {
    sequences: HashSet<SHA256Hash>,
    timestamp: std::time::Instant,
}

/// Main index structure combining multiple index types
#[derive(Debug)]
pub struct SequenceIndices {
    /// Maps accession numbers to canonical sequence hashes
    /// For compatibility with tools expecting accessions (e.g., "NP_123456.1")
    pub accession_to_hash: Arc<DashMap<String, SHA256Hash>>,

    /// Maps taxonomy IDs to sets of sequence hashes
    /// Enables efficient taxonomy-based queries (80% of bioinformatics use cases)
    pub taxonomy_to_sequences: Arc<DashMap<TaxonId, HashSet<SHA256Hash>>>,

    /// Bloom filter for O(1) "do we have this sequence?" checks
    /// Prevents unnecessary disk reads for non-existent sequences
    pub sequence_bloom: Arc<parking_lot::RwLock<BloomFilter>>,

    /// Maps database sources to their sequences
    /// Enables virtual database construction and cross-database queries
    pub database_to_sequences: Arc<DashMap<DatabaseSource, HashSet<SHA256Hash>>>,

    /// Path for persistent storage
    base_path: PathBuf,

    /// Query cache for frequently accessed taxonomy queries
    /// Key: "taxon:<id>" or "db:<source>:<dataset>"
    query_cache: Arc<DashMap<String, CachedQueryResult>>,
}

impl SequenceIndices {
    /// Create new indices, loading from disk if available
    pub fn new(base_path: &Path) -> Result<Self> {
        let indices_dir = base_path.join("indices");
        fs::create_dir_all(&indices_dir)?;

        // Try to load existing indices
        if let Ok(indices) = Self::load(&indices_dir) {
            return Ok(indices);
        }

        // Create new indices
        Ok(Self {
            accession_to_hash: Arc::new(DashMap::new()),
            taxonomy_to_sequences: Arc::new(DashMap::new()),
            sequence_bloom: Arc::new(parking_lot::RwLock::new(
                BloomFilter::new(1_000_000, 0.01)  // 1M sequences, 1% false positive
            )),
            database_to_sequences: Arc::new(DashMap::new()),
            base_path: indices_dir,
            query_cache: Arc::new(DashMap::new()),
        })
    }

    /// Add a sequence to indices
    pub fn add_sequence(
        &self,
        hash: SHA256Hash,
        accession: Option<String>,
        taxon_id: Option<TaxonId>,
        source: Option<DatabaseSource>,
    ) {
        // Update bloom filter
        {
            let mut bloom = self.sequence_bloom.write();
            bloom.insert(&hash);
        }

        // Update accession index if provided
        if let Some(acc) = accession {
            self.accession_to_hash.insert(acc, hash.clone());
        }

        // Update taxonomy index if provided
        if let Some(tax_id) = taxon_id {
            self.taxonomy_to_sequences
                .entry(tax_id)
                .or_insert_with(HashSet::new)
                .insert(hash.clone());
        }

        // Update database index if provided
        if let Some(db_source) = source {
            self.database_to_sequences
                .entry(db_source)
                .or_insert_with(HashSet::new)
                .insert(hash);
        }
    }

    /// Check if sequence exists (fast bloom filter check)
    pub fn sequence_exists(&self, hash: &SHA256Hash) -> bool {
        self.sequence_bloom.read().contains(hash)
    }

    /// Get sequence hash by accession
    pub fn get_by_accession(&self, accession: &str) -> Option<SHA256Hash> {
        self.accession_to_hash.get(accession).map(|e| e.clone())
    }

    /// Get all sequences for a taxonomy ID (with caching for performance)
    pub fn get_by_taxonomy(&self, taxon_id: TaxonId) -> HashSet<SHA256Hash> {
        let cache_key = format!("taxon:{}", taxon_id.0);

        // Check cache first (valid for 60 seconds)
        if let Some(cached) = self.query_cache.get(&cache_key) {
            if cached.timestamp.elapsed().as_secs() < 60 {
                return cached.sequences.clone();
            }
        }

        // Perform query
        let result = self.taxonomy_to_sequences
            .get(&taxon_id)
            .map(|e| e.clone())
            .unwrap_or_default();

        // Cache result
        self.query_cache.insert(
            cache_key,
            CachedQueryResult {
                sequences: result.clone(),
                timestamp: std::time::Instant::now(),
            },
        );

        result
    }

    /// Get all sequences from a database source
    pub fn get_by_database(&self, source: &DatabaseSource) -> HashSet<SHA256Hash> {
        self.database_to_sequences
            .get(source)
            .map(|e| e.clone())
            .unwrap_or_default()
    }

    /// Get sequences matching multiple taxonomy IDs
    pub fn get_by_taxonomies(&self, taxon_ids: &[TaxonId]) -> HashSet<SHA256Hash> {
        let mut result = HashSet::new();
        for tax_id in taxon_ids {
            if let Some(sequences) = self.taxonomy_to_sequences.get(tax_id) {
                result.extend(sequences.iter().cloned());
            }
        }
        result
    }

    /// Save indices to disk
    pub fn save(&self) -> Result<()> {
        let format = TalariaFormat;

        // Save accession index
        let accession_path = self.base_path.join("accession_index.tal");
        let accession_data: HashMap<String, SHA256Hash> = self.accession_to_hash
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();
        let accession_bytes = serialize(&format, &accession_data)?;
        fs::write(&accession_path, accession_bytes)?;

        // Save taxonomy index (convert TaxonId to string for serialization)
        let taxonomy_path = self.base_path.join("taxonomy_index.tal");
        let taxonomy_data: HashMap<String, HashSet<SHA256Hash>> = self.taxonomy_to_sequences
            .iter()
            .map(|e| (e.key().to_string(), e.value().clone()))
            .collect();
        let taxonomy_bytes = serialize(&format, &taxonomy_data)?;
        fs::write(&taxonomy_path, taxonomy_bytes)?;

        // Save database index (convert DatabaseSource to string for serialization)
        let database_path = self.base_path.join("database_index.tal");
        let database_data: HashMap<String, HashSet<SHA256Hash>> = self.database_to_sequences
            .iter()
            .map(|e| (format!("{}/{}", e.key().source_name(), e.key().dataset_name()), e.value().clone()))
            .collect();
        let database_bytes = serialize(&format, &database_data)?;
        fs::write(&database_path, database_bytes)?;

        // Save bloom filter
        let bloom_path = self.base_path.join("bloom_filter.tal");
        let bloom_data = BloomFilterData::from(&*self.sequence_bloom.read());
        let bloom_bytes = serialize(&format, &bloom_data)?;
        fs::write(&bloom_path, bloom_bytes)?;

        Ok(())
    }

    /// Load indices from disk
    pub fn load(indices_dir: &Path) -> Result<Self> {
        let format = TalariaFormat;

        // Load accession index
        let accession_path = indices_dir.join("accession_index.tal");
        let accession_map = if accession_path.exists() {
            let data = fs::read(&accession_path)?;
            let map: HashMap<String, SHA256Hash> = deserialize(&format, &data)?;
            let dash_map = DashMap::new();
            for (k, v) in map {
                dash_map.insert(k, v);
            }
            Arc::new(dash_map)
        } else {
            Arc::new(DashMap::new())
        };

        // Load taxonomy index (convert string keys back to TaxonId)
        let taxonomy_path = indices_dir.join("taxonomy_index.tal");
        let taxonomy_map = if taxonomy_path.exists() {
            let data = fs::read(&taxonomy_path)?;
            let map: HashMap<String, HashSet<SHA256Hash>> = deserialize(&format, &data)?;
            let dash_map = DashMap::new();
            for (k, v) in map {
                if let Ok(taxon_value) = k.parse::<u32>() {
                    dash_map.insert(TaxonId(taxon_value), v);
                }
            }
            Arc::new(dash_map)
        } else {
            Arc::new(DashMap::new())
        };

        // Load database index (convert string keys back to DatabaseSource)
        let database_path = indices_dir.join("database_index.tal");
        let database_map = if database_path.exists() {
            let data = fs::read(&database_path)?;
            let map: HashMap<String, HashSet<SHA256Hash>> = deserialize(&format, &data)?;
            let dash_map = DashMap::new();
            for (k, v) in map {
                // Parse "source/dataset" format
                let parts: Vec<&str> = k.split('/').collect();
                if parts.len() == 2 {
                    // Parse source and dataset to create appropriate enum variant
                    let source = match parts[0] {
                        "uniprot" => match parts[1] {
                            "swissprot" => DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt),
                            "trembl" => DatabaseSource::UniProt(talaria_core::UniProtDatabase::TrEMBL),
                            _ => DatabaseSource::Custom(format!("{}/{}", parts[0], parts[1])),
                        },
                        "ncbi" => match parts[1] {
                            "nr" => DatabaseSource::NCBI(talaria_core::NCBIDatabase::NR),
                            "nt" => DatabaseSource::NCBI(talaria_core::NCBIDatabase::NT),
                            "refseq" => DatabaseSource::NCBI(talaria_core::NCBIDatabase::RefSeq),
                            "genbank" => DatabaseSource::NCBI(talaria_core::NCBIDatabase::GenBank),
                            _ => DatabaseSource::Custom(format!("{}/{}", parts[0], parts[1])),
                        },
                        _ => DatabaseSource::Custom(format!("{}/{}", parts[0], parts[1])),
                    };
                    dash_map.insert(source, v);
                }
            }
            Arc::new(dash_map)
        } else {
            Arc::new(DashMap::new())
        };

        // Load bloom filter
        let bloom_path = indices_dir.join("bloom_filter.tal");
        let bloom = if bloom_path.exists() {
            let data = fs::read(&bloom_path)?;
            let bloom_data: BloomFilterData = deserialize(&format, &data)?;
            BloomFilter::from(bloom_data)
        } else {
            BloomFilter::new(1_000_000, 0.01)
        };

        Ok(Self {
            accession_to_hash: accession_map,
            taxonomy_to_sequences: taxonomy_map,
            sequence_bloom: Arc::new(parking_lot::RwLock::new(bloom)),
            database_to_sequences: database_map,
            base_path: indices_dir.to_path_buf(),
            query_cache: Arc::new(DashMap::new()),
        })
    }

    /// Get statistics about indices
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            total_sequences: self.sequence_bloom.read().estimate_count(),
            total_accessions: self.accession_to_hash.len(),
            total_taxa: self.taxonomy_to_sequences.len(),
            total_databases: self.database_to_sequences.len(),
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