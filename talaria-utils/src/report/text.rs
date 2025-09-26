#![allow(dead_code)]

use super::ComparisonResult;
use super::ReportOptions;
use anyhow::Result;
use comfy_table::{modifiers, presets, Attribute, Cell, Color, ContentArrangement, Table};
use std::fmt::Write;

pub fn generate_text_report(result: &ComparisonResult, options: &ReportOptions) -> Result<String> {
    let mut output = String::new();

    // Header
    writeln!(&mut output, "\n● Database Comparison Report")?;
    writeln!(&mut output, "═══════════════════════════════")?;
    writeln!(&mut output)?;

    // Database info table
    let mut info_table = Table::new();
    info_table
        .load_preset(presets::UTF8_FULL)
        .apply_modifier(modifiers::UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    info_table.set_header(vec![
        Cell::new("Database").add_attribute(Attribute::Bold),
        Cell::new("Path").add_attribute(Attribute::Bold),
        Cell::new("Sequences").add_attribute(Attribute::Bold),
    ]);

    info_table.add_row(vec![
        Cell::new("Old").fg(Color::DarkGrey),
        Cell::new(result.old_path.display().to_string()),
        Cell::new(format_number(result.old_count)),
    ]);
    info_table.add_row(vec![
        Cell::new("New").fg(Color::Blue),
        Cell::new(result.new_path.display().to_string()),
        Cell::new(format_number(result.new_count)),
    ]);

    writeln!(&mut output, "{}", info_table)?;
    writeln!(&mut output)?;

    // Summary table
    writeln!(&mut output, "► Summary of Changes")?;
    writeln!(&mut output, "─────────────────────")?;

    let mut summary_table = Table::new();
    summary_table
        .load_preset(presets::UTF8_FULL)
        .apply_modifier(modifiers::UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    summary_table.set_header(vec![
        Cell::new("Change Type").add_attribute(Attribute::Bold),
        Cell::new("Count").add_attribute(Attribute::Bold),
        Cell::new("Percentage").add_attribute(Attribute::Bold),
    ]);

    let total = result.old_count.max(result.new_count) as f64;

    summary_table.add_row(vec![
        Cell::new("+ Added").fg(Color::Green),
        Cell::new(format_number(result.added.len())).fg(Color::Green),
        Cell::new(format!("{:.1}%", result.added.len() as f64 / total * 100.0)),
    ]);
    summary_table.add_row(vec![
        Cell::new("- Removed").fg(Color::Red),
        Cell::new(format_number(result.removed.len())).fg(Color::Red),
        Cell::new(format!(
            "{:.1}%",
            result.removed.len() as f64 / total * 100.0
        )),
    ]);
    summary_table.add_row(vec![
        Cell::new("~ Modified").fg(Color::Yellow),
        Cell::new(format_number(result.modified.len())).fg(Color::Yellow),
        Cell::new(format!(
            "{:.1}%",
            result.modified.len() as f64 / total * 100.0
        )),
    ]);
    summary_table.add_row(vec![
        Cell::new("↻ Renamed").fg(Color::Cyan),
        Cell::new(format_number(result.renamed.len())).fg(Color::Cyan),
        Cell::new(format!(
            "{:.1}%",
            result.renamed.len() as f64 / total * 100.0
        )),
    ]);
    summary_table.add_row(vec![
        Cell::new("✓ Unchanged").fg(Color::DarkGrey),
        Cell::new(format_number(result.unchanged_count)),
        Cell::new(format!(
            "{:.1}%",
            result.unchanged_count as f64 / total * 100.0
        )),
    ]);

    writeln!(&mut output, "{}", summary_table)?;
    writeln!(&mut output)?;

    // Statistics table
    writeln!(&mut output, "● Database Statistics")?;
    writeln!(&mut output, "─────────────────────")?;

    let stats = &result.statistics;
    let mut stats_table = Table::new();
    stats_table
        .load_preset(presets::UTF8_FULL)
        .apply_modifier(modifiers::UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    stats_table.set_header(vec![
        Cell::new("Metric").add_attribute(Attribute::Bold),
        Cell::new("Old").add_attribute(Attribute::Bold),
        Cell::new("New").add_attribute(Attribute::Bold),
        Cell::new("Change").add_attribute(Attribute::Bold),
    ]);

    let length_change = stats.new_total_length as i64 - stats.old_total_length as i64;
    let avg_change = stats.new_avg_length as i64 - stats.old_avg_length as i64;

    stats_table.add_row(vec![
        Cell::new("Total Length"),
        Cell::new(format_number(stats.old_total_length)),
        Cell::new(format_number(stats.new_total_length)),
        format_change(length_change),
    ]);

    stats_table.add_row(vec![
        Cell::new("Average Length"),
        Cell::new(format!("{:.1}", stats.old_avg_length)),
        Cell::new(format!("{:.1}", stats.new_avg_length)),
        format_change(avg_change),
    ]);

    writeln!(&mut output, "{}", stats_table)?;
    writeln!(&mut output)?;

    // Taxonomic changes
    if options.include_taxonomy {
        writeln!(&mut output, "Taxonomic Changes")?;
        writeln!(&mut output, "-----------------")?;
        writeln!(
            &mut output,
            "Unique taxa: {} → {} ({:+})",
            stats.old_unique_taxa,
            stats.new_unique_taxa,
            stats.new_unique_taxa as i64 - stats.old_unique_taxa as i64
        )?;
        writeln!(&mut output, "- New taxa: {}", stats.added_taxa.len())?;
        writeln!(&mut output, "- Removed taxa: {}", stats.removed_taxa.len())?;
        writeln!(&mut output)?;
    }

    // Detailed changes
    if options.include_details {
        if !result.added.is_empty() {
            writeln!(&mut output, "➕ Added Sequences (Top 10)")?;
            writeln!(&mut output, "───────────────────────────")?;

            let mut added_table = Table::new();
            added_table
                .load_preset(presets::UTF8_FULL)
                .apply_modifier(modifiers::UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic);

            added_table.set_header(vec![
                Cell::new("ID").add_attribute(Attribute::Bold),
                Cell::new("Length").add_attribute(Attribute::Bold),
                Cell::new("Description").add_attribute(Attribute::Bold),
            ]);

            for seq in result.added.iter().take(10) {
                added_table.add_row(vec![
                    Cell::new(&seq.id).fg(Color::Green),
                    Cell::new(format_number(seq.length)),
                    Cell::new(seq.description.as_deref().unwrap_or("-")),
                ]);
            }

            if result.added.len() > 10 {
                added_table.add_row(vec![
                    Cell::new(format!("... and {} more", result.added.len() - 10))
                        .fg(Color::DarkGrey)
                        .add_attribute(Attribute::Italic),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }

            writeln!(&mut output, "{}", added_table)?;
            writeln!(&mut output)?;
        }

        if !result.removed.is_empty() {
            writeln!(&mut output, "➖ Removed Sequences (Top 10)")?;
            writeln!(&mut output, "────────────────────────────")?;

            let mut removed_table = Table::new();
            removed_table
                .load_preset(presets::UTF8_FULL)
                .apply_modifier(modifiers::UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic);

            removed_table.set_header(vec![
                Cell::new("ID").add_attribute(Attribute::Bold),
                Cell::new("Length").add_attribute(Attribute::Bold),
                Cell::new("Description").add_attribute(Attribute::Bold),
            ]);

            for seq in result.removed.iter().take(10) {
                removed_table.add_row(vec![
                    Cell::new(&seq.id).fg(Color::Red),
                    Cell::new(format_number(seq.length)),
                    Cell::new(seq.description.as_deref().unwrap_or("-")),
                ]);
            }

            if result.removed.len() > 10 {
                removed_table.add_row(vec![
                    Cell::new(format!("... and {} more", result.removed.len() - 10))
                        .fg(Color::DarkGrey)
                        .add_attribute(Attribute::Italic),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }

            writeln!(&mut output, "{}", removed_table)?;
            writeln!(&mut output)?;
        }

        if !result.modified.is_empty() {
            writeln!(&mut output, "✏️  Modified Sequences (Top 10)")?;
            writeln!(&mut output, "─────────────────────────────")?;

            let mut modified_table = Table::new();
            modified_table
                .load_preset(presets::UTF8_FULL)
                .apply_modifier(modifiers::UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic);

            modified_table.set_header(vec![
                Cell::new("ID").add_attribute(Attribute::Bold),
                Cell::new("Similarity").add_attribute(Attribute::Bold),
                Cell::new("Length Change").add_attribute(Attribute::Bold),
                Cell::new("Changes").add_attribute(Attribute::Bold),
            ]);

            for mod_seq in result.modified.iter().take(10) {
                let old_len = mod_seq.old.as_ref().map(|o| o.length).unwrap_or(0);
                let new_len = mod_seq.new.as_ref().map(|n| n.length).unwrap_or(0);
                let length_diff = new_len as i64 - old_len as i64;
                let changes_str = mod_seq
                    .changes
                    .as_ref()
                    .map(|changes| changes
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(", "))
                    .unwrap_or_else(|| "None".to_string());

                let old_data = mod_seq.old.as_ref();
                let new_data = mod_seq.new.as_ref();
                let old_id = old_data.map(|o| o.id.as_str()).unwrap_or("unknown");
                let old_len = old_data.map(|o| o.length).unwrap_or(0);
                let new_len = new_data.map(|n| n.length).unwrap_or(0);
                let similarity = mod_seq.similarity.unwrap_or(0.0);

                modified_table.add_row(vec![
                    Cell::new(old_id).fg(Color::Yellow),
                    Cell::new(format!("{:.1}%", similarity * 100.0)),
                    Cell::new(format!(
                        "{} → {} ({:+})",
                        format_number(old_len),
                        format_number(new_len),
                        length_diff
                    )),
                    Cell::new(changes_str),
                ]);
            }

            if result.modified.len() > 10 {
                modified_table.add_row(vec![
                    Cell::new(format!("... and {} more", result.modified.len() - 10))
                        .fg(Color::DarkGrey)
                        .add_attribute(Attribute::Italic),
                    Cell::new(""),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }

            writeln!(&mut output, "{}", modified_table)?;
            writeln!(&mut output)?;
        }
    }

    Ok(output)
}

/// Format a number with thousand separators
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;

    for c in s.chars().rev() {
        if count == 3 && !result.is_empty() {
            result.push(',');
            count = 0;
        }
        result.push(c);
        count += 1;
    }

    result.chars().rev().collect()
}

/// Format a change value with color
fn format_change(change: i64) -> Cell {
    if change > 0 {
        Cell::new(format!("+{}", format_number(change.unsigned_abs() as usize))).fg(Color::Green)
    } else if change < 0 {
        Cell::new(format!("-{}", format_number(change.unsigned_abs() as usize))).fg(Color::Red)
    } else {
        Cell::new("0").fg(Color::DarkGrey)
    }
}
