#![allow(dead_code)]

/// Database download implementation using content-addressed storage
use crate::cli::commands::database::download::DownloadArgs;
use crate::cli::formatter::{self, format_bytes, format_number, print_tip, TaskList, TaskStatus};
use crate::cli::output::*;
use crate::core::database_manager::{DatabaseManager, DownloadResult};
use crate::download::DatabaseSource;
use crate::utils::progress::create_spinner;
use anyhow::Result;
use colored::Colorize;
use indicatif::ProgressBar;
use std::sync::{Arc, Mutex};

pub fn run_database_download(args: DownloadArgs, database_source: DatabaseSource) -> Result<()> {
    // Apply CLI flag overrides for environment variables
    if let Some(ref manifest_server) = args.manifest_server {
        std::env::set_var("TALARIA_MANIFEST_SERVER", manifest_server);
    }
    if let Some(ref talaria_home) = args.talaria_home {
        std::env::set_var("TALARIA_HOME", talaria_home);
    }
    if args.preserve_lambda_on_failure {
        std::env::set_var("TALARIA_PRESERVE_LAMBDA_ON_FAILURE", "1");
    }

    let runtime = tokio::runtime::Runtime::new()?;

    // Initialize formatter
    formatter::init();

    // Print header based on mode (only if not called from interactive mode)
    let _db_name = format!("{}", database_source);
    if args.dry_run {
        info("Running in dry-run mode - no downloads will be performed");
        println!();
    }

    // Create task list for tracking operations
    let mut task_list = TaskList::new();

    // Add all tasks upfront
    let init_task = task_list.add_task("Initialize SEQUOIA repository");
    let check_task = task_list.add_task("Check for existing data");
    let download_task = task_list.add_task("Download database");
    let process_task = task_list.add_task("Process into chunks");
    let store_task = task_list.add_task("Store in repository");
    let manifest_task = task_list.add_task("Create manifest");

    // Initialize SEQUOIA repository
    task_list.update_task(init_task, TaskStatus::InProgress);
    let mut manager = DatabaseManager::with_options(None, args.json)?;
    task_list.update_task(init_task, TaskStatus::Complete);

    // Ensure version integrity (fix symlinks, create version.json if missing)
    manager.ensure_version_integrity(&database_source)?;

    // Check for resumable operations
    if args.resume {
        println!();
        let spinner = create_spinner("Checking for resumable downloads...");
        let resumable_ops = manager.list_resumable_operations()?;
        spinner.finish_and_clear();

        if !resumable_ops.is_empty() {
            subsection_header(&format!(
                "◆ Found {} resumable operation(s)",
                resumable_ops.len()
            ));
            for (i, (op_id, state)) in resumable_ops.iter().enumerate() {
                let is_last = i == resumable_ops.len() - 1;
                tree_item(is_last, &format!("{}: {}", op_id, state.summary()), None);
            }
        }
    }

    // Create a shared task list for progress callback
    let shared_task_list = Arc::new(Mutex::new(task_list));
    let task_list_clone = Arc::clone(&shared_task_list);

    // Track current phase and spinner
    let current_spinner: Arc<Mutex<Option<ProgressBar>>> = Arc::new(Mutex::new(None));
    let spinner_clone = Arc::clone(&current_spinner);

    // Progress callback that updates task list and shows detailed output
    let progress = move |msg: &str| {
        let mut tl = task_list_clone.lock().unwrap();
        let mut spinner = spinner_clone.lock().unwrap();

        // Parse message and update task states
        if msg.contains("[NEW]") || msg.contains("Initial download required") {
            tl.update_task(check_task, TaskStatus::Complete);
            tl.update_task(download_task, TaskStatus::InProgress);
            drop(tl);

            // Clear any existing spinner and show message
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
            println!("\n  {} Initial download required", "●".yellow());
        } else if msg.contains("Checking for updates") {
            tl.update_task(check_task, TaskStatus::InProgress);
        } else if msg.contains("up to date") || msg.contains("Up to date") {
            tl.update_task(check_task, TaskStatus::Complete);
            tl.update_task(download_task, TaskStatus::Skipped);
            tl.update_task(process_task, TaskStatus::Skipped);
            tl.update_task(store_task, TaskStatus::Skipped);
            tl.update_task(manifest_task, TaskStatus::Skipped);

            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
        } else if msg.contains("Downloading full database") {
            tl.update_task(download_task, TaskStatus::InProgress);
            tl.pause_updates();
            drop(tl);

            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
            println!("\n  {} Download", "○".dimmed());
            println!("{}", msg);
        } else if msg.contains("Reading sequences from FASTA") {
            // Clear download phase, start processing
            tl.resume_updates();
            tl.update_task(download_task, TaskStatus::Complete);
            tl.update_task(process_task, TaskStatus::InProgress);
            drop(tl);

            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }

            println!("\n  {} Process into chunks", "●".yellow());
            *spinner = Some(create_spinner("Reading sequences from FASTA file..."));
        } else if msg.contains("Analyzing database structure") {
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
            *spinner = Some(create_spinner("Analyzing database structure..."));
        } else if msg.contains("Database analysis:") || msg.contains("Total sequences:") {
            // Clear spinner and show analysis results
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
            println!("{}", msg);
        } else if msg.contains("Creating taxonomy-aware chunks") {
            *spinner = Some(create_spinner("Creating taxonomy-aware chunks..."));
        } else if msg.contains("Special taxa rules applied") {
            if let Some(ref s) = *spinner {
                s.set_message("Applying special taxa rules...");
            }
        } else if msg.contains("Created") && msg.contains("chunks") {
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }

            // Extract chunk count for tree display
            if let Some(num_str) = msg
                .split("Created ")
                .nth(1)
                .and_then(|s| s.split(" chunks").next())
            {
                println!(
                    "  {} Created {} chunks",
                    "✓".green(),
                    format_number(num_str.parse::<usize>().unwrap_or(0))
                );
            }

            tl.update_task(process_task, TaskStatus::Complete);
            tl.update_task(store_task, TaskStatus::InProgress);
            drop(tl);

            println!("\n  {} Store in repository", "●".yellow());
            *spinner = Some(create_spinner("Storing chunks in SEQUOIA repository..."));
        } else if msg.contains("All chunks stored") {
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }

            tl.update_task(store_task, TaskStatus::Complete);
            tl.update_task(manifest_task, TaskStatus::InProgress);
            drop(tl);

            println!("  {} All chunks stored", "✓".green());
            println!("\n  {} Create manifest", "●".yellow());
            *spinner = Some(create_spinner("Creating and saving manifest..."));
        } else if msg.contains("Creating and saving manifest") {
            // Already handled above
        } else if !msg.is_empty() && !msg.contains("[0") {
            // Ignore progress bar messages
            // Pass through other messages but format them nicely
            if let Some(s) = spinner.as_ref() {
                s.set_message(msg.to_string());
            }
        }
    };

    // Run the download (or dry-run check)
    let result = if args.dry_run {
        // In dry-run mode, just check what would be downloaded
        runtime.block_on(async { manager.check_for_updates(&database_source, progress).await })?
    } else if args.force {
        // Force download even if up-to-date
        runtime.block_on(async { manager.force_download(&database_source, progress).await })?
    } else {
        // Normal download (idempotent)
        runtime.block_on(async { manager.download(&database_source, progress).await })?
    };

    // Clear any remaining spinner
    if let Some(s) = current_spinner.lock().unwrap().take() {
        s.finish_and_clear();
    }

    // Update final task statuses based on result
    {
        let mut tl = shared_task_list.lock().unwrap();
        tl.resume_updates(); // Make sure updates are resumed
        match result {
            DownloadResult::UpToDate => {
                // Tasks already marked as skipped
            }
            DownloadResult::Updated { .. } | DownloadResult::InitialDownload => {
                tl.update_task(manifest_task, TaskStatus::Complete);
            }
        }
    }

    println!("  {} Manifest created", "✓".green());
    println!();

    // Report results with nice formatting
    match result {
        DownloadResult::UpToDate => {
            println!("{}", "─".repeat(80).dimmed());
            if args.dry_run {
                success("Database is up to date - no download needed");
            } else {
                success("Database is already up to date!");
            }
            info("No downloads needed - saved bandwidth and time");

            // Show version information
            if let Ok(version_info) = manager.get_current_version_info(&database_source) {
                println!();
                subsection_header("◆ Current Version");
                tree_item(false, "Timestamp", Some(&version_info.timestamp));
                if let Some(upstream) = version_info.upstream_version {
                    tree_item(false, "Upstream", Some(&upstream));
                }
                tree_item(true, "Aliases", Some(&version_info.aliases.join(", ")));
            }
        }
        DownloadResult::Updated {
            chunks_added,
            chunks_removed,
        } => {
            println!("{}", "─".repeat(80).dimmed());
            if args.dry_run {
                warning("Updates available - run without --dry-run to download");
            } else {
                success("Database updated successfully!");
            }
            let mut items = vec![(
                "Added",
                format!("{} new chunks", format_number(chunks_added)),
            )];
            if chunks_removed > 0 {
                items.push((
                    "Removed",
                    format!("{} obsolete chunks", format_number(chunks_removed)),
                ));
            }
            items.push(("Efficiency", "Only downloaded what changed".to_string()));
            tree_section("Update Summary", items, true);

            if args.dry_run {
                println!();
                info(&format!(
                    "Run without --dry-run to download {} chunks",
                    chunks_added
                ));
            }
        }
        DownloadResult::InitialDownload => {
            println!("{}", "─".repeat(80).dimmed());
            if args.dry_run {
                warning("Initial download required - run without --dry-run to download");
                println!();
                info("This database has not been downloaded yet");
                info("Full download will be performed on first run");
            } else {
                success("Database downloaded successfully!");
                println!();
                subsection_header("◆ Download Summary");
                tree_item(false, "Status", Some("Initial download complete"));
                tree_item(
                    false,
                    "Storage",
                    Some("Database chunked and stored in SEQUOIA"),
                );
                tree_item(true, "Updates", Some("Future updates will be incremental"));
                println!();
                print_tip("Set TALARIA_MANIFEST_SERVER to enable incremental updates from a manifest server");
            }
        }
    }

    // Show stats in a tree format
    let stats = manager.get_stats()?;

    println!();
    subsection_header("◆ Repository Statistics");
    tree_item(
        false,
        "Total chunks",
        Some(&format_number(stats.total_chunks)),
    );
    tree_item(
        false,
        "Total size",
        Some(&format_bytes(stats.total_size as u64)),
    );
    tree_item(
        false,
        "Compressed chunks",
        Some(&format_number(stats.compressed_chunks)),
    );
    tree_item(
        false,
        "Deduplication ratio",
        Some(&format!("{:.2}x", stats.deduplication_ratio)),
    );
    tree_item(
        true,
        "Databases managed",
        Some(&format_number(stats.database_count)),
    );

    println!();

    Ok(())
}
