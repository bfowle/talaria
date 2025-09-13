use clap::Args;
use std::path::PathBuf;
use std::fs;
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use chrono::Local;
use serde_json::json;

#[derive(Args)]
pub struct AddArgs {
    /// Path to the FASTA file to add as a custom database
    #[arg(short, long, value_name = "FILE")]
    pub input: PathBuf,

    /// Name for the custom database (e.g., "team-proteins")
    /// If not specified, uses the filename without extension
    #[arg(short, long)]
    pub name: Option<String>,

    /// Source category (default: "custom")
    #[arg(short, long, default_value = "custom")]
    pub source: String,

    /// Dataset name within the source
    /// If not specified, uses --name or filename
    #[arg(short, long)]
    pub dataset: Option<String>,

    /// Description of the database
    #[arg(long)]
    pub description: Option<String>,

    /// Version identifier (default: current date)
    #[arg(long)]
    pub version: Option<String>,

    /// Replace existing database if it exists
    #[arg(long)]
    pub replace: bool,

    /// Copy file instead of moving (keeps original in place)
    #[arg(long)]
    pub copy: bool,
}

pub fn run(args: AddArgs) -> Result<()> {
    use crate::core::config::load_config;
    use crate::core::database_manager::DatabaseManager;
    use crate::utils::format::get_file_size;

    // Validate input file exists
    if !args.input.exists() {
        anyhow::bail!("Input file does not exist: {:?}", args.input);
    }

    if !args.input.is_file() {
        anyhow::bail!("Input must be a file, not a directory: {:?}", args.input);
    }

    // Determine database name
    let db_name = args.name.clone().or_else(|| {
        args.dataset.clone()
    }).or_else(|| {
        args.input.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    }).ok_or_else(|| anyhow::anyhow!("Could not determine database name"))?;

    let dataset_name = args.dataset.unwrap_or_else(|| db_name.clone());

    // Initialize progress bar
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
    );
    pb.set_message(format!("Adding custom database '{}/{}'...", args.source, dataset_name));

    // Load config and database manager
    let config = load_config("talaria.toml").unwrap_or_default();
    let db_manager = DatabaseManager::new(config.database.database_dir)?;

    // Check if database already exists
    let db_dir = db_manager.get_database_dir(&args.source, &dataset_name);
    if db_dir.exists() && !args.replace {
        anyhow::bail!(
            "Database '{}/{}' already exists. Use --replace to overwrite.",
            args.source, dataset_name
        );
    }

    // Determine version
    let version = args.version.unwrap_or_else(|| {
        Local::now().format("%Y-%m-%d").to_string()
    });

    // Create version directory
    let version_dir = db_manager.get_version_dir(&args.source, &dataset_name, &version);
    fs::create_dir_all(&version_dir)
        .context("Failed to create version directory")?;

    // Determine target filename
    let target_filename = format!("{}.fasta", dataset_name);
    let target_path = version_dir.join(&target_filename);

    // Copy or move the file
    pb.set_message(if args.copy { "Copying FASTA file..." } else { "Moving FASTA file..." });

    if args.copy {
        fs::copy(&args.input, &target_path)
            .context("Failed to copy FASTA file")?;
    } else {
        // Try to rename first (fast if same filesystem), fall back to copy+delete
        if fs::rename(&args.input, &target_path).is_err() {
            fs::copy(&args.input, &target_path)
                .context("Failed to copy FASTA file")?;
            fs::remove_file(&args.input)
                .context("Failed to remove original file after copy")?;
        }
    }

    // Quick validation - read first few lines to ensure it's a FASTA file
    pb.set_message("Validating FASTA format...");
    let content = fs::read_to_string(&target_path)
        .context("Failed to read copied FASTA file")?;

    let first_line = content.lines().next()
        .ok_or_else(|| anyhow::anyhow!("File is empty"))?;

    if !first_line.starts_with('>') {
        // Clean up on error
        fs::remove_dir_all(&version_dir).ok();
        anyhow::bail!("File does not appear to be in FASTA format (first line should start with '>')");
    }

    // Count sequences
    pb.set_message("Counting sequences...");
    let sequence_count = content.lines()
        .filter(|line| line.starts_with('>'))
        .count();

    // Get file size
    let file_size = get_file_size(&target_path).unwrap_or(0);

    // Create metadata
    pb.set_message("Creating metadata...");
    let metadata = json!({
        "database_type": "custom",
        "source": args.source,
        "dataset": dataset_name,
        "version": version,
        "description": args.description.unwrap_or_else(|| format!("Custom database from {}",
            args.input.file_name().and_then(|s| s.to_str()).unwrap_or("unknown"))),
        "original_file": args.input.to_string_lossy(),
        "added_date": Local::now().to_rfc3339(),
        "file_size": file_size,
        "sequence_count": sequence_count,
        "format": "fasta",
    });

    let metadata_path = version_dir.join("metadata.json");
    fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)
        .context("Failed to write metadata")?;

    // Update current symlink
    pb.set_message("Updating current version link...");
    db_manager.update_current_link(&args.source, &dataset_name, &version)?;

    pb.finish_and_clear();

    // Print success message with statistics
    println!("✓ Successfully added custom database '{}/{}'", args.source, dataset_name);
    println!("  Version: {}", version);
    println!("  Sequences: {}", sequence_count);
    println!("  Size: {:.2} MB", file_size as f64 / 1_048_576.0);
    println!("  Location: {:?}", version_dir);
    println!();
    println!("You can now use this database with:");
    println!("  talaria reduce {}/{} -r 0.3", args.source, dataset_name);
    println!("  talaria database info {}/{}", args.source, dataset_name);

    Ok(())
}