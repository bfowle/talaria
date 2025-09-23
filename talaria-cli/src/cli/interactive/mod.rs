pub mod config_editor;
pub mod docs_viewer;
pub mod download;
pub mod markdown;
pub mod reduce;
pub mod stats;
pub mod themes;
pub mod wizard;

use colored::*;

/// Display a success message with formatting
pub fn show_success(message: &str) {
    println!("{} {}", "●".green().bold(), message.green());
}

/// Display an info message with formatting
pub fn show_info(message: &str) {
    println!("{} {}", "ℹ".blue().bold(), message.blue());
}

/// Create a formatted header
pub fn print_header(title: &str) {
    let width = 60;
    let border = "═".repeat(width);
    println!("\n{}", border.cyan());
    println!("{}", title.cyan().bold());
    println!("{}", border.cyan());
}
