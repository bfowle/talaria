use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct InfoArgs {
    /// Database reference (e.g., "uniprot/swissprot") or file path
    pub database: String,
    
    /// Show sequence statistics
    #[arg(long)]
    pub stats: bool,
    
    /// Show taxonomic distribution
    #[arg(long)]
    pub taxonomy: bool,
    
    /// Output format
    #[arg(long, value_enum, default_value = "text")]
    pub format: OutputFormat,
    
    /// Show reduction profiles if available
    #[arg(long)]
    pub show_reductions: bool,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

pub fn run(args: InfoArgs) -> anyhow::Result<()> {
    use crate::bio::fasta::parse_fasta;
    use crate::core::database_manager::DatabaseManager;
    use crate::core::config::load_config;
    use std::collections::HashMap;
    
    // Determine if this is a database reference or a file path
    let db_path = if args.database.contains('/') && !args.database.contains('.') {
        // Looks like a database reference (e.g., "uniprot/swissprot")
        let config = load_config("talaria.toml").unwrap_or_default();
        let db_manager = DatabaseManager::new(config.database.database_dir)?;
        
        // Parse and resolve the database reference
        let db_ref = db_manager.parse_reference(&args.database)?;
        let db_dir = db_manager.resolve_reference(&db_ref)?;
        
        // Find the FASTA file in the database directory
        db_manager.find_fasta_in_dir(&db_dir)?
    } else {
        // Direct file path
        PathBuf::from(&args.database)
    };
    
    if !db_path.exists() {
        anyhow::bail!("Database file does not exist: {}", db_path.display());
    }
    
    let file_metadata = std::fs::metadata(&db_path)?;
    let file_size = file_metadata.len();
    let modified = file_metadata.modified()?;
    
    println!("Loading database...");
    let sequences = parse_fasta(&db_path)?;
    
    let count = sequences.len();
    let mut total_length = 0;
    let mut min_length = usize::MAX;
    let mut max_length = 0;
    let mut length_sum_sq = 0.0;
    let mut lengths = Vec::new();
    let mut taxon_counts = HashMap::new();
    
    println!("Analyzing {} sequences...", count);
    
    for sequence in &sequences {
        let length = sequence.sequence.len();
        total_length += length;
        min_length = min_length.min(length);
        max_length = max_length.max(length);
        lengths.push(length);
        length_sum_sq += (length as f64).powi(2);
        
        if args.taxonomy {
            if let Some(taxon_id) = sequence.taxon_id {
                *taxon_counts.entry(taxon_id).or_insert(0) += 1;
            }
        }
    }
    
    println!("\r");
    
    let avg_length = if count > 0 { total_length / count } else { 0 };
    let std_dev = if count > 1 {
        let mean = total_length as f64 / count as f64;
        ((length_sum_sq / count as f64) - mean.powi(2)).sqrt()
    } else {
        0.0
    };
    
    lengths.sort_unstable();
    let median_length = if !lengths.is_empty() {
        lengths[lengths.len() / 2]
    } else {
        0
    };
    
    // Check for reduction profiles if requested
    let reductions = if args.show_reductions && args.database.contains('/') && !args.database.contains('.') {
        let config = load_config("talaria.toml").unwrap_or_default();
        let db_manager = DatabaseManager::new(config.database.database_dir)?;
        let db_ref = db_manager.parse_reference(&args.database)?;
        let db_dir = db_manager.resolve_reference(&db_ref)?;
        
        let reduced_dir = db_dir.join("reduced");
        if reduced_dir.exists() {
            let mut profiles = Vec::new();
            for entry in std::fs::read_dir(&reduced_dir)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        profiles.push(name.to_string());
                    }
                }
            }
            Some(profiles)
        } else {
            None
        }
    } else {
        None
    };
    
    match args.format {
        OutputFormat::Text => print_text_info(
            &db_path,
            file_size,
            &modified,
            count,
            total_length,
            min_length,
            max_length,
            avg_length,
            median_length,
            std_dev,
            &taxon_counts,
            args.stats,
            args.taxonomy,
            reductions.as_ref(),
            &args.database,
        ),
        OutputFormat::Json => print_json_info(
            &db_path,
            file_size,
            &modified,
            count,
            total_length,
            min_length,
            max_length,
            avg_length,
            median_length,
            std_dev,
            &taxon_counts,
            reductions.as_ref(),
        ),
    }
    
    Ok(())
}

fn print_text_info(
    path: &PathBuf,
    file_size: u64,
    modified: &std::time::SystemTime,
    count: usize,
    total_length: usize,
    min_length: usize,
    max_length: usize,
    avg_length: usize,
    median_length: usize,
    std_dev: f64,
    taxon_counts: &std::collections::HashMap<u32, usize>,
    show_stats: bool,
    show_taxonomy: bool,
    reductions: Option<&Vec<String>>,
    database_ref: &str,
) {
    use chrono::{DateTime, Local};
    use humansize::{format_size, BINARY};
    use comfy_table::{Table, presets::UTF8_FULL, Cell, Attribute, Color};
    
    // Main database info table
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        Cell::new("Database Information").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new(database_ref).add_attribute(Attribute::Bold)
    ]);
    
    table.add_row(vec!["File", &path.display().to_string()]);
    table.add_row(vec!["Size", &format_size(file_size, BINARY)]);
    
    let modified_dt: DateTime<Local> = (*modified).into();
    table.add_row(vec!["Modified", &modified_dt.format("%Y-%m-%d %H:%M:%S").to_string()]);
    table.add_row(vec!["Sequences", &count.to_string()]);
    table.add_row(vec!["Total length", &format!("{} bp/aa", total_length)]);
    
    println!("{}", table);
    
    // Reduction profiles table if available
    if let Some(profiles) = reductions {
        if !profiles.is_empty() {
            println!();
            let mut red_table = Table::new();
            red_table.load_preset(UTF8_FULL);
            red_table.set_header(vec![
                Cell::new("Reduction Profiles").add_attribute(Attribute::Bold).fg(Color::Green)
            ]);
            
            for profile in profiles {
                red_table.add_row(vec![profile]);
            }
            
            println!("{}", red_table);
        }
    }
    
    // Length statistics table
    if show_stats {
        println!();
        let mut stats_table = Table::new();
        stats_table.load_preset(UTF8_FULL);
        stats_table.set_header(vec![
            Cell::new("Length Statistics").add_attribute(Attribute::Bold).fg(Color::Yellow),
            Cell::new("Value").add_attribute(Attribute::Bold)
        ]);
        
        stats_table.add_row(vec!["Minimum", &min_length.to_string()]);
        stats_table.add_row(vec!["Maximum", &max_length.to_string()]);
        stats_table.add_row(vec!["Average", &avg_length.to_string()]);
        stats_table.add_row(vec!["Median", &median_length.to_string()]);
        stats_table.add_row(vec!["Std Dev", &format!("{:.2}", std_dev)]);
        
        println!("{}", stats_table);
    }
    
    // Taxonomic distribution table
    if show_taxonomy && !taxon_counts.is_empty() {
        println!();
        let mut tax_table = Table::new();
        tax_table.load_preset(UTF8_FULL);
        tax_table.set_header(vec![
            Cell::new("Taxonomic Distribution").add_attribute(Attribute::Bold).fg(Color::Magenta),
            Cell::new("Count").add_attribute(Attribute::Bold)
        ]);
        
        tax_table.add_row(vec!["Unique taxa", &taxon_counts.len().to_string()]);
        
        let mut sorted_taxa: Vec<_> = taxon_counts.iter().collect();
        sorted_taxa.sort_by(|a, b| b.1.cmp(a.1));
        
        println!("{}", tax_table);
        
        // Top taxa table
        if !sorted_taxa.is_empty() {
            let mut top_table = Table::new();
            top_table.load_preset(UTF8_FULL);
            top_table.set_header(vec!["Top 10 Taxa", "Sequences"]);
            
            for (taxon_id, count) in sorted_taxa.iter().take(10) {
                top_table.add_row(vec![
                    &format!("Taxon {}", taxon_id),
                    &count.to_string()
                ]);
            }
            
            println!("{}", top_table);
        }
    }
}

fn print_json_info(
    path: &PathBuf,
    file_size: u64,
    modified: &std::time::SystemTime,
    count: usize,
    total_length: usize,
    min_length: usize,
    max_length: usize,
    avg_length: usize,
    median_length: usize,
    std_dev: f64,
    taxon_counts: &std::collections::HashMap<u32, usize>,
    reductions: Option<&Vec<String>>,
) {
    use serde_json::json;
    use chrono::{DateTime, Utc};
    
    let modified_dt: DateTime<Utc> = (*modified).into();
    
    let mut info = json!({
        "file": path.to_string_lossy(),
        "file_size": file_size,
        "modified": modified_dt.to_rfc3339(),
        "sequences": count,
        "total_length": total_length,
        "statistics": {
            "min_length": min_length,
            "max_length": max_length,
            "avg_length": avg_length,
            "median_length": median_length,
            "std_dev": std_dev,
        },
        "taxonomy": {
            "unique_taxa": taxon_counts.len(),
            "distribution": taxon_counts,
        }
    });
    
    if let Some(profiles) = reductions {
        info["reduction_profiles"] = json!(profiles);
    }
    
    println!("{}", serde_json::to_string_pretty(&info).unwrap());
}