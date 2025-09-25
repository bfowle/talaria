pub mod fasta;

// Re-export commonly used functions
pub use fasta::{parse_fasta, write_fasta, parse_fasta_parallel};
pub use fasta::{FastaReadable, FastaFile};