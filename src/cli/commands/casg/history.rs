use anyhow::{Context, Result};
use clap::Args;
use colored::*;
use std::path::PathBuf;
use chrono::{DateTime, Utc};

#[derive(Args)]
pub struct HistoryArgs {
    /// Path to CASG repository
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Output format (text, json, markdown, html)
    #[arg(short = 'f', long, default_value = "text")]
    pub format: String,

    /// Maximum number of versions to show
    #[arg(short = 'n', long, default_value = "20")]
    pub limit: usize,

    /// Show full details for each version
    #[arg(long)]
    pub detailed: bool,

    /// Filter by date (YYYY-MM-DD)
    #[arg(long)]
    pub since: Option<String>,

    /// Filter by date (YYYY-MM-DD)
    #[arg(long)]
    pub until: Option<String>,

    /// Show diff between versions
    #[arg(long)]
    pub diff: bool,

    /// Compare two specific versions
    #[arg(long)]
    pub compare: Option<Vec<String>>,

    /// Output file (default: stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Show visual timeline
    #[arg(long)]
    pub timeline: bool,
}

pub fn run(args: HistoryArgs) -> Result<()> {
    use crate::casg::CASGRepository;
    use crate::casg::temporal::TemporalIndex;
    use crate::utils::progress::create_spinner;

    // Determine CASG path
    let casg_path = if let Some(path) = args.path.clone() {
        path
    } else {
        use crate::core::paths;
        paths::talaria_databases_dir()
    };

    if !casg_path.exists() {
        anyhow::bail!("CASG repository not found at {}. Initialize it first with 'talaria casg init'",
                     casg_path.display());
    }

    // Open repository
    let spinner = create_spinner("Loading version history...");
    let repository = CASGRepository::open(&casg_path)?;

    // Initialize temporal tracking for existing data if needed
    {
        use crate::core::database_manager::DatabaseManager as CASGDatabaseManager;
        let mut manager = CASGDatabaseManager::new(Some(casg_path.to_string_lossy().to_string()))?;
        let _ = manager.init_temporal_for_existing();
    }

    // Get temporal index
    let temporal_index = TemporalIndex::load(&casg_path)?;
    spinner.finish_and_clear();

    // Generate report based on format
    let report = match args.format.as_str() {
        "json" => generate_json_report(&temporal_index, &repository, &args)?,
        "markdown" | "md" => generate_markdown_report(&temporal_index, &repository, &args)?,
        "html" => generate_html_report(&temporal_index, &repository, &args)?,
        _ => generate_text_report(&temporal_index, &repository, &args)?,
    };

    // Output report
    if let Some(output_path) = args.output.clone() {
        std::fs::write(&output_path, report)
            .context("Failed to write output file")?;
        println!("{} Report saved to {}",
                 "✓".green().bold(),
                 output_path.display());
    } else {
        println!("{}", report);
    }

    Ok(())
}

fn generate_text_report(
    temporal_index: &crate::casg::temporal::TemporalIndex,
    repository: &crate::casg::CASGRepository,
    args: &HistoryArgs,
) -> Result<String> {
    use std::fmt::Write;
    let mut report = String::new();

    writeln!(report, "\n{}", "═".repeat(80))?;
    writeln!(report, "{:^80}", "VERSION HISTORY REPORT")?;
    writeln!(report, "{}", "═".repeat(80))?;
    writeln!(report)?;

    // Get version history
    let history = temporal_index.get_version_history(args.limit)?;

    // Apply date filters if provided
    let filtered_history = filter_by_date(history, &args.since, &args.until)?;

    if filtered_history.is_empty() {
        writeln!(report, "No versions found matching the criteria")?;
        return Ok(report);
    }

    // Show timeline if requested
    if args.timeline {
        writeln!(report, "{}", "Timeline:".bold().underline())?;
        generate_ascii_timeline(&mut report, &filtered_history)?;
        writeln!(report)?;
    }

    // Show version details
    writeln!(report, "{}", "Version History:".bold().underline())?;
    writeln!(report)?;

    for (i, version) in filtered_history.iter().enumerate() {
        if i >= args.limit {
            break;
        }

        // Version header
        writeln!(report, "{} Version: {}",
                 "●".cyan().bold(),
                 version.version.bold())?;
        writeln!(report, "  {}: {}",
                 "Date".bold(),
                 version.timestamp.format("%Y-%m-%d %H:%M:%S UTC"))?;
        writeln!(report, "  {}: {}",
                 "Type".bold(),
                 version.version_type)?;

        if args.detailed {
            // Show detailed information
            writeln!(report, "  {}: {}",
                     "Sequence Root".bold(),
                     version.sequence_root)?;
            writeln!(report, "  {}: {}",
                     "Taxonomy Root".bold(),
                     version.taxonomy_root)?;
            writeln!(report, "  {}: {} chunks, {} sequences",
                     "Content".bold(),
                     version.chunk_count,
                     version.sequence_count)?;

            if !version.changes.is_empty() {
                writeln!(report, "  {}:", "Changes".bold())?;
                for change in &version.changes {
                    writeln!(report, "    • {}", change)?;
                }
            }

            if let Some(ref parent) = version.parent_version {
                writeln!(report, "  {}: {}",
                         "Parent".bold(),
                         parent)?;
            }
        }

        // Show diff if requested
        if args.diff && i < filtered_history.len() - 1 {
            let next_version = &filtered_history[i + 1];
            writeln!(report, "  {}:", "Diff from previous".bold())?;
            generate_version_diff(&mut report, version, next_version, repository)?;
        }

        writeln!(report)?;
    }

    // Handle version comparison if requested
    if let Some(ref versions) = args.compare {
        if versions.len() == 2 {
            writeln!(report, "{}", "═".repeat(80))?;
            writeln!(report, "{:^80}", "VERSION COMPARISON")?;
            writeln!(report, "{}", "═".repeat(80))?;
            writeln!(report)?;

            compare_versions(&mut report, &versions[0], &versions[1], temporal_index, repository)?;
        }
    }

    // Summary statistics
    writeln!(report, "{}", "═".repeat(80))?;
    writeln!(report, "{}", "Summary:".bold())?;
    writeln!(report, "  Total versions: {}", filtered_history.len())?;

    if !filtered_history.is_empty() {
        let first = filtered_history.last().unwrap();
        let last = filtered_history.first().unwrap();
        writeln!(report, "  Date range: {} to {}",
                 first.timestamp.format("%Y-%m-%d"),
                 last.timestamp.format("%Y-%m-%d"))?;

        let total_sequences: usize = filtered_history.iter()
            .map(|v| v.sequence_count)
            .max()
            .unwrap_or(0);
        writeln!(report, "  Peak sequences: {}", total_sequences)?;
    }

    Ok(report)
}

fn generate_json_report(
    temporal_index: &crate::casg::temporal::TemporalIndex,
    _repository: &crate::casg::CASGRepository,
    args: &HistoryArgs,
) -> Result<String> {
    use serde_json::json;

    let history = temporal_index.get_version_history(args.limit)?;
    let filtered_history = filter_by_date(history, &args.since, &args.until)?;

    let report = json!({
        "version_history": filtered_history,
        "total_versions": filtered_history.len(),
        "date_range": {
            "from": filtered_history.last().map(|v| v.timestamp.to_rfc3339()),
            "to": filtered_history.first().map(|v| v.timestamp.to_rfc3339()),
        },
    });

    Ok(serde_json::to_string_pretty(&report)?)
}

fn generate_markdown_report(
    temporal_index: &crate::casg::temporal::TemporalIndex,
    _repository: &crate::casg::CASGRepository,
    args: &HistoryArgs,
) -> Result<String> {
    use std::fmt::Write;
    let mut report = String::new();

    writeln!(report, "# Version History Report\n")?;
    writeln!(report, "Generated: {}\n", Utc::now().format("%Y-%m-%d %H:%M:%S UTC"))?;

    let history = temporal_index.get_version_history(args.limit)?;
    let filtered_history = filter_by_date(history, &args.since, &args.until)?;

    writeln!(report, "## Summary\n")?;
    writeln!(report, "- **Total versions**: {}", filtered_history.len())?;

    if !filtered_history.is_empty() {
        let first = filtered_history.last().unwrap();
        let last = filtered_history.first().unwrap();
        writeln!(report, "- **Date range**: {} to {}",
                 first.timestamp.format("%Y-%m-%d"),
                 last.timestamp.format("%Y-%m-%d"))?;
    }

    writeln!(report, "\n## Version Timeline\n")?;

    if args.timeline {
        writeln!(report, "```")?;
        generate_ascii_timeline(&mut report, &filtered_history)?;
        writeln!(report, "```\n")?;
    }

    writeln!(report, "## Versions\n")?;

    for (i, version) in filtered_history.iter().enumerate() {
        if i >= args.limit {
            break;
        }

        writeln!(report, "### {}\n", version.version)?;
        writeln!(report, "| Field | Value |")?;
        writeln!(report, "|-------|-------|")?;
        writeln!(report, "| Date | {} |", version.timestamp.format("%Y-%m-%d %H:%M:%S UTC"))?;
        writeln!(report, "| Type | {} |", version.version_type)?;
        writeln!(report, "| Chunks | {} |", version.chunk_count)?;
        writeln!(report, "| Sequences | {} |", version.sequence_count)?;

        if args.detailed {
            writeln!(report, "| Sequence Root | `{}` |", version.sequence_root)?;
            writeln!(report, "| Taxonomy Root | `{}` |", version.taxonomy_root)?;

            if let Some(ref parent) = version.parent_version {
                writeln!(report, "| Parent | {} |", parent)?;
            }
        }

        if !version.changes.is_empty() {
            writeln!(report, "\n#### Changes\n")?;
            for change in &version.changes {
                writeln!(report, "- {}", change)?;
            }
        }

        writeln!(report)?;
    }

    Ok(report)
}

fn generate_html_report(
    temporal_index: &crate::casg::temporal::TemporalIndex,
    _repository: &crate::casg::CASGRepository,
    args: &HistoryArgs,
) -> Result<String> {
    let history = temporal_index.get_version_history(args.limit)?;
    let filtered_history = filter_by_date(history, &args.since, &args.until)?;

    let mut html = String::from(r#"<!DOCTYPE html>
<html>
<head>
    <title>Version History Report</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 20px; background: #f5f5f5; }
        .container { max-width: 1200px; margin: 0 auto; background: white; padding: 20px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
        h1, h2 { color: #333; border-bottom: 2px solid #e0e0e0; padding-bottom: 10px; }
        table { width: 100%; border-collapse: collapse; margin: 20px 0; }
        th, td { border: 1px solid #ddd; padding: 12px; text-align: left; }
        th { background-color: #f2f2f2; font-weight: bold; }
        tr:nth-child(even) { background-color: #f9f9f9; }
        .version-card { background: #f8f9fa; border-left: 4px solid #007bff; padding: 15px; margin: 20px 0; border-radius: 4px; }
        .timeline { font-family: monospace; background: #2d2d2d; color: #f0f0f0; padding: 15px; border-radius: 4px; overflow-x: auto; }
        .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 15px; margin: 20px 0; }
        .stat-box { background: #f8f9fa; padding: 15px; border-radius: 4px; text-align: center; }
        .stat-value { font-size: 24px; font-weight: bold; color: #007bff; }
        .stat-label { color: #666; margin-top: 5px; }
        code { background: #f4f4f4; padding: 2px 4px; border-radius: 3px; font-family: monospace; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Version History Report</h1>
        <p>Generated: )"#);

    html.push_str(&Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string());
    html.push_str("</p>");

    // Summary statistics
    html.push_str(r#"<div class="stats">"#);
    html.push_str(&format!(r#"
        <div class="stat-box">
            <div class="stat-value">{}</div>
            <div class="stat-label">Total Versions</div>
        </div>"#, filtered_history.len()));

    if !filtered_history.is_empty() {
        let total_chunks: usize = filtered_history.iter().map(|v| v.chunk_count).sum();
        let max_sequences: usize = filtered_history.iter().map(|v| v.sequence_count).max().unwrap_or(0);

        html.push_str(&format!(r#"
        <div class="stat-box">
            <div class="stat-value">{}</div>
            <div class="stat-label">Total Chunks</div>
        </div>
        <div class="stat-box">
            <div class="stat-value">{}</div>
            <div class="stat-label">Peak Sequences</div>
        </div>"#, total_chunks, max_sequences));
    }
    html.push_str("</div>");

    // Timeline
    if args.timeline {
        html.push_str(r#"<h2>Timeline</h2><div class="timeline"><pre>"#);
        let mut timeline = String::new();
        generate_ascii_timeline(&mut timeline, &filtered_history)?;
        html.push_str(&html_escape(&timeline));
        html.push_str("</pre></div>");
    }

    // Version table
    html.push_str(r#"
        <h2>Version History</h2>
        <table>
            <tr>
                <th>Version</th>
                <th>Date</th>
                <th>Type</th>
                <th>Chunks</th>
                <th>Sequences</th>"#);

    if args.detailed {
        html.push_str("<th>Changes</th>");
    }

    html.push_str("</tr>");

    for version in filtered_history.iter().take(args.limit) {
        html.push_str(&format!(r#"
            <tr>
                <td><code>{}</code></td>
                <td>{}</td>
                <td>{}</td>
                <td>{}</td>
                <td>{}</td>"#,
            version.version,
            version.timestamp.format("%Y-%m-%d %H:%M:%S"),
            version.version_type,
            version.chunk_count,
            version.sequence_count
        ));

        if args.detailed {
            html.push_str("<td>");
            if !version.changes.is_empty() {
                html.push_str("<ul>");
                for change in &version.changes {
                    html.push_str(&format!("<li>{}</li>", html_escape(change)));
                }
                html.push_str("</ul>");
            }
            html.push_str("</td>");
        }

        html.push_str("</tr>");
    }

    html.push_str("</table></div></body></html>");
    Ok(html)
}

fn filter_by_date(
    history: Vec<VersionInfo>,
    since: &Option<String>,
    until: &Option<String>,
) -> Result<Vec<VersionInfo>> {
    let mut filtered = history;

    if let Some(since_str) = since {
        let since_date = chrono::NaiveDate::parse_from_str(since_str, "%Y-%m-%d")
            .context("Invalid since date format (use YYYY-MM-DD)")?;
        let since_datetime = since_date.and_hms_opt(0, 0, 0).unwrap();
        let since_utc = DateTime::<Utc>::from_naive_utc_and_offset(since_datetime, Utc);

        filtered.retain(|v| v.timestamp >= since_utc);
    }

    if let Some(until_str) = until {
        let until_date = chrono::NaiveDate::parse_from_str(until_str, "%Y-%m-%d")
            .context("Invalid until date format (use YYYY-MM-DD)")?;
        let until_datetime = until_date.and_hms_opt(23, 59, 59).unwrap();
        let until_utc = DateTime::<Utc>::from_naive_utc_and_offset(until_datetime, Utc);

        filtered.retain(|v| v.timestamp <= until_utc);
    }

    Ok(filtered)
}

fn generate_ascii_timeline(output: &mut String, history: &[VersionInfo]) -> Result<()> {
    use std::fmt::Write;

    if history.is_empty() {
        return Ok(());
    }

    // Create a simple ASCII timeline
    writeln!(output, "  Newest")?;
    writeln!(output, "    │")?;

    for (i, version) in history.iter().enumerate() {
        let marker = if i == 0 { "●" } else { "○" };
        let date = version.timestamp.format("%Y-%m-%d");
        let line = format!("    {} {} - {}", marker, date, version.version);

        writeln!(output, "{}", line)?;

        if i < history.len() - 1 {
            writeln!(output, "    │")?;
        }
    }

    writeln!(output, "    │")?;
    writeln!(output, "  Oldest")?;

    Ok(())
}

fn generate_version_diff(
    output: &mut String,
    current: &VersionInfo,
    previous: &VersionInfo,
    _repository: &crate::casg::CASGRepository,
) -> Result<()> {
    use std::fmt::Write;

    let seq_diff = current.sequence_count as i64 - previous.sequence_count as i64;
    let chunk_diff = current.chunk_count as i64 - previous.chunk_count as i64;

    if seq_diff != 0 {
        let sign = if seq_diff > 0 { "+" } else { "" };
        writeln!(output, "      Sequences: {}{}", sign, seq_diff)?;
    }

    if chunk_diff != 0 {
        let sign = if chunk_diff > 0 { "+" } else { "" };
        writeln!(output, "      Chunks: {}{}", sign, chunk_diff)?;
    }

    Ok(())
}

fn compare_versions(
    output: &mut String,
    version1: &str,
    version2: &str,
    temporal_index: &crate::casg::temporal::TemporalIndex,
    _repository: &crate::casg::CASGRepository,
) -> Result<()> {
    use std::fmt::Write;

    let v1 = temporal_index.get_version(version1)?
        .context(format!("Version {} not found", version1))?;
    let v2 = temporal_index.get_version(version2)?
        .context(format!("Version {} not found", version2))?;

    writeln!(output, "Comparing {} vs {}", version1.bold(), version2.bold())?;
    writeln!(output)?;

    writeln!(output, "{:<20} {:<30} {:<30}", "", version1, version2)?;
    writeln!(output, "{}", "-".repeat(80))?;
    writeln!(output, "{:<20} {:<30} {:<30}",
             "Date",
             v1.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
             v2.timestamp.format("%Y-%m-%d %H:%M:%S").to_string())?;
    writeln!(output, "{:<20} {:<30} {:<30}",
             "Sequences",
             v1.sequence_count.to_string(),
             v2.sequence_count.to_string())?;
    writeln!(output, "{:<20} {:<30} {:<30}",
             "Chunks",
             v1.chunk_count.to_string(),
             v2.chunk_count.to_string())?;

    Ok(())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

// Version information structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub timestamp: DateTime<Utc>,
    pub version_type: String,
    pub sequence_root: String,
    pub taxonomy_root: String,
    pub chunk_count: usize,
    pub sequence_count: usize,
    pub changes: Vec<String>,
    pub parent_version: Option<String>,
}