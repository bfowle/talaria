#![allow(dead_code)]

use anyhow::Result;
use clap::Args;
use colored::*;
use std::time::Instant;

#[derive(Args)]
pub struct CheckUpdatesArgs {
    /// Database(s) to check (e.g., "uniprot/swissprot" or "all")
    pub database: Vec<String>,

    /// Show detailed information about changes
    #[arg(short, long)]
    pub detailed: bool,

    /// Check against remote manifest server
    #[arg(long)]
    pub remote: bool,

    /// Output format
    #[arg(long, value_enum, default_value = "text")]
    pub format: OutputFormat,
}

// Use OutputFormat from talaria-core
use talaria_core::OutputFormat;

pub fn run(args: CheckUpdatesArgs) -> Result<()> {
    use talaria_sequoia::database::DatabaseManager;
    

    let start_time = Instant::now();

    println!(
        "{} Checking for database updates...",
        "►".cyan().bold()
    );

    let manager = DatabaseManager::new(None)?;

    // Collect databases to check
    let databases_to_check = if args.database.iter().any(|d| d == "all") {
        // Get all local databases
        get_all_local_databases(&manager)?
    } else {
        args.database.clone()
    };

    if databases_to_check.is_empty() {
        println!("{} No databases found to check", "⚠".yellow().bold());
        return Ok(());
    }

    println!("  Checking {} databases\n", databases_to_check.len());

    let mut updates_available = Vec::new();
    let mut up_to_date = Vec::new();
    let mut errors = Vec::new();

    for db_name in &databases_to_check {
        match check_single_database(&manager, db_name, args.remote) {
            Ok(status) => {
                match status {
                    UpdateStatus::UpdateAvailable { current, latest, size_diff } => {
                        updates_available.push((db_name.clone(), current, latest, size_diff));
                    }
                    UpdateStatus::UpToDate { version } => {
                        up_to_date.push((db_name.clone(), version));
                    }
                    UpdateStatus::NotInstalled => {
                        // Skip
                    }
                }
            }
            Err(e) => {
                errors.push((db_name.clone(), e.to_string()));
            }
        }
    }

    // Display results based on format
    match args.format {
        OutputFormat::Text => display_text_results(
            &updates_available,
            &up_to_date,
            &errors,
            args.detailed,
            start_time.elapsed().as_secs_f64(),
        ),
        OutputFormat::Json => display_json_results(
            &updates_available,
            &up_to_date,
            &errors,
        )?,
        OutputFormat::Yaml => {
            // For now, fallback to JSON
            display_json_results(&updates_available, &up_to_date, &errors)?
        }
        OutputFormat::Csv | OutputFormat::Tsv | OutputFormat::Fasta | OutputFormat::Summary | OutputFormat::Detailed | OutputFormat::HashOnly => {
            // Default to text for unsupported formats
            display_text_results(
                &updates_available,
                &up_to_date,
                &errors,
                args.detailed,
                start_time.elapsed().as_secs_f64(),
            )
        }
    }

    // Return error if any updates are available (useful for CI)
    if !updates_available.is_empty() {
        std::process::exit(1);
    }

    Ok(())
}

enum UpdateStatus {
    UpdateAvailable {
        current: String,
        latest: String,
        size_diff: i64,
    },
    UpToDate {
        version: String,
    },
    NotInstalled,
}

fn check_single_database(
    manager: &talaria_sequoia::database::DatabaseManager,
    database: &str,
    check_remote: bool,
) -> Result<UpdateStatus> {
    use talaria_utils::database::database_ref::parse_database_reference;

    let db_ref = parse_database_reference(database)?;

    // Get local manifest
    let local_data = match manager.get_manifest(database) {
        Ok(m) => m,
        Err(_) => return Ok(UpdateStatus::NotInstalled),
    };

    if check_remote {
        // Check against remote manifest server
        let remote_manifest = fetch_remote_manifest(&db_ref)?;
        
        // Compare manifests by root hash
        if local_data.sequence_root != remote_manifest.sequence_root ||
           local_data.taxonomy_root != remote_manifest.taxonomy_root {
            let size_diff = calculate_size_difference(&local_data, &remote_manifest);
            
            Ok(UpdateStatus::UpdateAvailable {
                current: local_data.version.clone(),
                latest: remote_manifest.version.clone(),
                size_diff,
            })
        } else {
            Ok(UpdateStatus::UpToDate {
                version: local_data.version.clone(),
            })
        }
    } else {
        // Just report current version
        Ok(UpdateStatus::UpToDate {
            version: local_data.version.clone(),
        })
    }
}

fn fetch_remote_manifest(
    db_ref: &talaria_utils::database::database_ref::DatabaseReference,
) -> Result<talaria_sequoia::TemporalManifest> {
    // Check if TALARIA_MANIFEST_SERVER is set
    let manifest_url = if let Ok(server) = std::env::var("TALARIA_MANIFEST_SERVER") {
        format!(
            "{}/{}/{}/current/manifest.json",
            server.trim_end_matches('/'),
            db_ref.source,
            db_ref.dataset
        )
    } else {
        // Default to GitHub releases or other public source
        format!(
            "https://raw.githubusercontent.com/talaria-bio/databases/main/{}/{}/manifest.json",
            db_ref.source,
            db_ref.dataset
        )
    };

    // Fetch manifest (with 1-second timeout for performance)
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()?;

    let response = client.get(&manifest_url).send()?;

    if !response.status().is_success() {
        anyhow::bail!("Remote manifest not found: {}", response.status());
    }

    let manifest: talaria_sequoia::TemporalManifest = response.json()?;
    Ok(manifest)
}

fn calculate_size_difference(
    local: &talaria_sequoia::TemporalManifest,
    remote: &talaria_sequoia::TemporalManifest,
) -> i64 {
    let local_size: i64 = local.chunk_index.iter()
        .map(|c| c.size as i64)
        .sum();
    
    let remote_size: i64 = remote.chunk_index.iter()
        .map(|c| c.size as i64)
        .sum();

    remote_size - local_size
}

fn get_all_local_databases(
    _manager: &talaria_sequoia::database::DatabaseManager,
) -> Result<Vec<String>> {
    use std::fs;
    use talaria_core::system::paths;

    let versions_dir = paths::talaria_databases_dir().join("versions");
    let mut databases = Vec::new();

    // Scan for databases in the versions directory
    if versions_dir.exists() {
        for source_entry in fs::read_dir(&versions_dir)? {
            let source_entry = source_entry?;
            if source_entry.file_type()?.is_dir() {
                let source_name = source_entry.file_name();
                let source_str = source_name.to_string_lossy();

                // Skip special directories
                if source_str.starts_with('.') {
                    continue;
                }

                for dataset_entry in fs::read_dir(source_entry.path())? {
                    let dataset_entry = dataset_entry?;
                    if dataset_entry.file_type()?.is_dir() {
                        let dataset_name = dataset_entry.file_name();
                        let dataset_str = dataset_name.to_string_lossy();

                        // Check if there's at least one version directory
                        let has_version = fs::read_dir(dataset_entry.path())?
                            .any(|e| {
                                e.ok()
                                    .and_then(|e| e.file_type().ok())
                                    .map(|ft| ft.is_dir())
                                    .unwrap_or(false)
                            });

                        if has_version {
                            databases.push(format!("{}/{}", source_str, dataset_str));
                        }
                    }
                }
            }
        }
    }

    databases.sort();
    Ok(databases)
}

fn display_text_results(
    updates: &[(String, String, String, i64)],
    up_to_date: &[(String, String)],
    errors: &[(String, String)],
    detailed: bool,
    elapsed_seconds: f64,
) {
    use crate::cli::formatting::output::format_number;

    println!("{}", "═".repeat(60));
    println!("{:^60}", "UPDATE CHECK RESULTS".bold());
    println!("{}", "═".repeat(60));
    println!();

    if !updates.is_empty() {
        println!("{} {} Updates Available:", 
                 "▶".yellow().bold(),
                 updates.len());
        
        for (db, current, latest, size_diff) in updates {
            println!("  {} {}: {} → {}",
                     "•".yellow(),
                     db.bold(),
                     current.red(),
                     latest.green());
            
            if detailed {
                let size_str = if *size_diff > 0 {
                    format!("+{} bytes", format_number(*size_diff as usize))
                } else {
                    format!("{} bytes", format_number(size_diff.abs() as usize))
                };
                println!("    Size change: {}", size_str);
                println!("    Run: talaria database update {}", db);
            }
        }
        println!();
    }

    if !up_to_date.is_empty() {
        println!("{} {} Up to date:",
                 "✓".green().bold(),
                 up_to_date.len());
        
        if detailed {
            for (db, version) in up_to_date {
                println!("  {} {}: {}", "•".green(), db, version);
            }
        } else {
            let db_names: Vec<_> = up_to_date.iter()
                .map(|(db, _)| db.as_str())
                .collect();
            println!("  {}", db_names.join(", "));
        }
        println!();
    }

    if !errors.is_empty() {
        println!("{} {} Errors:",
                 "✗".red().bold(),
                 errors.len());
        
        for (db, error) in errors {
            println!("  {} {}: {}", "•".red(), db, error);
        }
        println!();
    }

    println!("Check completed in {:.1}s", elapsed_seconds);

    if !updates.is_empty() {
        println!("\n{} Run 'talaria database update all' to update all databases",
                 "Tip:".cyan().bold());
    }
}

fn display_json_results(
    updates: &[(String, String, String, i64)],
    up_to_date: &[(String, String)],
    errors: &[(String, String)],
) -> Result<()> {
    let result = serde_json::json!({
        "updates_available": updates.iter().map(|(db, current, latest, size)| {
            serde_json::json!({
                "database": db,
                "current_version": current,
                "latest_version": latest,
                "size_difference": size,
            })
        }).collect::<Vec<_>>(),
        "up_to_date": up_to_date.iter().map(|(db, version)| {
            serde_json::json!({
                "database": db,
                "version": version,
            })
        }).collect::<Vec<_>>(),
        "errors": errors.iter().map(|(db, error)| {
            serde_json::json!({
                "database": db,
                "error": error,
            })
        }).collect::<Vec<_>>(),
        "summary": {
            "total_checked": updates.len() + up_to_date.len() + errors.len(),
            "updates_available": updates.len(),
            "up_to_date": up_to_date.len(),
            "errors": errors.len(),
        }
    });

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}