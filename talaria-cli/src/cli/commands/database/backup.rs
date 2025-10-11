use anyhow::Result;
use clap::{Args, Subcommand};
use colored::Colorize;

use talaria_sequoia::backup::BackupManager;
use talaria_sequoia::database::DatabaseManager;

#[derive(Debug, Args)]
pub struct BackupCommand {
    #[clap(subcommand)]
    pub command: BackupSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum BackupSubcommand {
    /// Create a new backup of the RocksDB database
    Create {
        /// Name for the backup
        name: String,

        #[arg(long, help = "Description of why this backup was created")]
        description: Option<String>,
    },

    /// Restore database from a backup
    Restore {
        /// Name of backup to restore
        name: String,
    },

    /// List available backups
    List {
        #[arg(long, short = 'd', help = "Show detailed information")]
        detailed: bool,
    },

    /// Verify a backup's integrity
    Verify {
        /// Name of backup to verify
        name: String,
    },

    /// Delete a backup
    Delete {
        /// Name of backup to delete
        name: String,
    },

    /// Purge old backups, keeping only N most recent
    Purge {
        /// Number of backups to keep
        #[arg(long, short = 'k', default_value = "5")]
        keep: usize,
    },
}

pub fn execute(cmd: &BackupCommand) -> Result<()> {
    let manager = BackupManager::new()?;

    match &cmd.command {
        BackupSubcommand::Create { name, description } => {
            // Get RocksDB backend from DatabaseManager
            let db_manager = DatabaseManager::new(None)?;
            let rocksdb = db_manager
                .get_repository()
                .storage
                .sequence_storage
                .get_rocksdb();

            let metadata = manager.create_backup(&rocksdb, name, description.clone())?;
            println!();
            println!("{} Backup created successfully", "✓".green().bold());
            println!("  Name: {}", metadata.name.cyan());
            println!("  ID: {}", metadata.id);
            if let Some(ref desc) = metadata.description {
                println!("  Description: {}", desc);
            }
            println!(
                "  Created: {}",
                metadata.created_at.format("%Y-%m-%d %H:%M:%S UTC")
            );
        }

        BackupSubcommand::Restore { name } => {
            println!();
            println!(
                "{} {}",
                "⚠".yellow().bold(),
                "WARNING: Restore requires manual database shutdown!".yellow()
            );
            println!();
            manager.restore_backup(name)?;
        }

        BackupSubcommand::List { detailed } => {
            let backups = manager.list_backups()?;

            if backups.is_empty() {
                println!();
                println!("{}", "No backups found.".dimmed());
                println!();
                println!(
                    "Create one with: {}",
                    "talaria database backup create <name>".cyan()
                );
                return Ok(());
            }

            println!();
            println!("{} {}", "●".cyan().bold(), "Available Backups".bold());
            println!();

            for backup in &backups {
                if *detailed {
                    println!("  {} {}", "▶".cyan(), backup.name.bold());
                    println!("    ID:          {}", backup.id);
                    println!(
                        "    Created:     {}",
                        backup.created_at.format("%Y-%m-%d %H:%M:%S UTC")
                    );
                    if let Some(ref desc) = backup.description {
                        println!("    Description: {}", desc);
                    }
                    println!(
                        "    Size:        {:.2} MB",
                        backup.size_bytes as f64 / 1_048_576.0
                    );
                    println!();
                } else {
                    let desc = backup
                        .description
                        .as_ref()
                        .map(|d| format!(" - {}", d.dimmed()))
                        .unwrap_or_default();
                    println!(
                        "  {} {} ({}{})",
                        "●".dimmed(),
                        backup.name.cyan().bold(),
                        backup.created_at.format("%Y-%m-%d").to_string().dimmed(),
                        desc
                    );
                }
            }

            if !*detailed {
                println!();
                println!("Use --detailed for more information");
            }
        }

        BackupSubcommand::Verify { name } => {
            manager.verify_backup(name)?;
        }

        BackupSubcommand::Delete { name } => {
            // Confirm deletion
            use dialoguer::Confirm;
            let confirmed = Confirm::new()
                .with_prompt(format!(
                    "Are you sure you want to delete backup '{}'?",
                    name
                ))
                .default(false)
                .interact()?;

            if confirmed {
                manager.delete_backup(name)?;
            } else {
                println!("Deletion cancelled");
            }
        }

        BackupSubcommand::Purge { keep } => {
            // Confirm purge
            use dialoguer::Confirm;
            let confirmed = Confirm::new()
                .with_prompt(format!(
                    "Purge old backups, keeping only {} most recent?",
                    keep
                ))
                .default(false)
                .interact()?;

            if confirmed {
                manager.purge_old_backups(*keep)?;
            } else {
                println!("Purge cancelled");
            }
        }
    }

    Ok(())
}
