//! Database operations and processing

pub mod assembler;
pub mod differ;
pub mod reduction;
pub mod state;

// Re-export main types
pub use assembler::{FastaAssembler, AssemblyResult, AssemblyBuilder};
pub use differ::{TemporalManifestDiffer, StandardTemporalManifestDiffer, DiffResult,
                 DiffOptions, ChangeType, ChunkChange, DiffStats};
pub use reduction::{ReductionManifest, ReductionParameters, ReferenceChunk,
                    DeltaChunkRef, ReductionStatistics, ReductionManager};
pub use state::{ProcessingState, ProcessingStateManager, OperationType, SourceInfo};