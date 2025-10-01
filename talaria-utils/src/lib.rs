//! Shared utilities for Talaria
//!
//! Provides common functionality for progress bars, output formatting,
//! tree visualization, workspace management, and other utilities.

pub mod database;
pub mod display;
pub mod parallel;
pub mod performance;
pub mod progress;
pub mod report;
pub mod taxonomy;
pub mod workspace;

// Re-export commonly used types
pub use display::{
    // progress module
    create_progress_bar,
    create_spinner,
    create_standard_table,
    error,
    // format module
    format_bytes,
    format_duration,
    format_number,
    get_file_size,
    header_cell,
    info,
    success,
    tree_section,
    warning,
    Item,
    OutputFormattable,
    // formatter module
    OutputFormatter,
    ProgressBarManager,
    Section,
    Status,
    StatusReporter,
    // output module
    TreeNode,
};

// Database utilities
pub use database::{
    DatabaseReference, DatabaseVersion, VersionAliases, VersionDetector, VersionManager,
};

// Parallel processing utilities
pub use parallel::{
    chunk_size_for_parallelism, configure_thread_pool, get_available_cores, should_parallelize,
};

// Workspace utilities
pub use workspace::{
    find_workspace, list_workspaces, SequoiaStatistics, SequoiaTransaction,
    SequoiaWorkspaceManager, TempWorkspace, WorkspaceConfig, WorkspaceMetadata, WorkspaceStats,
    WorkspaceStatus,
};

// Performance utilities
pub use performance::MemoryEstimator;

// Report utilities
pub use report::{
    ComparisonResult, DatabaseStatistics, ModifiedSequence, RenamedSequence, ReportFormat,
    ReportOptions, SequenceChange, SequenceInfo,
};

// Taxonomy utilities
pub use taxonomy::{
    get_taxonomy_mappings_dir, get_taxonomy_tree_path, has_taxonomy, load_taxonomy_mappings,
    require_taxonomy, TaxonomyMappingSource, TaxonomyProvider,
};
