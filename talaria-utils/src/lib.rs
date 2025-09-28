//! Shared utilities for Talaria
//!
//! Provides common functionality for progress bars, output formatting,
//! tree visualization, workspace management, and other utilities.

pub mod database;
pub mod display;
pub mod parallel;
pub mod performance;
pub mod progress;  // New unified progress module
pub mod report;
pub mod workspace;

// Re-export commonly used types for backward compatibility
// Display utilities
pub use display::{
    // format module
    format_bytes, format_duration, get_file_size,
    // formatter module
    OutputFormatter, Section, Item, Status, StatusReporter, OutputFormattable,
    // output module
    TreeNode, format_number, warning, info, success, error,
    tree_section, create_standard_table, header_cell,
    // progress module
    create_progress_bar, create_spinner, ProgressBarManager,
};

// Database utilities
pub use database::{
    DatabaseReference, DatabaseVersion, VersionAliases,
    VersionDetector, VersionManager,
};

// Parallel processing utilities
pub use parallel::{
    configure_thread_pool, chunk_size_for_parallelism, get_available_cores,
    should_parallelize,
};

// Workspace utilities
pub use workspace::{
    TempWorkspace, WorkspaceConfig, WorkspaceMetadata,
    WorkspaceStatus, WorkspaceStats,
    list_workspaces, find_workspace,
    SequoiaWorkspaceManager, SequoiaStatistics, SequoiaTransaction,
};

// Performance utilities
pub use performance::{MemoryEstimator};

// Report utilities
pub use report::{
    ReportGenerator, ReportOptions, Format, Reporter,
    ComparisonResult, SequenceInfo,
};