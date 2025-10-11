/// Generic HTML renderer for Report type
use crate::report::core::{
    Cell, CellStyle, ChangeDirection, ChartData, Metric, MetricSeverity, Report, Section,
    SectionContent, Table,
};
use anyhow::Result;

/// Render a Report to HTML format
pub fn render_html(report: &Report) -> Result<String> {
    let mut html = String::new();

    // HTML header with embedded CSS
    html.push_str(&format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            line-height: 1.6;
            color: #333;
            max-width: 1200px;
            margin: 0 auto;
            padding: 20px;
            background-color: #f5f5f5;
        }}
        .container {{
            background-color: white;
            border-radius: 8px;
            padding: 30px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        h1 {{
            color: #2c3e50;
            border-bottom: 3px solid #3498db;
            padding-bottom: 10px;
            margin-bottom: 20px;
        }}
        h2 {{
            color: #34495e;
            margin-top: 30px;
            margin-bottom: 15px;
            border-left: 4px solid #3498db;
            padding-left: 10px;
        }}
        .metadata {{
            background-color: #ecf0f1;
            border-radius: 4px;
            padding: 15px;
            margin-bottom: 20px;
        }}
        .metadata-item {{
            margin: 5px 0;
        }}
        .metadata-label {{
            font-weight: bold;
            color: #7f8c8d;
        }}
        .metrics {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 15px;
            margin: 20px 0;
        }}
        .metric {{
            background-color: #f8f9fa;
            border-radius: 6px;
            padding: 15px;
            border-left: 4px solid #95a5a6;
        }}
        .metric.success {{
            border-left-color: #27ae60;
        }}
        .metric.warning {{
            border-left-color: #f39c12;
        }}
        .metric.error {{
            border-left-color: #e74c3c;
        }}
        .metric.info {{
            border-left-color: #3498db;
        }}
        .metric-label {{
            font-size: 0.9em;
            color: #7f8c8d;
            margin-bottom: 5px;
        }}
        .metric-value {{
            font-size: 1.5em;
            font-weight: bold;
            color: #2c3e50;
        }}
        .metric-change {{
            font-size: 0.85em;
            margin-top: 5px;
        }}
        .change-increase {{
            color: #27ae60;
        }}
        .change-decrease {{
            color: #e74c3c;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
            margin: 15px 0;
            background-color: white;
        }}
        th {{
            background-color: #34495e;
            color: white;
            padding: 12px;
            text-align: left;
            font-weight: 600;
        }}
        td {{
            padding: 10px 12px;
            border-bottom: 1px solid #ecf0f1;
        }}
        tr:hover {{
            background-color: #f8f9fa;
        }}
        .cell-success {{
            color: #27ae60;
            font-weight: 500;
        }}
        .cell-warning {{
            color: #f39c12;
            font-weight: 500;
        }}
        .cell-error {{
            color: #e74c3c;
            font-weight: 500;
        }}
        .cell-highlight {{
            background-color: #fff3cd;
            font-weight: 500;
        }}
        .cell-muted {{
            color: #95a5a6;
            font-style: italic;
        }}
        .key-value-list {{
            background-color: #f8f9fa;
            border-radius: 4px;
            padding: 15px;
            margin: 15px 0;
        }}
        .key-value-item {{
            display: flex;
            padding: 8px 0;
            border-bottom: 1px solid #ecf0f1;
        }}
        .key-value-item:last-child {{
            border-bottom: none;
        }}
        .key-value-key {{
            font-weight: 600;
            color: #34495e;
            min-width: 200px;
        }}
        .key-value-value {{
            color: #2c3e50;
        }}
        ul.bullet-list {{
            list-style-type: none;
            padding-left: 0;
        }}
        ul.bullet-list li {{
            padding: 8px 0;
            padding-left: 20px;
            position: relative;
        }}
        ul.bullet-list li:before {{
            content: "▶";
            position: absolute;
            left: 0;
            color: #3498db;
        }}
        .text-section {{
            background-color: #f8f9fa;
            border-radius: 4px;
            padding: 15px;
            margin: 15px 0;
            white-space: pre-wrap;
            font-family: "Courier New", monospace;
        }}
        .timestamp {{
            color: #7f8c8d;
            font-size: 0.9em;
            margin-top: 20px;
            text-align: right;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>{}</h1>
"#,
        report.title, report.title
    ));

    // Metadata section
    if !report.metadata.is_empty() {
        html.push_str("<div class=\"metadata\">\n");
        html.push_str(&format!(
            "<div class=\"metadata-item\"><span class=\"metadata-label\">Command:</span> {}</div>\n",
            report.command
        ));
        for (key, value) in &report.metadata {
            html.push_str(&format!(
                "<div class=\"metadata-item\"><span class=\"metadata-label\">{}:</span> {}</div>\n",
                key, value
            ));
        }
        html.push_str("</div>\n");
    }

    // Render each section
    for section in &report.sections {
        render_section(&mut html, section)?;
    }

    // Timestamp
    html.push_str(&format!(
        "<div class=\"timestamp\">Generated: {}</div>\n",
        report.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
    ));

    html.push_str("    </div>\n</body>\n</html>");

    Ok(html)
}

fn render_section(html: &mut String, section: &Section) -> Result<()> {
    html.push_str(&format!("<h2>{}</h2>\n", section.title));

    match &section.content {
        SectionContent::Metrics(metrics) => render_metrics(html, metrics),
        SectionContent::Table(table) => render_table(html, table),
        SectionContent::KeyValueList(items) => render_key_value_list(html, items),
        SectionContent::BulletList(items) => render_bullet_list(html, items),
        SectionContent::Chart(chart) => render_chart(html, chart),
        SectionContent::Text(text) => render_text(html, text),
    }

    Ok(())
}

fn render_metrics(html: &mut String, metrics: &[Metric]) {
    html.push_str("<div class=\"metrics\">\n");

    for metric in metrics {
        let severity_class = match metric.severity {
            MetricSeverity::Success => "success",
            MetricSeverity::Warning => "warning",
            MetricSeverity::Error => "error",
            MetricSeverity::Info => "info",
            MetricSeverity::Normal => "",
        };

        html.push_str(&format!("<div class=\"metric {}\">\n", severity_class));
        html.push_str(&format!(
            "  <div class=\"metric-label\">{}</div>\n",
            metric.label
        ));
        html.push_str(&format!(
            "  <div class=\"metric-value\">{}</div>\n",
            metric.value
        ));

        if let Some(change) = &metric.change {
            let change_class = match change.direction {
                ChangeDirection::Increase => "change-increase",
                ChangeDirection::Decrease => "change-decrease",
                ChangeDirection::NoChange => "",
            };
            let arrow = match change.direction {
                ChangeDirection::Increase => "↑",
                ChangeDirection::Decrease => "↓",
                ChangeDirection::NoChange => "→",
            };
            html.push_str(&format!(
                "  <div class=\"metric-change {}\">from {} {} {}</div>\n",
                change_class, change.from, arrow, change.to
            ));
        }

        html.push_str("</div>\n");
    }

    html.push_str("</div>\n");
}

fn render_table(html: &mut String, table: &Table) {
    html.push_str("<table>\n");

    // Headers
    html.push_str("  <thead>\n    <tr>\n");
    for header in &table.headers {
        html.push_str(&format!("      <th>{}</th>\n", header));
    }
    html.push_str("    </tr>\n  </thead>\n");

    // Rows
    html.push_str("  <tbody>\n");
    for row in &table.rows {
        html.push_str("    <tr>\n");
        for cell in row {
            render_cell(html, cell);
        }
        html.push_str("    </tr>\n");
    }
    html.push_str("  </tbody>\n");

    html.push_str("</table>\n");
}

fn render_cell(html: &mut String, cell: &Cell) {
    let class = match cell.style {
        CellStyle::Success => " class=\"cell-success\"",
        CellStyle::Warning => " class=\"cell-warning\"",
        CellStyle::Error => " class=\"cell-error\"",
        CellStyle::Highlight => " class=\"cell-highlight\"",
        CellStyle::Muted => " class=\"cell-muted\"",
        CellStyle::Normal => "",
    };

    html.push_str(&format!("      <td{}>{}</td>\n", class, cell.value));
}

fn render_key_value_list(html: &mut String, items: &[(String, String)]) {
    html.push_str("<div class=\"key-value-list\">\n");

    for (key, value) in items {
        html.push_str("  <div class=\"key-value-item\">\n");
        html.push_str(&format!("    <div class=\"key-value-key\">{}</div>\n", key));
        html.push_str(&format!(
            "    <div class=\"key-value-value\">{}</div>\n",
            value
        ));
        html.push_str("  </div>\n");
    }

    html.push_str("</div>\n");
}

fn render_bullet_list(html: &mut String, items: &[String]) {
    html.push_str("<ul class=\"bullet-list\">\n");

    for item in items {
        html.push_str(&format!("  <li>{}</li>\n", item));
    }

    html.push_str("</ul>\n");
}

fn render_chart(html: &mut String, chart: &ChartData) {
    // For now, render chart data as a simple table
    // In the future, this could use Chart.js or similar for actual charts
    html.push_str("<div class=\"chart-placeholder\">\n");
    html.push_str(&format!("<h3>{}</h3>\n", chart.title));
    html.push_str("<table>\n");

    // Headers
    html.push_str("  <thead>\n    <tr>\n");
    html.push_str("      <th>Label</th>\n");
    for dataset in &chart.datasets {
        html.push_str(&format!("      <th>{}</th>\n", dataset.label));
    }
    html.push_str("    </tr>\n  </thead>\n");

    // Data rows
    html.push_str("  <tbody>\n");
    for (i, label) in chart.labels.iter().enumerate() {
        html.push_str("    <tr>\n");
        html.push_str(&format!("      <td>{}</td>\n", label));
        for dataset in &chart.datasets {
            let value = dataset
                .data
                .get(i)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            html.push_str(&format!("      <td>{}</td>\n", value));
        }
        html.push_str("    </tr>\n");
    }
    html.push_str("  </tbody>\n");

    html.push_str("</table>\n");
    html.push_str("</div>\n");
}

fn render_text(html: &mut String, text: &str) {
    html.push_str(&format!("<div class=\"text-section\">{}</div>\n", text));
}
