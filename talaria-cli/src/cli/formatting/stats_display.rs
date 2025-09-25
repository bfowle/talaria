#![allow(dead_code)]

use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table};
use std::fmt::Display;

pub struct StatsTable {
    table: Table,
    has_sections: bool,
}

impl StatsTable {
    pub fn new(title: &str) -> Self {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_content_arrangement(ContentArrangement::Dynamic);

        // Add title as header
        table.set_header(vec![
            Cell::new(title)
                .add_attribute(Attribute::Bold)
                .fg(Color::Green),
            Cell::new(""),
        ]);

        Self {
            table,
            has_sections: false,
        }
    }

    pub fn add_section(&mut self, name: &str) {
        // Add a section separator if not the first section
        if self.has_sections {
            self.table.add_row(vec!["", ""]);
        }

        self.table.add_row(vec![
            Cell::new(name.to_uppercase())
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new(""),
        ]);

        self.has_sections = true;
    }

    pub fn add_metric(&mut self, name: &str, value: impl Display, percentage: Option<f64>) {
        let value_str = if let Some(pct) = percentage {
            format!("{} ({:.1}%)", value, pct)
        } else {
            value.to_string()
        };

        self.table.add_row(vec![
            Cell::new(format!("  {}", name)),
            Cell::new(value_str).add_attribute(Attribute::Bold),
        ]);
    }

    pub fn add_metric_with_unit(
        &mut self,
        name: &str,
        value: u64,
        unit: &str,
        percentage: Option<f64>,
    ) {
        let formatted_value = format_number(value);
        let value_str = if let Some(pct) = percentage {
            format!("{} {} ({:.1}%)", formatted_value, unit, pct)
        } else {
            format!("{} {}", formatted_value, unit)
        };

        self.table.add_row(vec![
            Cell::new(format!("  {}", name)),
            Cell::new(value_str).add_attribute(Attribute::Bold),
        ]);
    }

    pub fn add_file_size(&mut self, name: &str, bytes: u64, percentage: Option<f64>) {
        use talaria_utils::display::format::format_bytes;

        let value_str = if let Some(pct) = percentage {
            format!("{} ({:.1}%)", format_bytes(bytes), pct)
        } else {
            format_bytes(bytes)
        };

        self.table.add_row(vec![
            Cell::new(format!("  {}", name)),
            Cell::new(value_str).add_attribute(Attribute::Bold),
        ]);
    }

    pub fn render(&self) -> String {
        self.table.to_string()
    }
}

/// Format a number with comma separators
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;

    for c in s.chars().rev() {
        if count == 3 {
            result.push(',');
            count = 0;
        }
        result.push(c);
        count += 1;
    }

    result.chars().rev().collect()
}

/// Create a statistics table for reduction operations
pub fn create_reduction_stats(
    original_count: usize,
    reference_count: usize,
    child_count: usize,
    input_size: u64,
    output_size: u64,
    avg_deltas: f64,
) -> String {
    let mut table = StatsTable::new("Reduction Statistics");

    // Sequences section
    table.add_section("Sequences");
    table.add_metric_with_unit("Original", original_count as u64, "", None);

    let ref_pct = (reference_count as f64 / original_count as f64) * 100.0;
    table.add_metric_with_unit("References", reference_count as u64, "", Some(ref_pct));

    let child_pct = (child_count as f64 / original_count as f64) * 100.0;
    table.add_metric_with_unit("Children", child_count as u64, "", Some(child_pct));

    let total_output = reference_count + child_count;
    let coverage_pct = (total_output as f64 / original_count as f64) * 100.0;
    table.add_metric_with_unit("Coverage", total_output as u64, "", Some(coverage_pct));

    // File sizes section
    if input_size > 0 {
        table.add_section("File Sizes");
        table.add_file_size("Original", input_size, None);

        if output_size > 0 {
            table.add_file_size("Reduced", output_size, None);

            let reduction_bytes = input_size.saturating_sub(output_size);
            let reduction_pct = (reduction_bytes as f64 / input_size as f64) * 100.0;
            table.add_file_size("Reduction", reduction_bytes, Some(reduction_pct));
        }
    }

    // Delta metrics section
    if child_count > 0 {
        table.add_section("Delta Metrics");
        table.add_metric("Avg deltas/child", format!("{:.1}", avg_deltas), None);

        let total_delta_ops = (avg_deltas * child_count as f64) as u64;
        table.add_metric_with_unit("Total delta operations", total_delta_ops, "", None);
    }

    table.render()
}

/// Create a statistics table for validation operations
pub fn create_validation_stats(
    total_sequences: usize,
    reference_count: usize,
    child_count: usize,
    covered_sequences: usize,
    sequence_coverage: f64,
    covered_taxa: usize,
    _total_taxa: usize,
    taxonomic_coverage: f64,
    original_file_size: u64,
    reduced_file_size: u64,
    avg_delta_size: f64,
) -> String {
    let mut table = StatsTable::new("Validation Results");

    // Sequences section
    table.add_section("Sequences");
    table.add_metric_with_unit("Original", total_sequences as u64, "", None);

    let ref_pct = (reference_count as f64 / total_sequences as f64) * 100.0;
    table.add_metric_with_unit("References", reference_count as u64, "", Some(ref_pct));

    let child_pct = (child_count as f64 / total_sequences as f64) * 100.0;
    table.add_metric_with_unit("Children", child_count as u64, "", Some(child_pct));

    table.add_metric_with_unit(
        "Sequence coverage",
        covered_sequences as u64,
        "",
        Some(sequence_coverage * 100.0),
    );
    table.add_metric_with_unit(
        "Taxonomic coverage",
        covered_taxa as u64,
        "",
        Some(taxonomic_coverage * 100.0),
    );

    // File sizes section
    table.add_section("File Sizes");
    table.add_file_size("Original", original_file_size, None);
    table.add_file_size("Reduced", reduced_file_size, None);

    let reduction_bytes = original_file_size.saturating_sub(reduced_file_size);
    let file_reduction_pct = (reduction_bytes as f64 / original_file_size as f64) * 100.0;
    table.add_file_size("Reduction", reduction_bytes, Some(file_reduction_pct));

    // Delta metrics section
    table.add_section("Delta Metrics");
    table.add_metric("Avg delta size", format!("{:.1}", avg_delta_size), None);

    table.render()
}
