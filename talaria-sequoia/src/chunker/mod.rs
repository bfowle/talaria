// Chunker implementations
pub mod canonical_taxonomic;
pub mod hierarchical_taxonomic;

// Re-export chunkers
pub use canonical_taxonomic::TaxonomicChunker;
pub use hierarchical_taxonomic::{HierarchicalTaxonomicChunker, TaxonomicRank};

// Re-export ChunkingStrategy from types
pub use crate::types::ChunkingStrategy;