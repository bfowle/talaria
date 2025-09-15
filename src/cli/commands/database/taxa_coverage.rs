use crate::bio::taxonomy::{TaxonomyDB, ncbi};
use crate::bio::taxonomy_stats::{TaxonomyCoverage, format_tree};
use crate::bio::fasta;
use anyhow::{Context, Result};
use clap::Args;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;

#[derive(Args)]
pub struct TaxaCoverageArgs {
    /// Input FASTA file or directory
    #[arg(short, long)]
    pub input: PathBuf,

    /// Second database for comparison (optional)
    #[arg(short = 'c', long)]
    pub compare: Option<PathBuf>,

    /// Path to NCBI taxonomy database
    #[arg(short = 't', long)]
    pub taxonomy: Option<PathBuf>,

    /// Output format (text, json, html, csv)
    #[arg(short = 'f', long, default_value = "text")]
    pub format: String,

    /// Maximum depth for tree display
    #[arg(short = 'd', long, default_value = "5")]
    pub max_depth: usize,

    /// Filter by taxonomic rank (e.g., species, genus, family)
    #[arg(short = 'r', long)]
    pub rank_filter: Option<String>,

    /// Show only taxa with at least this many sequences
    #[arg(short = 'm', long, default_value = "1")]
    pub min_sequences: usize,

    /// Output file (default: stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

pub fn run(args: TaxaCoverageArgs) -> Result<()> {
    // Load or download taxonomy database
    let taxonomy_db = load_taxonomy(&args.taxonomy)?;

    // Process primary database
    println!("{} Analyzing taxonomic coverage for {}...",
             "►".cyan().bold(),
             args.input.display());

    let primary_coverage = analyze_database(&args.input, &taxonomy_db)?;

    // Process comparison database if provided
    let comparison = if let Some(compare_path) = &args.compare {
        println!("{} Analyzing comparison database {}...",
                 "►".cyan().bold(),
                 compare_path.display());
        Some(analyze_database(compare_path, &taxonomy_db)?)
    } else {
        None
    };

    // Generate report based on format
    let report = match args.format.as_str() {
        "json" => generate_json_report(&primary_coverage, comparison.as_ref()),
        "csv" => generate_csv_report(&primary_coverage, comparison.as_ref()),
        "html" => generate_html_report(&primary_coverage, comparison.as_ref(), args.max_depth),
        _ => generate_text_report(&primary_coverage, comparison.as_ref(), &taxonomy_db, args.max_depth),
    }?;

    // Output report
    if let Some(output_path) = args.output {
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

fn load_taxonomy(taxonomy_path: &Option<PathBuf>) -> Result<TaxonomyDB> {
    let taxonomy_dir = if let Some(path) = taxonomy_path {
        path.clone()
    } else {
        // Use default location from centralized paths
        use crate::core::paths;
        let default_path = paths::talaria_data_dir().join("taxonomy").join("ncbi");

        if !default_path.exists() {
            println!("{} Taxonomy database not found. Downloading NCBI taxonomy...",
                     "►".yellow().bold());
            download_ncbi_taxonomy(&default_path)?;
        }

        default_path
    };

    println!("{} Loading taxonomy database...", "►".cyan().bold());

    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap());
    pb.set_message("Parsing taxonomy files...");

    let names_file = taxonomy_dir.join("names.dmp");
    let nodes_file = taxonomy_dir.join("nodes.dmp");

    let taxonomy_db = ncbi::parse_ncbi_taxonomy(&names_file, &nodes_file)
        .context("Failed to parse taxonomy database")?;

    pb.finish_with_message(format!("Loaded {} taxa", taxonomy_db.taxa_count()));

    Ok(taxonomy_db)
}

fn download_ncbi_taxonomy(target_dir: &PathBuf) -> Result<()> {
    use std::fs;
    use flate2::read::GzDecoder;
    use tar::Archive;

    fs::create_dir_all(target_dir)?;

    let pb = ProgressBar::new(100);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
        .unwrap()
        .progress_chars("=>-"));

    pb.set_message("Downloading NCBI taxonomy...");
    pb.set_position(25);

    let url = "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/taxdump.tar.gz";
    let response = reqwest::blocking::get(url)
        .context("Failed to download taxonomy")?;

    pb.set_position(50);
    pb.set_message("Extracting taxonomy files...");

    let tar_gz = GzDecoder::new(response);
    let mut archive = Archive::new(tar_gz);
    archive.unpack(target_dir)
        .context("Failed to extract taxonomy")?;

    pb.set_position(100);
    pb.finish_with_message("Taxonomy downloaded successfully");

    Ok(())
}

fn analyze_database(path: &PathBuf, taxonomy_db: &TaxonomyDB) -> Result<TaxonomyCoverage> {
    let db_name = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut coverage = TaxonomyCoverage::new(db_name);

    // Parse FASTA file
    let sequences = fasta::parse_fasta(path)
        .context("Failed to parse FASTA file")?;

    let pb = ProgressBar::new(sequences.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
        .unwrap()
        .progress_chars("=>-"));
    pb.set_message("Analyzing sequences...");

    // Extract taxon IDs from sequences
    for seq in &sequences {
        // Try taxon_id field first, then extract from description
        let taxon_id = seq.taxon_id
            .or_else(|| seq.description.as_ref().and_then(|desc| extract_taxon_id(desc)))
            .or_else(|| extract_taxon_from_accession(&seq.id))
            .unwrap_or(1); // Default to root if not found

        coverage.add_sequence(taxon_id);
        pb.inc(1);
    }

    pb.finish_with_message("Analysis complete");

    // Calculate statistics
    coverage.calculate_stats(taxonomy_db);

    Ok(coverage)
}

fn extract_taxon_id(header: &str) -> Option<u32> {
    // Look for patterns like "TaxID=12345" or "tax_id:12345" or "[taxid:12345]"
    let patterns = vec![
        r"TaxID[=:](\d+)",
        r"tax_id[=:](\d+)",
        r"\[taxid:(\d+)\]",
        r"OX=(\d+)",  // UniProt format
    ];

    for pattern in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(captures) = re.captures(header) {
                if let Some(taxid_str) = captures.get(1) {
                    if let Ok(taxid) = taxid_str.as_str().parse::<u32>() {
                        return Some(taxid);
                    }
                }
            }
        }
    }

    None
}

fn extract_taxon_from_accession(_accession: &str) -> Option<u32> {
    // This would require an accession2taxid mapping file
    // For now, return None
    None
}

fn generate_text_report(
    coverage: &TaxonomyCoverage,
    comparison: Option<&TaxonomyCoverage>,
    taxonomy_db: &TaxonomyDB,
    max_depth: usize,
) -> Result<String> {
    use std::fmt::Write;
    let mut report = String::new();

    writeln!(report, "\n{}", "═".repeat(80))?;
    writeln!(report, "{:^80}", "TAXONOMIC COVERAGE REPORT")?;
    writeln!(report, "{}", "═".repeat(80))?;

    // Primary database statistics
    writeln!(report, "\n{} {}", "Database:".bold(), coverage.database)?;
    writeln!(report, "{} {}", "Total sequences:".bold(), coverage.total_sequences)?;
    writeln!(report, "{} {}", "Unique taxa:".bold(), coverage.unique_taxa)?;
    writeln!(report)?;

    // Coverage by rank
    writeln!(report, "{}", "Coverage by Taxonomic Rank:".bold().underline())?;
    writeln!(report, "{:<20} {:>10} {:>15} {:>10}", "Rank", "Taxa", "Sequences", "Percentage")?;
    writeln!(report, "{}", "-".repeat(60))?;

    let important_ranks = vec!["superkingdom", "kingdom", "phylum", "class", "order", "family", "genus", "species"];

    for rank in &important_ranks {
        if let Some(stats) = coverage.rank_coverage.get(*rank) {
            writeln!(report, "{:<20} {:>10} {:>15} {:>10.1}%",
                     rank,
                     stats.unique_taxa,
                     stats.count,
                     stats.percentage)?;
        }
    }

    // Taxonomic tree
    writeln!(report, "\n{}", "Taxonomic Tree (Top Taxa):".bold().underline())?;
    let tree = coverage.build_tree(taxonomy_db, Some(1));
    report.push_str(&format_tree(&tree, "", false, Some(max_depth), 0));

    // Comparison if provided
    if let Some(other) = comparison {
        writeln!(report, "\n{}", "═".repeat(80))?;
        writeln!(report, "{:^80}", "COMPARISON ANALYSIS")?;
        writeln!(report, "{}", "═".repeat(80))?;

        let comp = coverage.compare(other);

        writeln!(report, "\n{} {} vs {}", "Comparing:".bold(), comp.db1, comp.db2)?;
        writeln!(report, "{} {}", "Common taxa:".bold(), comp.common_taxa_count)?;
        writeln!(report, "{} {}", "Unique to first:".bold(), comp.unique_to_db1)?;
        writeln!(report, "{} {}", "Unique to second:".bold(), comp.unique_to_db2)?;
    }

    Ok(report)
}

fn generate_json_report(
    coverage: &TaxonomyCoverage,
    comparison: Option<&TaxonomyCoverage>,
) -> Result<String> {
    use serde_json::json;

    let mut report = json!({
        "database": coverage.database,
        "total_sequences": coverage.total_sequences,
        "unique_taxa": coverage.unique_taxa,
        "rank_coverage": coverage.rank_coverage,
        "taxon_counts": coverage.taxon_counts,
    });

    if let Some(other) = comparison {
        let comp = coverage.compare(other);
        report["comparison"] = json!({
            "database1": comp.db1,
            "database2": comp.db2,
            "common_taxa": comp.common_taxa_count,
            "unique_to_db1": comp.unique_to_db1,
            "unique_to_db2": comp.unique_to_db2,
        });
    }

    Ok(serde_json::to_string_pretty(&report)?)
}

fn generate_csv_report(
    coverage: &TaxonomyCoverage,
    _comparison: Option<&TaxonomyCoverage>,
) -> Result<String> {
    use std::fmt::Write;
    let mut csv = String::new();

    // Header
    writeln!(csv, "TaxonID,Count,Percentage")?;

    // Data rows
    let total = coverage.total_sequences as f64;
    for (taxon_id, count) in &coverage.taxon_counts {
        let percentage = (*count as f64 / total) * 100.0;
        writeln!(csv, "{},{},{:.2}", taxon_id, count, percentage)?;
    }

    Ok(csv)
}

fn generate_html_report(
    coverage: &TaxonomyCoverage,
    comparison: Option<&TaxonomyCoverage>,
    _max_depth: usize,
) -> Result<String> {
    let mut html = String::new();
    html.push_str(r#"<!DOCTYPE html>
<html>
<head>
    <title>Taxonomic Coverage Report</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 20px; }
        h1, h2 { color: #333; }
        table { border-collapse: collapse; width: 100%; margin: 20px 0; }
        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
        th { background-color: #f2f2f2; }
        .tree { font-family: monospace; white-space: pre; background: #f5f5f5; padding: 10px; }
        .stats { display: flex; gap: 20px; margin: 20px 0; }
        .stat-box { background: #f0f0f0; padding: 15px; border-radius: 5px; }
    </style>
</head>
<body>
    <h1>Taxonomic Coverage Report</h1>
"#);

    // Database statistics
    html.push_str(&format!(r#"
    <div class="stats">
        <div class="stat-box">
            <h3>Database: {}</h3>
            <p>Total Sequences: {}</p>
            <p>Unique Taxa: {}</p>
        </div>
    </div>
"#, coverage.database, coverage.total_sequences, coverage.unique_taxa));

    // Rank coverage table
    html.push_str(r#"
    <h2>Coverage by Taxonomic Rank</h2>
    <table>
        <tr>
            <th>Rank</th>
            <th>Unique Taxa</th>
            <th>Sequences</th>
            <th>Percentage</th>
        </tr>
"#);

    for (rank, stats) in &coverage.rank_coverage {
        html.push_str(&format!(r#"
        <tr>
            <td>{}</td>
            <td>{}</td>
            <td>{}</td>
            <td>{:.2}%</td>
        </tr>
"#, rank, stats.unique_taxa, stats.count, stats.percentage));
    }

    html.push_str("</table>");

    // Comparison if present
    if let Some(other) = comparison {
        let comp = coverage.compare(other);
        html.push_str(&format!(r#"
    <h2>Comparison: {} vs {}</h2>
    <div class="stats">
        <div class="stat-box">
            <p>Common Taxa: {}</p>
            <p>Unique to {}: {}</p>
            <p>Unique to {}: {}</p>
        </div>
    </div>
"#, comp.db1, comp.db2, comp.common_taxa_count, comp.db1, comp.unique_to_db1, comp.db2, comp.unique_to_db2));
    }

    html.push_str("</body></html>");
    Ok(html)
}