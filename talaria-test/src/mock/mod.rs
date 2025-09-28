//! Mock implementations for testing
//!
//! Provides mock versions of core components for unit testing.

mod aligner;
mod taxonomy;
mod download;
mod download_state;

pub use aligner::{MockAligner, MockAlignerConfig};
pub use taxonomy::{MockTaxonomyManager, MockTaxonomyEntry};
pub use download::{MockDownloadSource, TestDatabaseMetadata, create_test_download_state};
pub use download_state::{
    setup_test_download_workspace,
    create_mock_compressed_file,
    create_completed_download_state,
    create_download_state_at_stage,
};