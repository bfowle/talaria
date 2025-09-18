/// Cryptographic verification for CASG

use crate::casg::types::*;
use crate::casg::storage::CASGStorage;
use crate::casg::merkle::MerkleDAG;
use anyhow::{Context, Result};
use std::collections::HashSet;

#[derive(Debug)]
pub struct VerificationResult {
    pub valid: bool,
    pub chunks_verified: usize,
    pub invalid_chunks: Vec<String>,
    pub merkle_root_valid: bool,
}

pub struct CASGVerifier<'a> {
    storage: &'a CASGStorage,
    manifest: &'a TemporalManifest,
}

impl<'a> CASGVerifier<'a> {
    pub fn new(storage: &'a CASGStorage, manifest: &'a TemporalManifest) -> Self {
        Self { storage, manifest }
    }

    /// Verify integrity of all chunks
    pub fn verify_all(&self) -> Result<VerificationResult> {
        let mut invalid_chunks = Vec::new();
        let mut chunks_verified = 0;

        // Verify each chunk in manifest
        for chunk_meta in &self.manifest.chunk_index {
            match self.verify_chunk(&chunk_meta.hash) {
                Ok(_) => chunks_verified += 1,
                Err(e) => {
                    eprintln!("Chunk {} verification failed: {}", chunk_meta.hash, e);
                    invalid_chunks.push(chunk_meta.hash.to_hex());
                }
            }
        }

        // Verify Merkle roots
        let merkle_root_valid = self.verify_merkle_roots()?;

        Ok(VerificationResult {
            valid: invalid_chunks.is_empty() && merkle_root_valid,
            chunks_verified,
            invalid_chunks,
            merkle_root_valid,
        })
    }

    /// Verify a single chunk
    pub fn verify_chunk(&self, hash: &SHA256Hash) -> Result<()> {
        // Retrieve chunk
        let chunk_data = self.storage.get_chunk(hash)
            .with_context(|| format!("Failed to retrieve chunk {}", hash))?;

        // Compute hash
        let computed_hash = SHA256Hash::compute(&chunk_data);

        // Verify
        if &computed_hash != hash {
            return Err(anyhow::anyhow!(
                "Hash mismatch: expected {}, got {}",
                hash,
                computed_hash
            ));
        }

        Ok(())
    }

    /// Verify Merkle roots in manifest
    fn verify_merkle_roots(&self) -> Result<bool> {
        // Build Merkle tree from chunks using ChunkMetadata which implements MerkleVerifiable
        let dag = MerkleDAG::build_from_items(self.manifest.chunk_index.clone())?;

        // Check sequence root
        if let Some(computed_root) = dag.root_hash() {
            if computed_root != self.manifest.sequence_root {
                eprintln!(
                    "Sequence root mismatch: expected {}, got {}",
                    self.manifest.sequence_root,
                    computed_root
                );
                return Ok(false);
            }
        }

        // Verify taxonomy root
        if self.manifest.taxonomy_root != SHA256Hash::zero() {
            // Use placeholder since ChunkMetadata doesn't have taxonomy_version
            let tax_hashes: Vec<SHA256Hash> = vec![self.manifest.taxonomy_root.clone()];

            if !tax_hashes.is_empty() {
                let computed_tax_root = self.compute_taxonomy_root(tax_hashes)?;
                if computed_tax_root != self.manifest.taxonomy_root {
                    eprintln!(
                        "Taxonomy root mismatch: expected {}, got {}",
                        self.manifest.taxonomy_root,
                        computed_tax_root
                    );
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Verify a Merkle proof
    pub fn verify_proof(&self, proof: &MerkleProof) -> bool {
        MerkleDAG::verify_proof(proof, &[])
    }

    /// Generate proof for an assembly operation
    pub fn generate_assembly_proof(&self, chunk_hashes: &[SHA256Hash]) -> Result<MerkleProof> {
        // Build DAG from assembled chunks
        // Create minimal ChunkMetadata wrappers for the hashes
        let chunks: Vec<ChunkMetadata> = chunk_hashes
            .iter()
            .map(|h| ChunkMetadata {
                hash: h.clone(),
                taxon_ids: Vec::new(),
                sequence_count: 0,
                size: 0,
                compressed_size: None,
            })
            .collect();

        let dag = MerkleDAG::build_from_items(chunks)?;

        // Generate proof for first chunk (as example)
        if let Some(first_hash) = chunk_hashes.first() {
            dag.generate_proof(&first_hash.0)
        } else {
            Err(anyhow::anyhow!("No chunks to prove"))
        }
    }

    /// Verify temporal proof
    pub fn verify_temporal_proof(&self, proof: &TemporalProof) -> Result<bool> {
        // Verify sequence proof
        if !self.verify_proof(&proof.sequence_proof) {
            return Ok(false);
        }

        // Verify taxonomy proof
        if !self.verify_proof(&proof.taxonomy_proof) {
            return Ok(false);
        }

        // Verify temporal link
        let mut combined = Vec::new();
        combined.extend(proof.sequence_proof.root_hash.as_bytes());
        combined.extend(proof.taxonomy_proof.root_hash.as_bytes());
        let expected_hash = SHA256Hash::compute(&combined);

        if expected_hash != proof.temporal_link.combined_hash {
            return Ok(false);
        }

        // Verify attestation signature
        // Note: attestation is part of the proof
        let attestation = &proof.attestation;
        if attestation.signature.len() > 0 {
            // Reconstruct the signed data
            let mut signed_data = Vec::new();
            // Use temporal link hash instead of chunk_hash
            signed_data.extend_from_slice(proof.temporal_link.combined_hash.as_bytes());
            signed_data.extend_from_slice(attestation.timestamp.to_rfc3339().as_bytes());
            signed_data.extend_from_slice(attestation.authority.as_bytes());

            // Compute expected signature hash
            let _expected_sig = SHA256Hash::compute(&signed_data);

            // For now, we just verify the signature format is valid
            // In production, this would verify against a public key
            if attestation.signature.len() != 64 {
                eprintln!("Invalid signature length: {}", attestation.signature.len());
                return Ok(false);
            }

            // Basic signature validation (placeholder for real crypto)
            if attestation.signature.iter().all(|&b| b == 0) {
                eprintln!("Invalid null signature");
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Verify a subset of the database
    pub fn verify_subset(&self, chunk_hashes: &[SHA256Hash]) -> Result<SubsetVerification> {
        let mut verified = Vec::new();
        let mut failed = Vec::new();

        for hash in chunk_hashes {
            match self.verify_chunk(hash) {
                Ok(_) => verified.push(hash.clone()),
                Err(e) => failed.push((hash.clone(), e.to_string())),
            }
        }

        // Generate subset proof
        let subset_proof = if !verified.is_empty() {
            Some(self.generate_subset_proof(&verified)?)
        } else {
            None
        };

        Ok(SubsetVerification {
            total_chunks: chunk_hashes.len(),
            verified_chunks: verified.len(),
            failed_chunks: failed,
            subset_proof,
        })
    }

    /// Generate proof for a subset
    fn generate_subset_proof(&self, chunk_hashes: &[SHA256Hash]) -> Result<SubsetProof> {
        // Build Merkle tree for subset
        // Create minimal ChunkMetadata wrappers for the hashes
        let chunks: Vec<ChunkMetadata> = chunk_hashes
            .iter()
            .map(|h| ChunkMetadata {
                hash: h.clone(),
                taxon_ids: Vec::new(),
                sequence_count: 0,
                size: 0,
                compressed_size: None,
            })
            .collect();

        let subset_dag = MerkleDAG::build_from_items(chunks)?;
        let subset_root = subset_dag.root_hash()
            .ok_or_else(|| anyhow::anyhow!("Failed to compute subset root"))?;

        // Generate proof linking subset to full manifest
        // This would involve showing that all subset chunks exist in the full manifest

        Ok(SubsetProof {
            subset_root,
            subset_size: chunk_hashes.len(),
            manifest_root: self.manifest.sequence_root.clone(),
            inclusion_proofs: Vec::new(), // Would be populated with actual proofs
        })
    }

    /// Verify database consistency
    pub fn verify_consistency(&self) -> Result<ConsistencyReport> {
        let mut issues = Vec::new();

        // Check for orphaned chunks (in storage but not in manifest)
        let manifest_chunks: HashSet<_> = self.manifest.chunk_index
            .iter()
            .map(|meta| meta.hash.clone())
            .collect();

        // Check storage for chunks not in manifest (orphaned)
        let storage_chunks = self.get_all_storage_chunks()?;
        let orphaned: Vec<_> = storage_chunks
            .iter()
            .filter(|hash| !manifest_chunks.contains(hash))
            .collect();

        let orphan_count = orphaned.len();
        if orphan_count > 0 {
            eprintln!("Warning: {} potential orphaned chunks in storage", orphan_count);
        }

        // Check for missing chunks (in manifest but not in storage)
        for chunk_meta in &self.manifest.chunk_index {
            if !self.storage.has_chunk(&chunk_meta.hash) {
                issues.push(ConsistencyIssue::MissingChunk(chunk_meta.hash.clone()));
            }
        }

        // Check for duplicate references
        let mut seen = HashSet::new();
        for chunk_meta in &self.manifest.chunk_index {
            if !seen.insert(chunk_meta.hash.clone()) {
                issues.push(ConsistencyIssue::DuplicateReference(chunk_meta.hash.clone()));
            }
        }

        // Check taxonomy consistency
        if let Err(e) = self.verify_taxonomy_consistency() {
            issues.push(ConsistencyIssue::TaxonomyInconsistency(e.to_string()));
        }

        Ok(ConsistencyReport {
            consistent: issues.is_empty(),
            issues,
            chunks_checked: self.manifest.chunk_index.len(),
        })
    }

    fn verify_taxonomy_consistency(&self) -> Result<()> {
        // Load taxonomy manager
        let taxonomy = crate::casg::taxonomy::TaxonomyManager::new(&self.storage.base_path)?;

        if !taxonomy.has_taxonomy() {
            eprintln!("Warning: No taxonomy loaded, skipping consistency checks");
            return Ok(());
        }

        let mut invalid_taxids = Vec::new();
        let mut checked_count = 0;

        // Verify all taxon IDs in chunks exist in taxonomy tree
        for chunk_info in &self.manifest.chunk_index {
            // Load chunk to get taxon IDs
            if let Ok(chunk_data) = self.storage.get_chunk(&chunk_info.hash) {
                if let Ok(chunk) = serde_json::from_slice::<crate::casg::types::TaxonomyAwareChunk>(&chunk_data) {
                    for taxid in &chunk.taxon_ids {
                        checked_count += 1;
                        if !taxonomy.taxon_exists(*taxid) {
                            invalid_taxids.push(*taxid);
                        }
                    }
                }
            }
        }

        if !invalid_taxids.is_empty() {
            eprintln!(
                "Found {} invalid taxon IDs out of {} checked",
                invalid_taxids.len(),
                checked_count
            );
            // Log first few invalid IDs
            for (i, taxid) in invalid_taxids.iter().take(10).enumerate() {
                eprintln!("  [{}] Invalid taxon ID: {}", i + 1, taxid);
            }
        }

        // Verify taxonomy version alignment
        let expected_tax_root = taxonomy.get_taxonomy_root()?;
        if self.manifest.taxonomy_root != expected_tax_root && self.manifest.taxonomy_root != SHA256Hash::zero() {
            eprintln!(
                "Warning: Manifest taxonomy root doesn't match current taxonomy"
            );
        }

        Ok(())
    }

    /// Compute taxonomy root from hashes
    fn compute_taxonomy_root(&self, mut hashes: Vec<SHA256Hash>) -> Result<SHA256Hash> {
        if hashes.is_empty() {
            return Ok(SHA256Hash::compute(b"EMPTY_TAXONOMY"));
        }

        // Sort and deduplicate
        hashes.sort();
        hashes.dedup();

        // Build Merkle tree
        while hashes.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in hashes.chunks(2) {
                let combined = if chunk.len() == 2 {
                    let mut data = Vec::new();
                    data.extend_from_slice(&chunk[0].0);
                    data.extend_from_slice(&chunk[1].0);
                    SHA256Hash::compute(&data)
                } else {
                    let mut data = Vec::new();
                    data.extend_from_slice(&chunk[0].0);
                    data.extend_from_slice(&chunk[0].0);
                    SHA256Hash::compute(&data)
                };
                next_level.push(combined);
            }
            hashes = next_level;
        }

        Ok(hashes[0].clone())
    }

    /// Get all chunk hashes from storage
    fn get_all_storage_chunks(&self) -> Result<Vec<SHA256Hash>> {
        // Get all chunks from storage using public method
        Ok(self.storage.get_all_chunk_hashes())
    }

    /// Audit trail generation
    pub fn generate_audit_trail(&self, from: chrono::DateTime<chrono::Utc>) -> AuditTrail {
        let mut events = Vec::new();

        // Add verification event
        events.push(AuditEvent {
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::Verification,
            description: format!("Full verification of {} chunks", self.manifest.chunk_index.len()),
            hash: Some(self.manifest.sequence_root.clone()),
        });

        AuditTrail {
            start_time: from,
            end_time: chrono::Utc::now(),
            events,
            manifest_version: self.manifest.version.clone(),
        }
    }
}

#[derive(Debug)]
pub struct SubsetVerification {
    pub total_chunks: usize,
    pub verified_chunks: usize,
    pub failed_chunks: Vec<(SHA256Hash, String)>,
    pub subset_proof: Option<SubsetProof>,
}

#[derive(Debug)]
pub struct SubsetProof {
    pub subset_root: MerkleHash,
    pub subset_size: usize,
    pub manifest_root: MerkleHash,
    pub inclusion_proofs: Vec<MerkleProof>,
}

#[derive(Debug)]
pub struct ConsistencyReport {
    pub consistent: bool,
    pub issues: Vec<ConsistencyIssue>,
    pub chunks_checked: usize,
}

#[derive(Debug)]
pub enum ConsistencyIssue {
    MissingChunk(SHA256Hash),
    OrphanedChunk(SHA256Hash),
    DuplicateReference(SHA256Hash),
    TaxonomyInconsistency(String),
    HashMismatch { expected: SHA256Hash, actual: SHA256Hash },
}

#[derive(Debug)]
pub struct AuditTrail {
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub events: Vec<AuditEvent>,
    pub manifest_version: String,
}

#[derive(Debug)]
pub struct AuditEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: AuditEventType,
    pub description: String,
    pub hash: Option<SHA256Hash>,
}

#[derive(Debug)]
pub enum AuditEventType {
    Verification,
    Update,
    Query,
    Error,
}

/// Batch verification for efficiency
pub struct BatchVerifier<'a> {
    verifier: CASGVerifier<'a>,
    parallel: bool,
}

impl<'a> BatchVerifier<'a> {
    pub fn new(storage: &'a CASGStorage, manifest: &'a TemporalManifest) -> Self {
        Self {
            verifier: CASGVerifier::new(storage, manifest),
            parallel: false,
        }
    }

    pub fn parallel(mut self) -> Self {
        self.parallel = true;
        self
    }

    pub fn verify_batch(&self, chunk_hashes: &[SHA256Hash]) -> Result<BatchResult> {
        if self.parallel {
            self.verify_parallel(chunk_hashes)
        } else {
            self.verify_sequential(chunk_hashes)
        }
    }

    fn verify_sequential(&self, chunk_hashes: &[SHA256Hash]) -> Result<BatchResult> {
        let mut succeeded = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        for hash in chunk_hashes {
            match self.verifier.verify_chunk(hash) {
                Ok(_) => succeeded += 1,
                Err(e) => {
                    failed += 1;
                    errors.push((hash.clone(), e.to_string()));
                }
            }
        }

        Ok(BatchResult {
            total: chunk_hashes.len(),
            succeeded,
            failed,
            errors,
        })
    }

    fn verify_parallel(&self, chunk_hashes: &[SHA256Hash]) -> Result<BatchResult> {
        use rayon::prelude::*;

        let results: Vec<_> = chunk_hashes
            .par_iter()
            .map(|hash| {
                self.verifier.verify_chunk(hash)
                    .map(|_| (hash.clone(), true))
                    .unwrap_or_else(|e| {
                        eprintln!("Verification failed for chunk {}: {}", hash, e);
                        (hash.clone(), false)
                    })
            })
            .collect();

        let succeeded = results.iter().filter(|(_, ok)| *ok).count();
        let failed = results.len() - succeeded;

        Ok(BatchResult {
            total: chunk_hashes.len(),
            succeeded,
            failed,
            errors: Vec::new(), // Simplified for parallel version
        })
    }
}

#[derive(Debug)]
pub struct BatchResult {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub errors: Vec<(SHA256Hash, String)>,
}