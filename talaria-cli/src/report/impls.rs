#![allow(dead_code)]

/// Reporter trait implementations for various output formats
use super::traits::{InteractiveOptions, InteractiveReporter, ReportData, ReportFormat, Reporter};
use anyhow::Result;
use serde_json;
use std::fs;
use std::path::Path;

/// JSON format reporter
pub struct JsonReporter {
    pretty: bool,
}

impl JsonReporter {
    pub fn new(pretty: bool) -> Self {
        Self { pretty }
    }
}

impl Reporter for JsonReporter {
    fn generate(&self, data: &ReportData) -> Result<String> {
        if self.pretty {
            Ok(serde_json::to_string_pretty(data)?)
        } else {
            Ok(serde_json::to_string(data)?)
        }
    }

    fn format(&self) -> ReportFormat {
        ReportFormat::Json
    }

    fn export(&self, content: &str, output: &Path) -> Result<()> {
        fs::write(output, content)?;
        Ok(())
    }

    fn file_extension(&self) -> &str {
        "json"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    fn name(&self) -> &str {
        "JSON Reporter"
    }
}

/// Markdown format reporter
pub struct MarkdownReporter {
    include_toc: bool,
    include_badges: bool,
}

impl MarkdownReporter {
    pub fn new() -> Self {
        Self {
            include_toc: true,
            include_badges: true,
        }
    }

    pub fn with_options(include_toc: bool, include_badges: bool) -> Self {
        Self {
            include_toc,
            include_badges,
        }
    }

    fn generate_toc(&self, data: &ReportData) -> String {
        let mut toc = String::from("## Table of Contents\n\n");

        for (i, section) in data.sections.iter().enumerate() {
            toc.push_str(&format!(
                "{}. [{}](#section-{})\n",
                i + 1,
                section.title,
                i + 1
            ));
        }

        toc.push('\n');
        toc
    }

    fn generate_badges(&self, data: &ReportData) -> String {
        let mut badges = String::new();

        let stats = &data.statistics;
        // Coverage badge
        if let Some(crate::report::traits::StatValue::Float(coverage)) =
            stats.custom_stats.get("coverage")
        {
            let coverage_pct = coverage * 100.0;
            let color = if coverage_pct > 90.0 {
                "green"
            } else if coverage_pct > 70.0 {
                "yellow"
            } else {
                "red"
            };
            badges.push_str(&format!(
                "![Coverage](https://img.shields.io/badge/coverage-{:.1}%25-{})\n",
                coverage_pct, color
            ));
        }

        // Status badge
        if let Some(crate::report::traits::StatValue::String(status_str)) =
            stats.custom_stats.get("status")
        {
            let color = if status_str == "valid" {
                "green"
            } else {
                "red"
            };
            badges.push_str(&format!(
                "![Status](https://img.shields.io/badge/status-{}-{})\n",
                status_str, color
            ));
        }

        if !badges.is_empty() {
            badges.push('\n');
        }

        badges
    }
}

impl Reporter for MarkdownReporter {
    fn generate(&self, data: &ReportData) -> Result<String> {
        let mut output = String::new();

        // Title
        output.push_str(&format!("# {}\n\n", data.title));

        // Description from metadata
        if let Some(desc) = data.metadata.get("description") {
            output.push_str(&format!("{}\n\n", desc));
        }

        // Badges
        if self.include_badges {
            output.push_str(&self.generate_badges(data));
        }

        // Table of contents
        if self.include_toc && data.sections.len() > 2 {
            output.push_str(&self.generate_toc(data));
        }

        // Sections
        for (i, section) in data.sections.iter().enumerate() {
            output.push_str(&format!("## {} {{#section-{}}}\n\n", section.title, i + 1));

            // Section content
            match &section.content {
                crate::report::traits::SectionContent::Text(text) => {
                    output.push_str(&format!("{}\n\n", text));
                }
                crate::report::traits::SectionContent::Table(table) => {
                    // Table headers
                    output.push('|');
                    for header in &table.headers {
                        output.push_str(&format!(" {} |", header));
                    }
                    output.push_str("\n|");
                    for _ in &table.headers {
                        output.push_str(" --- |");
                    }
                    output.push('\n');

                    // Table rows
                    for row in &table.rows {
                        output.push('|');
                        for cell in row {
                            output.push_str(&format!(" {} |", cell));
                        }
                        output.push('\n');
                    }
                    output.push('\n');
                }
                crate::report::traits::SectionContent::Code(code) => {
                    output.push_str(&format!("```{}\n{}\n```\n\n", code.language, code.code));
                }
                crate::report::traits::SectionContent::List(items) => {
                    for item in items {
                        output.push_str(&format!("- {}\n", item));
                    }
                    output.push('\n');
                }
                crate::report::traits::SectionContent::Mixed(contents) => {
                    for content in contents {
                        match content {
                            crate::report::traits::SectionContent::Text(text) => {
                                output.push_str(&format!("{}\n", text))
                            }
                            _ => {} // Handle other types as needed
                        }
                    }
                    output.push('\n');
                }
                crate::report::traits::SectionContent::Chart(_) => {
                    output.push_str("*Chart visualization not supported in Markdown*\n\n");
                }
            }
        }

        // Footer from metadata
        if let Some(footer) = data.metadata.get("footer") {
            output.push_str("---\n\n");
            output.push_str(&format!("{}\n", footer));
        }

        Ok(output)
    }

    fn format(&self) -> ReportFormat {
        ReportFormat::Markdown
    }

    fn export(&self, content: &str, output: &Path) -> Result<()> {
        fs::write(output, content)?;
        Ok(())
    }

    fn file_extension(&self) -> &str {
        "md"
    }

    fn mime_type(&self) -> &str {
        "text/markdown"
    }

    fn name(&self) -> &str {
        "Markdown Reporter"
    }
}

/// HTML format reporter with interactive features
pub struct HtmlReporter {
    include_css: bool,
    include_js: bool,
    theme: String,
}

impl HtmlReporter {
    pub fn new() -> Self {
        Self {
            include_css: true,
            include_js: true,
            theme: "light".to_string(),
        }
    }

    pub fn with_theme(theme: String) -> Self {
        Self {
            include_css: true,
            include_js: true,
            theme,
        }
    }

    fn generate_css(&self) -> &str {
        match self.theme.as_str() {
            "dark" => include_str!("../assets/dark-theme.css"),
            _ => include_str!("../assets/light-theme.css"),
        }
    }

    fn generate_js(&self) -> &str {
        include_str!("../assets/report-interactive.js")
    }
}

impl Reporter for HtmlReporter {
    fn generate(&self, data: &ReportData) -> Result<String> {
        let mut html = String::from("<!DOCTYPE html>\n<html>\n<head>\n");
        html.push_str(&format!("<title>{}</title>\n", data.title));
        html.push_str("<meta charset=\"UTF-8\">\n");
        html.push_str(
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n",
        );

        if self.include_css {
            html.push_str("<style>\n");
            html.push_str(self.generate_css());
            html.push_str("</style>\n");
        }

        html.push_str("</head>\n<body>\n");
        html.push_str(&format!("<h1>{}</h1>\n", data.title));

        if let Some(desc) = data.metadata.get("description") {
            html.push_str(&format!(
                "<p class=\"description\">{}</p>\n",
                html_escape(desc)
            ));
        }

        // Generate sections
        for section in &data.sections {
            html.push_str(&format!("<section>\n<h2>{}</h2>\n", section.title));

            match &section.content {
                crate::report::traits::SectionContent::Text(text) => {
                    html.push_str(&format!("<p>{}</p>\n", text));
                }
                crate::report::traits::SectionContent::Table(table) => {
                    html.push_str("<table>\n<thead>\n<tr>\n");
                    for header in &table.headers {
                        html.push_str(&format!("<th>{}</th>\n", header));
                    }
                    html.push_str("</tr>\n</thead>\n<tbody>\n");

                    for row in &table.rows {
                        html.push_str("<tr>\n");
                        for cell in row {
                            html.push_str(&format!("<td>{}</td>\n", cell));
                        }
                        html.push_str("</tr>\n");
                    }
                    html.push_str("</tbody>\n</table>\n");
                }
                crate::report::traits::SectionContent::Code(code) => {
                    html.push_str(&format!(
                        "<pre><code class=\"language-{}\">{}</code></pre>\n",
                        code.language,
                        html_escape(&code.code)
                    ));
                }
                crate::report::traits::SectionContent::Chart(chart) => {
                    html.push_str(&format!(
                        "<div class=\"chart\" data-type=\"{:?}\">\n",
                        chart.chart_type
                    ));
                    html.push_str(&format!(
                        "<canvas id=\"chart-{}\"></canvas>\n",
                        section.title.replace(' ', "_")
                    ));
                    html.push_str("</div>\n");
                }
                crate::report::traits::SectionContent::List(items) => {
                    html.push_str("<ul>\n");
                    for item in items {
                        html.push_str(&format!("<li>{}</li>\n", html_escape(item)));
                    }
                    html.push_str("</ul>\n");
                }
                crate::report::traits::SectionContent::Mixed(contents) => {
                    for content in contents {
                        match content {
                            crate::report::traits::SectionContent::Text(text) => {
                                html.push_str(&format!("<p>{}</p>\n", html_escape(text)));
                            }
                            _ => {} // Handle other types as needed
                        }
                    }
                }
            }

            html.push_str("</section>\n");
        }

        if let Some(footer) = data.metadata.get("footer") {
            html.push_str(&format!("<footer>{}</footer>\n", html_escape(footer)));
        }

        if self.include_js {
            html.push_str("<script>\n");
            html.push_str(self.generate_js());
            html.push_str("</script>\n");
        }

        html.push_str("</body>\n</html>");

        Ok(html)
    }

    fn format(&self) -> ReportFormat {
        ReportFormat::Html
    }

    fn export(&self, content: &str, output: &Path) -> Result<()> {
        fs::write(output, content)?;
        Ok(())
    }

    fn file_extension(&self) -> &str {
        "html"
    }

    fn mime_type(&self) -> &str {
        "text/html"
    }

    fn supports_embedded_resources(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "HTML Reporter"
    }
}

impl InteractiveReporter for HtmlReporter {
    fn generate_interactive(
        &self,
        data: &ReportData,
        options: &InteractiveOptions,
    ) -> Result<String> {
        // Generate base HTML
        let mut html = self.generate(data)?;

        // Add interactive features based on options
        if options.enable_search {
            // Add search functionality
            let search_js = r#"
                function searchReport(query) {
                    // Search implementation
                }
            "#;
            html = html.replace("</script>", &format!("{}</script>", search_js));
        }

        if options.enable_export {
            // Add export buttons
            let export_html = r#"
                <div class="export-buttons">
                    <button onclick="exportToPDF()">Export PDF</button>
                    <button onclick="exportToCSV()">Export CSV</button>
                </div>
            "#;
            html = html.replace("<body>", &format!("<body>\n{}", export_html));
        }

        Ok(html)
    }

    fn add_chart(
        &mut self,
        chart_type: crate::report::traits::ChartType,
        _data: crate::report::traits::ChartData,
    ) -> Result<String> {
        let chart_id = format!("chart_{}", uuid::Uuid::new_v4());
        let chart_html = format!(
            "<div id='{}' class='chart' data-type='{:?}'></div>",
            chart_id, chart_type
        );
        Ok(chart_html)
    }

    fn add_table(
        &mut self,
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
        sortable: bool,
    ) -> Result<String> {
        let table_id = format!("table_{}", uuid::Uuid::new_v4());
        let mut html = format!(
            "<table id='{}' class='data-table{}'>\n",
            table_id,
            if sortable { " sortable" } else { "" }
        );

        // Headers
        html.push_str("<thead><tr>");
        for header in headers {
            html.push_str(&format!("<th>{}</th>", header));
        }
        html.push_str("</tr></thead>\n");

        // Rows
        html.push_str("<tbody>");
        for row in rows {
            html.push_str("<tr>");
            for cell in row {
                html.push_str(&format!("<td>{}</td>", cell));
            }
            html.push_str("</tr>\n");
        }
        html.push_str("</tbody></table>\n");

        Ok(html)
    }

    fn required_libraries(&self) -> Vec<String> {
        vec![
            "https://cdn.jsdelivr.net/npm/chart.js".to_string(),
            "https://cdn.jsdelivr.net/npm/tablesort".to_string(),
        ]
    }
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Factory function to create a reporter based on file extension
pub fn create_reporter_from_path(path: &Path) -> Box<dyn Reporter> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => Box::new(JsonReporter::new(true)),
        Some("md") | Some("markdown") => Box::new(MarkdownReporter::new()),
        Some("html") | Some("htm") => Box::new(HtmlReporter::new()),
        _ => Box::new(JsonReporter::new(true)), // Default to JSON
    }
}
