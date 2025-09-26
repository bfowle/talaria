//! Database operations and processing

pub mod assembler;
pub mod differ;
pub mod migrator;
pub mod reducer;
pub mod reference_selector;
pub mod reference_selector_optimized;
pub mod reduction;
pub mod selection;
pub mod state;
pub mod validator;

// Re-export main types
pub use assembler::{FastaAssembler, AssemblyResult, AssemblyBuilder};
pub use differ::{TemporalManifestDiffer, StandardTemporalManifestDiffer, DiffResult,
                 DiffOptions, ChangeType, ChunkChange, DiffStats};
pub use reduction::{ReductionManifest, ReductionParameters, ReferenceChunk,
                    DeltaChunkRef, ReductionStatistics, ReductionManager};
pub use reducer::Reducer;
pub use reference_selector::{SelectionAlgorithm, ReferenceSelectorImpl, SelectionResult};
pub use selection::traits::{ReferenceSelector, AlignmentBasedSelector, TraitSelectionResult, SelectionStats, AlignmentScore, RecommendedParams};
pub use state::{ProcessingState, ProcessingStateManager, OperationType, SourceInfo};