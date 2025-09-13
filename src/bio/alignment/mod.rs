pub mod nw_aligner;
pub mod scoring;

pub use nw_aligner::{NeedlemanWunsch, Alignment, AlignmentResult};
pub use scoring::{ScoringMatrix, BLOSUM62, NucleotideMatrix};