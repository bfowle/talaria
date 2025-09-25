pub mod delta;

// Re-export commonly used types
pub use delta::{
    DeltaRecord, HeaderChange, DeltaRange,
    DeltaEncoder, DeltaReconstructor
};