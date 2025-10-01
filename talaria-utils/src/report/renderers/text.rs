/// Generic text renderer for Report type

use crate::report::core::{
    CellStyle, ChangeDirection, ChartData, Metric, MetricSeverity, Report, Section,
    SectionContent, Table,
};
use anyhow::Result;

/// Render a Report to plain text format
pub fn render_text(report: &Report) -> Result<String> {
    let mut output = String::new();

    // Title
    output.push_str(&format!("{}\n", report.title));
    output.push_str(&format!("{}\n\n", "=".repeat(report.title.len())));

    // Metadata
    output.push_str(&format!("Command: {}\n", report.command));
    output.push_str(&format!(
        "Generated: {}\n",
        report.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    for (key, value) in &report.metadata {
        output.push_str(&format!("{}: {}\n", key, value));
    }
    output.push_str("\n");

    // Sections
    for section in &report.sections {
        render_section(&mut output, section)?;
    }

    Ok(output)
}

fn render_section(output: &mut String, section: &Section) -> Result<()> {
    output.push_str(&format!("{}\n", section.title));
    output.push_str(&format!("{}\n", "-".repeat(section.title.len())));

    match &section.content {
        SectionContent::Metrics(metrics) => render_metrics(output, metrics),
        SectionContent::Table(table) => render_table(output, table),
        SectionContent::KeyValueList(items) => render_key_value_list(output, items),
        SectionContent::BulletList(items) => render_bullet_list(output, items),
        SectionContent::Chart(chart) => render_chart(output, chart),
        SectionContent::Text(text) => render_text_content(output, text),
    }

    output.push_str("\n");
    Ok(())
}

fn render_metrics(output: &mut String, metrics: &[Metric]) {
    for metric in metrics {
        let severity_indicator = match metric.severity {
            MetricSeverity::Success => "✓",
            MetricSeverity::Warning => "⚠",
            MetricSeverity::Error => "✗",
            MetricSeverity::Info => "ℹ",
            MetricSeverity::Normal => "●",
        };

        output.push_str(&format!(
            "  {} {}: {}\n",
            severity_indicator, metric.label, metric.value
        ));

        if let Some(change) = &metric.change {
            let arrow = match change.direction {
                ChangeDirection::Increase => "↑",
                ChangeDirection::Decrease => "↓",
                ChangeDirection::NoChange => "→",
            };
            output.push_str(&format!(
                "     (from {} {} {})\n",
                change.from, arrow, change.to
            ));
        }
    }
}

fn render_table(output: &mut String, table: &Table) {
    if table.rows.is_empty() {
        output.push_str("  (empty)\n");
        return;
    }

    // Calculate column widths
    let mut widths: Vec<usize> = table
        .headers
        .iter()
        .map(|h| h.len())
        .collect();

    for row in &table.rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.value.len());
            }
        }
    }

    // Header
    output.push_str("  ");
    for (i, header) in table.headers.iter().enumerate() {
        output.push_str(&format!("{:width$}", header, width = widths[i]));
        if i < table.headers.len() - 1 {
            output.push_str("  ");
        }
    }
    output.push_str("\n  ");
    for (i, width) in widths.iter().enumerate() {
        output.push_str(&"-".repeat(*width));
        if i < widths.len() - 1 {
            output.push_str("  ");
        }
    }
    output.push_str("\n");

    // Rows
    for row in &table.rows {
        output.push_str("  ");
        for (i, cell) in row.iter().enumerate() {
            let style_prefix = match cell.style {
                CellStyle::Success => "✓ ",
                CellStyle::Warning => "⚠ ",
                CellStyle::Error => "✗ ",
                CellStyle::Highlight => "▶ ",
                CellStyle::Muted => "",
                CellStyle::Normal => "",
            };

            let value = format!("{}{}", style_prefix, cell.value);
            if i < widths.len() {
                output.push_str(&format!("{:width$}", value, width = widths[i]));
            }
            if i < row.len() - 1 {
                output.push_str("  ");
            }
        }
        output.push_str("\n");
    }
}

fn render_key_value_list(output: &mut String, items: &[(String, String)]) {
    let max_key_width = items
        .iter()
        .map(|(k, _)| k.len())
        .max()
        .unwrap_or(0);

    for (key, value) in items {
        output.push_str(&format!(
            "  {:width$}: {}\n",
            key,
            value,
            width = max_key_width
        ));
    }
}

fn render_bullet_list(output: &mut String, items: &[String]) {
    for item in items {
        output.push_str(&format!("  ▶ {}\n", item));
    }
}

fn render_chart(output: &mut String, chart: &ChartData) {
    output.push_str(&format!("  {} ({})\n", chart.title, format_chart_type(&chart.chart_type)));

    // Simple text representation of chart data
    let max_label_width = chart.labels.iter().map(|l| l.len()).max().unwrap_or(0);

    for (i, label) in chart.labels.iter().enumerate() {
        output.push_str(&format!("    {:width$}: ", label, width = max_label_width));

        for (j, dataset) in chart.datasets.iter().enumerate() {
            if let Some(value) = dataset.data.get(i) {
                output.push_str(&format!("{}", value));
                if j < chart.datasets.len() - 1 {
                    output.push_str(" | ");
                }
            }
        }
        output.push_str("\n");
    }
}

fn render_text_content(output: &mut String, text: &str) {
    for line in text.lines() {
        output.push_str(&format!("  {}\n", line));
    }
}

fn format_chart_type(chart_type: &crate::report::core::ChartType) -> &str {
    match chart_type {
        crate::report::core::ChartType::Bar => "Bar Chart",
        crate::report::core::ChartType::Line => "Line Chart",
        crate::report::core::ChartType::Pie => "Pie Chart",
        crate::report::core::ChartType::Doughnut => "Doughnut Chart",
    }
}
