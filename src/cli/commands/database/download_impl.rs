/// Database download implementation using content-addressed storage

use crate::cli::commands::database::download::DownloadArgs;
use crate::cli::formatter::{self, TaskList, TaskStatus, info_box, print_tip, format_bytes};
use crate::cli::output::*;
use crate::core::database_manager::{DatabaseManager, DownloadResult};
use crate::download::DatabaseSource;
use anyhow::Result;
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

    // Create task list for tracking operations
    let mut task_list = TaskList::new();

    // Print header
    let db_name = format!("{}", database_source);
    task_list.print_header(&format!("Database Download: {}", db_name));

    // Info box about CASG
    info_box("Content-Addressed Storage (CASG)", &[
        "Automatic deduplication",
        "Incremental updates",
        "Cryptographic verification",
        "Bandwidth-efficient downloads"
    ]);

    // Add initialization task
    let init_task = task_list.add_task("Initialize CASG repository");
    let spinner = task_list.start_spinner(init_task, "Setting up storage system...");

    // Initialize database manager with JSON flag
    let mut manager = DatabaseManager::with_options(None, args.json)?;

    if let Some(spinner) = spinner {
        spinner.finish_and_clear();
    }
    task_list.update_task(init_task, TaskStatus::Complete);

    // Check for resumable operations
    if args.resume {
        let resume_task = task_list.add_task("Check for resumable downloads");
        task_list.update_task(resume_task, TaskStatus::InProgress);

        let resumable_ops = manager.list_resumable_operations()?;
        if !resumable_ops.is_empty() {
            subsection_header(&format!("Found {} resumable operation(s)", resumable_ops.len()));
            for (i, (op_id, state)) in resumable_ops.iter().enumerate() {
                let is_last = i == resumable_ops.len() - 1;
                tree_item(is_last, &format!("{}: {}", op_id, state.summary()), None);
            }
            task_list.update_task(resume_task, TaskStatus::Complete);
        } else {
            task_list.update_task(resume_task, TaskStatus::Skipped);
        }
    }

    // Add download tasks
    let check_task = task_list.add_task("Check for existing data");
    let download_task = task_list.add_task("Download database");
    let process_task = task_list.add_task("Process into chunks");
    let store_task = task_list.add_task("Store in repository");
    let manifest_task = task_list.add_task("Create manifest");

    // Create a shared task list for progress callback
    let shared_task_list = Arc::new(Mutex::new(task_list));
    let task_list_clone = Arc::clone(&shared_task_list);

    // Progress callback that updates task list
    let progress = move |msg: &str| {
        // Parse the message to update appropriate task
        if msg.contains("[NEW]") || msg.contains("Initial download required") {
            let mut tl = task_list_clone.lock().unwrap();
            tl.update_task(check_task, TaskStatus::Complete);
            // Don't print warnings here, they interfere with task list
        } else if msg.contains("Checking for updates") {
            let mut tl = task_list_clone.lock().unwrap();
            tl.update_task(check_task, TaskStatus::InProgress);
        } else if msg.contains("up to date") || msg.contains("Up to date") {
            let mut tl = task_list_clone.lock().unwrap();
            tl.update_task(check_task, TaskStatus::Complete);
            tl.update_task(download_task, TaskStatus::Skipped);
            tl.update_task(process_task, TaskStatus::Skipped);
            tl.update_task(store_task, TaskStatus::Skipped);
            tl.update_task(manifest_task, TaskStatus::Skipped);
        } else if msg.contains("Downloading full database") {
            // This is when the actual download with progress bar starts
            let mut tl = task_list_clone.lock().unwrap();
            tl.update_task(download_task, TaskStatus::InProgress);
            tl.pause_updates(); // Pause task list updates during download progress bar
            drop(tl);
            println!("\n{}", msg); // Print the message below task list
        } else if msg.contains("Downloading") && msg.contains("database") {
            let mut tl = task_list_clone.lock().unwrap();
            tl.update_task(download_task, TaskStatus::InProgress);
        } else if msg.contains("Processing") || msg.contains("Creating") && msg.contains("chunks") {
            let mut tl = task_list_clone.lock().unwrap();
            tl.resume_updates(); // Resume updates after download completes
            tl.update_task(download_task, TaskStatus::Complete);
            tl.update_task(process_task, TaskStatus::InProgress);
        } else if msg.contains("Storing") || msg.contains("chunks stored") {
            let mut tl = task_list_clone.lock().unwrap();
            tl.update_task(process_task, TaskStatus::Complete);
            tl.update_task(store_task, TaskStatus::InProgress);
        } else if msg.contains("manifest") || msg.contains("Manifest") {
            let mut tl = task_list_clone.lock().unwrap();
            tl.update_task(store_task, TaskStatus::Complete);
            tl.update_task(manifest_task, TaskStatus::InProgress);
        }

        // Don't print raw messages during task list updates - they interfere with rendering
    };

    // Run the download
    let result = runtime.block_on(async {
        manager.download(&database_source, progress).await
    })?;

    // Update final task statuses based on result
    {
        let mut tl = shared_task_list.lock().unwrap();
        match result {
            DownloadResult::UpToDate => {
                // Tasks already marked as skipped
            }
            DownloadResult::Updated { .. } | DownloadResult::InitialDownload => {
                tl.update_task(manifest_task, TaskStatus::Complete);
            }
        }
    }

    // Report results with nice formatting
    match result {
        DownloadResult::UpToDate => {
            success("Database is already up to date!");
            info("No downloads needed - saved bandwidth and time");
        }
        DownloadResult::Updated { chunks_added, chunks_removed } => {
            success("Database updated successfully!");
            let mut items = vec![
                ("Added", format!("{} new chunks", format_number(chunks_added))),
            ];
            if chunks_removed > 0 {
                items.push(("Removed", format!("{} obsolete chunks", format_number(chunks_removed))));
            }
            items.push(("Efficiency", "Only downloaded what changed".to_string()));
            tree_section("Update Summary", items, true);
        }
        DownloadResult::InitialDownload => {
            success("Initial database setup complete!");
            info("Database has been chunked and stored");
            info("Future updates will only download changed chunks");
            print_tip("Set TALARIA_MANIFEST_SERVER environment variable to enable incremental updates from a manifest server");
        }
    }

    // Show stats in a nice table
    let stats = manager.get_stats()?;
    let stats_data = vec![
        ("Total chunks", stats.total_chunks.to_string()),
        ("Total size", format_bytes(stats.total_size as u64)),
        ("Compressed chunks", stats.compressed_chunks.to_string()),
        ("Deduplication ratio", format!("{:.2}x", stats.deduplication_ratio)),
        ("Databases managed", stats.database_count.to_string()),
    ];

    formatter::print_stats_table("Repository Statistics", stats_data);

    Ok(())
}