//! Bioinformatics utilities for Talaria

pub mod alignment;
pub mod clustering;
pub mod compression;
pub mod formats;
pub mod providers;
pub mod sequence;
pub mod taxonomy;

// Re-export commonly used types from sequence module
pub use sequence::{Sequence, SequenceType};

// Re-export commonly used functions from formats module
pub use formats::fasta::{parse_fasta, parse_fasta_from_bytes, parse_fasta_parallel, write_fasta};

// Re-export commonly used taxonomy types
pub use taxonomy::{TaxonomyDB, TaxonomyInfo, TaxonomySources};

// Re-export clustering types
pub use clustering::{
    ClusteringConfig, ClusteringRules, GroupingStrategy, PhylogeneticClusterer, TaxonomicCluster,
};
