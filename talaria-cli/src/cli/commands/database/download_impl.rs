#![allow(dead_code)]

/// Database download implementation using content-addressed storage
use crate::cli::commands::database::download::DownloadArgs;
use crate::cli::formatting::{format_number, print_tip, TaskList, TaskStatus};
use talaria_utils::display::format::format_bytes;
use crate::cli::formatting::output::*;
use talaria_sequoia::database::{DatabaseManager, DownloadResult};
use talaria_sequoia::download::DatabaseSource;
use crate::cli::progress::create_spinner;
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

    // Handle continue/resume flag alias
    let resume = args.resume || args.continue_download;

    // Set verbosity level
    if args.quiet {
        std::env::set_var("TALARIA_LOG", "error");
    } else if std::env::var("TALARIA_VERBOSE").unwrap_or_default().parse::<u8>().unwrap_or(0) > 0 {
        std::env::set_var("TALARIA_LOG", "debug");
    }

    let runtime = tokio::runtime::Runtime::new()?;

    // Initialize formatter
    use crate::cli::formatting::formatter;
    formatter::init();

    // Print header based on mode (only if not called from interactive mode and not quiet)
    let _db_name = format!("{}", database_source);
    if !args.quiet {
        if args.dry_run {
            info("Running in dry-run mode - no downloads will be performed");
            println!();
        }
    }

    // Set up rate limiting if specified
    if let Some(rate_kb) = args.limit_rate {
        info(&format!("Download rate limited to {} KB/s", rate_kb));
        // Note: Actual rate limiting would be implemented in the downloader
        std::env::set_var("TALARIA_DOWNLOAD_RATE_LIMIT", rate_kb.to_string());
    }

    // Initialize manager first
    let mut manager = DatabaseManager::with_options(None, args.json)?;

    // Ensure version integrity (fix symlinks, create version.json if missing)
    manager.ensure_version_integrity(&database_source)?;

    // Check for resumable operations BEFORE creating the task list UI
    if resume && !args.quiet {
        println!();
        let spinner = create_spinner("Checking for resumable state...");

        // First check for existing downloads in workspace
        use talaria_sequoia::download::{find_existing_workspace_for_source, Stage};
        let workspace_state = find_existing_workspace_for_source(&database_source)?;
        let found_workspace = workspace_state.is_some();

        // Then check for SEQUOIA processing states
        let resumable_ops = manager.list_resumable_operations()?;
        spinner.finish_and_clear();

        // Report workspace state
        if let Some((workspace_path, state)) = workspace_state {
            if state.stage == Stage::Complete {
                println!("  {} Found complete download ready for processing", "●".green());
                if let Some(decompressed) = state.files.decompressed.as_ref() {
                    if let Ok(metadata) = decompressed.metadata() {
                        let size_gb = metadata.len() as f64 / 1_073_741_824.0;
                        println!("    └─ Using existing {:.1}GB file from {}", size_gb, workspace_path.display());
                    }
                }
            } else {
                println!("  {} Found incomplete download to resume", "●".yellow());
                println!("    └─ Stage: {:?}", state.stage);
            }
        }

        // Report SEQUOIA processing state
        if !resumable_ops.is_empty() {
            if found_workspace {
                println!();
            }
            println!("  {} Found {} SEQUOIA processing operation(s) to resume", "●".cyan(), resumable_ops.len());
            for (i, (op_id, state)) in resumable_ops.iter().enumerate() {
                let is_last = i == resumable_ops.len() - 1;
                tree_item(is_last, &format!("{}: {}", op_id, state.summary()), None);
            }
        } else if !found_workspace {
            println!("  {} No resumable state found - will start fresh download", "○".white());
        }
    } else if resume && args.quiet {
        // In quiet mode, still check but don't print
        let resumable_ops = manager.list_resumable_operations()?;
        if resumable_ops.is_empty() {
            // Nothing to resume, will start fresh
        }
    }

    // NOW create task list for tracking operations (after resume check)
    let mut task_list = if args.quiet {
        TaskList::silent()
    } else {
        TaskList::new()
    };

    // Add all tasks upfront
    let init_task = task_list.add_task("Initialize SEQUOIA repository");
    let check_task = task_list.add_task("Check for existing data");
    let download_task = task_list.add_task("Download database");
    let process_task = task_list.add_task("Process into chunks");
    let store_task = task_list.add_task("Store in repository");
    let manifest_task = task_list.add_task("Create manifest");

    // Mark init as complete since we already did it
    task_list.update_task(init_task, TaskStatus::Complete);

    // Create a shared task list for progress callback
    let shared_task_list = Arc::new(Mutex::new(task_list));
    let task_list_clone = Arc::clone(&shared_task_list);

    // Track current phase and spinner
    let current_spinner: Arc<Mutex<Option<ProgressBar>>> = Arc::new(Mutex::new(None));
    let spinner_clone = Arc::clone(&current_spinner);

    // Progress callback that updates task list and shows detailed output
    let quiet_mode = args.quiet;
    let progress = move |msg: &str| {
        // In quiet mode, don't output anything
        if quiet_mode {
            return;
        }
        let mut tl = task_list_clone.lock().unwrap();
        let mut spinner = spinner_clone.lock().unwrap();

        // Parse message and update task states
        if msg.contains("Searching for existing downloads") {
            // Show search message
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
            println!("  {} {}", "●".cyan(), msg);
            *spinner = Some(create_spinner(""));
        } else if msg.contains("Found existing download at") {
            // Show found existing download
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
            println!("  {} {}", "✓".green(), msg);
        } else if msg.contains("Found download from") {
            // Show age of download
            println!("    {} {}", "├─".white(), msg);
        } else if msg.contains("Download complete, resuming SEQUOIA") {
            // Show resuming processing
            println!("    {} {}", "└─".white(), msg);
        } else if msg.contains("Using existing downloaded file") {
            // Show using existing file
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
            println!("  {} {}", "✓".green(), msg);
        } else if msg.contains("Checking for resumable download") {
            // Show resume check message
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
            println!("  {} {}", "●".cyan(), msg);
            *spinner = Some(create_spinner(""));
        } else if msg.contains("Found incomplete download from") {
            // Show found resumable download
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
            println!("  {} {}", "✓".green(), msg);
        } else if msg.contains("Download was") && msg.contains("% complete") {
            // Show download progress info
            println!("    {} {}", "└─".white(), msg);
        } else if msg.contains("Resuming") {
            // Show what's being resumed
            println!("  {} {}", "▶".cyan(), msg);
        } else if msg.contains("No resumable download found") {
            // Show no resume found
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
            println!("  {} {}", "○".white(), msg);
        } else if msg.contains("[NEW]") || msg.contains("Initial download required") {
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
        } else if msg.contains("SEQUOIA manifest already exists") || msg.contains("SEQUOIA manifest was created") {
            // Show manifest exists - no processing needed
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
            println!("  {} {}", "✓".green(), msg);
            tl.update_task(download_task, TaskStatus::Skipped);
            tl.update_task(process_task, TaskStatus::Skipped);
            tl.update_task(store_task, TaskStatus::Complete);
            tl.update_task(manifest_task, TaskStatus::Complete);
        } else if msg.contains("Large file detected") {
            // Show large file detection message clearly
            if let Some(s) = spinner.take() {
                s.finish_and_clear();
            }
            println!("\n  {} {}", "●".yellow(), msg);
        } else if msg.contains("Processing sequences in batches") {
            // Start of streaming processing
            tl.resume_updates();
            tl.update_task(download_task, TaskStatus::Complete);
            tl.update_task(process_task, TaskStatus::InProgress);
            drop(tl);

            println!("  {} {}", "▶".cyan(), msg);
        } else if msg.contains("Reading sequences from FASTA") || msg.contains("Processing FASTA file") {
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
        } else if msg.contains("Processed") && msg.contains("sequences") && msg.contains("batches") {
            // Batch progress update - just print it
            println!("  {}", msg);
        } else if msg.contains("Processing final batch") {
            // Final batch message
            println!("  {}", msg);
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
        } else if msg.contains("Taxonomy data is loaded") || msg.contains("Successfully loaded") {
            println!("  {} {}", "✓".green(), msg);
        } else if msg.contains("Taxonomy data not loaded") || msg.contains("WARNING: No taxonomy") {
            println!("  {} {}", "⚠".yellow(), msg);
        } else if msg.contains("Taxonomy directory not found") || msg.contains("Found taxonomy") || msg.contains("Found NCBI") {
            println!("    {}", msg);
        } else if msg.contains("To download taxonomy") || msg.contains("talaria database download") {
            println!("    {}", msg.dimmed());
        } else if msg.contains("Continuing with placeholder") {
            println!("  {} {}", "→".yellow(), msg);
        } else if msg.contains("Checking for taxonomy") {
            println!("  {} {}", "●".cyan(), msg);
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
        // Force download even if up-to-date (clears resume state)
        runtime.block_on(async { manager.force_download_clear_resume(&database_source, progress).await })?
    } else if resume {
        // Use resume-enabled download when --resume flag is set
        runtime.block_on(async { manager.download_with_resume(&database_source, true, progress).await })?
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
            DownloadResult::Updated { .. } | DownloadResult::InitialDownload { .. } | DownloadResult::Downloaded { .. } => {
                tl.update_task(manifest_task, TaskStatus::Complete);
            }
        }
    }

    if !args.quiet {
        println!("  {} Manifest created", "✓".green());
        println!();
    }

    // Handle output document option (export to FASTA if specified)
    if let Some(ref output_doc) = args.output_document {
        info(&format!("Exporting database to {}", output_doc.display()));

        // Use the export command to create the FASTA file
        use crate::cli::commands::database::export::{ExportArgs, ExportFormat, run as export_run};
        let export_args = ExportArgs {
            database: format!("{}", database_source),
            output: Some(output_doc.clone()),
            force: true,
            format: ExportFormat::Fasta,
            compress: output_doc.extension().map_or(false, |ext| ext == "gz"),
            no_cache: false,
            cached_only: false,
            with_taxonomy: false,
            quiet: args.quiet,
            stream: true,
            sequence_date: None,
            taxonomy_date: None,
            taxonomy_filter: None,
            redundancy: None,
            max_sequences: None,
            sample: None,
        };

        export_run(export_args)?;
        success(&format!("Database exported to {}", output_doc.display()));
    }

    // Handle mirror mode
    if args.mirror {
        info("Mirror mode: maintaining exact database structure");
        // Mirror mode would copy the exact directory structure
        // This is already handled by the default behavior of DatabaseManager
    }

    // Report results with nice formatting (unless quiet mode)
    if args.quiet {
        // In quiet mode, just return success
        return Ok(());
    }

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
                subsection_header("Current Version");
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
            ..
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
        DownloadResult::InitialDownload { .. } => {
            println!("{}", "─".repeat(80).dimmed());
            if args.dry_run {
                warning("Initial download required - run without --dry-run to download");
                println!();
                info("This database has not been downloaded yet");
                info("Full download will be performed on first run");
            } else {
                success("Database downloaded successfully!");
                println!();
                subsection_header("Download Summary");
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
        DownloadResult::Downloaded { total_chunks, total_size } => {
            println!("{}", "─".repeat(80).dimmed());
            success("Database downloaded successfully!");
            info(&format!("Downloaded {} chunks", total_chunks));
            info(&format!("Total size: {} bytes", total_size));
        }
    }

    // Show stats in a tree format
    let stats = manager.get_stats()?;

    println!();
    subsection_header("Repository Statistics");
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
