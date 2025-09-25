/// Delta encoding and reconstruction module

pub mod traits;
pub mod generator;
pub mod reconstructor;
pub mod canonical;

// Re-export main types
pub use traits::{DeltaGenerator, DeltaGeneratorConfig, DeltaReconstructor};
pub use generator::DeltaGenerator as SequenceDeltaGenerator;
pub use reconstructor::{DeltaReconstructor as SequenceDeltaReconstructor, ReconstructorConfig};
pub use canonical::{CanonicalDelta, CanonicalDeltaManager, Delta, DeltaOp};
