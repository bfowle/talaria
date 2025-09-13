use crate::core::database_diff::ComparisonResult;
use anyhow::Result;

pub mod text;
pub mod html;
pub mod json;

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
        writeln!(&mut output, "Old Database,{},\"{}\"", result.old_count, result.old_path.display())?;
        writeln!(&mut output, "New Database,{},\"{}\"", result.new_count, result.new_path.display())?;
        writeln!(&mut output, "Added Sequences,{},", result.added.len())?;
        writeln!(&mut output, "Removed Sequences,{},", result.removed.len())?;
        writeln!(&mut output, "Modified Sequences,{},", result.modified.len())?;
        writeln!(&mut output, "Renamed Sequences,{},", result.renamed.len())?;
        writeln!(&mut output, "Unchanged Sequences,{},", result.unchanged_count)?;
        
        if self.options.include_details {
            writeln!(&mut output)?;
            writeln!(&mut output, "Type,ID,Length,Description")?;
            
            for seq in &result.added {
                writeln!(&mut output, "Added,\"{}\",{},\"{}\"", 
                    seq.id, seq.length, seq.description.as_deref().unwrap_or(""))?;
            }
            
            for seq in &result.removed {
                writeln!(&mut output, "Removed,\"{}\",{},\"{}\"", 
                    seq.id, seq.length, seq.description.as_deref().unwrap_or(""))?;
            }
        }
        
        Ok(output)
    }
}