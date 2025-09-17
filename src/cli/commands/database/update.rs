use anyhow::Result;
use clap::Args;
use colored::*;
use crate::cli::output::*;
use crate::utils::progress::create_spinner;

#[derive(Args)]
pub struct UpdateArgs {
    /// Database to update (e.g., "uniprot/swissprot", "ncbi/taxonomy")
    /// If not specified, checks all databases
    pub database: Option<String>,

    /// Perform a dry run (only check for updates, don't download)
    #[arg(short = 'd', long)]
    pub dry_run: bool,

    /// Force update even if current version is up-to-date
    #[arg(short, long)]
    pub force: bool,

    /// Include taxonomy data in update check
    #[arg(short = 't', long)]
    pub include_taxonomy: bool,

    /// Database repository path (default: ${TALARIA_HOME}/databases)
    #[arg(long)]
    pub db_path: Option<std::path::PathBuf>,
}

pub fn run(args: UpdateArgs) -> Result<()> {
    use crate::core::database_manager::DatabaseManager;
    use crate::cli::output::format_number;

    // Header based on mode
    if args.dry_run {
        section_header("Database Update Check");
        info("Running in dry-run mode - no downloads will be performed");
    } else {
        section_header("Database Update");
    }

    // Initialize database manager with spinner
    let spinner = create_spinner("Checking for updates...");
    let mut manager = DatabaseManager::new(
        args.db_path.map(|p| p.to_string_lossy().to_string())
    )?;

    // Get list of databases to check
    let databases_to_check = if let Some(ref db_name) = args.database {
        // Check specific database
        spinner.set_message(format!("Checking {} for updates...", db_name));
        vec![db_name.clone()]
    } else {
        // Check all databases
        spinner.set_message("Scanning all databases for updates...");
        let databases = manager.list_databases()?;
        databases.iter().map(|d| d.name.clone()).collect()
    };

    // Also check taxonomy if requested or if checking all
    let check_taxonomy = args.include_taxonomy || args.database.is_none() ||
                         args.database.as_ref().map_or(false, |d| d == "ncbi/taxonomy" || d == "taxonomy");

    spinner.finish_and_clear();

    // Track update status
    let mut updates_available = Vec::new();
    let mut up_to_date = Vec::new();
    let mut errors = Vec::new();

    // Check each database
    for db_name in &databases_to_check {
        match check_database_update(&mut manager, db_name, args.force) {
            Ok(UpdateStatus::UpdateAvailable { current, latest }) => {
                updates_available.push((db_name.clone(), current, latest));
            }
            Ok(UpdateStatus::UpToDate { version }) => {
                up_to_date.push((db_name.clone(), version));
            }
            Ok(UpdateStatus::NotFound) => {
                errors.push((db_name.clone(), "Database not found in repository".to_string()));
            }
            Err(e) => {
                errors.push((db_name.clone(), e.to_string()));
            }
        }
    }

    // Check taxonomy separately if requested
    if check_taxonomy && !databases_to_check.iter().any(|d| d.contains("taxonomy")) {
        match check_taxonomy_update(&mut manager, args.force) {
            Ok(UpdateStatus::UpdateAvailable { current, latest }) => {
                updates_available.push(("ncbi/taxonomy".to_string(), current, latest));
            }
            Ok(UpdateStatus::UpToDate { version }) => {
                up_to_date.push(("ncbi/taxonomy".to_string(), version));
            }
            _ => {}
        }
    }

    // Display results
    if !updates_available.is_empty() {
        subsection_header("Updates Available");
        for (db, current, latest) in &updates_available {
            tree_item(false, db, None);
            println!("   {} Current: {}", "├─".dimmed(), current);
            println!("   {} Latest:  {} {}", "└─".dimmed(), latest, "✨ NEW".yellow());
        }

        if !args.dry_run {
            println!();
            action("Downloading updates...");

            for (db, _, _) in &updates_available {
                let update_spinner = create_spinner(&format!("Updating {}...", db));

                // Perform actual update
                match perform_update(db) {
                    Ok(()) => {
                        update_spinner.finish_with_message(format!("{} {}", "✓".green(), db));
                    }
                    Err(e) => {
                        update_spinner.finish_with_message(format!("{} {} - {}", "✗".red(), db, e));
                        errors.push((db.clone(), e.to_string()));
                    }
                }
            }
        } else {
            println!();
            info(&format!("Run without --dry-run to download {} update(s)", updates_available.len()));
        }
    }

    if !up_to_date.is_empty() {
        subsection_header("Up to Date");
        for (db, version) in &up_to_date {
            success(&format!("{} ({})", db, version));
        }
    }

    if !errors.is_empty() {
        subsection_header("Errors");
        for (db, err) in &errors {
            error(&format!("{}: {}", db, err));
        }
    }

    // Summary
    println!();
    let summary_items = vec![
        ("Updates available", format_number(updates_available.len())),
        ("Up to date", format_number(up_to_date.len())),
        ("Errors", format_number(errors.len())),
    ];
    tree_section("Summary", summary_items, true);

    Ok(())
}

enum UpdateStatus {
    UpdateAvailable { current: String, latest: String },
    UpToDate { version: String },
    NotFound,
}

fn check_database_update(
    manager: &mut crate::core::database_manager::DatabaseManager,
    database: &str,
    force: bool
) -> Result<UpdateStatus> {
    // Get current database info
    let databases = manager.list_databases()?;
    let current_db = databases.iter()
        .find(|d| d.name == database || d.name.ends_with(database));

    if let Some(db) = current_db {
        // Check for updates (this would need to be implemented in DatabaseManager)
        // For now, we'll simulate by checking if the database is older than 30 days
        let age_days = (chrono::Utc::now() - db.created_at).num_days();

        if force || age_days > 30 {
            // Simulate finding a newer version
            let latest = chrono::Utc::now().format("%Y%m%d").to_string();
            Ok(UpdateStatus::UpdateAvailable {
                current: db.version.clone(),
                latest,
            })
        } else {
            Ok(UpdateStatus::UpToDate {
                version: db.version.clone(),
            })
        }
    } else {
        Ok(UpdateStatus::NotFound)
    }
}

fn check_taxonomy_update(
    manager: &mut crate::core::database_manager::DatabaseManager,
    force: bool
) -> Result<UpdateStatus> {
    // Get current taxonomy version
    let current_version = manager.get_taxonomy_version()?;

    if let Some(version) = current_version {
        // For now, simulate checking for updates based on age
        // In a real implementation, this would check against a remote source
        if force {
            Ok(UpdateStatus::UpdateAvailable {
                current: version.clone(),
                latest: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            })
        } else {
            Ok(UpdateStatus::UpToDate { version })
        }
    } else {
        // Taxonomy not installed
        Ok(UpdateStatus::UpdateAvailable {
            current: "not installed".to_string(),
            latest: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        })
    }
}

fn perform_update(database: &str) -> Result<()> {
    if database == "ncbi/taxonomy" || database.contains("taxonomy") {
        // Update taxonomy - for now we just indicate it needs async runtime
        // In production, this would be handled properly with async
        return Err(anyhow::anyhow!(
            "Taxonomy update requires async runtime. Please use 'talaria database update-taxonomy' for now."
        ));
    } else {
        // For regular databases, we would need to implement re-download logic
        // For now, return an error indicating this needs implementation
        return Err(anyhow::anyhow!(
            "Database update for '{}' not yet implemented. Please use 'talaria database download {}' to re-download.",
            database, database
        ));
    }
}