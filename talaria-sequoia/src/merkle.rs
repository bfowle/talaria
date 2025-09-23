/// Merkle DAG implementation for cryptographic verification
use crate::types::*;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Trait for types that can participate in Merkle tree construction
pub trait MerkleVerifiable {
    /// Compute the hash of this item
    fn compute_hash(&self) -> SHA256Hash;
}

/// Trait for types that can provide Merkle proofs
pub trait ProofProvider {
    /// Generate a proof that the target hash is in the tree
    fn generate_proof(&self, target: &SHA256Hash) -> Result<MerkleProof>;

    /// Verify a proof against the expected root
    fn verify_proof(&self, proof: &MerkleProof, data: &[u8]) -> bool;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleDAG {
    root: Option<MerkleNode>,
}

impl Default for MerkleDAG {
    fn default() -> Self {
        Self::new()
    }
}

impl MerkleDAG {
    pub fn new() -> Self {
        Self { root: None }
    }

    /// Build a Merkle tree from verifiable items
    pub fn build_from_items<T: MerkleVerifiable>(items: Vec<T>) -> Result<Self> {
        if items.is_empty() {
            return Ok(Self { root: None });
        }

        // Create leaf nodes from verifiable items
        let mut nodes: Vec<MerkleNode> = items
            .into_iter()
            .map(|item| {
                let hash = item.compute_hash();
                MerkleNode {
                    hash: hash.clone(),
                    data: Some(hash.0.to_vec()),
                    left: None,
                    right: None,
                }
            })
            .collect();

        // Build tree bottom-up
        while nodes.len() > 1 {
            let mut next_level = Vec::new();

            // Pair up nodes
            let mut i = 0;
            while i < nodes.len() {
                if i + 1 < nodes.len() {
                    // Create branch from pair
                    let left = nodes[i].clone();
                    let right = nodes[i + 1].clone();
                    next_level.push(MerkleNode::branch(left, right));
                    i += 2;
                } else {
                    // Odd node - promote to next level
                    next_level.push(nodes[i].clone());
                    i += 1;
                }
            }

            nodes = next_level;
        }

        Ok(Self {
            root: nodes.into_iter().next(),
        })
    }

    /// Get the root hash
    pub fn root_hash(&self) -> Option<MerkleHash> {
        self.root.as_ref().map(|r| r.hash.clone())
    }

    /// Generate a proof of inclusion for a leaf
    pub fn generate_proof(&self, leaf_data: &[u8]) -> Result<MerkleProof> {
        let leaf_hash = SHA256Hash::compute(leaf_data);

        let root = self
            .root
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Empty Merkle tree"))?;

        let mut path = Vec::new();
        if self.find_path(root, &leaf_hash, &mut path) {
            Ok(MerkleProof {
                leaf_hash,
                root_hash: root.hash.clone(),
                path,
            })
        } else {
            Err(anyhow::anyhow!("Leaf not found in tree"))
        }
    }

    /// Recursively find path to a leaf
    fn find_path(&self, node: &MerkleNode, target: &MerkleHash, path: &mut Vec<ProofStep>) -> bool {
        // Check if this is the target leaf
        if node.data.is_some() && node.hash == *target {
            return true;
        }

        // Check left subtree
        if let Some(ref left) = node.left {
            if self.find_path(left, target, path) {
                // Add right sibling to path if it exists
                if let Some(ref right) = node.right {
                    path.push(ProofStep {
                        hash: right.hash.clone(),
                        position: Position::Right,
                    });
                }
                return true;
            }
        }

        // Check right subtree
        if let Some(ref right) = node.right {
            if self.find_path(right, target, path) {
                // Add left sibling to path
                if let Some(ref left) = node.left {
                    path.push(ProofStep {
                        hash: left.hash.clone(),
                        position: Position::Left,
                    });
                }
                return true;
            }
        }

        false
    }

    /// Verify a Merkle proof
    pub fn verify_proof(proof: &MerkleProof, _data: &[u8]) -> bool {
        // Note: data parameter kept for future use when we might verify against actual data
        let mut current_hash = proof.leaf_hash.clone();

        for step in &proof.path {
            let mut hasher = Sha256::new();

            match step.position {
                Position::Left => {
                    hasher.update(step.hash.as_bytes());
                    hasher.update(current_hash.as_bytes());
                }
                Position::Right => {
                    hasher.update(current_hash.as_bytes());
                    hasher.update(step.hash.as_bytes());
                }
            }

            let result = hasher.finalize();
            let mut hash_bytes = [0u8; 32];
            hash_bytes.copy_from_slice(&result);
            current_hash = SHA256Hash(hash_bytes);
        }

        current_hash == proof.root_hash
    }

    /// Create a Merkle DAG for taxonomy tree
    pub fn build_taxonomy_dag(taxonomy: TaxonomyTree) -> Result<Self> {
        let root = Self::build_taxonomy_node(&taxonomy.root)?;
        Ok(Self { root: Some(root) })
    }

    fn build_taxonomy_node(node: &TaxonomyNode) -> Result<MerkleNode> {
        let mut hasher = Sha256::new();

        // Hash node data
        hasher.update(node.taxon_id.to_string().as_bytes());
        hasher.update(node.name.as_bytes());
        hasher.update(node.rank.as_bytes());

        // Include children hashes
        let child_nodes: Result<Vec<_>> = node
            .children
            .iter()
            .map(Self::build_taxonomy_node)
            .collect();

        let children = child_nodes?;
        for child in &children {
            hasher.update(child.hash.as_bytes());
        }

        let result = hasher.finalize();
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(&result);

        // Build Merkle node
        if children.is_empty() {
            Ok(MerkleNode {
                hash: SHA256Hash(hash_bytes),
                left: None,
                right: None,
                data: Some(node.to_bytes()?),
            })
        } else if children.len() == 1 {
            Ok(MerkleNode {
                hash: SHA256Hash(hash_bytes),
                left: Some(Box::new(children.into_iter().next().unwrap())),
                right: None,
                data: Some(node.to_bytes()?),
            })
        } else {
            // For multiple children, create a balanced tree
            let mid = children.len() / 2;
            let left_children = &children[..mid];
            let right_children = &children[mid..];

            let left_subtree = Self::merge_nodes(left_children.to_vec());
            let right_subtree = Self::merge_nodes(right_children.to_vec());

            Ok(MerkleNode {
                hash: SHA256Hash(hash_bytes),
                left: Some(Box::new(left_subtree)),
                right: Some(Box::new(right_subtree)),
                data: Some(node.to_bytes()?),
            })
        }
    }

    fn merge_nodes(mut nodes: Vec<MerkleNode>) -> MerkleNode {
        while nodes.len() > 1 {
            let mut next_level = Vec::new();
            let mut i = 0;

            while i < nodes.len() {
                if i + 1 < nodes.len() {
                    let left = nodes[i].clone();
                    let right = nodes[i + 1].clone();
                    next_level.push(MerkleNode::branch(left, right));
                    i += 2;
                } else {
                    next_level.push(nodes[i].clone());
                    i += 1;
                }
            }

            nodes = next_level;
        }

        nodes.into_iter().next().unwrap()
    }
}

impl ProofProvider for MerkleDAG {
    fn generate_proof(&self, target: &SHA256Hash) -> Result<MerkleProof> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Empty Merkle tree"))?;

        let mut path = Vec::new();
        if self.find_path(root, target, &mut path) {
            Ok(MerkleProof {
                leaf_hash: target.clone(),
                root_hash: root.hash.clone(),
                path,
            })
        } else {
            Err(anyhow::anyhow!("Target not found in tree"))
        }
    }

    fn verify_proof(&self, proof: &MerkleProof, data: &[u8]) -> bool {
        MerkleDAG::verify_proof(proof, data)
    }
}

/// Implement MerkleVerifiable for ChunkMetadata
impl MerkleVerifiable for ChunkMetadata {
    fn compute_hash(&self) -> SHA256Hash {
        // The chunk already has its hash computed
        self.hash.clone()
    }
}

impl MerkleDAG {
    /// Sign a temporal proof using SHA256 (placeholder for real signature)
    fn sign_temporal_proof(temporal_link: &CrossTimeHash) -> Vec<u8> {
        // In production, this would use actual cryptographic signing (e.g., Ed25519)
        // For now, we create a deterministic signature using SHA256
        let mut hasher = Sha256::new();
        hasher.update(b"TALARIA_SEQUOIA_SIGNATURE");
        hasher.update(temporal_link.combined_hash.as_bytes());
        hasher.update(temporal_link.sequence_time.timestamp().to_le_bytes());
        hasher.update(temporal_link.taxonomy_time.timestamp().to_le_bytes());
        hasher.finalize().to_vec()
    }

    /// Generate a cross-time proof linking sequences to taxonomy
    pub fn generate_temporal_proof(
        &self,
        sequence_dag: &MerkleDAG,
        sequence_id: &str,
        taxon_id: TaxonId,
    ) -> Result<TemporalProof> {
        // Generate sequence proof
        let sequence_data = sequence_id.as_bytes();
        let sequence_proof = sequence_dag.generate_proof(sequence_data)?;

        // Generate taxonomy proof
        let taxon_data = taxon_id.to_string();
        let taxonomy_proof = self.generate_proof(taxon_data.as_bytes())?;

        // Create cross-time link
        let mut hasher = Sha256::new();
        hasher.update(sequence_proof.root_hash.as_bytes());
        hasher.update(taxonomy_proof.root_hash.as_bytes());
        let result = hasher.finalize();
        let mut combined_hash = [0u8; 32];
        combined_hash.copy_from_slice(&result);

        let temporal_link = CrossTimeHash {
            sequence_time: chrono::Utc::now(),
            taxonomy_time: chrono::Utc::now(),
            combined_hash: SHA256Hash(combined_hash),
        };

        let signature = Self::sign_temporal_proof(&temporal_link);

        Ok(TemporalProof {
            sequence_proof,
            taxonomy_proof,
            temporal_link,
            timestamp: chrono::Utc::now(),
            attestation: CryptographicSeal {
                timestamp: chrono::Utc::now(),
                signature,
                authority: "talaria-sequoia".to_string(),
            },
        })
    }
}

// Taxonomy tree structures for DAG building
#[derive(Debug, Clone)]
pub struct TaxonomyTree {
    pub root: TaxonomyNode,
}

#[derive(Debug, Clone)]
pub struct TaxonomyNode {
    pub taxon_id: TaxonId,
    pub name: String,
    pub rank: String,
    pub children: Vec<TaxonomyNode>,
}

impl TaxonomyNode {
    fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }
}

// Make TaxonomyNode serializable
impl serde::Serialize for TaxonomyNode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("TaxonomyNode", 4)?;
        state.serialize_field("taxon_id", &self.taxon_id)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("rank", &self.rank)?;
        state.serialize_field("children", &self.children)?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test implementation of MerkleVerifiable for testing
    struct TestChunk {
        data: Vec<u8>,
    }

    impl MerkleVerifiable for TestChunk {
        fn compute_hash(&self) -> SHA256Hash {
            SHA256Hash::compute(&self.data)
        }
    }

    #[test]
    fn test_merkle_tree_construction() {
        let chunks = vec![
            TestChunk {
                data: b"chunk1".to_vec(),
            },
            TestChunk {
                data: b"chunk2".to_vec(),
            },
            TestChunk {
                data: b"chunk3".to_vec(),
            },
            TestChunk {
                data: b"chunk4".to_vec(),
            },
        ];

        let dag = MerkleDAG::build_from_items(chunks).unwrap();
        assert!(dag.root_hash().is_some());
    }

    #[test]
    fn test_merkle_proof_generation_and_verification() {
        let chunks = vec![b"chunk1".to_vec(), b"chunk2".to_vec(), b"chunk3".to_vec()];

        // Create test chunks as MerkleVerifiable items
        let test_chunks: Vec<TestChunk> =
            chunks.into_iter().map(|data| TestChunk { data }).collect();

        let dag = MerkleDAG::build_from_items(test_chunks).unwrap();
        let proof = dag.generate_proof(&b"chunk1".to_vec()).unwrap();

        assert!(MerkleDAG::verify_proof(&proof, b"chunk1"));
    }

    #[test]
    fn test_invalid_proof_fails() {
        let chunks = vec![b"chunk1".to_vec(), b"chunk2".to_vec()];

        // Create test chunks as MerkleVerifiable items
        let test_chunks: Vec<TestChunk> =
            chunks.into_iter().map(|data| TestChunk { data }).collect();

        let dag = MerkleDAG::build_from_items(test_chunks).unwrap();
        let mut proof = dag.generate_proof(&b"chunk1".to_vec()).unwrap();

        // Tamper with proof
        proof.leaf_hash = SHA256Hash::compute(b"tampered");

        assert!(!MerkleDAG::verify_proof(&proof, b"chunk1"));
    }
}
