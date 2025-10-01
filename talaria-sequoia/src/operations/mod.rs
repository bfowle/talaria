//! Database operations and processing
//!
//! This module provides core database operations including:
//! - **Database comparison**: Compare chunks, sequences, and taxonomies between databases
//! - **Reduction**: Compress sequence databases using delta encoding
//! - **Assembly**: Reconstruct original sequences from reduced format
//! - **Migration**: Convert between database formats
//! - **Validation**: Verify database integrity and consistency
//!
//! # Key Components
//!
//! ## Database Diff
//! Compare two databases to find similarities and differences:
//! ```rust,no_run
//! # use anyhow::Result;
//! # use std::path::PathBuf;
//! # fn main() -> Result<()> {
//! use talaria_sequoia::operations::DatabaseDiffer;
//!
//! # let db1_path = PathBuf::from("/path/to/db1");
//! # let db2_path = PathBuf::from("/path/to/db2");
//! let differ = DatabaseDiffer::new(&db1_path, &db2_path)?;
//! let comparison = differ.compare()?;
//! println!("Shared chunks: {}", comparison.chunk_analysis.shared_chunks.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Reduction
//! Reduce database size using reference selection and delta encoding:
//! ```rust,no_run
//! # use anyhow::Result;
//! # use std::path::PathBuf;
//! # fn main() -> Result<()> {
//! use talaria_sequoia::operations::{ReductionManager, ReductionManifest};
//! use talaria_sequoia::SequoiaStorage;
//!
//! # let storage_path = PathBuf::from("/tmp/storage");
//! let storage = SequoiaStorage::new(&storage_path)?;
//! let mut manager = ReductionManager::new(storage);
//! // Save and load reduction manifests
//! # use talaria_sequoia::operations::ReductionParameters;
//! # use talaria_sequoia::types::SHA256Hash;
//! # let manifest = ReductionManifest::new(
//! #     "test_profile".to_string(),
//! #     SHA256Hash::zero(),
//! #     "test_db".to_string(),
//! #     ReductionParameters::default(),
//! # );
//! manager.save_manifest(&manifest)?;
//! # Ok(())
//! # }
//! ```

pub mod assembler;
pub mod database_diff;
pub mod differ;
pub mod migrator;
pub mod reducer;
pub mod reduction;
pub mod reference_selector;
pub mod reference_selector_optimized;
pub mod results;
pub mod selection;
pub mod state;
pub mod validator;

// Re-export main types
pub use assembler::{AssemblyBuilder, AssemblyResult, FastaAssembler};
pub use database_diff::{
    format_bytes, ChunkAnalysis, DatabaseComparison, DatabaseDiffer, SequenceAnalysis,
    StorageMetrics, TaxonDistribution, TaxonomyAnalysis,
};
pub use differ::{
    ChangeType, ChunkChange, DiffOptions, DiffResult, DiffStats, StandardTemporalManifestDiffer,
    TemporalManifestDiffer,
};
pub use reducer::Reducer;
pub use reduction::{
    DeltaChunkRef, ReductionManager, ReductionManifest, ReductionParameters, ReductionStatistics,
    ReferenceChunk,
};
pub use reference_selector::{ReferenceSelectorImpl, SelectionAlgorithm, SelectionResult};
pub use selection::traits::{
    AlignmentBasedSelector, AlignmentScore, RecommendedParams, ReferenceSelector, SelectionStats,
    TraitSelectionResult,
};
pub use state::{OperationType, ProcessingState, ProcessingStateManager, SourceInfo};
pub use results::{
    CompositionStats, DiscrepancyResult, GarbageCollectionResult, HistoryResult, MirrorResult,
    OptimizationResult, ReconstructionResult, ReductionResult, StatsResult,
    TaxonomyComparison, TaxonomyCoverageInfo, TaxonomyCoverageResult, UpdateCheckResult,
    UpdateResult, ValidationResult, VersionHistoryEntry, VerificationResult, DatabaseInfoResult,
};
