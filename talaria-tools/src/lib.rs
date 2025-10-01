//! Tool management for external bioinformatics tools
//!
//! This module provides functionality to download, install, and manage
//! external tools like LAMBDA, BLAST, and DIAMOND that are used for
//! alignment-based sequence reduction.

// Modules
pub mod aligners;
pub mod manager;
pub mod optimizers;
pub mod testing;
pub mod traits;
pub mod types;

// Re-exports for convenience
pub use aligners::LambdaAligner;
pub use manager::{ToolInfo, ToolManager};
pub use testing::MockAligner;
pub use traits::{Aligner, AlignmentConfig, AlignmentSummary, ConfigurableAligner};
pub use types::Tool;
