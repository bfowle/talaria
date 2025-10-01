//! Display and formatting utilities

pub mod format;
pub mod formatter;
pub mod output;
pub mod progress;

// Re-export commonly used types
pub use format::{format_bytes, format_duration, get_file_size};
pub use formatter::{Item, OutputFormattable, OutputFormatter, Section, Status, StatusReporter};
pub use output::{
    create_standard_table, error, format_number, header_cell, info, success, tree_section, warning,
    TreeNode,
};
// Re-export progress utilities
pub use crate::progress::{
    create_hidden_progress_bar, create_progress_bar, create_spinner, OperationType,
    ProgressManager, ProgressManagerBuilder,
};
pub use progress::ProgressBarManager;
