pub mod nw_aligner;
pub mod scoring;

pub use nw_aligner::{Alignment, AlignmentResult, NeedlemanWunsch};
pub use scoring::{NucleotideMatrix, ScoringMatrix, BLOSUM62};
