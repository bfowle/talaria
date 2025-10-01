pub mod canonical;
pub mod generator;
pub mod reconstructor;
/// Delta encoding and reconstruction module
pub mod traits;

// Re-export main types
pub use canonical::{CanonicalDelta, CanonicalDeltaManager, Delta, DeltaOp};
pub use generator::DeltaGenerator as SequenceDeltaGenerator;
pub use reconstructor::{DeltaReconstructor as SequenceDeltaReconstructor, ReconstructorConfig};
pub use traits::{DeltaGenerator, DeltaGeneratorConfig, DeltaReconstructor};
