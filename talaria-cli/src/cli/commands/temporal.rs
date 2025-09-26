use talaria_sequoia::RetroactiveAnalyzer;
use talaria_sequoia::traits::temporal::*;
use talaria_sequoia::traits::renderable::{EvolutionRenderable, DiffRenderable, TemporalRenderable};
use talaria_sequoia::{BiTemporalCoordinate, TaxonId};
use crate::cli::formatting::output::*;
use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
/// Temporal query commands for bi-temporal database operations
///
/// Enables retroactive analysis, historical reproduction, and temporal joins
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args)]
pub struct TemporalArgs {
    #[command(subcommand)]
    pub command: TemporalCommand,
}

#[derive(Subcommand)]
pub enum TemporalCommand {
    /// Reproduce exact database state at a specific time
    Reproduce(ReproduceArgs),

    /// Apply modern taxonomy to historical sequences
    Retroactive(RetroactiveArgs),

    /// Show how classifications evolved over time
    Evolution(EvolutionArgs),

    /// Find sequences affected by taxonomic reclassifications
    Join(JoinArgs),

    /// Compare two temporal snapshots
    Diff(DiffArgs),
}

#[derive(Args)]
pub struct ReproduceArgs {
    /// Date to reproduce (YYYY-MM-DD)
    #[arg(long)]
    pub date: String,

    /// Specific taxon to extract (name or ID)
    #[arg(long)]
    pub taxon: Option<String>,

    /// Output file for sequences
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Database to query
    #[arg(short, long)]
    pub database: Option<String>,

    /// Output format (tree, table, json)
    #[arg(long, default_value = "tree")]
    pub format: String,
}

#[derive(Args)]
pub struct RetroactiveArgs {
    /// Date for sequences (YYYY-MM-DD)
    #[arg(long)]
    pub sequences_from: String,

    /// Date for taxonomy (YYYY-MM-DD or "latest")
    #[arg(long)]
    pub taxonomy: String,

    /// Filter by taxon IDs
    #[arg(long)]
    pub taxon_ids: Option<String>,

    /// Output file for results
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Database to query
    #[arg(short, long)]
    pub database: Option<String>,

    /// Show detailed changes
    #[arg(long)]
    pub detailed: bool,
}

#[derive(Args)]
pub struct EvolutionArgs {
    /// Sequence or taxon ID to track
    #[arg(long)]
    pub entity_id: String,

    /// Start date (YYYY-MM-DD)
    #[arg(long)]
    pub from: String,

    /// End date (YYYY-MM-DD or "now")
    #[arg(long, default_value = "now")]
    pub to: String,

    /// Database to query
    #[arg(short, long)]
    pub database: Option<String>,

    /// Visualization type (timeline, graph, stats)
    #[arg(long, default_value = "timeline")]
    pub view: String,
}

#[derive(Args)]
pub struct JoinArgs {
    /// Reference date (YYYY-MM-DD)
    #[arg(long)]
    pub date: String,

    /// Comparison date (YYYY-MM-DD or "now")
    #[arg(long, default_value = "now")]
    pub compare_to: String,

    /// Filter by specific taxon
    #[arg(long)]
    pub taxon: Option<String>,

    /// Find reclassified sequences
    #[arg(long)]
    pub find_reclassified: bool,

    /// Database to query
    #[arg(short, long)]
    pub database: Option<String>,

    /// Output format (tree, table, csv)
    #[arg(long, default_value = "table")]
    pub format: String,
}

#[derive(Args)]
pub struct DiffArgs {
    /// First date (YYYY-MM-DD)
    #[arg(long)]
    pub from: String,

    /// Second date (YYYY-MM-DD)
    #[arg(long)]
    pub to: String,

    /// Database to query
    #[arg(short, long)]
    pub database: Option<String>,

    /// Diff style (unified, side-by-side, stats)
    #[arg(long, default_value = "unified")]
    pub style: String,

    /// Filter by taxon
    #[arg(long)]
    pub taxon: Option<String>,
}

pub fn run(args: TemporalArgs) -> Result<()> {
    match args.command {
        TemporalCommand::Reproduce(args) => run_reproduce(args),
        TemporalCommand::Retroactive(args) => run_retroactive(args),
        TemporalCommand::Evolution(args) => run_evolution(args),
        TemporalCommand::Join(args) => run_join(args),
        TemporalCommand::Diff(args) => run_diff(args),
    }
}

fn run_reproduce(args: ReproduceArgs) -> Result<()> {
    action("Reproducing historical database state");

    let analyzer = create_analyzer(&args.database)?;
    let date = parse_date(&args.date)?;
    let coordinate = BiTemporalCoordinate::at(date);

    // Parse taxon filter if provided
    let taxon_ids = if let Some(taxon) = args.taxon {
        Some(parse_taxon_filter(&taxon)?)
    } else {
        None
    };

    let query = SnapshotQuery {
        coordinate,
        taxon_filter: taxon_ids,
    };

    info(&format!("Querying snapshot at {}", date.format("%Y-%m-%d")));

    let snapshot = analyzer.query_snapshot(query)?;

    // Render based on format
    match args.format.as_str() {
        "tree" => {
            for node in snapshot.render_tree() {
                print_tree_node(&node, 0);
            }
        }
        "table" => {
            println!("{}", snapshot.render_table());
        }
        "json" => {
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        _ => {
            println!("{}", snapshot.render_summary());
        }
    }

    // Write sequences if output specified
    if let Some(output_path) = args.output {
        write_sequences(&snapshot.sequences, &output_path)?;
        success(&format!(
            "Wrote {} sequences to {:?}",
            snapshot.sequences.len(),
            output_path
        ));
    }

    Ok(())
}

fn run_retroactive(args: RetroactiveArgs) -> Result<()> {
    action("Performing retroactive analysis");

    let analyzer = create_analyzer(&args.database)?;
    let sequence_date = parse_date(&args.sequences_from)?;
    let taxonomy_date = if args.taxonomy == "latest" {
        Utc::now()
    } else {
        parse_date(&args.taxonomy)?
    };

    let coordinate = BiTemporalCoordinate::new(sequence_date, taxonomy_date);

    info(&format!(
        "Applying {} taxonomy to {} sequences",
        if args.taxonomy == "latest" {
            "latest"
        } else {
            &args.taxonomy
        },
        args.sequences_from
    ));

    // Parse taxon filter
    let taxon_ids = if let Some(taxon_str) = args.taxon_ids {
        Some(parse_taxon_filter(&taxon_str)?)
    } else {
        None
    };

    let query = SnapshotQuery {
        coordinate,
        taxon_filter: taxon_ids,
    };

    let retroactive_result = analyzer.query_snapshot(query)?;

    // Show summary
    println!("{}", retroactive_result.render_summary());

    if args.detailed {
        println!("\nDetailed Results:");
        println!("{}", retroactive_result.render_table());
    }

    // Write results if output specified
    if let Some(output_path) = args.output {
        write_sequences(&retroactive_result.sequences, &output_path)?;
        success(&format!("Wrote retroactive analysis to {:?}", output_path));
    }

    Ok(())
}

fn run_evolution(args: EvolutionArgs) -> Result<()> {
    action("Tracking taxonomic evolution");

    let analyzer = create_analyzer(&args.database)?;
    let from_date = parse_date(&args.from)?;
    let to_date = if args.to == "now" {
        Utc::now()
    } else {
        parse_date(&args.to)?
    };

    let query = EvolutionQuery {
        entity_id: args.entity_id.clone(),
        from_date,
        to_date,
    };

    info(&format!(
        "Tracking {} from {} to {}",
        args.entity_id,
        from_date.format("%Y-%m-%d"),
        to_date.format("%Y-%m-%d")
    ));

    let history = analyzer.query_evolution(query)?;

    // Render based on view type
    match args.view.as_str() {
        "timeline" => {
            println!("{}", history.render_evolution_timeline());
        }
        "graph" => {
            println!("{}", history.render_evolution_graph());
        }
        "stats" => {
            println!("{}", history.render_evolution_stats());
        }
        _ => {
            println!("{}", history.render_evolution_timeline());
        }
    }

    Ok(())
}

fn run_join(args: JoinArgs) -> Result<()> {
    action("Finding reclassified sequences");

    let analyzer = create_analyzer(&args.database)?;
    let reference_date = parse_date(&args.date)?;
    let comparison_date = if args.compare_to == "now" {
        Some(Utc::now())
    } else {
        Some(parse_date(&args.compare_to)?)
    };

    let query = JoinQuery {
        reference_date,
        comparison_date,
        taxon_filter: if let Some(taxon) = args.taxon {
            Some(parse_taxon_filter(&taxon)?)
        } else {
            None
        },
        find_reclassified: args.find_reclassified,
    };

    info(&format!(
        "Comparing {} to {}",
        reference_date.format("%Y-%m-%d"),
        comparison_date
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "current".to_string())
    ));

    let join_result = analyzer.query_join(query)?;

    // Render based on format
    match args.format.as_str() {
        "tree" => {
            for node in join_result.render_tree() {
                print_tree_node(&node, 0);
            }
        }
        "table" => {
            println!("{}", join_result.render_table());
        }
        "csv" => {
            // Output as CSV for further processing
            println!("old_taxon,new_taxon,count");
            for group in &join_result.reclassified {
                println!(
                    "{},{},{}",
                    group.old_taxon.map(|t| t.0.to_string()).unwrap_or_default(),
                    group.new_taxon.map(|t| t.0.to_string()).unwrap_or_default(),
                    group.count
                );
            }
        }
        _ => {
            println!("{}", join_result.render_summary());
        }
    }

    Ok(())
}

fn run_diff(args: DiffArgs) -> Result<()> {
    action("Computing temporal diff");

    let analyzer = create_analyzer(&args.database)?;
    let from_date = parse_date(&args.from)?;
    let to_date = parse_date(&args.to)?;

    let from_coordinate = BiTemporalCoordinate::at(from_date);
    let to_coordinate = BiTemporalCoordinate::at(to_date);

    let query = DiffQuery {
        from: from_coordinate,
        to: to_coordinate,
        taxon_filter: if let Some(taxon) = args.taxon {
            Some(parse_taxon_filter(&taxon)?)
        } else {
            None
        },
    };

    info(&format!(
        "Comparing {} to {}",
        from_date.format("%Y-%m-%d"),
        to_date.format("%Y-%m-%d")
    ));

    let diff = analyzer.query_diff(query)?;

    // Render based on style
    match args.style.as_str() {
        "unified" => {
            println!("{}", diff.render_unified_diff());
        }
        "side-by-side" => {
            println!("{}", diff.render_side_by_side());
        }
        "stats" => {
            println!("{}", diff.render_diff_stats());
        }
        _ => {
            println!("{}", diff.render_unified_diff());
        }
    }

    Ok(())
}

// Helper functions

fn create_analyzer(database: &Option<String>) -> Result<RetroactiveAnalyzer> {
    use talaria_sequoia::SEQUOIARepository;
    use talaria_core::system::paths::talaria_home;

    let base_path = if let Some(db) = database {
        talaria_home().join("databases").join("data").join(db)
    } else {
        talaria_home().join("databases").join("data")
    };

    let repository = SEQUOIARepository::open(&base_path)?;
    Ok(RetroactiveAnalyzer::from_repository(repository))
}

fn parse_date(date_str: &str) -> Result<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?;
    Ok(DateTime::from_naive_utc_and_offset(
        date.and_hms_opt(0, 0, 0).unwrap(),
        Utc,
    ))
}

fn parse_taxon_filter(taxon_str: &str) -> Result<Vec<TaxonId>> {
    // Try to parse as comma-separated IDs first
    if let Ok(ids) = taxon_str
        .split(',')
        .map(|s| s.trim().parse::<u32>().map(TaxonId))
        .collect::<Result<Vec<_>, _>>()
    {
        return Ok(ids);
    }

    // Otherwise treat as taxon name and look up
    // For now, return empty vec - would need taxonomy lookup
    use crate::cli::formatting::output::warning;
    warning(&format!(
        "Taxon name lookup not yet implemented for '{}'",
        taxon_str
    ));
    Ok(vec![])
}

fn write_sequences(sequences: &[talaria_bio::sequence::Sequence], path: &PathBuf) -> Result<()> {
    use std::io::Write;

    let mut file = std::fs::File::create(path)?;

    for seq in sequences {
        writeln!(file, ">{}", seq.header())?;

        // Write sequence in 80-character lines
        for chunk in seq.sequence.chunks(80) {
            writeln!(file, "{}", String::from_utf8_lossy(chunk))?;
        }
    }

    Ok(())
}

fn print_tree_node(node: &talaria_utils::display::TreeNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let prefix = if depth == 0 { "" } else { "├─ " };

    println!("{}{}{}", indent, prefix, node.name);

    for child in &node.children {
        print_tree_node(child, depth + 1);
    }
}
