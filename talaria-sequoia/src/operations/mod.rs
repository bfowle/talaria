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
//! use talaria_sequoia::SEQUOIAStorage;
//!
//! # let storage_path = PathBuf::from("/tmp/storage");
//! let storage = SEQUOIAStorage::new(&storage_path)?;
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
pub mod reference_selector;
pub mod reference_selector_optimized;
pub mod reduction;
pub mod selection;
pub mod state;
pub mod validator;

// Re-export main types
pub use assembler::{FastaAssembler, AssemblyResult, AssemblyBuilder};
pub use database_diff::{DatabaseDiffer, DatabaseComparison, ChunkAnalysis, SequenceAnalysis,
                         TaxonomyAnalysis, TaxonDistribution, StorageMetrics, format_bytes};
pub use differ::{TemporalManifestDiffer, StandardTemporalManifestDiffer, DiffResult,
                 DiffOptions, ChangeType, ChunkChange, DiffStats};
pub use reduction::{ReductionManifest, ReductionParameters, ReferenceChunk,
                    DeltaChunkRef, ReductionStatistics, ReductionManager};
pub use reducer::Reducer;
pub use reference_selector::{SelectionAlgorithm, ReferenceSelectorImpl, SelectionResult};
pub use selection::traits::{ReferenceSelector, AlignmentBasedSelector, TraitSelectionResult, SelectionStats, AlignmentScore, RecommendedParams};
pub use state::{ProcessingState, ProcessingStateManager, OperationType, SourceInfo};