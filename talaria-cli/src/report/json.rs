#![allow(dead_code)]

use crate::core::database_diff::ComparisonResult;
use crate::report::ReportOptions;
use anyhow::Result;
use serde_json::json;

pub fn generate_json_report(result: &ComparisonResult, _options: &ReportOptions) -> Result<String> {
    let report = json!({
        "metadata": {
            "generated": chrono::Utc::now().to_rfc3339(),
            "old_database": {
                "path": result.old_path.to_string_lossy(),
                "sequences": result.old_count,
            },
            "new_database": {
                "path": result.new_path.to_string_lossy(),
                "sequences": result.new_count,
            }
        },
        "summary": {
            "added": result.added.len(),
            "removed": result.removed.len(),
            "modified": result.modified.len(),
            "renamed": result.renamed.len(),
            "unchanged": result.unchanged_count,
        },
        "statistics": {
            "length": {
                "old_total": result.statistics.old_total_length,
                "new_total": result.statistics.new_total_length,
                "old_average": result.statistics.old_avg_length,
                "new_average": result.statistics.new_avg_length,
            },
            "taxonomy": {
                "old_unique_taxa": result.statistics.old_unique_taxa,
                "new_unique_taxa": result.statistics.new_unique_taxa,
                "added_taxa": result.statistics.added_taxa,
                "removed_taxa": result.statistics.removed_taxa,
            }
        },
        "changes": {
            "added": result.added.iter().map(|seq| json!({
                "id": seq.id,
                "length": seq.length,
                "description": seq.description,
                "taxon_id": seq.taxon_id,
            })).collect::<Vec<_>>(),
            "removed": result.removed.iter().map(|seq| json!({
                "id": seq.id,
                "length": seq.length,
                "description": seq.description,
                "taxon_id": seq.taxon_id,
            })).collect::<Vec<_>>(),
            "modified": result.modified.iter().map(|mod_seq| json!({
                "id": mod_seq.old.id,
                "old_length": mod_seq.old.length,
                "new_length": mod_seq.new.length,
                "similarity": mod_seq.similarity,
                "changes": mod_seq.changes.iter().map(|c| format!("{:?}", c)).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "renamed": result.renamed.iter().map(|ren| json!({
                "old_id": ren.old_id,
                "new_id": ren.new_id,
                "old_description": ren.old_description,
                "new_description": ren.new_description,
            })).collect::<Vec<_>>(),
        }
    });

    Ok(serde_json::to_string_pretty(&report)?)
}
