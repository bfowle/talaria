/// Generic JSON renderer for Report type

use crate::report::core::Report;
use anyhow::Result;

/// Render a Report to JSON format
pub fn render_json(report: &Report) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}
