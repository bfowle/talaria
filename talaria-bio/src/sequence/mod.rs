pub mod types;
pub mod stats;

// Re-export commonly used types
pub use types::{Sequence, SequenceType, sanitize_sequences};
pub use stats::SequenceStats;