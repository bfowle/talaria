use crate::core::database_diff::{ComparisonResult, SequenceChange};
use crate::report::ReportOptions;
use anyhow::Result;
use std::fmt::Write;

pub fn generate_text_report(result: &ComparisonResult, options: &ReportOptions) -> Result<String> {
    let mut output = String::new();
    
    // Header
    writeln!(&mut output, "Database Comparison Report")?;
    writeln!(&mut output, "==========================")?;
    writeln!(&mut output)?;
    
    // Database info
    writeln!(&mut output, "Old: {} ({} sequences)", 
        result.old_path.display(), result.old_count)?;
    writeln!(&mut output, "New: {} ({} sequences)", 
        result.new_path.display(), result.new_count)?;
    writeln!(&mut output)?;
    
    // Summary
    writeln!(&mut output, "Summary")?;
    writeln!(&mut output, "-------")?;
    writeln!(&mut output, "- Added:     {:6} sequences", result.added.len())?;
    writeln!(&mut output, "- Removed:   {:6} sequences", result.removed.len())?;
    writeln!(&mut output, "- Modified:  {:6} sequences", result.modified.len())?;
    writeln!(&mut output, "- Renamed:   {:6} sequences", result.renamed.len())?;
    writeln!(&mut output, "- Unchanged: {:6} sequences", result.unchanged_count)?;
    writeln!(&mut output)?;
    
    // Statistics
    let stats = &result.statistics;
    writeln!(&mut output, "Database Statistics")?;
    writeln!(&mut output, "-------------------")?;
    writeln!(&mut output, "Total length: {} → {} ({:+} bp/aa)",
        stats.old_total_length,
        stats.new_total_length,
        stats.new_total_length as i64 - stats.old_total_length as i64)?;
    writeln!(&mut output, "Average length: {} → {} ({:+})",
        stats.old_avg_length,
        stats.new_avg_length,
        stats.new_avg_length as i64 - stats.old_avg_length as i64)?;
    writeln!(&mut output)?;
    
    // Taxonomic changes
    if options.include_taxonomy {
        writeln!(&mut output, "Taxonomic Changes")?;
        writeln!(&mut output, "-----------------")?;
        writeln!(&mut output, "Unique taxa: {} → {} ({:+})",
            stats.old_unique_taxa,
            stats.new_unique_taxa,
            stats.new_unique_taxa as i64 - stats.old_unique_taxa as i64)?;
        writeln!(&mut output, "- New taxa: {}", stats.added_taxa)?;
        writeln!(&mut output, "- Removed taxa: {}", stats.removed_taxa)?;
        writeln!(&mut output)?;
    }
    
    // Detailed changes
    if options.include_details {
        if !result.added.is_empty() {
            writeln!(&mut output, "Added Sequences (Top 10)")?;
            writeln!(&mut output, "------------------------")?;
            for seq in result.added.iter().take(10) {
                writeln!(&mut output, "  {} (length: {})", seq.id, seq.length)?;
                if let Some(desc) = &seq.description {
                    writeln!(&mut output, "    {}", desc)?;
                }
            }
            if result.added.len() > 10 {
                writeln!(&mut output, "  ... and {} more", result.added.len() - 10)?;
            }
            writeln!(&mut output)?;
        }
        
        if !result.removed.is_empty() {
            writeln!(&mut output, "Removed Sequences (Top 10)")?;
            writeln!(&mut output, "--------------------------")?;
            for seq in result.removed.iter().take(10) {
                writeln!(&mut output, "  {} (length: {})", seq.id, seq.length)?;
                if let Some(desc) = &seq.description {
                    writeln!(&mut output, "    {}", desc)?;
                }
            }
            if result.removed.len() > 10 {
                writeln!(&mut output, "  ... and {} more", result.removed.len() - 10)?;
            }
            writeln!(&mut output)?;
        }
        
        if !result.modified.is_empty() {
            writeln!(&mut output, "Modified Sequences (Top 10)")?;
            writeln!(&mut output, "---------------------------")?;
            for mod_seq in result.modified.iter().take(10) {
                writeln!(&mut output, "  {} (similarity: {:.1}%)", 
                    mod_seq.old.id, mod_seq.similarity * 100.0)?;
                writeln!(&mut output, "    Length: {} → {}", 
                    mod_seq.old.length, mod_seq.new.length)?;
                
                for change in &mod_seq.changes {
                    match change {
                        SequenceChange::HeaderChanged => {
                            writeln!(&mut output, "    - Header changed")?;
                        }
                        SequenceChange::Extended(n) => {
                            writeln!(&mut output, "    - Extended by {} bp/aa", n)?;
                        }
                        SequenceChange::Truncated(n) => {
                            writeln!(&mut output, "    - Truncated by {} bp/aa", n)?;
                        }
                        SequenceChange::Mutations(n) => {
                            writeln!(&mut output, "    - {} mutations", n)?;
                        }
                    }
                }
            }
            if result.modified.len() > 10 {
                writeln!(&mut output, "  ... and {} more", result.modified.len() - 10)?;
            }
            writeln!(&mut output)?;
        }
    }
    
    Ok(output)
}