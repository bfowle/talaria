#![allow(dead_code)]

// ComparisonResult will need to be defined here or imported from appropriate module
// For now, let's create a placeholder until we can establish the proper dependency
use anyhow::Result;

// Temporary placeholder - will be properly resolved in dependency update phase
#[derive(Debug, Clone)]
pub struct ComparisonResult {
    pub old_count: usize,
    pub new_count: usize,
    pub old_path: std::path::PathBuf,
    pub new_path: std::path::PathBuf,
    pub added: Vec<SequenceInfo>,
    pub removed: Vec<SequenceInfo>,
    pub modified: Vec<SequenceInfo>,
    pub renamed: Vec<SequenceInfo>,
    pub unchanged_count: usize,
    pub statistics: ComparisonStatistics,
}

#[derive(Debug, Clone, Default)]
pub struct ComparisonStatistics {
    pub old_total_length: usize,
    pub new_total_length: usize,
    pub old_avg_length: f64,
    pub new_avg_length: f64,
    pub old_unique_taxa: usize,
    pub new_unique_taxa: usize,
    pub added_taxa: Vec<u32>,
    pub removed_taxa: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct SequenceInfo {
    pub id: String,
    pub length: usize,
    pub description: Option<String>,
    pub taxon_id: Option<u32>,
    // Fields for modified sequences
    pub old: Option<Box<SequenceData>>,
    pub new: Option<Box<SequenceData>>,
    pub similarity: Option<f64>,
    pub changes: Option<Vec<String>>,
    // Fields for renamed sequences
    pub old_id: Option<String>,
    pub new_id: Option<String>,
    pub old_description: Option<String>,
    pub new_description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SequenceData {
    pub id: String,
    pub length: usize,
    pub description: Option<String>,
}

// Additional types needed by report files
#[derive(Debug, Clone)]
pub enum SequenceChange {
    Added(SequenceInfo),
    Removed(SequenceInfo),
    Modified { old: SequenceInfo, new: SequenceInfo },
    Renamed { from: String, to: String },
    HeaderChanged,
    Extended(usize),
    Truncated(usize),
    Mutations(usize),
}

impl std::fmt::Display for SequenceChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SequenceChange::Added(info) => write!(f, "Added {}", info.id),
            SequenceChange::Removed(info) => write!(f, "Removed {}", info.id),
            SequenceChange::Modified { old, new } => write!(f, "Modified {} -> {}", old.id, new.id),
            SequenceChange::Renamed { from, to } => write!(f, "Renamed {} -> {}", from, to),
            SequenceChange::HeaderChanged => write!(f, "Header"),
            SequenceChange::Extended(n) => write!(f, "Ext +{}", n),
            SequenceChange::Truncated(n) => write!(f, "Trunc -{}", n),
            SequenceChange::Mutations(n) => write!(f, "{} mut", n),
        }
    }
}

pub mod html;
pub mod impls;
pub mod json;
pub mod reduction_html;
pub mod text;
pub mod traits;

pub use impls::create_reporter_from_path;
pub use traits::Reporter;

#[derive(Debug, Clone)]
pub struct ReportOptions {
    pub format: Format,
    pub include_taxonomy: bool,
    pub include_details: bool,
    pub include_visuals: bool,
}

#[derive(Debug, Clone)]
pub enum Format {
    Text,
    Html,
    Json,
    Csv,
}

pub struct ReportGenerator {
    options: ReportOptions,
}

impl ReportGenerator {
    pub fn new(options: ReportOptions) -> Self {
        Self { options }
    }

    pub fn generate(&self, result: &ComparisonResult) -> Result<String> {
        match self.options.format {
            Format::Text => text::generate_text_report(result, &self.options),
            Format::Html => html::generate_html_report(result, &self.options),
            Format::Json => json::generate_json_report(result, &self.options),
            Format::Csv => self.generate_csv_report(result),
        }
    }

    fn generate_csv_report(&self, result: &ComparisonResult) -> Result<String> {
        use std::fmt::Write;

        let mut output = String::new();

        // Header
        writeln!(&mut output, "Category,Count,Details")?;

        // Summary
        writeln!(
            &mut output,
            "Old Database,{},\"{}\"",
            result.old_count,
            result.old_path.display()
        )?;
        writeln!(
            &mut output,
            "New Database,{},\"{}\"",
            result.new_count,
            result.new_path.display()
        )?;
        writeln!(&mut output, "Added Sequences,{},", result.added.len())?;
        writeln!(&mut output, "Removed Sequences,{},", result.removed.len())?;
        writeln!(&mut output, "Modified Sequences,{},", result.modified.len())?;
        writeln!(&mut output, "Renamed Sequences,{},", result.renamed.len())?;
        writeln!(
            &mut output,
            "Unchanged Sequences,{},",
            result.unchanged_count
        )?;

        if self.options.include_details {
            writeln!(&mut output)?;
            writeln!(&mut output, "Type,ID,Length,Description")?;

            for seq in &result.added {
                writeln!(
                    &mut output,
                    "Added,\"{}\",{},\"{}\"",
                    seq.id,
                    seq.length,
                    seq.description.as_deref().unwrap_or("")
                )?;
            }

            for seq in &result.removed {
                writeln!(
                    &mut output,
                    "Removed,\"{}\",{},\"{}\"",
                    seq.id,
                    seq.length,
                    seq.description.as_deref().unwrap_or("")
                )?;
            }
        }

        Ok(output)
    }
}
