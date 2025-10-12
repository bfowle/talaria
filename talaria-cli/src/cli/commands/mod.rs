pub mod chunk;
pub mod database;
pub mod interactive;
pub mod reconstruct;
pub mod reduce;
pub mod herald;
pub mod stats;
pub mod temporal;
pub mod tools;
pub mod validate;
pub mod verify;

use anyhow::Result;
use std::path::Path;
use talaria_utils::report::Reportable;

/// Save a report to a file in the specified format
///
/// This is a helper function that all commands can use to generate
/// reports in a consistent way.
pub fn save_report<T: Reportable>(result: &T, format: &str, output_path: &Path) -> Result<()> {
    use talaria_utils::report::{render_csv, render_html, render_json, render_text};

    let report = result.to_report();
    let content = match format.to_lowercase().as_str() {
        "html" => render_html(&report)?,
        "json" => render_json(&report)?,
        "csv" => render_csv(&report)?,
        "text" | "txt" => render_text(&report)?,
        _ => anyhow::bail!("Unknown format '{}'. Use: text, html, json, csv", format),
    };

    std::fs::write(output_path, content)?;
    Ok(())
}
