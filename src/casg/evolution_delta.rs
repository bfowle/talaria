use crate::bio::sequence::Sequence;
use crate::bio::taxonomy::TaxonomyManager;
use crate::casg::delta::traits::{DeltaGenerator, DeltaGeneratorConfig};
use crate::casg::types::{ChunkType, DeltaChunk, DeltaOperation, SHA256Hash, TaxonId};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

/// Evolution-aware delta generator that uses phylogenetic distance
pub struct EvolutionAwareDeltaGenerator {
    taxonomy_manager: Arc<TaxonomyManager>,
    distance_calculator: PhylogeneticDistance,
    config: DeltaGeneratorConfig,
}

impl EvolutionAwareDeltaGenerator {
    pub fn new(taxonomy_manager: Arc<TaxonomyManager>) -> Self {
        Self {
            distance_calculator: PhylogeneticDistance::new(taxonomy_manager.clone()),
            taxonomy_manager,
            config: DeltaGeneratorConfig::default(),
        }
    }

    /// Select best reference based on phylogenetic distance
    fn select_best_reference(
        &self,
        sequence: &Sequence,
        references: &[Sequence],
    ) -> Option<usize> {
        let seq_taxon = sequence.get_taxid().ok()?;

        let mut best_idx = None;
        let mut best_score = f32::MAX;

        for (idx, ref_seq) in references.iter().enumerate() {
            if let Ok(ref_taxon) = ref_seq.get_taxid() {
                let distance = self.distance_calculator.calculate(seq_taxon, ref_taxon);

                // Combine phylogenetic distance with sequence similarity
                let similarity = self.calculate_similarity(&sequence.sequence, &ref_seq.sequence);
                let combined_score = distance * 0.4 + (1.0 - similarity) * 0.6;

                if combined_score < best_score {
                    best_score = combined_score;
                    best_idx = Some(idx);
                }
            }
        }

        best_idx
    }

    fn calculate_similarity(&self, seq1: &[u8], seq2: &[u8]) -> f32 {
        if seq1.is_empty() || seq2.is_empty() {
            return 0.0;
        }

        let k = 6; // k-mer size
        let mut common_kmers = 0;
        let mut total_kmers = 0;

        // Build k-mer set for seq1
        let mut kmers1 = HashMap::new();
        for i in 0..seq1.len().saturating_sub(k - 1) {
            let kmer = &seq1[i..i + k];
            *kmers1.entry(kmer).or_insert(0) += 1;
        }

        // Count common k-mers with seq2
        for i in 0..seq2.len().saturating_sub(k - 1) {
            let kmer = &seq2[i..i + k];
            if let Some(count) = kmers1.get_mut(&kmer) {
                if *count > 0 {
                    common_kmers += 1;
                    *count -= 1;
                }
            }
            total_kmers += 1;
        }

        if total_kmers == 0 {
            0.0
        } else {
            common_kmers as f32 / total_kmers as f32
        }
    }

    /// Generate delta operations between two sequences
    fn generate_delta_operations(
        &self,
        target: &[u8],
        reference: &[u8],
    ) -> Vec<DeltaOperation> {
        let mut ops = Vec::new();

        // Simple diff algorithm (Myers' algorithm would be better)
        let mut ref_pos = 0;
        let mut tgt_pos = 0;

        while tgt_pos < target.len() && ref_pos < reference.len() {
            if target[tgt_pos] == reference[ref_pos] {
                // Match - find run length
                let start_ref = ref_pos;
                let start_tgt = tgt_pos;

                while tgt_pos < target.len()
                    && ref_pos < reference.len()
                    && target[tgt_pos] == reference[ref_pos]
                {
                    tgt_pos += 1;
                    ref_pos += 1;
                }

                ops.push(DeltaOperation::Copy {
                    offset: start_ref,
                    length: ref_pos - start_ref,
                });
            } else {
                // Mismatch - collect insertions
                let start_tgt = tgt_pos;

                while tgt_pos < target.len()
                    && (ref_pos >= reference.len() || target[tgt_pos] != reference[ref_pos])
                {
                    tgt_pos += 1;
                }

                ops.push(DeltaOperation::Insert {
                    data: target[start_tgt..tgt_pos].to_vec(),
                });

                // Skip mismatched reference bytes
                if ref_pos < reference.len() {
                    ref_pos += 1;
                }
            }
        }

        // Handle remaining target bytes
        if tgt_pos < target.len() {
            ops.push(DeltaOperation::Insert {
                data: target[tgt_pos..].to_vec(),
            });
        }

        ops
    }
}

impl DeltaGenerator for EvolutionAwareDeltaGenerator {
    fn generate_deltas(
        &mut self,
        sequences: &[Sequence],
        references: &[Sequence],
        reference_hash: SHA256Hash,
    ) -> Result<Vec<DeltaChunk>> {
        let mut delta_chunks = Vec::new();

        for sequence in sequences {
            // Find best reference using evolution-aware selection
            if let Some(ref_idx) = self.select_best_reference(sequence, references) {
                let reference = &references[ref_idx];

                // Generate delta operations
                let operations = self.generate_delta_operations(
                    &sequence.sequence,
                    &reference.sequence,
                );

                // Calculate compression ratio
                let original_size = sequence.sequence.len();
                let delta_size: usize = operations.iter().map(|op| match op {
                    DeltaOperation::Copy { .. } => 16, // Size of copy operation
                    DeltaOperation::Insert { data } => 8 + data.len(),
                }).sum();

                let compression_ratio = delta_size as f32 / original_size as f32;

                // Only use delta if it provides good compression
                if compression_ratio < self.config.compression_threshold as f32 {
                    let chunk_data = bincode::serialize(&operations)?;
                    let content_hash = SHA256Hash::compute(&chunk_data);

                    delta_chunks.push(DeltaChunk {
                        content_hash,
                        reference_hash: reference_hash.clone(),
                        operations,
                        chunk_type: ChunkType::Delta {
                            reference_hash: reference_hash.clone(),
                        },
                        compression_ratio,
                        sequence_ids: vec![sequence.id.clone()],
                        taxon_ids: sequence.get_taxid().ok().map(|t| vec![t]).unwrap_or_default(),
                    });
                } else {
                    // Fall back to full chunk if delta doesn't compress well
                    let content_hash = SHA256Hash::compute(&sequence.sequence);
                    delta_chunks.push(DeltaChunk {
                        content_hash,
                        reference_hash: SHA256Hash::zero(),
                        operations: vec![DeltaOperation::Insert {
                            data: sequence.sequence.clone(),
                        }],
                        chunk_type: ChunkType::Full,
                        compression_ratio: 1.0,
                        sequence_ids: vec![sequence.id.clone()],
                        taxon_ids: sequence.get_taxid().ok().map(|t| vec![t]).unwrap_or_default(),
                    });
                }
            } else {
                // No suitable reference found
                let content_hash = SHA256Hash::compute(&sequence.sequence);
                delta_chunks.push(DeltaChunk {
                    content_hash,
                    reference_hash: SHA256Hash::zero(),
                    operations: vec![DeltaOperation::Insert {
                        data: sequence.sequence.clone(),
                    }],
                    chunk_type: ChunkType::Full,
                    compression_ratio: 1.0,
                    sequence_ids: vec![sequence.id.clone()],
                    taxon_ids: sequence.get_taxid().ok().map(|t| vec![t]).unwrap_or_default(),
                });
            }
        }

        Ok(delta_chunks)
    }

    fn set_config(&mut self, config: DeltaGeneratorConfig) {
        self.config = config;
    }

    fn get_config(&self) -> &DeltaGeneratorConfig {
        &self.config
    }
}

/// Calculator for phylogenetic distance between taxa
pub struct PhylogeneticDistance {
    taxonomy_manager: Arc<TaxonomyManager>,
    distance_cache: HashMap<(TaxonId, TaxonId), f32>,
}

impl PhylogeneticDistance {
    pub fn new(taxonomy_manager: Arc<TaxonomyManager>) -> Self {
        Self {
            taxonomy_manager,
            distance_cache: HashMap::new(),
        }
    }

    /// Calculate phylogenetic distance between two taxa
    pub fn calculate(&self, taxon1: TaxonId, taxon2: TaxonId) -> f32 {
        if taxon1 == taxon2 {
            return 0.0;
        }

        // Check cache
        let key = if taxon1 < taxon2 {
            (taxon1, taxon2)
        } else {
            (taxon2, taxon1)
        };

        if let Some(&distance) = self.distance_cache.get(&key) {
            return distance;
        }

        // Calculate distance based on common ancestor
        let distance = self.calculate_uncached(taxon1, taxon2);

        // Note: In real implementation, we'd need mutable access to cache
        // For now, we just return the calculated distance
        distance
    }

    fn calculate_uncached(&self, taxon1: TaxonId, taxon2: TaxonId) -> f32 {
        // Get lineages for both taxa
        let lineage1 = self.get_lineage(taxon1);
        let lineage2 = self.get_lineage(taxon2);

        if lineage1.is_empty() || lineage2.is_empty() {
            return 1.0; // Maximum distance for unknown taxa
        }

        // Find common ancestor depth
        let mut common_depth = 0;
        for (l1, l2) in lineage1.iter().zip(lineage2.iter()) {
            if l1 == l2 {
                common_depth += 1;
            } else {
                break;
            }
        }

        // Calculate distance based on divergence depth
        let total_depth = lineage1.len().max(lineage2.len()) as f32;
        if total_depth == 0.0 {
            0.0
        } else {
            1.0 - (common_depth as f32 / total_depth)
        }
    }

    fn get_lineage(&self, taxon_id: TaxonId) -> Vec<TaxonId> {
        // Simplified: return a mock lineage
        // In real implementation, would query taxonomy_manager
        match taxon_id {
            1..=100 => vec![1, 10, 50, taxon_id],      // Bacteria
            101..=200 => vec![2, 20, 100, taxon_id],   // Archaea
            201..=1000 => vec![3, 30, 200, taxon_id],  // Eukaryota
            _ => vec![taxon_id],
        }
    }
}

/// Evolutionary distance metrics
#[derive(Debug, Clone)]
pub struct EvolutionaryMetrics {
    pub phylogenetic_distance: f32,
    pub sequence_similarity: f32,
    pub evolutionary_rate: f32,
    pub conservation_score: f32,
}

impl EvolutionaryMetrics {
    pub fn combined_score(&self) -> f32 {
        self.phylogenetic_distance * 0.3
            + (1.0 - self.sequence_similarity) * 0.3
            + self.evolutionary_rate * 0.2
            + (1.0 - self.conservation_score) * 0.2
    }
}

/// Phylogenetic tree representation for evolution tracking
#[derive(Debug, Clone)]
pub struct PhylogeneticTree {
    pub root: TaxonId,
    pub nodes: HashMap<TaxonId, PhylogeneticNode>,
}

#[derive(Debug, Clone)]
pub struct PhylogeneticNode {
    pub taxon_id: TaxonId,
    pub parent: Option<TaxonId>,
    pub children: Vec<TaxonId>,
    pub branch_length: f32,
    pub bootstrap_value: f32,
}

impl PhylogeneticTree {
    pub fn new(root: TaxonId) -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(root, PhylogeneticNode {
            taxon_id: root,
            parent: None,
            children: Vec::new(),
            branch_length: 0.0,
            bootstrap_value: 100.0,
        });

        Self { root, nodes }
    }

    pub fn add_node(&mut self, taxon_id: TaxonId, parent: TaxonId, branch_length: f32) {
        // Add new node
        self.nodes.insert(taxon_id, PhylogeneticNode {
            taxon_id,
            parent: Some(parent),
            children: Vec::new(),
            branch_length,
            bootstrap_value: 0.0,
        });

        // Update parent's children
        if let Some(parent_node) = self.nodes.get_mut(&parent) {
            parent_node.children.push(taxon_id);
        }
    }

    pub fn distance(&self, taxon1: TaxonId, taxon2: TaxonId) -> f32 {
        // Find path to common ancestor
        let path1 = self.path_to_root(taxon1);
        let path2 = self.path_to_root(taxon2);

        let mut distance = 0.0;

        // Find common ancestor
        let mut common_ancestor = None;
        for t1 in &path1 {
            if path2.contains(t1) {
                common_ancestor = Some(*t1);
                break;
            }
        }

        if let Some(ancestor) = common_ancestor {
            // Sum branch lengths from both taxa to common ancestor
            distance += self.distance_to_ancestor(taxon1, ancestor);
            distance += self.distance_to_ancestor(taxon2, ancestor);
        } else {
            // No common ancestor found
            distance = f32::MAX;
        }

        distance
    }

    fn path_to_root(&self, taxon_id: TaxonId) -> Vec<TaxonId> {
        let mut path = vec![taxon_id];
        let mut current = taxon_id;

        while let Some(node) = self.nodes.get(&current) {
            if let Some(parent) = node.parent {
                path.push(parent);
                current = parent;
            } else {
                break;
            }
        }

        path
    }

    fn distance_to_ancestor(&self, taxon_id: TaxonId, ancestor: TaxonId) -> f32 {
        let mut distance = 0.0;
        let mut current = taxon_id;

        while current != ancestor {
            if let Some(node) = self.nodes.get(&current) {
                distance += node.branch_length;
                if let Some(parent) = node.parent {
                    current = parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        distance
    }
}