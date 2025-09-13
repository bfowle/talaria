use clap::Args;
use std::path::PathBuf;
use comfy_table::{Table, Cell, Attribute, ContentArrangement, Color};
use comfy_table::presets::UTF8_FULL;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;

#[derive(Args)]
pub struct ListArgs {
    /// Directory to search for databases (overrides default)
    #[arg(short, long)]
    pub directory: Option<PathBuf>,
    
    /// Show detailed information
    #[arg(long)]
    pub detailed: bool,
    
    /// Show all versions (not just current)
    #[arg(long)]
    pub all_versions: bool,
    
    /// Specific database to list (e.g., "uniprot/swissprot")
    #[arg(long)]
    pub database: Option<String>,
    
    /// Sort by field (name, size, date)
    #[arg(long, default_value = "name")]
    pub sort: SortField,
    
    /// Show reduced versions
    #[arg(long)]
    pub show_reduced: bool,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum SortField {
    Name,
    Size,
    Date,
}

pub fn run(args: ListArgs) -> anyhow::Result<()> {
    
    
    use humansize::{format_size, BINARY};
    use crate::core::database_manager::DatabaseManager;
    use crate::core::config::load_config;
    
    // Load config to get database settings
    let config = load_config("talaria.toml").unwrap_or_default();
    
    // Use provided directory or default from config
    let base_dir = args.directory
        .map(|d| d.to_string_lossy().to_string())
        .or(config.database.database_dir.clone());
    
    // Initialize database manager
    let db_manager = DatabaseManager::new(base_dir)?;
    
    // Handle specific database listing
    if let Some(database_ref) = args.database {
        let reference = db_manager.parse_reference(&database_ref)?;
        let versions = db_manager.list_versions(&reference.source, &reference.dataset)?;
        
        if versions.is_empty() {
            println!("No versions found for {}/{}", reference.source, reference.dataset);
            return Ok(());
        }
        
        // Create table for version listing
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_content_arrangement(ContentArrangement::Dynamic);
        
        // Set header
        table.set_header(vec![
            Cell::new("Version").add_attribute(Attribute::Bold).fg(Color::Green),
            Cell::new("Size").add_attribute(Attribute::Bold).fg(Color::Green),
            Cell::new("Modified").add_attribute(Attribute::Bold).fg(Color::Green),
            Cell::new("Status").add_attribute(Attribute::Bold).fg(Color::Green),
        ]);
        
        // Add version rows
        for version in &versions {
            let size_str = format_size(version.size, BINARY);
            let date_str = version.modified.format("%Y-%m-%d %H:%M:%S");
            let status = if version.is_current { 
                Cell::new("CURRENT").fg(Color::Cyan).add_attribute(Attribute::Bold)
            } else { 
                Cell::new("") 
            };
            
            table.add_row(vec![
                Cell::new(&version.version),
                Cell::new(size_str),
                Cell::new(date_str.to_string()),
                status,
            ]);
            
            if args.detailed {
                // Add path info as a sub-row
                table.add_row(vec![
                    Cell::new("  Path:").fg(Color::DarkGrey),
                    Cell::new(version.path.display().to_string())
                        .fg(Color::DarkGrey)
                        .add_attribute(Attribute::Italic),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }
        }
        
        println!("Versions for {}/{}:\n", reference.source, reference.dataset);
        println!("{}", table);
        
        return Ok(());
    }
    
    // List all databases
    let databases = db_manager.list_all_databases()?;
    
    if databases.is_empty() {
        println!("No databases found. Use 'talaria database download' to get started.");
        return Ok(());
    }
    
    // Sort databases
    let mut sorted_databases = databases;
    match args.sort {
        SortField::Name => sorted_databases.sort_by(|a, b| 
            format!("{}/{}", a.source, a.dataset).cmp(&format!("{}/{}", b.source, b.dataset))),
        SortField::Size => sorted_databases.sort_by_key(|d| d.size),
        SortField::Date => sorted_databases.sort_by(|a, b| a.modified.cmp(&b.modified)),
    }
    
    if args.detailed {
        // Detailed view - create a separate table for each database
        for db in &sorted_databases {
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic);
            
            // Database name as header with custom indicator
            let header_text = if db_manager.is_custom_database(&db.source) {
                format!("{}/{} [custom]", db.source, db.dataset)
            } else {
                format!("{}/{}", db.source, db.dataset)
            };

            table.set_header(vec![
                Cell::new(header_text)
                    .add_attribute(Attribute::Bold)
                    .fg(if db_manager.is_custom_database(&db.source) {
                        Color::Cyan
                    } else {
                        Color::Green
                    }),
                Cell::new("")
            ]);
            
            // Add details
            table.add_row(vec![
                Cell::new("Current Version"),
                Cell::new(&db.current_version).add_attribute(Attribute::Bold),
            ]);
            table.add_row(vec![
                Cell::new("Path"),
                Cell::new(db.current_path.display().to_string()),
            ]);
            table.add_row(vec![
                Cell::new("Size"),
                Cell::new(format_size(db.size, BINARY)).add_attribute(Attribute::Bold),
            ]);
            table.add_row(vec![
                Cell::new("Modified"),
                Cell::new(db.modified.format("%Y-%m-%d %H:%M:%S").to_string()),
            ]);
            table.add_row(vec![
                Cell::new("Total Versions"),
                Cell::new(db.version_count.to_string()).add_attribute(Attribute::Bold),
            ]);
            
            // Show reductions in detailed view
            if args.show_reduced && !db.reductions.is_empty() {
                table.add_row(vec![
                    Cell::new("Reductions"),
                    Cell::new(format!("{} profile(s)", db.reductions.len())).add_attribute(Attribute::Bold),
                ]);
                
                for reduction in &db.reductions {
                    table.add_row(vec![
                        Cell::new(format!("  • {}", reduction.profile)).fg(Color::Cyan),
                        Cell::new(format!("{}% ({} sequences, {})", 
                            (reduction.reduction_ratio * 100.0) as u32,
                            reduction.sequences,
                            format_size(reduction.size, BINARY)
                        )).fg(Color::DarkGrey),
                    ]);
                }
            }
            
            println!("{}", table);
            
            if args.all_versions && db.version_count > 1 {
                let versions = db_manager.list_versions(&db.source, &db.dataset)?;
                
                let mut version_table = Table::new();
                version_table
                    .load_preset(UTF8_FULL)
                    .apply_modifier(UTF8_ROUND_CORNERS)
                    .set_content_arrangement(ContentArrangement::Dynamic);
                
                version_table.set_header(vec![
                    Cell::new("All Versions").add_attribute(Attribute::Bold).fg(Color::Green),
                    Cell::new("Size").add_attribute(Attribute::Bold).fg(Color::Green),
                    Cell::new("Status").add_attribute(Attribute::Bold).fg(Color::Green),
                ]);
                
                for version in &versions {
                    let status = if version.is_current {
                        Cell::new("CURRENT").fg(Color::Cyan).add_attribute(Attribute::Bold)
                    } else {
                        Cell::new("")
                    };
                    
                    version_table.add_row(vec![
                        Cell::new(&version.version),
                        Cell::new(format_size(version.size, BINARY)),
                        status,
                    ]);
                }
                
                println!("\n{}", version_table);
            }
            println!();
        }
    } else {
        // Regular view - single table with all databases
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_content_arrangement(ContentArrangement::Dynamic);
        
        // Set header
        table.set_header(vec![
            Cell::new("Database").add_attribute(Attribute::Bold).fg(Color::Green),
            Cell::new("Version").add_attribute(Attribute::Bold).fg(Color::Green),
            Cell::new("Size").add_attribute(Attribute::Bold).fg(Color::Green),
            Cell::new("Modified").add_attribute(Attribute::Bold).fg(Color::Green),
            Cell::new("Versions").add_attribute(Attribute::Bold).fg(Color::Green),
        ]);
        
        // Add database rows
        for db in &sorted_databases {
            let size_str = format_size(db.size, BINARY);
            let date_str = db.modified.format("%Y-%m-%d %H:%M:%S");

            // Add indicator for custom databases
            let db_name = if db_manager.is_custom_database(&db.source) {
                format!("{}/{} [custom]", db.source, db.dataset)
            } else {
                format!("{}/{}", db.source, db.dataset)
            };

            table.add_row(vec![
                Cell::new(db_name)
                    .fg(if db_manager.is_custom_database(&db.source) {
                        Color::Cyan
                    } else {
                        Color::Reset
                    }),
                Cell::new(&db.current_version),
                Cell::new(size_str),
                Cell::new(date_str.to_string()),
                Cell::new(db.version_count.to_string()).add_attribute(Attribute::Bold),
            ]);
            
            // Show reductions if flag is set and they exist
            if args.show_reduced && !db.reductions.is_empty() {
                for reduction in &db.reductions {
                    let reduced_name = format!("  └─ {}", reduction.profile);
                    let reduced_size = format_size(reduction.size, BINARY);
                    let reduction_percent = format!("{}%", (reduction.reduction_ratio * 100.0) as u32);
                    let seq_count = format!("{} seqs", reduction.sequences);
                    
                    table.add_row(vec![
                        Cell::new(reduced_name).fg(Color::DarkGrey).add_attribute(Attribute::Italic),
                        Cell::new(reduction_percent).fg(Color::DarkGrey),
                        Cell::new(reduced_size).fg(Color::DarkGrey),
                        Cell::new("").fg(Color::DarkGrey),
                        Cell::new(seq_count).fg(Color::DarkGrey),
                    ]);
                }
            }
        }
        
        println!("Found {} database(s):\n", sorted_databases.len());
        println!("{}", table);
    }
    
    Ok(())
}