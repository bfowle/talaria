//! Phylogenetic clustering and biological grouping algorithms
//!
//! This module provides sophisticated clustering strategies for biological sequences
//! based on phylogenetic relationships, taxonomic hierarchies, and biological constraints.

pub mod phylogenetic;
pub mod rules;

pub use phylogenetic::{ClusteringConfig, PhylogeneticClusterer, TaxonomicCluster};
pub use rules::{ClusteringRules, GroupingStrategy, TaxonGroupingRules};
