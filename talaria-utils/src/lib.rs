//! Shared utilities for Talaria
//!
//! Provides common functionality for progress bars, output formatting,
//! tree visualization, workspace management, and other utilities.

pub mod format;
pub mod formatter;
pub mod output;
pub mod parallel;
pub mod progress;
pub mod workspace;

// Re-export commonly used types
pub use format::{format_bytes, format_duration, get_file_size};
pub use formatter::{
    OutputFormatter, Section, Item, Status, StatusReporter, OutputFormattable
};
pub use output::{
    TreeNode, format_number, warning, info, success, error,
    tree_section, create_standard_table, header_cell,
};
pub use parallel::{
    configure_thread_pool, chunk_size_for_parallelism, get_available_cores,
    should_parallelize,
};
pub use progress::{create_progress_bar, create_spinner, ProgressBarManager};
pub use workspace::{
    TempWorkspace, WorkspaceConfig, WorkspaceMetadata,
    WorkspaceStatus, WorkspaceStats,
    list_workspaces, find_workspace,
};