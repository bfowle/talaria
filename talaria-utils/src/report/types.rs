/// Core types for database comparison and reporting
///
/// These types represent the results of comparing two sequence databases,
/// tracking additions, removals, modifications, and statistical changes.
use super::core::{Cell, CellStyle, Metric, MetricSeverity, Report, Reportable, Section, Table};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

/// Result of comparing two sequence databases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonResult {
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    pub old_count: usize,
    pub new_count: usize,
    pub added: Vec<SequenceInfo>,
    pub removed: Vec<SequenceInfo>,
    pub modified: Vec<ModifiedSequence>,
    pub renamed: Vec<RenamedSequence>,
    pub unchanged_count: usize,
    pub statistics: DatabaseStatistics,
}

impl ComparisonResult {
    pub fn new(old_path: PathBuf, new_path: PathBuf, old_count: usize, new_count: usize) -> Self {
        Self {
            old_path,
            new_path,
            old_count,
            new_count,
            added: Vec::new(),
            removed: Vec::new(),
            modified: Vec::new(),
            renamed: Vec::new(),
            unchanged_count: 0,
            statistics: DatabaseStatistics::default(),
        }
    }

    /// Calculate aggregate statistics from sequence data
    pub fn calculate_statistics<S, FL, FT>(
        &mut self,
        old_sequences: &HashMap<String, S>,
        new_sequences: &HashMap<String, S>,
        get_len: FL,
        get_taxon: FT,
    ) where
        FL: Fn(&S) -> usize,
        FT: Fn(&S) -> Option<u32>,
    {
        // Calculate length statistics
        let old_lengths: Vec<usize> = old_sequences.values().map(&get_len).collect();
        let new_lengths: Vec<usize> = new_sequences.values().map(&get_len).collect();

        self.statistics.old_total_length = old_lengths.iter().sum();
        self.statistics.new_total_length = new_lengths.iter().sum();

        if !old_lengths.is_empty() {
            self.statistics.old_avg_length = self.statistics.old_total_length / old_lengths.len();
        }

        if !new_lengths.is_empty() {
            self.statistics.new_avg_length = self.statistics.new_total_length / new_lengths.len();
        }

        // Taxonomic statistics
        use std::collections::HashSet;
        let old_taxa: HashSet<u32> = old_sequences.values().filter_map(&get_taxon).collect();
        let new_taxa: HashSet<u32> = new_sequences.values().filter_map(&get_taxon).collect();

        self.statistics.old_unique_taxa = old_taxa.len();
        self.statistics.new_unique_taxa = new_taxa.len();
        self.statistics.added_taxa = new_taxa.difference(&old_taxa).copied().collect();
        self.statistics.removed_taxa = old_taxa.difference(&new_taxa).copied().collect();
    }
}

/// Statistical summary of database comparison
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatabaseStatistics {
    pub old_total_length: usize,
    pub new_total_length: usize,
    pub old_avg_length: usize,
    pub new_avg_length: usize,
    pub old_unique_taxa: usize,
    pub new_unique_taxa: usize,
    pub added_taxa: Vec<u32>,
    pub removed_taxa: Vec<u32>,
}

/// Information about a single sequence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceInfo {
    pub id: String,
    pub description: Option<String>,
    pub length: usize,
    pub taxon_id: Option<u32>,
}

impl SequenceInfo {
    pub fn new(
        id: String,
        description: Option<String>,
        length: usize,
        taxon_id: Option<u32>,
    ) -> Self {
        Self {
            id,
            description,
            length,
            taxon_id,
        }
    }
}

/// A sequence that has been modified between databases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifiedSequence {
    pub old: SequenceInfo,
    pub new: SequenceInfo,
    pub similarity: f64,
    pub changes: Vec<SequenceChange>,
}

/// A sequence that has been renamed (same sequence, different ID or description)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenamedSequence {
    pub old_id: String,
    pub new_id: String,
    pub old_description: Option<String>,
    pub new_description: Option<String>,
}

/// Types of changes detected in modified sequences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SequenceChange {
    HeaderChanged,
    Extended(usize),
    Truncated(usize),
    Mutations(usize),
}

impl fmt::Display for SequenceChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SequenceChange::HeaderChanged => write!(f, "Header changed"),
            SequenceChange::Extended(n) => write!(f, "Extended by {} residues", n),
            SequenceChange::Truncated(n) => write!(f, "Truncated by {} residues", n),
            SequenceChange::Mutations(n) => write!(f, "{} mutations", n),
        }
    }
}

/// Output format for reports
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Text,
    Html,
    Json,
    Csv,
}

/// Options for report generation
#[derive(Debug, Clone)]
pub struct ReportOptions {
    pub include_taxonomy: bool,
    pub include_details: bool,
    pub include_visuals: bool,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            include_taxonomy: true,
            include_details: true,
            include_visuals: true,
        }
    }
}

impl fmt::Display for ReportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReportFormat::Text => write!(f, "text"),
            ReportFormat::Html => write!(f, "html"),
            ReportFormat::Json => write!(f, "json"),
            ReportFormat::Csv => write!(f, "csv"),
        }
    }
}

impl std::str::FromStr for ReportFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" | "txt" => Ok(ReportFormat::Text),
            "html" | "htm" => Ok(ReportFormat::Html),
            "json" => Ok(ReportFormat::Json),
            "csv" => Ok(ReportFormat::Csv),
            _ => Err(format!("Unknown report format: {}", s)),
        }
    }
}

/// Implement Reportable for ComparisonResult to enable generic reporting
impl Reportable for ComparisonResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Database Comparison", "database diff")
            .metadata("old_database", self.old_path.display().to_string())
            .metadata("new_database", self.new_path.display().to_string());

        // Summary metrics section
        let total_changes = self.added.len() + self.removed.len() + self.modified.len();
        let summary_metrics = vec![
            Metric::new("Total Changes", total_changes).with_severity(if total_changes > 1000 {
                MetricSeverity::Warning
            } else {
                MetricSeverity::Normal
            }),
            Metric::new("Added Sequences", self.added.len()).with_severity(
                if !self.added.is_empty() {
                    MetricSeverity::Success
                } else {
                    MetricSeverity::Normal
                },
            ),
            Metric::new("Removed Sequences", self.removed.len()).with_severity(
                if !self.removed.is_empty() {
                    MetricSeverity::Warning
                } else {
                    MetricSeverity::Normal
                },
            ),
            Metric::new("Modified Sequences", self.modified.len()),
            Metric::new("Renamed Sequences", self.renamed.len()),
            Metric::new("Unchanged Sequences", self.unchanged_count),
        ];

        report = report.section(Section::summary("Summary", summary_metrics));

        // Statistics section
        let stats_items = vec![
            (
                "Total Length".to_string(),
                format!(
                    "{} → {} ({:+})",
                    format_number(self.statistics.old_total_length),
                    format_number(self.statistics.new_total_length),
                    self.statistics.new_total_length as i64
                        - self.statistics.old_total_length as i64
                ),
            ),
            (
                "Average Length".to_string(),
                format!(
                    "{} → {} ({:+})",
                    self.statistics.old_avg_length,
                    self.statistics.new_avg_length,
                    self.statistics.new_avg_length as i64 - self.statistics.old_avg_length as i64
                ),
            ),
            (
                "Unique Taxa".to_string(),
                format!(
                    "{} → {} ({:+})",
                    self.statistics.old_unique_taxa,
                    self.statistics.new_unique_taxa,
                    self.statistics.new_unique_taxa as i64 - self.statistics.old_unique_taxa as i64
                ),
            ),
        ];

        report = report.section(Section::key_value("Statistics", stats_items));

        // Added sequences table (top 20)
        if !self.added.is_empty() {
            let mut table = Table::new(vec![
                "ID".to_string(),
                "Length".to_string(),
                "Taxon ID".to_string(),
            ]);
            for seq in self.added.iter().take(20) {
                table.add_row(vec![
                    Cell::new(&seq.id).with_style(CellStyle::Success),
                    Cell::new(seq.length),
                    Cell::new(
                        seq.taxon_id
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                    ),
                ]);
            }
            if self.added.len() > 20 {
                table.add_row(vec![
                    Cell::new(format!("... and {} more", self.added.len() - 20))
                        .with_style(CellStyle::Muted),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }
            report = report.section(Section::table(
                format!("Added Sequences ({} total)", self.added.len()),
                table,
            ));
        }

        // Removed sequences table (top 20)
        if !self.removed.is_empty() {
            let mut table = Table::new(vec![
                "ID".to_string(),
                "Length".to_string(),
                "Taxon ID".to_string(),
            ]);
            for seq in self.removed.iter().take(20) {
                table.add_row(vec![
                    Cell::new(&seq.id).with_style(CellStyle::Warning),
                    Cell::new(seq.length),
                    Cell::new(
                        seq.taxon_id
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                    ),
                ]);
            }
            if self.removed.len() > 20 {
                table.add_row(vec![
                    Cell::new(format!("... and {} more", self.removed.len() - 20))
                        .with_style(CellStyle::Muted),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }
            report = report.section(Section::table(
                format!("Removed Sequences ({} total)", self.removed.len()),
                table,
            ));
        }

        // Modified sequences table (top 20)
        if !self.modified.is_empty() {
            let mut table = Table::new(vec![
                "ID".to_string(),
                "Similarity".to_string(),
                "Length Change".to_string(),
                "Changes".to_string(),
            ]);
            for mod_seq in self.modified.iter().take(20) {
                let length_diff = mod_seq.new.length as i64 - mod_seq.old.length as i64;
                let changes_str = mod_seq
                    .changes
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");

                table.add_row(vec![
                    Cell::new(&mod_seq.old.id).with_style(CellStyle::Highlight),
                    Cell::new(format!("{:.1}%", mod_seq.similarity * 100.0)),
                    Cell::new(format!(
                        "{} → {} ({:+})",
                        mod_seq.old.length, mod_seq.new.length, length_diff
                    )),
                    Cell::new(changes_str),
                ]);
            }
            if self.modified.len() > 20 {
                table.add_row(vec![
                    Cell::new(format!("... and {} more", self.modified.len() - 20))
                        .with_style(CellStyle::Muted),
                    Cell::new(""),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }
            report = report.section(Section::table(
                format!("Modified Sequences ({} total)", self.modified.len()),
                table,
            ));
        }

        report.build()
    }
}

// Helper function to format numbers with commas
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
