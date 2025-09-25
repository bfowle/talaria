//! Display and formatting utilities

pub mod format;
pub mod formatter;
pub mod output;
pub mod progress;

// Re-export commonly used types
pub use format::{format_bytes, format_duration, get_file_size};
pub use formatter::{
    OutputFormatter, Section, Item, Status, StatusReporter, OutputFormattable
};
pub use output::{
    TreeNode, format_number, warning, info, success, error,
    tree_section, create_standard_table, header_cell,
};
pub use progress::{create_progress_bar, create_spinner, ProgressBarManager};