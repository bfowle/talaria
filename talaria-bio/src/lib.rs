//! Bioinformatics utilities for Talaria

pub mod alignment;
pub mod compression;
pub mod formats;
pub mod providers;
pub mod sequence;
pub mod taxonomy;

// Re-export commonly used types from sequence module
pub use sequence::{Sequence, SequenceType};

// Re-export commonly used functions from formats module
pub use formats::fasta::{parse_fasta, write_fasta, parse_fasta_parallel, parse_fasta_from_bytes};

// Re-export commonly used taxonomy types
pub use taxonomy::{TaxonomyDB, TaxonomyInfo, TaxonomySources};