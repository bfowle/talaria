use crate::traits::renderable::{
    create_timeline, timeline_marker, DiffRenderable, EvolutionRenderable, TemporalRenderable,
};
use crate::traits::temporal::*;
use crate::output::{create_standard_table, format_number, header_cell, TreeNode};
use chrono::Datelike;
/// Implementations of TemporalRenderable trait for temporal query types
///
/// This module provides rich visualization for temporal query results
/// using the existing output utilities.
use comfy_table::{Cell, Table};

impl TemporalRenderable for TemporalSnapshot {
    fn render_tree(&self) -> Vec<TreeNode> {
        vec![TreeNode::new("Temporal Snapshot")
            .add_child(
                TreeNode::new("Coordinate")
                    .add_child(
                        TreeNode::new("Sequence Time").with_value(
                            self.coordinate
                                .sequence_time
                                .format("%Y-%m-%d %H:%M:%S UTC")
                                .to_string(),
                        ),
                    )
                    .add_child(
                        TreeNode::new("Taxonomy Time").with_value(
                            self.coordinate
                                .taxonomy_time
                                .format("%Y-%m-%d %H:%M:%S UTC")
                                .to_string(),
                        ),
                    ),
            )
            .add_child(
                TreeNode::new("Sequences")
                    .add_child(
                        TreeNode::new("Total").with_value(format_number(self.sequences.len())),
                    )
                    .add_child(
                        TreeNode::new("Unique Taxa")
                            .with_value(format_number(self.metadata.unique_taxa)),
                    ),
            )
            .add_child(
                TreeNode::new("Versions")
                    .add_child(
                        TreeNode::new("Sequence Version")
                            .with_value(self.sequence_version.version.clone()),
                    )
                    .add_child(
                        TreeNode::new("Taxonomy Version")
                            .with_value(self.taxonomy_version.version.clone()),
                    )
                    .add_child(
                        TreeNode::new("Taxonomy Source").with_value(self.taxonomy_version.source.to_string()),
                    ),
            )
            .add_child(
                TreeNode::new("Storage")
                    .add_child(
                        TreeNode::new("Chunks")
                            .with_value(format_number(self.metadata.total_chunks)),
                    )
                    .add_child(
                        TreeNode::new("Snapshot Hash").with_value(format!(
                            "{}...",
                            &self.metadata.snapshot_hash.to_hex()[..8]
                        )),
                    ),
            )]
    }

    fn render_table(&self) -> Table {
        let mut table = create_standard_table();

        table.set_header(vec![header_cell("Property"), header_cell("Value")]);

        table.add_row(vec![
            Cell::new("Sequence Time"),
            Cell::new(
                self
                    .coordinate
                    .sequence_time
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
            ),
        ]);

        table.add_row(vec![
            Cell::new("Taxonomy Time"),
            Cell::new(
                self
                    .coordinate
                    .taxonomy_time
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
            ),
        ]);

        table.add_row(vec![
            Cell::new("Total Sequences"),
            Cell::new(format_number(self.sequences.len())),
        ]);

        table.add_row(vec![
            Cell::new("Unique Taxa"),
            Cell::new(format_number(self.metadata.unique_taxa)),
        ]);

        table.add_row(vec![
            Cell::new("Chunks"),
            Cell::new(format_number(self.metadata.total_chunks)),
        ]);

        table.add_row(vec![
            Cell::new("Sequence Version"),
            Cell::new(&self.sequence_version.version),
        ]);

        table.add_row(vec![
            Cell::new("Taxonomy Version"),
            Cell::new(format!(
                "{} ({})",
                self.taxonomy_version.version, self.taxonomy_version.source
            )),
        ]);

        table
    }

    fn render_timeline(&self) -> String {
        // Not applicable for a single snapshot
        self.render_summary()
    }

    fn render_summary(&self) -> String {
        format!(
            "Snapshot at {} with {} sequences from {} taxa",
            self.coordinate.sequence_time.format("%Y-%m-%d"),
            format_number(self.sequences.len()),
            format_number(self.metadata.unique_taxa)
        )
    }
}

impl TemporalRenderable for TemporalJoinResult {
    fn render_tree(&self) -> Vec<TreeNode> {
        let mut root = TreeNode::new("Temporal Join Results")
            .add_child(
                TreeNode::new("Query")
                    .add_child(
                        TreeNode::new("Reference Date")
                            .with_value(self.query.reference_date.format("%Y-%m-%d").to_string()),
                    )
                    .add_child(
                        TreeNode::new("Comparison Date").with_value(
                            self.query
                                .comparison_date
                                .map(|d| d.format("%Y-%m-%d").to_string())
                                .unwrap_or_else(|| "Current".to_string()),
                        ),
                    ),
            )
            .add_child(
                TreeNode::new("Statistics")
                    .add_child(
                        TreeNode::new("Total Affected")
                            .with_value(format_number(self.total_affected)),
                    )
                    .add_child(
                        TreeNode::new("Taxonomies Changed")
                            .with_value(format_number(self.taxonomies_changed)),
                    )
                    .add_child(
                        TreeNode::new("Stable Sequences")
                            .with_value(format_number(self.stable.len())),
                    )
                    .add_child(
                        TreeNode::new("Execution Time")
                            .with_value(format!("{} ms", self.execution_time_ms)),
                    ),
            );

        // Add reclassification groups
        if !self.reclassified.is_empty() {
            let mut reclass_node = TreeNode::new("Reclassified Groups");
            for group in &self.reclassified {
                let old_taxon = group
                    .old_taxon
                    .map(|t| format!("TaxID:{}", t.0))
                    .unwrap_or_else(|| "Unknown".to_string());
                let new_taxon = group
                    .new_taxon
                    .map(|t| format!("TaxID:{}", t.0))
                    .unwrap_or_else(|| "Unknown".to_string());

                reclass_node = reclass_node.add_child(
                    TreeNode::new(&format!("{} → {}", old_taxon, new_taxon))
                        .with_value(format_number(group.count)),
                );
            }
            root = root.add_child(reclass_node);
        }

        vec![root]
    }

    fn render_table(&self) -> Table {
        let mut table = create_standard_table();

        table.set_header(vec![
            header_cell("Old Taxon"),
            header_cell("New Taxon"),
            header_cell("Sequences"),
            header_cell("Percentage"),
        ]);

        let total = self.total_affected as f64;

        for group in &self.reclassified {
            let old_taxon = group
                .old_taxon
                .map(|t| format!("TaxID:{}", t.0))
                .unwrap_or_else(|| "Unknown".to_string());
            let new_taxon = group
                .new_taxon
                .map(|t| format!("TaxID:{}", t.0))
                .unwrap_or_else(|| "Unknown".to_string());
            let percentage = (group.count as f64 / total * 100.0) as usize;

            table.add_row(vec![
                Cell::new(&old_taxon),
                Cell::new(&new_taxon),
                Cell::new(format_number(group.count)),
                Cell::new(format!("{}%", percentage)),
            ]);
        }

        table
    }

    fn render_timeline(&self) -> String {
        // Create a simple before/after visualization
        let mut output = String::new();
        output.push_str(&format!(
            "Reference: {}    →    Comparison: {}\n",
            self.query.reference_date.format("%Y-%m-%d"),
            self.query
                .comparison_date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "Current".to_string())
        ));
        output.push_str("─────────────────────────────────────────\n");
        output.push_str(&format!(
            "  {} sequences affected\n",
            format_number(self.total_affected)
        ));
        output.push_str(&format!(
            "  {} taxonomies changed\n",
            format_number(self.taxonomies_changed)
        ));
        output.push_str(&format!(
            "  {} sequences stable\n",
            format_number(self.stable.len())
        ));

        output
    }

    fn render_summary(&self) -> String {
        format!(
            "{} sequences reclassified across {} taxonomic changes",
            format_number(self.total_affected),
            format_number(self.taxonomies_changed)
        )
    }
}

impl EvolutionRenderable for EvolutionHistory {
    fn render_evolution_timeline(&self) -> String {
        if self.events.is_empty() {
            return "No evolution events found".to_string();
        }

        let mut output = String::new();

        // Find year range
        let start_year = self.events.first().unwrap().timestamp.year();
        let end_year = self.events.last().unwrap().timestamp.year();

        // Convert events to timeline format
        let timeline_events: Vec<(i32, &str, &str)> = self
            .events
            .iter()
            .map(|e| {
                let marker = match e.event_type {
                    EventType::Created => timeline_marker("create"),
                    EventType::Modified => timeline_marker("modify"),
                    EventType::Reclassified => timeline_marker("rename"),
                    EventType::Renamed => timeline_marker("rename"),
                    EventType::Merged => timeline_marker("merge"),
                    EventType::Split => timeline_marker("split"),
                    EventType::Deleted => timeline_marker("delete"),
                };
                (e.timestamp.year(), marker, e.description.as_str())
            })
            .collect();

        output.push_str(&create_timeline(start_year, end_year, timeline_events));
        output
    }

    fn render_evolution_graph(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("Evolution of {}\n", self.entity_id));
        output.push_str("═══════════════════════════════════════\n\n");

        for (i, event) in self.events.iter().enumerate() {
            let is_last = i == self.events.len() - 1;
            let connector = if is_last { "└─" } else { "├─" };
            let marker = match event.event_type {
                EventType::Created => "✚",
                EventType::Modified => "◈",
                EventType::Reclassified => "⟲",
                EventType::Renamed => "✎",
                EventType::Merged => "⤝",
                EventType::Split => "⤜",
                EventType::Deleted => "✗",
            };

            output.push_str(&format!(
                "{} {} {} {}\n",
                connector,
                event.timestamp.format("%Y-%m-%d"),
                marker,
                event.description
            ));

            if !is_last {
                output.push_str("│\n");
            }
        }

        output
    }

    fn render_evolution_stats(&self) -> Table {
        let mut table = create_standard_table();

        table.set_header(vec![header_cell("Event Type"), header_cell("Count")]);

        // Count events by type
        let mut event_counts = std::collections::HashMap::new();
        for event in &self.events {
            *event_counts
                .entry(format!("{:?}", event.event_type))
                .or_insert(0) += 1;
        }

        for (event_type, count) in event_counts {
            table.add_row(vec![Cell::new(&event_type), Cell::new(count.to_string())]);
        }

        table
    }
}

impl DiffRenderable for TemporalDiff {
    fn render_unified_diff(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "--- {}\n+++ {}\n",
            self.from.sequence_time.format("%Y-%m-%d %H:%M:%S"),
            self.to.sequence_time.format("%Y-%m-%d %H:%M:%S")
        ));
        output.push_str("─────────────────────────────────────────\n\n");

        // Sequence changes
        if !self.sequence_changes.added.is_empty() {
            output.push_str(&format!(
                "+{} sequences added\n",
                self.sequence_changes.added.len()
            ));
        }
        if !self.sequence_changes.removed.is_empty() {
            output.push_str(&format!(
                "-{} sequences removed\n",
                self.sequence_changes.removed.len()
            ));
        }
        if !self.sequence_changes.modified.is_empty() {
            output.push_str(&format!(
                "~{} sequences modified\n",
                self.sequence_changes.modified.len()
            ));
        }

        output.push('\n');

        // Taxonomy changes
        if !self.taxonomy_changes.added_taxa.is_empty() {
            output.push_str(&format!(
                "+{} taxa added\n",
                self.taxonomy_changes.added_taxa.len()
            ));
        }
        if !self.taxonomy_changes.removed_taxa.is_empty() {
            output.push_str(&format!(
                "-{} taxa removed\n",
                self.taxonomy_changes.removed_taxa.len()
            ));
        }
        if !self.taxonomy_changes.renamed_taxa.is_empty() {
            output.push_str(&format!(
                "~{} taxa renamed\n",
                self.taxonomy_changes.renamed_taxa.len()
            ));
        }

        output
    }

    fn render_side_by_side(&self) -> String {
        let mut output = String::new();

        let left_header = format!("{}", self.from.sequence_time.format("%Y-%m-%d"));
        let right_header = format!("{}", self.to.sequence_time.format("%Y-%m-%d"));

        output.push_str(&format!("{:^40} │ {:^40}\n", left_header, right_header));
        output.push_str(&"─".repeat(81));
        output.push('\n');

        // Show net changes
        let from_count = self.sequence_changes.total_delta.saturating_neg() as usize;
        let to_count = (from_count as i64 + self.sequence_changes.total_delta) as usize;

        output.push_str(&format!(
            "{:>39} sequences │ {:>39} sequences\n",
            format_number(from_count),
            format_number(to_count)
        ));

        output
    }

    fn render_diff_stats(&self) -> Table {
        let mut table = create_standard_table();

        table.set_header(vec![header_cell("Change Type"), header_cell("Count")]);

        table.add_row(vec![
            Cell::new("Sequences Added"),
            Cell::new(format_number(self.sequence_changes.added.len())),
        ]);

        table.add_row(vec![
            Cell::new("Sequences Removed"),
            Cell::new(format_number(self.sequence_changes.removed.len())),
        ]);

        table.add_row(vec![
            Cell::new("Sequences Modified"),
            Cell::new(format_number(self.sequence_changes.modified.len())),
        ]);

        table.add_row(vec![
            Cell::new("Taxa Added"),
            Cell::new(format_number(self.taxonomy_changes.added_taxa.len())),
        ]);

        table.add_row(vec![
            Cell::new("Taxa Removed"),
            Cell::new(format_number(self.taxonomy_changes.removed_taxa.len())),
        ]);

        table.add_row(vec![
            Cell::new("Taxa Renamed"),
            Cell::new(format_number(self.taxonomy_changes.renamed_taxa.len())),
        ]);

        table.add_row(vec![
            Cell::new("Reclassifications"),
            Cell::new(format_number(self.reclassifications.len())),
        ]);

        table
    }
}
