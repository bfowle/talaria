use crate::bio::sequence::Sequence;
use crate::bio::taxonomy::TaxonomyManager;
use crate::casg::chunker::traits::{Chunker, ChunkingStats, TaxonomyAwareChunker};
use crate::casg::types::{ChunkMetadata, SHA256Hash, TaxonId};
use anyhow::Result;
use rand::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Advanced chunker with multi-objective optimization
pub struct AdvancedChunker {
    min_chunk_size: usize,
    max_chunk_size: usize,
    target_chunk_size: usize,
    optimization_weights: OptimizationWeights,
    taxonomy_manager: Arc<TaxonomyManager>,
    taxonomy_threshold: usize,
    stats: ChunkingStats,
}

/// Weights for different optimization objectives
#[derive(Debug, Clone)]
pub struct OptimizationWeights {
    pub size_uniformity: f64,
    pub taxonomic_coherence: f64,
    pub compression_ratio: f64,
    pub boundary_stability: f64,
}

impl Default for OptimizationWeights {
    fn default() -> Self {
        Self {
            size_uniformity: 0.3,
            taxonomic_coherence: 0.4,
            compression_ratio: 0.2,
            boundary_stability: 0.1,
        }
    }
}

impl AdvancedChunker {
    pub fn new(taxonomy_manager: Arc<TaxonomyManager>) -> Self {
        Self {
            min_chunk_size: 100_000,   // 100KB
            max_chunk_size: 10_000_000, // 10MB
            target_chunk_size: 1_000_000, // 1MB
            optimization_weights: OptimizationWeights::default(),
            taxonomy_manager,
            taxonomy_threshold: 100,
            stats: ChunkingStats {
                total_chunks: 0,
                total_sequences: 0,
                avg_chunk_size: 0,
                compression_ratio: 1.0,
            },
        }
    }

    pub fn with_weights(mut self, weights: OptimizationWeights) -> Self {
        self.optimization_weights = weights;
        self
    }

    /// Multi-objective cost function
    fn calculate_cost(&self, chunks: &[ProposedChunk]) -> f64 {
        let weights = &self.optimization_weights;

        // Size uniformity: minimize variance in chunk sizes
        let sizes: Vec<usize> = chunks.iter().map(|c| c.size).collect();
        let mean_size = sizes.iter().sum::<usize>() as f64 / sizes.len().max(1) as f64;
        let variance = sizes.iter()
            .map(|&s| ((s as f64 - mean_size) / mean_size).powi(2))
            .sum::<f64>() / sizes.len().max(1) as f64;
        let size_cost = variance;

        // Taxonomic coherence: maximize sequences from same taxon in same chunk
        let coherence = chunks.iter()
            .map(|c| self.calculate_taxonomic_coherence(c))
            .sum::<f64>() / chunks.len().max(1) as f64;
        let taxonomy_cost = 1.0 - coherence;

        // Compression ratio: prefer chunks that compress well
        let compression = chunks.iter()
            .map(|c| self.estimate_compression_ratio(c))
            .sum::<f64>() / chunks.len().max(1) as f64;
        let compression_cost = 1.0 - compression;

        // Boundary stability: prefer content-defined boundaries
        let stability = chunks.iter()
            .map(|c| self.calculate_boundary_score(c))
            .sum::<f64>() / chunks.len().max(1) as f64;
        let boundary_cost = 1.0 - stability;

        // Weighted sum
        weights.size_uniformity * size_cost +
        weights.taxonomic_coherence * taxonomy_cost +
        weights.compression_ratio * compression_cost +
        weights.boundary_stability * boundary_cost
    }

    fn calculate_taxonomic_coherence(&self, chunk: &ProposedChunk) -> f64 {
        if chunk.taxon_ids.is_empty() {
            return 0.0;
        }

        // Count occurrences of each taxon
        let mut taxon_counts = HashMap::new();
        for taxon in &chunk.taxon_ids {
            *taxon_counts.entry(*taxon).or_insert(0) += 1;
        }

        // Find dominant taxon
        let max_count = taxon_counts.values().max().copied().unwrap_or(0);
        let total_count = chunk.taxon_ids.len();

        max_count as f64 / total_count as f64
    }

    fn estimate_compression_ratio(&self, chunk: &ProposedChunk) -> f64 {
        // Estimate based on sequence diversity
        let diversity = self.calculate_diversity(chunk);
        // Higher diversity = lower compression
        1.0 / (1.0 + diversity)
    }

    fn calculate_diversity(&self, chunk: &ProposedChunk) -> f64 {
        if chunk.sequences.is_empty() {
            return 0.0;
        }

        // Use Shannon entropy on k-mers
        let mut kmer_counts = HashMap::new();
        let k = 4; // 4-mer

        for seq in &chunk.sequences {
            if seq.len() >= k {
                for i in 0..seq.len() - k + 1 {
                    let kmer = &seq[i..i + k];
                    *kmer_counts.entry(kmer.to_vec()).or_insert(0) += 1;
                }
            }
        }

        let total_kmers: usize = kmer_counts.values().sum();
        if total_kmers == 0 {
            return 0.0;
        }

        // Calculate Shannon entropy
        let entropy: f64 = kmer_counts.values()
            .map(|&count| {
                let p = count as f64 / total_kmers as f64;
                -p * p.ln()
            })
            .sum();

        entropy / (total_kmers as f64).ln()
    }

    fn calculate_boundary_score(&self, chunk: &ProposedChunk) -> f64 {
        // Use rolling hash to find natural boundaries
        if chunk.sequences.is_empty() {
            return 0.0;
        }

        let boundary_hash = self.rolling_hash(&chunk.sequences[0], 64);
        let mask = 0xFFFF;

        // Score based on how "round" the boundary hash is
        let zeros = (boundary_hash & mask).trailing_zeros();
        (zeros as f64) / 16.0
    }

    fn rolling_hash(&self, data: &[u8], window: usize) -> u64 {
        if data.len() < window {
            return 0;
        }

        let mut hash = 0u64;
        let prime = 31u64;

        for byte in &data[0..window] {
            hash = hash.wrapping_mul(prime).wrapping_add(*byte as u64);
        }

        hash
    }

    /// Optimize chunk boundaries using simulated annealing
    fn optimize_chunks(&self, initial_chunks: Vec<ProposedChunk>) -> Vec<ProposedChunk> {
        let mut current = initial_chunks;
        let mut current_cost = self.calculate_cost(&current);
        let mut best = current.clone();
        let mut best_cost = current_cost;

        let mut temperature = 1.0;
        let cooling_rate = 0.95;
        let min_temperature = 0.01;
        let mut rng = thread_rng();

        while temperature > min_temperature {
            // Generate neighbor solution
            let mut neighbor = current.clone();
            self.mutate_solution(&mut neighbor, &mut rng);

            let neighbor_cost = self.calculate_cost(&neighbor);
            let delta = neighbor_cost - current_cost;

            // Accept or reject the neighbor
            if delta < 0.0 || rng.gen::<f64>() < (-delta / temperature).exp() {
                current = neighbor;
                current_cost = neighbor_cost;

                if current_cost < best_cost {
                    best = current.clone();
                    best_cost = current_cost;
                }
            }

            temperature *= cooling_rate;
        }

        best
    }

    fn mutate_solution(&self, chunks: &mut Vec<ProposedChunk>, rng: &mut ThreadRng) {
        if chunks.len() < 2 {
            return;
        }

        match rng.gen_range(0..3) {
            0 => self.move_boundary(chunks, rng),
            1 => self.merge_chunks(chunks, rng),
            2 => self.split_chunk(chunks, rng),
            _ => {}
        }
    }

    fn move_boundary(&self, chunks: &mut Vec<ProposedChunk>, rng: &mut ThreadRng) {
        let idx = rng.gen_range(0..chunks.len() - 1);
        let move_count = rng.gen_range(1..5.min(chunks[idx].sequences.len()));

        // Move sequences from one chunk to the next
        for _ in 0..move_count {
            if let Some(seq) = chunks[idx].sequences.pop() {
                chunks[idx + 1].sequences.insert(0, seq);
            }
        }

        // Recalculate sizes
        chunks[idx].size = chunks[idx].sequences.iter().map(|s| s.len()).sum();
        chunks[idx + 1].size = chunks[idx + 1].sequences.iter().map(|s| s.len()).sum();
    }

    fn merge_chunks(&self, chunks: &mut Vec<ProposedChunk>, rng: &mut ThreadRng) {
        if chunks.len() < 2 {
            return;
        }

        let idx = rng.gen_range(0..chunks.len() - 1);
        let mut merged = chunks[idx].clone();
        merged.sequences.extend(chunks[idx + 1].sequences.clone());
        merged.taxon_ids.extend(chunks[idx + 1].taxon_ids.clone());
        merged.size = merged.sequences.iter().map(|s| s.len()).sum();

        chunks[idx] = merged;
        chunks.remove(idx + 1);
    }

    fn split_chunk(&self, chunks: &mut Vec<ProposedChunk>, rng: &mut ThreadRng) {
        let idx = rng.gen_range(0..chunks.len());
        if chunks[idx].sequences.len() < 2 {
            return;
        }

        let split_point = rng.gen_range(1..chunks[idx].sequences.len());
        let mut new_chunk = ProposedChunk {
            sequences: chunks[idx].sequences.split_off(split_point),
            taxon_ids: Vec::new(),
            size: 0,
        };

        // Recalculate taxon IDs and sizes
        new_chunk.size = new_chunk.sequences.iter().map(|s| s.len()).sum();
        chunks[idx].size = chunks[idx].sequences.iter().map(|s| s.len()).sum();

        chunks.insert(idx + 1, new_chunk);
    }
}

impl Chunker for AdvancedChunker {
    fn chunk_sequences(&mut self, sequences: &[Sequence]) -> Result<Vec<ChunkMetadata>> {
        // Create initial chunks
        let mut proposed_chunks = Vec::new();
        let mut current_chunk = ProposedChunk::default();

        for seq in sequences {
            let seq_size = seq.sequence.len();

            if current_chunk.size + seq_size > self.max_chunk_size && !current_chunk.sequences.is_empty() {
                proposed_chunks.push(current_chunk);
                current_chunk = ProposedChunk::default();
            }

            current_chunk.sequences.push(seq.sequence.clone());
            if let Ok(taxon_id) = seq.get_taxid() {
                current_chunk.taxon_ids.push(taxon_id);
            }
            current_chunk.size += seq_size;
        }

        if !current_chunk.sequences.is_empty() {
            proposed_chunks.push(current_chunk);
        }

        // Optimize chunks
        let optimized = self.optimize_chunks(proposed_chunks);

        // Convert to ChunkMetadata
        let mut chunks = Vec::new();
        for proposed in optimized {
            let chunk_data = proposed.sequences.concat();
            let hash = SHA256Hash::compute(&chunk_data);

            chunks.push(ChunkMetadata {
                hash,
                size: proposed.size,
                sequence_count: proposed.sequences.len(),
                compressed_size: (proposed.size as f64 * 0.7) as usize, // Estimate
            });
        }

        // Update stats
        self.stats.total_chunks = chunks.len();
        self.stats.total_sequences = sequences.len();
        self.stats.avg_chunk_size = chunks.iter().map(|c| c.size).sum::<usize>() / chunks.len().max(1);
        self.stats.compression_ratio = 0.7; // Estimated

        Ok(chunks)
    }

    fn get_stats(&self) -> ChunkingStats {
        self.stats.clone()
    }

    fn set_chunk_size(&mut self, min_size: usize, max_size: usize) {
        self.min_chunk_size = min_size;
        self.max_chunk_size = max_size;
        self.target_chunk_size = (min_size + max_size) / 2;
    }
}

impl TaxonomyAwareChunker for AdvancedChunker {
    fn chunk_by_taxonomy(
        &mut self,
        sequences: &[Sequence],
        taxonomy_map: &[(String, TaxonId)],
    ) -> Result<Vec<ChunkMetadata>> {
        // Group sequences by taxonomy
        let mut taxon_groups: HashMap<TaxonId, Vec<&Sequence>> = HashMap::new();
        let tax_map: HashMap<String, TaxonId> = taxonomy_map.iter().cloned().collect();

        for seq in sequences {
            let taxon_id = tax_map.get(&seq.id).copied()
                .or_else(|| seq.get_taxid().ok())
                .unwrap_or(0);

            taxon_groups.entry(taxon_id).or_default().push(seq);
        }

        // Chunk each taxonomic group
        let mut all_chunks = Vec::new();
        for (_taxon_id, group_sequences) in taxon_groups {
            if group_sequences.len() >= self.taxonomy_threshold {
                // Large group: chunk normally
                let owned_sequences: Vec<Sequence> = group_sequences.into_iter().cloned().collect();
                all_chunks.extend(self.chunk_sequences(&owned_sequences)?);
            } else {
                // Small group: keep together
                let chunk_data: Vec<u8> = group_sequences.iter()
                    .flat_map(|s| &s.sequence)
                    .copied()
                    .collect();

                let hash = SHA256Hash::compute(&chunk_data);
                all_chunks.push(ChunkMetadata {
                    hash,
                    size: chunk_data.len(),
                    sequence_count: group_sequences.len(),
                    compressed_size: (chunk_data.len() as f64 * 0.7) as usize,
                });
            }
        }

        // Update stats
        self.stats.total_chunks = all_chunks.len();
        self.stats.total_sequences = sequences.len();
        self.stats.avg_chunk_size = all_chunks.iter().map(|c| c.size).sum::<usize>() / all_chunks.len().max(1);

        Ok(all_chunks)
    }

    fn set_taxonomy_threshold(&mut self, threshold: usize) {
        self.taxonomy_threshold = threshold;
    }
}

/// Proposed chunk during optimization
#[derive(Clone, Default)]
struct ProposedChunk {
    sequences: Vec<Vec<u8>>,
    taxon_ids: Vec<TaxonId>,
    size: usize,
}