// The one and only chunker
pub mod canonical_taxonomic;

// Re-export
pub use canonical_taxonomic::TaxonomicChunker;

// Re-export ChunkingStrategy from types
pub use crate::types::ChunkingStrategy;