use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

use talaria_sequoia::backup::BackupManager;

#[derive(Debug, Args)]
pub struct BackupCommand {
    #[clap(subcommand)]
    pub command: BackupSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum BackupSubcommand {
    /// Create a new backup of current database state
    Create {
        /// Name for the backup
        name: String,

        #[arg(long, help = "Description of why this backup was created")]
        description: Option<String>,
    },

    /// Restore databases from a backup
    Restore {
        /// Name of backup to restore
        name: String,

        #[arg(long, help = "Verify chunk availability before restore")]
        verify: bool,
    },

    /// List available backups
    List {
        #[arg(long, short = 'd', help = "Show detailed information")]
        detailed: bool,
    },

    /// Export backup with all chunks to archive
    Export {
        /// Name of backup to export
        name: String,

        /// Output file path (tar.gz)
        output: PathBuf,
    },

    /// Import backup from archive
    Import {
        /// Archive file to import (tar.gz)
        archive: PathBuf,

        /// Name for imported backup
        name: String,
    },

    /// Delete a backup
    Delete {
        /// Name of backup to delete
        name: String,
    },
}

pub fn execute(cmd: &BackupCommand) -> Result<()> {
    let manager = BackupManager::new()?;

    match &cmd.command {
        BackupSubcommand::Create { name, description } => {
            manager.create_backup(name, description.clone())?;
        }
        BackupSubcommand::Restore { name, verify } => {
            manager.restore_backup(name, *verify)?;
        }
        BackupSubcommand::List { detailed } => {
            let backups = manager.list_backups(*detailed)?;
            if backups.is_empty() && !*detailed {
                println!("No backups found. Create one with: talaria database backup create <name>");
            }
        }
        BackupSubcommand::Export { name, output } => {
            manager.export_backup(name, output)?;
        }
        BackupSubcommand::Import { archive, name } => {
            manager.import_backup(archive, name)?;
        }
        BackupSubcommand::Delete { name } => {
            // Confirm deletion
            use dialoguer::Confirm;
            let confirmed = Confirm::new()
                .with_prompt(format!("Are you sure you want to delete backup '{}'?", name))
                .default(false)
                .interact()?;

            if confirmed {
                manager.delete_backup(name)?;
            } else {
                println!("Deletion cancelled");
            }
        }
    }

    Ok(())
}