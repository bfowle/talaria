//! Mock storage backend for testing
//!
//! Provides an in-memory implementation of `SequenceStorageBackend` for fast,
//! deterministic unit tests without RocksDB dependencies.

use anyhow::{anyhow, Result};
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use talaria_core::types::SHA256Hash;
use talaria_core::StorageStats;
use talaria_storage::types::{CanonicalSequence, SequenceRepresentations, SequenceStorageBackend};

/// In-memory storage backend for testing
///
/// This provides a simple HashMap-based implementation of the storage backend
/// that runs entirely in memory with no disk I/O. Perfect for unit tests.
///
/// # Example
///
/// ```ignore
/// use talaria_test::mock::InMemoryStorageBackend;
/// use talaria_herald::storage::SequenceStorage;
///
/// #[test]
/// fn test_something() {
///     let backend = InMemoryStorageBackend::new();
///     let storage = SequenceStorage::with_backend(Arc::new(backend)).unwrap();
///     // Test business logic without RocksDB complexity
/// }
/// ```
#[derive(Debug, Clone)]
pub struct InMemoryStorageBackend {
    /// Canonical sequences indexed by hash
    sequences: Arc<RwLock<HashMap<SHA256Hash, CanonicalSequence>>>,
    /// Sequence representations indexed by canonical hash
    representations: Arc<RwLock<HashMap<SHA256Hash, SequenceRepresentations>>>,
    /// Optional call recording for verification
    calls: Arc<RwLock<Vec<String>>>,
}

impl InMemoryStorageBackend {
    /// Create a new empty in-memory storage backend
    pub fn new() -> Self {
        Self {
            sequences: Arc::new(RwLock::new(HashMap::new())),
            representations: Arc::new(RwLock::new(HashMap::new())),
            calls: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create with pre-populated data
    pub fn with_sequences(sequences: Vec<CanonicalSequence>) -> Self {
        let backend = Self::new();
        for seq in sequences {
            backend
                .store_canonical(&seq)
                .expect("Failed to store sequence");
        }
        backend
    }

    /// Get the number of stored sequences
    pub fn sequence_count(&self) -> usize {
        self.sequences.read().unwrap().len()
    }

    /// Get the number of stored representation records
    pub fn representation_record_count(&self) -> usize {
        self.representations.read().unwrap().len()
    }

    /// Get the total number of individual representations
    pub fn total_representations(&self) -> usize {
        self.representations
            .read()
            .unwrap()
            .values()
            .map(|r| r.representations().len())
            .sum()
    }

    /// Clear all data
    pub fn clear(&self) {
        self.sequences.write().unwrap().clear();
        self.representations.write().unwrap().clear();
        self.calls.write().unwrap().clear();
    }

    /// Get recorded method calls (for verification in tests)
    pub fn get_calls(&self) -> Vec<String> {
        self.calls.read().unwrap().clone()
    }

    /// Enable call recording
    fn record_call(&self, call: impl Into<String>) {
        self.calls.write().unwrap().push(call.into());
    }
}

impl Default for InMemoryStorageBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SequenceStorageBackend for InMemoryStorageBackend {
    fn sequence_exists(&self, hash: &SHA256Hash) -> Result<bool> {
        self.record_call(format!("sequence_exists({})", hash));
        Ok(self.sequences.read().unwrap().contains_key(hash))
    }

    fn sequences_exist_batch(&self, hashes: &[SHA256Hash]) -> Result<Vec<bool>> {
        self.record_call(format!("sequences_exist_batch(count={})", hashes.len()));
        let sequences = self.sequences.read().unwrap();
        Ok(hashes.iter().map(|h| sequences.contains_key(h)).collect())
    }

    fn store_canonical(&self, sequence: &CanonicalSequence) -> Result<()> {
        self.record_call(format!("store_canonical({})", sequence.sequence_hash));
        self.sequences
            .write()
            .unwrap()
            .insert(sequence.sequence_hash, sequence.clone());
        Ok(())
    }

    fn store_canonical_batch(&self, sequences: &[CanonicalSequence]) -> Result<()> {
        self.record_call(format!("store_canonical_batch(count={})", sequences.len()));
        let mut storage = self.sequences.write().unwrap();
        for seq in sequences {
            storage.insert(seq.sequence_hash, seq.clone());
        }
        Ok(())
    }

    fn load_canonical(&self, hash: &SHA256Hash) -> Result<CanonicalSequence> {
        self.record_call(format!("load_canonical({})", hash));
        self.sequences
            .read()
            .unwrap()
            .get(hash)
            .cloned()
            .ok_or_else(|| anyhow!("Sequence not found: {}", hash))
    }

    fn store_representations(&self, representations: &SequenceRepresentations) -> Result<()> {
        self.record_call(format!(
            "store_representations({})",
            representations.canonical_hash
        ));
        self.representations
            .write()
            .unwrap()
            .insert(representations.canonical_hash, representations.clone());
        Ok(())
    }

    fn load_representations(&self, hash: &SHA256Hash) -> Result<SequenceRepresentations> {
        self.record_call(format!("load_representations({})", hash));
        self.representations
            .read()
            .unwrap()
            .get(hash)
            .cloned()
            .ok_or_else(|| {
                // Return empty representations if none exist (matches RocksDB behavior)
                SequenceRepresentations {
                    canonical_hash: *hash,
                    representations: Vec::new(),
                }
            })
            .or_else(Ok)
    }

    fn get_stats(&self) -> Result<StorageStats> {
        self.record_call("get_stats()".to_string());

        let sequences = self.sequences.read().unwrap();
        let representations = self.representations.read().unwrap();

        let total_sequences = sequences.len();

        // Calculate total size (approximate)
        let total_size: usize = sequences.values().map(|s| s.sequence.len()).sum();

        // Count total representations across all records
        let total_representations: usize = representations
            .values()
            .map(|r| r.representations().len())
            .sum();

        // Calculate deduplication ratio
        let deduplication_ratio = if total_sequences > 0 && total_representations > 0 {
            total_representations as f32 / total_sequences as f32
        } else {
            1.0
        };

        Ok(StorageStats {
            total_chunks: total_sequences,
            total_size,
            compressed_chunks: total_sequences,
            deduplication_ratio,
            total_sequences: Some(total_sequences),
            total_representations: Some(total_representations),
        })
    }

    fn list_all_hashes(&self) -> Result<Vec<SHA256Hash>> {
        self.record_call("list_all_hashes()".to_string());
        Ok(self.sequences.read().unwrap().keys().copied().collect())
    }

    fn get_sequence_size(&self, hash: &SHA256Hash) -> Result<usize> {
        self.record_call(format!("get_sequence_size({})", hash));
        self.sequences
            .read()
            .unwrap()
            .get(hash)
            .map(|s| s.sequence.len())
            .ok_or_else(|| anyhow!("Sequence not found: {}", hash))
    }

    fn remove_sequence(&self, hash: &SHA256Hash) -> Result<()> {
        self.record_call(format!("remove_sequence({})", hash));
        self.sequences.write().unwrap().remove(hash);
        self.representations.write().unwrap().remove(hash);
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        self.record_call("flush()".to_string());
        // No-op for in-memory storage
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use talaria_core::types::{SequenceType, TaxonId};
    use talaria_core::{DatabaseSource, NCBIDatabase};
    use talaria_storage::types::SequenceRepresentation;

    fn create_test_sequence(seq: &str, id: u8) -> CanonicalSequence {
        CanonicalSequence {
            sequence_hash: SHA256Hash::compute(seq.as_bytes()),
            sequence: seq.as_bytes().to_vec(),
            length: seq.len(),
            sequence_type: SequenceType::DNA,
            checksum: id as u64,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
        }
    }

    #[test]
    fn test_backend_creation() {
        let backend = InMemoryStorageBackend::new();
        assert_eq!(backend.sequence_count(), 0);
        assert_eq!(backend.total_representations(), 0);
    }

    #[test]
    fn test_sequence_storage_and_retrieval() {
        let backend = InMemoryStorageBackend::new();
        let seq = create_test_sequence("ATCG", 1);

        // Store
        backend.store_canonical(&seq).unwrap();
        assert_eq!(backend.sequence_count(), 1);

        // Check existence
        assert!(backend.sequence_exists(&seq.sequence_hash).unwrap());

        // Retrieve
        let loaded = backend.load_canonical(&seq.sequence_hash).unwrap();
        assert_eq!(loaded.sequence, seq.sequence);
    }

    #[test]
    fn test_batch_operations() {
        let backend = InMemoryStorageBackend::new();
        let sequences = vec![
            create_test_sequence("ATCG", 1),
            create_test_sequence("GCTA", 2),
            create_test_sequence("AAAA", 3),
        ];

        // Batch store
        backend.store_canonical_batch(&sequences).unwrap();
        assert_eq!(backend.sequence_count(), 3);

        // Batch check
        let hashes: Vec<_> = sequences.iter().map(|s| s.sequence_hash).collect();
        let exists = backend.sequences_exist_batch(&hashes).unwrap();
        assert!(exists.iter().all(|&e| e));
    }

    #[test]
    fn test_representations() {
        let backend = InMemoryStorageBackend::new();
        let seq = create_test_sequence("ATCG", 1);
        backend.store_canonical(&seq).unwrap();

        // Create representations
        let mut reps = SequenceRepresentations {
            canonical_hash: seq.sequence_hash,
            representations: Vec::new(),
        };

        for i in 0..3 {
            let rep = SequenceRepresentation {
                source: DatabaseSource::NCBI(NCBIDatabase::NR),
                header: format!("Header {}", i),
                accessions: vec![format!("ACC{}", i)],
                description: Some(format!("Description {}", i)),
                taxon_id: Some(TaxonId(9606)),
                metadata: Default::default(),
                last_seen: Utc::now(),
            };
            reps.add_representation(rep);
        }

        // Store representations
        backend.store_representations(&reps).unwrap();

        // Load back
        let loaded = backend.load_representations(&seq.sequence_hash).unwrap();
        assert_eq!(loaded.representations().len(), 3);
    }

    #[test]
    fn test_stats() {
        let backend = InMemoryStorageBackend::new();

        // Store 2 sequences
        backend
            .store_canonical(&create_test_sequence("ATCG", 1))
            .unwrap();
        backend
            .store_canonical(&create_test_sequence("GCTA", 2))
            .unwrap();

        // Store representations (3 reps for first seq, 2 for second)
        let hash1 = SHA256Hash::compute(b"ATCG");
        let hash2 = SHA256Hash::compute(b"GCTA");

        let reps1 = SequenceRepresentations {
            canonical_hash: hash1,
            representations: vec![
                SequenceRepresentation {
                    source: DatabaseSource::NCBI(NCBIDatabase::NR),
                    header: "H1".to_string(),
                    accessions: vec!["A1".to_string()],
                    description: Some("D1".to_string()),
                    taxon_id: None,
                    metadata: Default::default(),
                    last_seen: Utc::now(),
                },
                SequenceRepresentation {
                    source: DatabaseSource::NCBI(NCBIDatabase::RefSeq),
                    header: "H2".to_string(),
                    accessions: vec!["A2".to_string()],
                    description: Some("D2".to_string()),
                    taxon_id: None,
                    metadata: Default::default(),
                    last_seen: Utc::now(),
                },
                SequenceRepresentation {
                    source: DatabaseSource::Custom("test".to_string()),
                    header: "H3".to_string(),
                    accessions: vec!["A3".to_string()],
                    description: Some("D3".to_string()),
                    taxon_id: None,
                    metadata: Default::default(),
                    last_seen: Utc::now(),
                },
            ],
        };

        let reps2 = SequenceRepresentations {
            canonical_hash: hash2,
            representations: vec![
                SequenceRepresentation {
                    source: DatabaseSource::NCBI(NCBIDatabase::NR),
                    header: "H4".to_string(),
                    accessions: vec!["A4".to_string()],
                    description: Some("D4".to_string()),
                    taxon_id: None,
                    metadata: Default::default(),
                    last_seen: Utc::now(),
                },
                SequenceRepresentation {
                    source: DatabaseSource::Custom("test2".to_string()),
                    header: "H5".to_string(),
                    accessions: vec!["A5".to_string()],
                    description: Some("D5".to_string()),
                    taxon_id: None,
                    metadata: Default::default(),
                    last_seen: Utc::now(),
                },
            ],
        };

        backend.store_representations(&reps1).unwrap();
        backend.store_representations(&reps2).unwrap();

        // Check stats
        let stats = backend.get_stats().unwrap();
        assert_eq!(stats.total_sequences, Some(2));
        assert_eq!(stats.total_representations, Some(5)); // 3 + 2
        assert_eq!(stats.deduplication_ratio, 2.5); // 5 / 2
    }

    #[test]
    fn test_call_recording() {
        let backend = InMemoryStorageBackend::new();
        let seq = create_test_sequence("ATCG", 1);

        backend.store_canonical(&seq).unwrap();
        backend.sequence_exists(&seq.sequence_hash).unwrap();
        backend.load_canonical(&seq.sequence_hash).unwrap();

        let calls = backend.get_calls();
        assert!(calls.iter().any(|c| c.contains("store_canonical")));
        assert!(calls.iter().any(|c| c.contains("sequence_exists")));
        assert!(calls.iter().any(|c| c.contains("load_canonical")));
    }

    #[test]
    fn test_clear() {
        let backend = InMemoryStorageBackend::new();
        backend
            .store_canonical(&create_test_sequence("ATCG", 1))
            .unwrap();
        assert_eq!(backend.sequence_count(), 1);

        backend.clear();
        assert_eq!(backend.sequence_count(), 0);
        assert_eq!(backend.get_calls().len(), 0);
    }
}
