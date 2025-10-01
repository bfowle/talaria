pub mod fasta;

// Re-export commonly used functions
pub use fasta::{parse_fasta, parse_fasta_parallel, write_fasta};
pub use fasta::{FastaFile, FastaReadable};
