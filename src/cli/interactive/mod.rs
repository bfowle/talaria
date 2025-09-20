pub mod config_editor;
pub mod docs_viewer;
pub mod download;
pub mod markdown;
pub mod reduce;
pub mod stats;
pub mod themes;
pub mod wizard;

use comfy_table::Table;
use termimad::MadSkin;

/// Create a styled table for terminal display
pub fn create_styled_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(comfy_table::presets::UTF8_FULL)
        .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS);
    table
}

/// Get the default markdown skin for terminal rendering
pub fn get_markdown_skin() -> MadSkin {
    let mut skin = MadSkin::default();

    // Customize the skin for better appearance
    skin.bold.set_fg(crossterm::style::Color::Yellow);
    skin.italic.set_fg(crossterm::style::Color::Cyan);
    skin.code_block.set_bg(crossterm::style::Color::Rgb {
        r: 40,
        g: 40,
        b: 40,
    });
    skin.inline_code.set_fg(crossterm::style::Color::Green);
    skin.headers[0].set_fg(crossterm::style::Color::Magenta);
    skin.headers[1].set_fg(crossterm::style::Color::Blue);
    skin.headers[2].set_fg(crossterm::style::Color::Cyan);

    skin
}

/// Display a success message with formatting
pub fn show_success(message: &str) {
    use colored::*;
    println!("{} {}", "●".green().bold(), message.green());
}

/// Display an error message with formatting
pub fn show_error(message: &str) {
    use colored::*;
    eprintln!("{} {}", "■".red().bold(), message.red());
}

/// Display an info message with formatting
pub fn show_info(message: &str) {
    use colored::*;
    println!("{} {}", "ℹ".blue().bold(), message.blue());
}

/// Display a warning message with formatting
pub fn show_warning(message: &str) {
    use colored::*;
    println!("{} {}", "⚠".yellow().bold(), message.yellow());
}

/// Create a formatted header
pub fn print_header(title: &str) {
    use colored::*;
    let width = 60;
    let border = "═".repeat(width);
    println!("\n{}", border.cyan());
    println!("{}", title.cyan().bold());
    println!("{}", border.cyan());
}
