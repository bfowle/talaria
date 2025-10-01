#![allow(dead_code)]

use crate::cli::formatting::output::*;
use crate::cli::progress::create_spinner;
use anyhow::Result;
use clap::Args;
use colored::*;

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

    /// Report output file path
    #[arg(long = "report-output", value_name = "FILE")]
    pub report_output: Option<std::path::PathBuf>,

    /// Report output format (text, html, json, csv)
    #[arg(long = "report-format", value_name = "FORMAT", default_value = "text")]
    pub report_format: String,
}

pub fn run(args: UpdateArgs) -> Result<()> {
    use crate::cli::formatting::output::format_number;
    use talaria_sequoia::database::DatabaseManager;

    // Header based on mode
    if args.dry_run {
        section_header("Database Update Check");
        info("Running in dry-run mode - no downloads will be performed");
    } else {
        section_header("Database Update");
    }

    // Initialize database manager with spinner
    let spinner = create_spinner("Checking for updates...");
    let mut manager = DatabaseManager::new(args.db_path.map(|p| p.to_string_lossy().to_string()))?;

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
    let check_taxonomy = args.include_taxonomy
        || args.database.is_none()
        || args
            .database
            .as_ref()
            .is_some_and(|d| d == "ncbi/taxonomy" || d == "taxonomy");

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
                errors.push((
                    db_name.clone(),
                    "Database not found in repository".to_string(),
                ));
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
            println!(
                "   {} Latest:  {} {}",
                "└─".dimmed(),
                latest,
                "✨ NEW".yellow()
            );
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
            info(&format!(
                "Run without --dry-run to download {} update(s)",
                updates_available.len()
            ));
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

    // Generate report if requested
    if let Some(report_path) = &args.report_output {
        use talaria_sequoia::operations::{UpdateResult, DatabaseComparison, ChunkAnalysis, SequenceAnalysis, TaxonomyAnalysis, StorageMetrics};

        // Build empty comparison (not applicable for update operations)
        let comparison = DatabaseComparison {
            chunk_analysis: ChunkAnalysis {
                total_chunks_a: 0,
                total_chunks_b: 0,
                shared_chunks: Vec::new(),
                unique_to_a: Vec::new(),
                unique_to_b: Vec::new(),
                shared_percentage_a: 0.0,
                shared_percentage_b: 0.0,
            },
            sequence_analysis: SequenceAnalysis {
                total_sequences_a: 0,
                total_sequences_b: 0,
                shared_sequences: 0,
                unique_to_a: 0,
                unique_to_b: 0,
                sample_shared_ids: Vec::new(),
                sample_unique_a_ids: Vec::new(),
                sample_unique_b_ids: Vec::new(),
                shared_percentage_a: 0.0,
                shared_percentage_b: 0.0,
            },
            taxonomy_analysis: TaxonomyAnalysis {
                total_taxa_a: 0,
                total_taxa_b: 0,
                shared_taxa: Vec::new(),
                unique_to_a: Vec::new(),
                unique_to_b: Vec::new(),
                top_shared_taxa: Vec::new(),
                shared_percentage_a: 0.0,
                shared_percentage_b: 0.0,
            },
            storage_metrics: StorageMetrics {
                size_a_bytes: 0,
                size_b_bytes: 0,
                dedup_savings_bytes: 0,
                dedup_ratio_a: 0.0,
                dedup_ratio_b: 0.0,
            },
        };

        let result = UpdateResult {
            updated_databases: updates_available.iter().map(|(name, _, _)| name.clone()).collect(),
            failed_databases: errors.iter().map(|(name, err)| (name.clone(), err.clone())).collect(),
            dry_run: args.dry_run,
            comparison,
            duration: std::time::Duration::from_secs(0),
        };

        crate::cli::commands::save_report(&result, &args.report_format, report_path)?;
        success(&format!("Report saved to {}", report_path.display()));
    }

    Ok(())
}

enum UpdateStatus {
    UpdateAvailable { current: String, latest: String },
    UpToDate { version: String },
    NotFound,
}

fn check_database_update(
    manager: &mut talaria_sequoia::database::DatabaseManager,
    database: &str,
    force: bool,
) -> Result<UpdateStatus> {
    use talaria_sequoia::download::{DatabaseSource, NCBIDatabase, UniProtDatabase};
    use talaria_utils::database::database_ref::parse_database_ref;

    // Parse database reference to get source
    let (source_str, dataset) = parse_database_ref(database)?;

    // Map to DatabaseSource enum
    let source = match source_str.as_str() {
        "uniprot" => match dataset.as_str() {
            "swissprot" => DatabaseSource::UniProt(UniProtDatabase::SwissProt),
            "trembl" => DatabaseSource::UniProt(UniProtDatabase::TrEMBL),
            _ => return Ok(UpdateStatus::NotFound),
        },
        "ncbi" => match dataset.as_str() {
            "nr" => DatabaseSource::NCBI(NCBIDatabase::NR),
            "nt" => DatabaseSource::NCBI(NCBIDatabase::NT),
            "taxonomy" => DatabaseSource::NCBI(NCBIDatabase::Taxonomy),
            _ => return Ok(UpdateStatus::NotFound),
        },
        _ => return Ok(UpdateStatus::NotFound),
    };

    // Use async runtime to check for updates
    let runtime = tokio::runtime::Runtime::new()?;
    let result = runtime.block_on(async {
        if force {
            // Force indicates update available
            Ok(UpdateStatus::UpdateAvailable {
                current: "current".to_string(),
                latest: "latest".to_string(),
            })
        } else {
            // Check for actual updates
            let progress = |_msg: &str| {};
            match manager.check_for_updates(&source, progress).await {
                Ok(talaria_sequoia::database::DownloadResult::UpToDate) => {
                    Ok(UpdateStatus::UpToDate {
                        version: manager
                            .get_current_version_info(&source)
                            .map(|v| v.timestamp)
                            .unwrap_or_else(|_| "unknown".to_string()),
                    })
                }
                Ok(talaria_sequoia::database::DownloadResult::Updated { .. }) => {
                    Ok(UpdateStatus::UpdateAvailable {
                        current: "current".to_string(),
                        latest: "new version available".to_string(),
                    })
                }
                Ok(talaria_sequoia::database::DownloadResult::InitialDownload { .. }) => {
                    Ok(UpdateStatus::NotFound)
                }
                Ok(talaria_sequoia::database::DownloadResult::Downloaded { .. }) => {
                    Ok(UpdateStatus::UpdateAvailable {
                        current: "none".to_string(),
                        latest: "downloaded".to_string(),
                    })
                }
                Ok(talaria_sequoia::database::DownloadResult::AlreadyExists { .. }) => {
                    Ok(UpdateStatus::UpToDate {
                        version: manager
                            .get_current_version_info(&source)
                            .map(|v| v.timestamp)
                            .unwrap_or_else(|_| "unknown".to_string()),
                    })
                }
                Err(_) => Ok(UpdateStatus::NotFound),
            }
        }
    });

    result
}

fn check_taxonomy_update(
    manager: &mut talaria_sequoia::database::DatabaseManager,
    force: bool,
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
    // Use the download command with appropriate flags
    use crate::cli::commands::database::download::DownloadArgs;
    use talaria_utils::database::database_ref::parse_database_ref;

    // Parse database reference
    let (_source, _dataset) = parse_database_ref(database)?;

    // Create download args for update (not dry-run, not force)
    let mut download_args = DownloadArgs::default_with_database(database.to_string());
    download_args.resume = true;

    // Run download which will handle update if needed
    crate::cli::commands::database::download::run(download_args)
}
