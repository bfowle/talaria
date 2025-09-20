// Re-export the existing TaxonomicChunker
pub mod hierarchical_taxonomic;
mod taxonomic;
mod traits;
// mod advanced;  // Removed incomplete module

pub use hierarchical_taxonomic::{
    HierarchicalTaxonomicChunker, OrganismImportance, TaxonomicLevel,
};
pub use taxonomic::TaxonomicChunker;
pub use traits::{Chunker, ChunkingStats, TaxonomyAwareChunker};
// pub use advanced::{AdvancedChunker, OptimizationWeights};
