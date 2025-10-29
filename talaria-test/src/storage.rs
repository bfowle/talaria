//! Storage test helpers
//!
//! Utilities for testing HERALD storage operations.

use crate::fixtures::test_database_source;
use crate::mock::InMemoryStorageBackend;
use crate::TestEnvironment;
use anyhow::{Context, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use talaria_core::types::{SHA256Hash, SequenceType};
use talaria_storage::types::{CanonicalSequence, SequenceRepresentation, SequenceRepresentations};

/// Test storage wrapper with helpers
///
/// Uses InMemoryStorageBackend for fast, isolated unit tests
pub struct TestStorage {
    backend: Arc<InMemoryStorageBackend>,
    path: PathBuf,
}

impl TestStorage {
    /// Create a new test storage in the given environment
    pub fn new(env: &TestEnvironment) -> Result<Self> {
        let path = env.sequences_dir();
        std::fs::create_dir_all(&path)?;

        Ok(Self {
            backend: Arc::new(InMemoryStorageBackend::new()),
            path,
        })
    }

    /// Create with custom path
    pub fn with_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)?;

        Ok(Self {
            backend: Arc::new(InMemoryStorageBackend::new()),
            path,
        })
    }

    /// Get the storage backend
    pub fn backend(&self) -> &Arc<InMemoryStorageBackend> {
        &self.backend
    }

    /// Store a test sequence
    pub fn store_sequence(&mut self, sequence: &str, header: &str) -> Result<SHA256Hash> {
        // Compute canonical hash
        let hash = SHA256Hash::compute(sequence.as_bytes());

        // Create canonical sequence
        let canonical = CanonicalSequence {
            sequence_hash: hash,
            sequence: sequence.as_bytes().to_vec(),
            length: sequence.len(),
            sequence_type: SequenceType::DNA, // Simple default for tests
            checksum: sequence.as_bytes().iter().map(|&b| b as u64).sum(),
            first_seen: Utc::now(),
            last_seen: Utc::now(),
        };

        // Store canonical sequence
        self.backend.store_canonical(&canonical)?;

        // Create and store representation
        let representation = SequenceRepresentation {
            source: test_database_source("storage"),
            header: header.to_string(),
            accessions: vec![],
            description: None,
            taxon_id: None,
            metadata: Default::default(),
            last_seen: Utc::now(),
        };

        let mut reps = self
            .backend
            .load_representations(&hash)
            .unwrap_or_else(|_| SequenceRepresentations {
                canonical_hash: hash,
                representations: Vec::new(),
            });
        reps.add_representation(representation);
        self.backend.store_representations(&reps)?;

        Ok(hash)
    }

    /// Store multiple test sequences
    pub fn store_sequences(&mut self, sequences: &[(String, String)]) -> Result<Vec<SHA256Hash>> {
        let mut hashes = Vec::new();
        for (header, seq) in sequences {
            hashes.push(self.store_sequence(seq, header)?);
        }
        Ok(hashes)
    }

    /// Verify storage contains sequence
    pub fn contains(&self, hash: &SHA256Hash) -> bool {
        self.backend.sequence_exists(hash).unwrap_or(false)
    }

    /// Get storage statistics
    pub fn stats(&self) -> Result<StorageStats> {
        Ok(StorageStats {
            chunk_count: self.backend.sequence_count(),
            total_size: 0, // Mock doesn't track size
            path: self.path.clone(),
        })
    }
}

/// Storage statistics for testing
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub chunk_count: usize,
    pub total_size: u64,
    pub path: PathBuf,
}

/// Pre-configured storage fixture with test data
pub struct StorageFixture {
    storage: TestStorage,
    sequences: Vec<(String, String, SHA256Hash)>, // (header, sequence, hash)
}

impl StorageFixture {
    /// Create a fixture with bacterial sequences
    pub fn with_bacterial_sequences(env: &TestEnvironment) -> Result<Self> {
        let mut storage = TestStorage::new(env)?;

        let sequences = vec![
            ("E. coli K12", "ECOLI", "ATGCATGCATGC"),
            ("Salmonella", "SALM", "GCGCGCGCGCGC"),
            ("Bacillus", "BACI", "TATATATATATAT"),
        ];

        let mut stored = Vec::new();
        for (name, id, seq) in sequences {
            let header = format!(">{} {}", id, name);
            let hash = storage.store_sequence(seq, &header)?;
            stored.push((header, seq.to_string(), hash));
        }

        Ok(Self {
            storage,
            sequences: stored,
        })
    }

    /// Create a fixture with viral sequences
    pub fn with_viral_sequences(env: &TestEnvironment) -> Result<Self> {
        let mut storage = TestStorage::new(env)?;

        let sequences = vec![
            ("SARS-CoV-2", "COVID", "AAAATTTTGGGGCCCC"),
            ("Influenza A", "FLU", "CCCCGGGGTTTTAAAA"),
            ("HIV-1", "HIV", "GTGTGTGTGTGTGTGT"),
        ];

        let mut stored = Vec::new();
        for (name, id, seq) in sequences {
            let header = format!(">{} {}", id, name);
            let hash = storage.store_sequence(seq, &header)?;
            stored.push((header, seq.to_string(), hash));
        }

        Ok(Self {
            storage,
            sequences: stored,
        })
    }

    /// Get the test storage
    pub fn storage(&self) -> &TestStorage {
        &self.storage
    }

    /// Get mutable test storage
    pub fn storage_mut(&mut self) -> &mut TestStorage {
        &mut self.storage
    }

    /// Get stored sequences
    pub fn sequences(&self) -> &[(String, String, SHA256Hash)] {
        &self.sequences
    }

    /// Get sequence by index
    pub fn get_sequence(&self, index: usize) -> Option<&(String, String, SHA256Hash)> {
        self.sequences.get(index)
    }

    /// Get all hashes
    pub fn hashes(&self) -> Vec<SHA256Hash> {
        self.sequences.iter().map(|(_, _, h)| *h).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TestEnvironment;

    #[test]
    fn test_storage_creation() {
        let env = TestEnvironment::new().unwrap();
        let storage = TestStorage::new(&env).unwrap();
        assert!(storage.path.exists());
    }

    #[test]
    fn test_sequence_storage() {
        let env = TestEnvironment::new().unwrap();
        let mut storage = TestStorage::new(&env).unwrap();

        let hash = storage.store_sequence("ATGC", ">test").unwrap();
        assert!(storage.contains(&hash));

        let stats = storage.stats().unwrap();
        assert!(stats.chunk_count > 0);
    }

    #[test]
    fn test_fixture() {
        let env = TestEnvironment::new().unwrap();
        let fixture = StorageFixture::with_bacterial_sequences(&env).unwrap();

        assert_eq!(fixture.sequences().len(), 3);
        assert!(fixture.get_sequence(0).unwrap().0.contains("ECOLI"));
    }
}
