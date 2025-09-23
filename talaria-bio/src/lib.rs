//! Bioinformatics utilities for Talaria

pub mod alignment;
pub mod fasta;
pub mod sequence;
pub mod stats;
pub mod taxonomy;
pub mod taxonomy_formatter;
pub mod taxonomy_stats;
pub mod uniprot;
pub mod delta_encoder;

// Re-export commonly used types
pub use sequence::{Sequence, SequenceType};
// Re-export fasta functions
pub use fasta::{parse_fasta, write_fasta, parse_fasta_parallel};
// Re-export taxonomy types
pub use taxonomy::{TaxonomyDB, TaxonomyInfo, TaxonomySources};