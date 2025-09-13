use clap::Args;

#[derive(Args)]
pub struct ListSequencesArgs {
    /// Database reduction to list sequences from (e.g., "uniprot/swissprot:blast-30")
    /// Profile is required to find the specific reduction
    #[arg(value_name = "DATABASE:PROFILE")]
    pub database: String,
    
    /// Search for specific sequence IDs (partial match)
    #[arg(short, long)]
    pub search: Option<String>,
    
    /// Output format (text, json, csv)
    #[arg(short = 'f', long, default_value = "text")]
    pub format: String,
    
    /// Show only references (not delta children)
    #[arg(long)]
    pub references_only: bool,
    
    /// Show only delta children (not references)
    #[arg(long)]
    pub deltas_only: bool,
    
    /// Include sequence lengths
    #[arg(long)]
    pub show_lengths: bool,
    
    /// Include taxonomic IDs if available
    #[arg(long)]
    pub show_taxon: bool,
}

pub fn run(args: ListSequencesArgs) -> anyhow::Result<()> {
    use crate::core::database_manager::DatabaseManager;
    use crate::core::config::load_config;
    use crate::bio::fasta::parse_fasta;
    use crate::storage::metadata::load_metadata;
    use indicatif::{ProgressBar, ProgressStyle};
    
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
    );
    
    // Parse database reference with profile
    let (base_ref, profile) = parse_database_with_profile(&args.database)?;
    
    let profile = profile.ok_or_else(|| anyhow::anyhow!(
        "Reduction profile required. Use format: 'database:profile' (e.g., 'uniprot/swissprot:blast-30')"
    ))?;
    
    // Load config and database manager
    let config = load_config("talaria.toml").unwrap_or_default();
    let db_manager = DatabaseManager::new(config.database.database_dir)?;
    
    // Parse and resolve the database reference
    let db_ref = db_manager.parse_reference(&base_ref)?;
    let db_dir = db_manager.resolve_reference(&db_ref)?;
    
    // Find reduced directory
    let reduced_dir = db_dir.join("reduced").join(&profile);
    if !reduced_dir.exists() {
        anyhow::bail!("Reduction profile '{}' not found for {}/{}", 
                      profile, db_ref.source, db_ref.dataset);
    }
    
    pb.set_message(format!("Loading sequences from {}/{}:{}", 
                          db_ref.source, db_ref.dataset, profile));
    
    // Load reference sequences from FASTA
    let mut sequences = Vec::new();
    
    if !args.deltas_only {
        let fasta_path = db_manager.find_fasta_in_dir(&reduced_dir)?;
        pb.set_message("Loading reference sequences...");
        let references = parse_fasta(&fasta_path)?;
        
        for seq in references {
            let entry = SequenceEntry {
                id: seq.id.clone(),
                seq_type: "reference".to_string(),
                length: if args.show_lengths { Some(seq.sequence.len()) } else { None },
                taxon_id: if args.show_taxon { seq.taxon_id } else { None },
            };
            sequences.push(entry);
        }
        pb.set_message(format!("Loaded {} reference sequences", sequences.len()));
    }
    
    // Load delta child sequences
    if !args.references_only {
        let delta_path = find_delta_file(&reduced_dir)?;
        pb.set_message("Loading delta sequences...");
        let deltas = load_metadata(&delta_path)?;
        
        let delta_count = deltas.len();
        for delta in deltas {
            let entry = SequenceEntry {
                id: delta.child_id.clone(),
                seq_type: "delta".to_string(),
                length: None, // We don't have length without reconstruction
                taxon_id: if args.show_taxon { delta.taxon_id } else { None },
            };
            sequences.push(entry);
        }
        pb.set_message(format!("Loaded {} delta sequences", delta_count));
    }
    
    // Apply search filter if specified
    if let Some(search_term) = &args.search {
        let search_lower = search_term.to_lowercase();
        sequences.retain(|s| s.id.to_lowercase().contains(&search_lower));
        pb.set_message(format!("Found {} matching sequences", sequences.len()));
    }
    
    pb.finish_and_clear();
    
    // Output results
    match args.format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&sequences)?;
            println!("{}", json);
        }
        "csv" => {
            println!("sequence_id,type{}{}", 
                    if args.show_lengths { ",length" } else { "" },
                    if args.show_taxon { ",taxon_id" } else { "" });
            for seq in sequences {
                print!("{},{}", seq.id, seq.seq_type);
                if args.show_lengths {
                    print!(",{}", seq.length.map_or("N/A".to_string(), |l| l.to_string()));
                }
                if args.show_taxon {
                    print!(",{}", seq.taxon_id.map_or("N/A".to_string(), |t| t.to_string()));
                }
                println!();
            }
        }
        _ => {
            // Text format (default)
            use comfy_table::{Table, presets::UTF8_FULL};
            
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            
            let mut headers = vec!["Sequence ID", "Type"];
            if args.show_lengths {
                headers.push("Length");
            }
            if args.show_taxon {
                headers.push("Taxon ID");
            }
            table.set_header(headers);
            
            for seq in &sequences {
                let mut row = vec![seq.id.clone(), seq.seq_type.clone()];
                if args.show_lengths {
                    row.push(seq.length.map_or("N/A".to_string(), |l| format!("{}", l)));
                }
                if args.show_taxon {
                    row.push(seq.taxon_id.map_or("N/A".to_string(), |t| format!("{}", t)));
                }
                table.add_row(row);
            }
            
            println!("\n{}", table);
            
            // Summary
            let ref_count = sequences.iter().filter(|s| s.seq_type == "reference").count();
            let delta_count = sequences.iter().filter(|s| s.seq_type == "delta").count();
            
            println!("\nSummary for {}/{}:{}", db_ref.source, db_ref.dataset, profile);
            println!("  Reference sequences: {}", ref_count);
            println!("  Delta sequences:     {}", delta_count);
            println!("  Total sequences:     {}", sequences.len());
            
            if args.search.is_some() {
                println!("\n(Filtered by search term: '{}')", args.search.unwrap());
            }
        }
    }
    
    Ok(())
}

#[derive(serde::Serialize)]
struct SequenceEntry {
    id: String,
    #[serde(rename = "type")]
    seq_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    taxon_id: Option<u32>,
}

/// Parse a database reference that may include a reduction profile
fn parse_database_with_profile(reference: &str) -> anyhow::Result<(String, Option<String>)> {
    if let Some(colon_idx) = reference.find(':') {
        let base = &reference[..colon_idx];
        let remainder = &reference[colon_idx + 1..];
        
        if let Some(at_idx) = remainder.find('@') {
            let profile = &remainder[..at_idx];
            let version = &remainder[at_idx..];
            Ok((format!("{}{}", base, version), Some(profile.to_string())))
        } else {
            Ok((base.to_string(), Some(remainder.to_string())))
        }
    } else {
        Ok((reference.to_string(), None))
    }
}

/// Find a delta file in a directory
fn find_delta_file(dir: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    use std::fs;
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.contains(".deltas.") || name.ends_with(".deltas") || name.ends_with(".delta") {
                    return Ok(path);
                }
            }
        }
    }
    
    anyhow::bail!("No delta file found in directory: {}", dir.display())
}