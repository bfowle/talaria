#![allow(dead_code)]

/// Phylogenetic clustering for efficient taxonomic grouping in sequence reduction
///
/// This module provides intelligent clustering of sequences based on phylogenetic
/// relationships, optimizing for both biological relevance and computational efficiency.
use crate::sequence::Sequence;
use crate::taxonomy::{TaxonomicRank, TaxonomyDB};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tracing::info;

/// Type alias for taxon groups: (taxon_id, sequences)
type TaxonGroup = Vec<(u32, Vec<Sequence>)>;

/// A cluster of taxonomically related sequences
#[derive(Debug, Clone)]
pub struct TaxonomicCluster {
    /// Unique identifier for this cluster
    pub id: String,

    /// Representative taxon ID (e.g., common ancestor)
    pub representative_taxon: u32,

    /// All taxon IDs included in this cluster
    pub taxa: HashSet<u32>,

    /// Sequences in this cluster
    pub sequences: Vec<Sequence>,

    /// Phylogenetic diversity score (0.0 = homogeneous, 1.0 = maximum diversity)
    pub diversity_score: f32,

    /// Estimated memory usage in MB
    pub estimated_memory_mb: usize,

    /// Taxonomic rank at which clustering occurred
    pub cluster_rank: TaxonomicRank,
}

impl TaxonomicCluster {
    /// Get the total size in bytes of all sequences
    pub fn total_size(&self) -> usize {
        self.sequences.iter().map(|s| s.sequence.len()).sum()
    }

    /// Get average sequence length
    pub fn avg_sequence_length(&self) -> usize {
        if self.sequences.is_empty() {
            0
        } else {
            self.total_size() / self.sequences.len()
        }
    }
}

/// Configuration for phylogenetic clustering
#[derive(Debug, Clone)]
pub struct ClusteringConfig {
    /// Minimum number of sequences per cluster
    pub min_cluster_size: usize,

    /// Maximum number of sequences per cluster
    pub max_cluster_size: usize,

    /// Maximum phylogenetic diversity within a cluster (0.0 to 1.0)
    pub max_diversity: f32,

    /// Memory limit per batch in MB
    pub memory_limit_mb: usize,

    /// Taxa that should always be in separate clusters
    pub high_priority_taxa: HashSet<u32>,

    /// Whether to prefer clustering at specific ranks
    pub preferred_ranks: Vec<TaxonomicRank>,

    /// Enable parallel processing
    pub parallel: bool,
}

impl Default for ClusteringConfig {
    fn default() -> Self {
        let mut high_priority = HashSet::new();
        // Common model organisms and important species
        high_priority.insert(562); // E. coli
        high_priority.insert(9606); // Human
        high_priority.insert(10090); // Mouse
        high_priority.insert(559292); // S. cerevisiae (yeast)
        high_priority.insert(7227); // D. melanogaster (fruit fly)
        high_priority.insert(6239); // C. elegans (nematode)

        Self {
            min_cluster_size: 1000,
            max_cluster_size: 50000,
            max_diversity: 0.3,
            memory_limit_mb: 8000,
            high_priority_taxa: high_priority,
            preferred_ranks: vec![
                TaxonomicRank::Species,
                TaxonomicRank::Genus,
                TaxonomicRank::Family,
                TaxonomicRank::Order,
            ],
            parallel: true,
        }
    }
}

impl ClusteringConfig {
    /// Create configuration optimized for SwissProt database
    pub fn for_swissprot() -> Self {
        Self {
            min_cluster_size: 500,
            max_cluster_size: 30000,
            ..Self::default()
        }
    }

    /// Create configuration optimized for TrEMBL database
    pub fn for_trembl() -> Self {
        Self {
            min_cluster_size: 2000,
            max_cluster_size: 100000,
            memory_limit_mb: 16000,
            ..Self::default()
        }
    }
}

/// Main phylogenetic clustering engine
pub struct PhylogeneticClusterer {
    config: ClusteringConfig,
    taxonomy_db: Option<TaxonomyDB>,
}

impl PhylogeneticClusterer {
    /// Create a new clusterer with the given configuration
    pub fn new(config: ClusteringConfig) -> Self {
        Self {
            config,
            taxonomy_db: None,
        }
    }

    /// Set the taxonomy database for phylogenetic calculations
    pub fn with_taxonomy(mut self, db: TaxonomyDB) -> Self {
        self.taxonomy_db = Some(db);
        self
    }

    /// Create optimal clusters from sequences
    pub fn create_clusters(&self, sequences: Vec<Sequence>) -> Result<Vec<TaxonomicCluster>> {
        info!(
            "Creating phylogenetic clusters for {} sequences",
            sequences.len()
        );

        // Step 1: Group sequences by taxon ID
        let taxon_groups = self.group_by_taxon(sequences);
        info!("Grouped into {} taxa", taxon_groups.len());

        // Step 2: Identify anchor taxa (large groups or high priority)
        let (anchors, small_taxa) = self.identify_anchors(&taxon_groups);
        info!(
            "Found {} anchor taxa, {} small taxa",
            anchors.len(),
            small_taxa.len()
        );

        // Step 3: Create initial clusters from anchors
        let mut clusters = self.create_anchor_clusters(anchors);

        // Step 4: Aggregate small taxa into clusters
        if let Some(ref db) = self.taxonomy_db {
            self.aggregate_small_taxa(&mut clusters, small_taxa, db)?;
        } else {
            // Without taxonomy tree, create simple size-based clusters
            self.aggregate_by_size(&mut clusters, small_taxa);
        }

        // Step 5: Balance cluster sizes
        clusters = self.balance_cluster_sizes(clusters)?;

        // Step 6: Optimize for memory constraints
        clusters = self.optimize_for_memory(clusters)?;

        info!("Created {} optimized clusters", clusters.len());
        Ok(clusters)
    }

    /// Group sequences by their taxon ID
    fn group_by_taxon(&self, sequences: Vec<Sequence>) -> HashMap<u32, Vec<Sequence>> {
        let mut groups: HashMap<u32, Vec<Sequence>> = HashMap::new();
        let mut no_taxon = Vec::new();

        for seq in sequences {
            if let Some(taxon_id) = seq.taxon_id {
                groups.entry(taxon_id).or_default().push(seq);
            } else {
                no_taxon.push(seq);
            }
        }

        // Group sequences without taxon ID as special group (taxon 0)
        if !no_taxon.is_empty() {
            groups.insert(0, no_taxon);
        }

        groups
    }

    /// Identify anchor taxa that should form the basis of clusters
    fn identify_anchors(
        &self,
        taxon_groups: &HashMap<u32, Vec<Sequence>>,
    ) -> (TaxonGroup, TaxonGroup) {
        let mut anchors = Vec::new();
        let mut small_taxa = Vec::new();

        for (taxon_id, sequences) in taxon_groups.iter() {
            let is_anchor =
                // High priority taxa are always anchors
                self.config.high_priority_taxa.contains(taxon_id) ||
                // Large taxa are anchors
                sequences.len() >= self.config.min_cluster_size ||
                // Very diverse taxa (by sequence length variance) are anchors
                self.has_high_sequence_diversity(sequences);

            if is_anchor {
                anchors.push((*taxon_id, sequences.clone()));
            } else {
                small_taxa.push((*taxon_id, sequences.clone()));
            }
        }

        // Sort anchors by size (largest first) for better load balancing
        anchors.sort_by_key(|(_, seqs)| std::cmp::Reverse(seqs.len()));

        (anchors, small_taxa)
    }

    /// Check if a set of sequences has high diversity (by length variance)
    fn has_high_sequence_diversity(&self, sequences: &[Sequence]) -> bool {
        if sequences.len() < 10 {
            return false;
        }

        let lengths: Vec<usize> = sequences.iter().map(|s| s.sequence.len()).collect();
        let mean = lengths.iter().sum::<usize>() as f64 / lengths.len() as f64;
        let variance = lengths
            .iter()
            .map(|&len| {
                let diff = len as f64 - mean;
                diff * diff
            })
            .sum::<f64>()
            / lengths.len() as f64;

        let std_dev = variance.sqrt();
        let cv = std_dev / mean; // Coefficient of variation

        cv > 0.5 // High diversity if CV > 50%
    }

    /// Create initial clusters from anchor taxa
    fn create_anchor_clusters(&self, anchors: Vec<(u32, Vec<Sequence>)>) -> Vec<TaxonomicCluster> {
        let mut clusters = Vec::new();

        for (taxon_id, sequences) in anchors {
            // Split large anchors if necessary
            if sequences.len() > self.config.max_cluster_size {
                clusters.extend(self.split_large_taxon(taxon_id, sequences));
            } else {
                clusters.push(self.create_cluster(
                    vec![taxon_id],
                    sequences,
                    TaxonomicRank::Species,
                ));
            }
        }

        clusters
    }

    /// Split a large taxon into multiple clusters
    fn split_large_taxon(&self, taxon_id: u32, sequences: Vec<Sequence>) -> Vec<TaxonomicCluster> {
        let mut clusters = Vec::new();
        let mut current_batch = Vec::new();

        for seq in sequences {
            current_batch.push(seq);

            if current_batch.len() >= self.config.max_cluster_size {
                clusters.push(self.create_cluster(
                    vec![taxon_id],
                    current_batch.clone(),
                    TaxonomicRank::Species,
                ));
                current_batch.clear();
            }
        }

        if !current_batch.is_empty() {
            clusters.push(self.create_cluster(
                vec![taxon_id],
                current_batch,
                TaxonomicRank::Species,
            ));
        }

        clusters
    }

    /// Aggregate small taxa into existing clusters based on phylogeny
    fn aggregate_small_taxa(
        &self,
        clusters: &mut Vec<TaxonomicCluster>,
        small_taxa: Vec<(u32, Vec<Sequence>)>,
        db: &TaxonomyDB,
    ) -> Result<()> {
        for (taxon_id, sequences) in small_taxa {
            // Find the best cluster for this taxon
            let best_cluster_idx = self.find_best_cluster(taxon_id, clusters, db);

            if let Some(idx) = best_cluster_idx {
                // Add to existing cluster if it won't exceed size limits
                if clusters[idx].sequences.len() + sequences.len() <= self.config.max_cluster_size {
                    clusters[idx].taxa.insert(taxon_id);
                    clusters[idx].sequences.extend(sequences);
                    // Recalculate diversity
                    clusters[idx].diversity_score =
                        self.calculate_cluster_diversity(&clusters[idx], db);
                } else {
                    // Create new cluster if adding would exceed limits
                    clusters.push(self.create_cluster(
                        vec![taxon_id],
                        sequences,
                        TaxonomicRank::Species,
                    ));
                }
            } else {
                // No suitable cluster found, create new one
                clusters.push(self.create_cluster(
                    vec![taxon_id],
                    sequences,
                    TaxonomicRank::Species,
                ));
            }
        }

        Ok(())
    }

    /// Find the best existing cluster for a taxon based on phylogenetic distance
    fn find_best_cluster(
        &self,
        taxon_id: u32,
        clusters: &[TaxonomicCluster],
        db: &TaxonomyDB,
    ) -> Option<usize> {
        let mut best_idx = None;
        let mut best_distance = f32::MAX;

        for (idx, cluster) in clusters.iter().enumerate() {
            // Skip if cluster is already at max size
            if cluster.sequences.len() >= self.config.max_cluster_size {
                continue;
            }

            // Calculate average phylogenetic distance to taxa in cluster
            let avg_distance = self.average_distance_to_cluster(taxon_id, cluster, db);

            // Check if this cluster is within diversity limits
            if avg_distance < self.config.max_diversity && avg_distance < best_distance {
                best_distance = avg_distance;
                best_idx = Some(idx);
            }
        }

        best_idx
    }

    /// Calculate average phylogenetic distance from a taxon to all taxa in a cluster
    fn average_distance_to_cluster(
        &self,
        taxon_id: u32,
        cluster: &TaxonomicCluster,
        db: &TaxonomyDB,
    ) -> f32 {
        if cluster.taxa.is_empty() {
            return 1.0; // Maximum distance
        }

        let distances: Vec<f32> = cluster
            .taxa
            .iter()
            .map(|&other_taxon| db.phylogenetic_distance(taxon_id, other_taxon) as f32)
            .collect();

        distances.iter().sum::<f32>() / distances.len() as f32
    }

    /// Calculate diversity score for a cluster
    fn calculate_cluster_diversity(&self, cluster: &TaxonomicCluster, db: &TaxonomyDB) -> f32 {
        if cluster.taxa.len() <= 1 {
            return 0.0;
        }

        let mut total_distance = 0.0;
        let mut pair_count = 0;

        let taxa_vec: Vec<u32> = cluster.taxa.iter().copied().collect();
        for i in 0..taxa_vec.len() {
            for j in i + 1..taxa_vec.len() {
                total_distance += db.phylogenetic_distance(taxa_vec[i], taxa_vec[j]) as f32;
                pair_count += 1;
            }
        }

        if pair_count > 0 {
            total_distance / pair_count as f32
        } else {
            0.0
        }
    }

    /// Aggregate small taxa by size when no taxonomy is available
    fn aggregate_by_size(
        &self,
        clusters: &mut Vec<TaxonomicCluster>,
        small_taxa: Vec<(u32, Vec<Sequence>)>,
    ) {
        let mut pending_sequences = Vec::new();
        let mut pending_taxa = HashSet::new();

        for (taxon_id, sequences) in small_taxa {
            pending_taxa.insert(taxon_id);
            pending_sequences.extend(sequences);

            // Create a new cluster when we have enough sequences
            if pending_sequences.len() >= self.config.min_cluster_size {
                clusters.push(self.create_cluster(
                    pending_taxa.iter().copied().collect(),
                    pending_sequences.clone(),
                    TaxonomicRank::NoRank,
                ));
                pending_sequences.clear();
                pending_taxa.clear();
            }
        }

        // Add remaining sequences to last cluster or create new one
        if !pending_sequences.is_empty() {
            if let Some(last) = clusters.last_mut() {
                if last.sequences.len() + pending_sequences.len() <= self.config.max_cluster_size {
                    last.sequences.extend(pending_sequences);
                    last.taxa.extend(pending_taxa);
                } else {
                    clusters.push(self.create_cluster(
                        pending_taxa.iter().copied().collect(),
                        pending_sequences,
                        TaxonomicRank::NoRank,
                    ));
                }
            } else {
                clusters.push(self.create_cluster(
                    pending_taxa.iter().copied().collect(),
                    pending_sequences,
                    TaxonomicRank::NoRank,
                ));
            }
        }
    }

    /// Balance cluster sizes to avoid very small or very large clusters
    fn balance_cluster_sizes(
        &self,
        mut clusters: Vec<TaxonomicCluster>,
    ) -> Result<Vec<TaxonomicCluster>> {
        let mut balanced = Vec::new();

        // Merge very small clusters
        let mut small_buffer = Vec::new();
        for cluster in clusters.drain(..) {
            if cluster.sequences.len() < self.config.min_cluster_size / 2 {
                small_buffer.extend(cluster.sequences);
            } else {
                balanced.push(cluster);
            }

            // Create new cluster from buffer when large enough
            if small_buffer.len() >= self.config.min_cluster_size {
                balanced.push(self.create_cluster(
                    vec![0], // Mixed taxon group
                    small_buffer.clone(),
                    TaxonomicRank::NoRank,
                ));
                small_buffer.clear();
            }
        }

        // Add remaining sequences to the smallest cluster
        if !small_buffer.is_empty() {
            if let Some(smallest) = balanced.iter_mut().min_by_key(|c| c.sequences.len()) {
                smallest.sequences.extend(small_buffer);
            } else {
                balanced.push(self.create_cluster(vec![0], small_buffer, TaxonomicRank::NoRank));
            }
        }

        Ok(balanced)
    }

    /// Optimize clusters for memory constraints
    fn optimize_for_memory(
        &self,
        mut clusters: Vec<TaxonomicCluster>,
    ) -> Result<Vec<TaxonomicCluster>> {
        let mut optimized = Vec::new();

        for mut cluster in clusters.drain(..) {
            // Estimate memory usage
            cluster.estimated_memory_mb = self.estimate_memory(&cluster);

            // Split if exceeds memory limit
            if cluster.estimated_memory_mb > self.config.memory_limit_mb {
                optimized.extend(self.split_for_memory(cluster)?);
            } else {
                optimized.push(cluster);
            }
        }

        Ok(optimized)
    }

    /// Estimate memory usage for a cluster (for LAMBDA alignment)
    fn estimate_memory(&self, cluster: &TaxonomicCluster) -> usize {
        let num_sequences = cluster.sequences.len();
        let avg_length = cluster.avg_sequence_length();

        // Rough estimate: LAMBDA uses ~10x the input size for indexing
        // Plus overhead for alignment matrices
        let index_memory = (cluster.total_size() * 10) / 1_048_576; // Convert to MB
        let alignment_memory = (num_sequences * num_sequences * avg_length) / 1_048_576 / 100;

        index_memory + alignment_memory
    }

    /// Split a cluster that exceeds memory limits
    fn split_for_memory(&self, cluster: TaxonomicCluster) -> Result<Vec<TaxonomicCluster>> {
        let mut splits = Vec::new();
        let sequences = cluster.sequences;
        let batch_size = sequences.len() / 2;

        for chunk in sequences.chunks(batch_size) {
            let mut new_cluster = self.create_cluster(
                cluster.taxa.iter().copied().collect(),
                chunk.to_vec(),
                cluster.cluster_rank,
            );
            new_cluster.estimated_memory_mb = self.estimate_memory(&new_cluster);

            // Recursive split if still too large
            if new_cluster.estimated_memory_mb > self.config.memory_limit_mb {
                splits.extend(self.split_for_memory(new_cluster)?);
            } else {
                splits.push(new_cluster);
            }
        }

        Ok(splits)
    }

    /// Create a new cluster
    fn create_cluster(
        &self,
        taxa: Vec<u32>,
        sequences: Vec<Sequence>,
        rank: TaxonomicRank,
    ) -> TaxonomicCluster {
        let taxa_set: HashSet<u32> = taxa.iter().copied().collect();
        let representative = taxa.first().copied().unwrap_or(0);

        TaxonomicCluster {
            id: format!("cluster_{}_{}", representative, sequences.len()),
            representative_taxon: representative,
            taxa: taxa_set,
            sequences,
            diversity_score: 0.0, // Will be calculated if taxonomy tree is available
            estimated_memory_mb: 0, // Will be calculated later
            cluster_rank: rank,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_creation() {
        let mut config = ClusteringConfig::default();
        // Set min_cluster_size to 1 for test with small number of sequences
        config.min_cluster_size = 1;
        let clusterer = PhylogeneticClusterer::new(config);

        // Create test sequences
        let sequences = vec![
            Sequence {
                id: "seq1".to_string(),
                description: None,
                sequence: vec![b'A'; 100],
                taxon_id: Some(562), // E. coli
                taxonomy_sources: Default::default(),
            },
            Sequence {
                id: "seq2".to_string(),
                description: None,
                sequence: vec![b'C'; 100],
                taxon_id: Some(562), // E. coli
                taxonomy_sources: Default::default(),
            },
            Sequence {
                id: "seq3".to_string(),
                description: None,
                sequence: vec![b'G'; 100],
                taxon_id: Some(9606), // Human
                taxonomy_sources: Default::default(),
            },
        ];

        let clusters = clusterer.create_clusters(sequences).unwrap();

        // Should create separate clusters for E. coli and Human (high priority taxa)
        assert!(clusters.len() >= 2);
    }
}
