//! Mock implementations for testing
//!
//! Provides mock versions of core components for unit testing.

mod aligner;
mod download;
mod download_state;
mod taxonomy;

pub use aligner::{MockAligner, MockAlignerConfig};
pub use download::{create_test_download_state, MockDownloadSource, TestDatabaseMetadata};
pub use download_state::{
    create_completed_download_state, create_download_state_at_stage, create_mock_compressed_file,
    setup_test_download_workspace,
};
pub use taxonomy::{MockTaxonomyEntry, MockTaxonomyManager};
