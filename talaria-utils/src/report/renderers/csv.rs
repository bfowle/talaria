/// Generic CSV renderer for Report type
use crate::report::core::{Report, Section, SectionContent, Table};
use anyhow::Result;

/// Render a Report to CSV format
///
/// Since CSV is inherently tabular, this renderer:
/// - Exports Table sections as separate CSV blocks
/// - Exports KeyValueList sections as two-column tables
/// - Exports Metrics as key-value pairs
/// - Skips non-tabular content (charts, text) with comments
pub fn render_csv(report: &Report) -> Result<String> {
    let mut output = String::new();

    // Metadata as comments
    output.push_str(&format!("# {}\n", report.title));
    output.push_str(&format!("# Command: {}\n", report.command));
    output.push_str(&format!(
        "# Generated: {}\n",
        report.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    for (key, value) in &report.metadata {
        output.push_str(&format!("# {}: {}\n", key, value));
    }
    output.push_str("\n");

    // Sections
    for section in &report.sections {
        render_section(&mut output, section)?;
    }

    Ok(output)
}

fn render_section(output: &mut String, section: &Section) -> Result<()> {
    output.push_str(&format!("# {}\n", section.title));

    match &section.content {
        SectionContent::Metrics(metrics) => {
            output.push_str("Metric,Value,Change From,Change To\n");
            for metric in metrics {
                let change_from = metric
                    .change
                    .as_ref()
                    .map(|c| c.from.as_str())
                    .unwrap_or("");
                let change_to = metric.change.as_ref().map(|c| c.to.as_str()).unwrap_or("");
                output.push_str(&format!(
                    "\"{}\",\"{}\",\"{}\",\"{}\"\n",
                    escape_csv(&metric.label),
                    escape_csv(&metric.value),
                    escape_csv(change_from),
                    escape_csv(change_to)
                ));
            }
        }
        SectionContent::Table(table) => {
            render_table(output, table)?;
        }
        SectionContent::KeyValueList(items) => {
            output.push_str("Key,Value\n");
            for (key, value) in items {
                output.push_str(&format!(
                    "\"{}\",\"{}\"\n",
                    escape_csv(key),
                    escape_csv(value)
                ));
            }
        }
        SectionContent::BulletList(items) => {
            output.push_str("Item\n");
            for item in items {
                output.push_str(&format!("\"{}\"\n", escape_csv(item)));
            }
        }
        SectionContent::Chart(chart) => {
            // Export chart as table
            output.push_str(&format!("# {} (Chart Data)\n", chart.title));
            output.push_str("Label");
            for dataset in &chart.datasets {
                output.push_str(&format!(",\"{}\"", escape_csv(&dataset.label)));
            }
            output.push_str("\n");

            for (i, label) in chart.labels.iter().enumerate() {
                output.push_str(&format!("\"{}\"", escape_csv(label)));
                for dataset in &chart.datasets {
                    let value = dataset
                        .data
                        .get(i)
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    output.push_str(&format!(",{}", value));
                }
                output.push_str("\n");
            }
        }
        SectionContent::Text(text) => {
            output.push_str("# (Text content - not suitable for CSV)\n");
            for line in text.lines() {
                output.push_str(&format!("# {}\n", line));
            }
        }
    }

    output.push_str("\n");
    Ok(())
}

fn render_table(output: &mut String, table: &Table) -> Result<()> {
    // Headers
    for (i, header) in table.headers.iter().enumerate() {
        if i > 0 {
            output.push(',');
        }
        output.push_str(&format!("\"{}\"", escape_csv(header)));
    }
    output.push_str("\n");

    // Rows
    for row in &table.rows {
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                output.push(',');
            }
            output.push_str(&format!("\"{}\"", escape_csv(&cell.value)));
        }
        output.push_str("\n");
    }

    Ok(())
}

/// Escape CSV special characters
fn escape_csv(s: &str) -> String {
    s.replace('"', "\"\"")
}
