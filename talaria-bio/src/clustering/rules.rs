#![allow(dead_code)]

/// Biological clustering rules and constraints for taxonomic grouping
use crate::taxonomy::{TaxonomicRank, TaxonomyDB};
use std::collections::{HashMap, HashSet};

/// Rules for biologically-informed clustering
#[derive(Debug, Clone)]
pub struct ClusteringRules {
    /// Minimum sequences per cluster for effective alignment
    pub min_sequences_per_cluster: usize,

    /// Maximum sequences per cluster (memory constraint)
    pub max_sequences_per_cluster: usize,

    /// Maximum phylogenetic distance for merging taxa
    pub max_phylogenetic_distance: f64,

    /// Organisms that should always be in separate clusters
    pub isolation_taxa: HashSet<u32>,

    /// Organisms that can be merged with similar taxa
    pub mergeable_taxa: HashSet<u32>,

    /// Priority organisms that should be cluster anchors
    pub priority_taxa: HashSet<u32>,

    /// Taxonomic rank at which to force separation
    pub separation_rank: TaxonomicRank,

    /// Minimum rank for aggregation
    pub aggregation_rank: TaxonomicRank,
}

impl Default for ClusteringRules {
    fn default() -> Self {
        Self {
            min_sequences_per_cluster: 100,
            max_sequences_per_cluster: 50000,
            max_phylogenetic_distance: 0.3, // 30% divergence maximum

            // Important model organisms that should be isolated
            isolation_taxa: [
                9606,   // Homo sapiens
                10090,  // Mus musculus
                7227,   // Drosophila melanogaster
                6239,   // Caenorhabditis elegans
                559292, // Saccharomyces cerevisiae
                3702,   // Arabidopsis thaliana
            ]
            .into_iter()
            .collect(),

            // Taxa that can be merged with phylogenetically close organisms
            mergeable_taxa: HashSet::new(),

            // High-priority taxa for reference selection
            priority_taxa: [
                9606,   // Human
                10090,  // Mouse
                562,    // E. coli
                511145, // E. coli K12
                559292, // S. cerevisiae
            ]
            .into_iter()
            .collect(),

            separation_rank: TaxonomicRank::Family,
            aggregation_rank: TaxonomicRank::Genus,
        }
    }
}

impl ClusteringRules {
    /// Create rules optimized for SwissProt/UniProt databases
    pub fn for_swissprot() -> Self {
        let mut rules = Self::default();

        // SwissProt has high-quality curated sequences
        rules.min_sequences_per_cluster = 50;
        rules.max_sequences_per_cluster = 30000;

        // Add common lab organisms as priority
        rules.priority_taxa.extend([
            10116, // Rattus norvegicus
            9031,  // Gallus gallus
            7955,  // Danio rerio
            9913,  // Bos taurus
            9823,  // Sus scrofa
        ]);

        rules
    }

    /// Create rules optimized for TrEMBL databases
    pub fn for_trembl() -> Self {
        let mut rules = Self::default();

        // TrEMBL is much larger and less curated
        rules.min_sequences_per_cluster = 200;
        rules.max_sequences_per_cluster = 100000;
        rules.max_phylogenetic_distance = 0.25; // Stricter for larger database

        rules
    }

    /// Create rules optimized for bacterial databases
    pub fn for_bacteria() -> Self {
        let mut rules = Self::default();

        rules.min_sequences_per_cluster = 100;
        rules.max_sequences_per_cluster = 50000;
        rules.separation_rank = TaxonomicRank::Genus;
        rules.aggregation_rank = TaxonomicRank::Species;

        // Important bacterial species
        rules.isolation_taxa = [
            562,    // E. coli
            511145, // E. coli K12
            1280,   // Staphylococcus aureus
            1773,   // Mycobacterium tuberculosis
            632,    // Yersinia pestis
            470,    // Acinetobacter baumannii
        ]
        .into_iter()
        .collect();

        rules
    }

    /// Check if two taxa can be clustered together
    pub fn can_cluster_together(
        &self,
        taxon_a: u32,
        taxon_b: u32,
        taxonomy_db: Option<&TaxonomyDB>,
    ) -> bool {
        // Never cluster isolation taxa with others
        if self.isolation_taxa.contains(&taxon_a) || self.isolation_taxa.contains(&taxon_b) {
            return taxon_a == taxon_b;
        }

        // Check phylogenetic distance if taxonomy database available
        if let Some(db) = taxonomy_db {
            let distance = db.phylogenetic_distance(taxon_a, taxon_b);
            if distance > self.max_phylogenetic_distance {
                return false;
            }

            // Check separation rank
            if let Some(common_rank) = db.lowest_common_rank(taxon_a, taxon_b) {
                if common_rank < self.separation_rank {
                    return false;
                }
            }
        }

        true
    }

    /// Determine if a taxon should be a cluster anchor
    pub fn is_anchor_taxon(&self, taxon_id: u32) -> bool {
        self.priority_taxa.contains(&taxon_id) || self.isolation_taxa.contains(&taxon_id)
    }

    /// Calculate clustering priority score for a taxon
    pub fn taxon_priority(&self, taxon_id: u32, sequence_count: usize) -> f64 {
        let mut score = sequence_count as f64;

        if self.priority_taxa.contains(&taxon_id) {
            score *= 10.0; // High priority
        }

        if self.isolation_taxa.contains(&taxon_id) {
            score *= 100.0; // Must be separate
        }

        score
    }

    /// Validate a proposed cluster
    pub fn validate_cluster(&self, taxa: &[u32], sequence_count: usize) -> Result<(), String> {
        // Check size constraints
        if sequence_count < self.min_sequences_per_cluster {
            return Err(format!(
                "Cluster too small: {} sequences (minimum: {})",
                sequence_count, self.min_sequences_per_cluster
            ));
        }

        if sequence_count > self.max_sequences_per_cluster {
            return Err(format!(
                "Cluster too large: {} sequences (maximum: {})",
                sequence_count, self.max_sequences_per_cluster
            ));
        }

        // Check for multiple isolation taxa in same cluster
        let isolation_count = taxa
            .iter()
            .filter(|t| self.isolation_taxa.contains(t))
            .count();

        if isolation_count > 1 {
            return Err("Multiple isolation taxa in same cluster".to_string());
        }

        Ok(())
    }

    /// Suggest optimal cluster size based on total sequences
    pub fn suggest_cluster_count(&self, total_sequences: usize) -> usize {
        let avg_cluster_size =
            (self.min_sequences_per_cluster + self.max_sequences_per_cluster) / 2;
        let suggested = total_sequences / avg_cluster_size;

        // Ensure reasonable bounds
        suggested.clamp(1, 1000)
    }
}

/// Strategy for handling specific taxonomic groups
#[derive(Debug, Clone)]
pub enum GroupingStrategy {
    /// Keep all sequences from this taxon together
    KeepTogether,

    /// Split this taxon across multiple clusters if needed
    AllowSplitting,

    /// This taxon must be in its own cluster
    Isolate,

    /// Merge with phylogenetically similar taxa
    MergeWithSimilar,
}

/// Rules for specific taxonomic groups
#[derive(Debug, Clone)]
pub struct TaxonGroupingRules {
    rules: HashMap<u32, GroupingStrategy>,
}

impl Default for TaxonGroupingRules {
    fn default() -> Self {
        Self::new()
    }
}

impl TaxonGroupingRules {
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
        }
    }

    /// Add default rules for common organisms
    pub fn with_defaults() -> Self {
        let mut rules = Self::new();

        // Model organisms - isolate
        for taxon in [9606, 10090, 7227, 6239, 559292, 3702] {
            rules.add_rule(taxon, GroupingStrategy::Isolate);
        }

        // Large bacterial groups - allow splitting
        for taxon in [562, 511145] {
            rules.add_rule(taxon, GroupingStrategy::AllowSplitting);
        }

        rules
    }

    pub fn add_rule(&mut self, taxon_id: u32, strategy: GroupingStrategy) {
        self.rules.insert(taxon_id, strategy);
    }

    pub fn get_strategy(&self, taxon_id: u32) -> GroupingStrategy {
        self.rules
            .get(&taxon_id)
            .cloned()
            .unwrap_or(GroupingStrategy::MergeWithSimilar)
    }

    pub fn should_isolate(&self, taxon_id: u32) -> bool {
        matches!(self.get_strategy(taxon_id), GroupingStrategy::Isolate)
    }

    pub fn can_split(&self, taxon_id: u32) -> bool {
        matches!(
            self.get_strategy(taxon_id),
            GroupingStrategy::AllowSplitting
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_rules() {
        let rules = ClusteringRules::default();

        assert!(rules.is_anchor_taxon(9606)); // Human
        assert!(rules.isolation_taxa.contains(&9606));
        assert_eq!(rules.separation_rank, TaxonomicRank::Family);
    }

    #[test]
    fn test_can_cluster_together() {
        let rules = ClusteringRules::default();

        // Isolation taxa can't cluster with others
        assert!(!rules.can_cluster_together(9606, 10090, None)); // Human vs Mouse
        assert!(rules.can_cluster_together(9606, 9606, None)); // Same taxon

        // Non-isolation taxa can cluster
        assert!(rules.can_cluster_together(1234, 5678, None));
    }

    #[test]
    fn test_validate_cluster() {
        let rules = ClusteringRules::default();

        // Too small
        assert!(rules.validate_cluster(&[562], 10).is_err());

        // Just right
        assert!(rules.validate_cluster(&[562], 500).is_ok());

        // Too large
        assert!(rules.validate_cluster(&[562], 100000).is_err());

        // Multiple isolation taxa
        assert!(rules.validate_cluster(&[9606, 10090], 500).is_err());
    }

    #[test]
    fn test_taxon_priority() {
        let rules = ClusteringRules::default();

        let normal_score = rules.taxon_priority(1234, 100);
        let priority_score = rules.taxon_priority(562, 100); // E. coli
        let isolation_score = rules.taxon_priority(9606, 100); // Human

        assert!(priority_score > normal_score);
        assert!(isolation_score > priority_score);
    }
}
