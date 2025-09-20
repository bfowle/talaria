use crate::casg::merkle::{MerkleDAG, MerkleVerifiable, ProofProvider};
use crate::casg::types::{BiTemporalCoordinate, ChunkMetadata, MerkleHash, SHA256Hash};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Dual Merkle DAG system for bi-temporal verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DualMerkleDAG {
    pub sequence_dag: MerkleDAG,
    pub taxonomy_dag: MerkleDAG,
    pub cross_reference_root: SHA256Hash,
    pub created_at: DateTime<Utc>,
}

impl DualMerkleDAG {
    /// Create a new dual Merkle DAG from sequence and taxonomy data
    pub fn new(
        sequence_chunks: Vec<ChunkMetadata>,
        taxonomy_chunks: Vec<ChunkMetadata>,
    ) -> Result<Self> {
        let sequence_dag = MerkleDAG::build_from_items(sequence_chunks)?;
        let taxonomy_dag = MerkleDAG::build_from_items(taxonomy_chunks)?;

        // Compute cross-reference root
        let cross_reference_root = Self::compute_cross_reference(
            sequence_dag.root_hash().unwrap_or(SHA256Hash::zero()),
            taxonomy_dag.root_hash().unwrap_or(SHA256Hash::zero()),
        );

        Ok(Self {
            sequence_dag,
            taxonomy_dag,
            cross_reference_root,
            created_at: Utc::now(),
        })
    }

    /// Compute cross-reference hash linking both DAGs
    fn compute_cross_reference(seq_root: SHA256Hash, tax_root: SHA256Hash) -> SHA256Hash {
        let mut data = Vec::new();
        data.extend_from_slice(&seq_root.0);
        data.extend_from_slice(&tax_root.0);
        SHA256Hash::compute(&data)
    }

    /// Verify a bi-temporal proof
    pub fn verify_bitemporal_proof(&self, proof: &DualProof, item: &ChunkMetadata) -> bool {
        // Verify sequence proof
        let seq_valid = self.sequence_dag.verify_proof(&proof.sequence_proof, item);

        // Verify taxonomy proof
        let tax_valid = self.taxonomy_dag.verify_proof(&proof.taxonomy_proof, item);

        // Verify cross-reference
        let computed_cross = Self::compute_cross_reference(
            proof.sequence_proof.root,
            proof.taxonomy_proof.root,
        );
        let cross_valid = computed_cross == self.cross_reference_root;

        seq_valid && tax_valid && cross_valid
    }

    /// Create a snapshot at a specific bi-temporal coordinate
    pub fn snapshot_at(&self, coordinate: &BiTemporalCoordinate) -> BiTemporalSnapshot {
        BiTemporalSnapshot {
            coordinate: coordinate.clone(),
            sequence_root: self.sequence_dag.root_hash().unwrap_or(SHA256Hash::zero()),
            taxonomy_root: self.taxonomy_dag.root_hash().unwrap_or(SHA256Hash::zero()),
            cross_reference_root: self.cross_reference_root.clone(),
            snapshot_time: Utc::now(),
        }
    }

    /// Get the combined root hash
    pub fn get_root(&self) -> SHA256Hash {
        self.cross_reference_root.clone()
    }
}

impl MerkleVerifiable for DualMerkleDAG {
    fn compute_hash(&self) -> SHA256Hash {
        self.cross_reference_root.clone()
    }
}

impl ProofProvider for DualMerkleDAG {
    fn generate_proof(&self, item_hash: &SHA256Hash) -> Result<MerkleProof> {
        // Try to generate proof from sequence DAG first
        if let Ok(proof) = self.sequence_dag.generate_proof_for_hash(item_hash) {
            return Ok(proof);
        }

        // If not found, try taxonomy DAG
        if let Ok(proof) = self.taxonomy_dag.generate_proof_for_hash(item_hash) {
            return Ok(proof);
        }

        Err(anyhow::anyhow!("Item not found in either DAG"))
    }

    fn verify_proof(&self, proof: &MerkleProof, item_hash: &SHA256Hash) -> bool {
        // Verify against the cross-reference root
        let mut current_hash = item_hash.clone();

        for sibling in &proof.siblings {
            let combined = if sibling.is_left {
                let mut data = Vec::new();
                data.extend_from_slice(&sibling.hash.0);
                data.extend_from_slice(&current_hash.0);
                data
            } else {
                let mut data = Vec::new();
                data.extend_from_slice(&current_hash.0);
                data.extend_from_slice(&sibling.hash.0);
                data
            };
            current_hash = SHA256Hash::compute(&combined);
        }

        current_hash == proof.root
    }
}

/// Proof for dual Merkle DAG verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DualProof {
    pub sequence_proof: MerkleProof,
    pub taxonomy_proof: MerkleProof,
    pub cross_reference: SHA256Hash,
}

/// Merkle proof structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    pub root: SHA256Hash,
    pub siblings: Vec<ProofNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofNode {
    pub hash: SHA256Hash,
    pub is_left: bool,
}

impl DualProof {
    pub fn new(
        sequence_proof: MerkleProof,
        taxonomy_proof: MerkleProof,
        cross_reference: SHA256Hash,
    ) -> Self {
        Self {
            sequence_proof,
            taxonomy_proof,
            cross_reference,
        }
    }

    /// Verify the proof is internally consistent
    pub fn verify_consistency(&self) -> bool {
        let computed = DualMerkleDAG::compute_cross_reference(
            self.sequence_proof.root.clone(),
            self.taxonomy_proof.root.clone(),
        );
        computed == self.cross_reference
    }
}

/// A snapshot of the bi-temporal state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiTemporalSnapshot {
    pub coordinate: BiTemporalCoordinate,
    pub sequence_root: MerkleHash,
    pub taxonomy_root: MerkleHash,
    pub cross_reference_root: SHA256Hash,
    pub snapshot_time: DateTime<Utc>,
}

impl BiTemporalSnapshot {
    /// Verify this snapshot against a dual DAG
    pub fn verify_against(&self, dag: &DualMerkleDAG) -> bool {
        let seq_match = dag.sequence_dag.root_hash()
            .map_or(false, |h| h == self.sequence_root);
        let tax_match = dag.taxonomy_dag.root_hash()
            .map_or(false, |h| h == self.taxonomy_root);
        let cross_match = dag.cross_reference_root == self.cross_reference_root;

        seq_match && tax_match && cross_match
    }
}

/// Manager for bi-temporal operations
pub struct BiTemporalManager {
    snapshots: HashMap<BiTemporalCoordinate, BiTemporalSnapshot>,
    current_dag: Option<DualMerkleDAG>,
}

impl BiTemporalManager {
    pub fn new() -> Self {
        Self {
            snapshots: HashMap::new(),
            current_dag: None,
        }
    }

    /// Create a new snapshot at the current time
    pub fn create_snapshot(
        &mut self,
        sequence_chunks: Vec<ChunkMetadata>,
        taxonomy_chunks: Vec<ChunkMetadata>,
    ) -> Result<BiTemporalSnapshot> {
        let dag = DualMerkleDAG::new(sequence_chunks, taxonomy_chunks)?;
        let coordinate = BiTemporalCoordinate {
            sequence_time: Utc::now(),
            taxonomy_time: Utc::now(),
        };

        let snapshot = dag.snapshot_at(&coordinate);
        self.snapshots.insert(coordinate.clone(), snapshot.clone());
        self.current_dag = Some(dag);

        Ok(snapshot)
    }

    /// Get a snapshot at a specific coordinate
    pub fn get_snapshot(&self, coordinate: &BiTemporalCoordinate) -> Option<&BiTemporalSnapshot> {
        self.snapshots.get(coordinate)
    }

    /// List all available snapshots
    pub fn list_snapshots(&self) -> Vec<&BiTemporalSnapshot> {
        self.snapshots.values().collect()
    }

    /// Verify a snapshot is valid
    pub fn verify_snapshot(&self, snapshot: &BiTemporalSnapshot) -> bool {
        if let Some(dag) = &self.current_dag {
            snapshot.verify_against(dag)
        } else {
            false
        }
    }

    /// Get the current DAG
    pub fn current_dag(&self) -> Option<&DualMerkleDAG> {
        self.current_dag.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dual_merkle_dag() {
        let seq_chunks = vec![
            ChunkMetadata {
                hash: SHA256Hash::compute(b"seq1"),
                size: 100,
                sequence_count: 1,
                compressed_size: 80,
            },
            ChunkMetadata {
                hash: SHA256Hash::compute(b"seq2"),
                size: 200,
                sequence_count: 2,
                compressed_size: 160,
            },
        ];

        let tax_chunks = vec![
            ChunkMetadata {
                hash: SHA256Hash::compute(b"tax1"),
                size: 50,
                sequence_count: 1,
                compressed_size: 40,
            },
        ];

        let dag = DualMerkleDAG::new(seq_chunks, tax_chunks).unwrap();
        assert!(dag.sequence_dag.root_hash().is_some());
        assert!(dag.taxonomy_dag.root_hash().is_some());
    }

    #[test]
    fn test_bitemporal_snapshot() {
        let mut manager = BiTemporalManager::new();

        let seq_chunks = vec![
            ChunkMetadata {
                hash: SHA256Hash::compute(b"test"),
                size: 100,
                sequence_count: 1,
                compressed_size: 80,
            },
        ];

        let tax_chunks = vec![
            ChunkMetadata {
                hash: SHA256Hash::compute(b"taxonomy"),
                size: 50,
                sequence_count: 1,
                compressed_size: 40,
            },
        ];

        let snapshot = manager.create_snapshot(seq_chunks, tax_chunks).unwrap();
        assert!(manager.verify_snapshot(&snapshot));
    }
}