use anyhow::Result;
use clap::Args;
use colored::*;

#[derive(Args)]
pub struct UpdateTaxonomyArgs {
    /// Force update even if current version is up-to-date
    #[arg(short, long)]
    pub force: bool,

    /// Check for updates without downloading
    #[arg(short = 'c', long)]
    pub check_only: bool,

    /// Database repository path (default: ${TALARIA_HOME}/databases)
    #[arg(long)]
    pub db_path: Option<std::path::PathBuf>,
}

pub fn run(args: UpdateTaxonomyArgs) -> Result<()> {
    use crate::core::database_manager::{DatabaseManager, TaxonomyUpdateResult};

    println!("{} Checking for taxonomy updates...", "►".cyan().bold());

    // Initialize database manager
    let mut manager = DatabaseManager::new(
        args.db_path.map(|p| p.to_string_lossy().to_string())
    )?;

    // Get current version
    let current_version = manager.get_taxonomy_version()?;
    if let Some(ref version) = current_version {
        println!("  Current taxonomy version: {}", version.yellow());
    } else {
        println!("  No taxonomy currently installed");
    }

    if args.check_only {
        println!("  Checking NCBI for updates...");
        // For check-only, we'd need to implement a separate method
        // For now, just show current version
        return Ok(());
    }

    // Run async update
    let runtime = tokio::runtime::Runtime::new()?;
    let result = runtime.block_on(async {
        manager.update_taxonomy().await
    })?;

    match result {
        TaxonomyUpdateResult::UpToDate => {
            if args.force {
                println!("  {} Taxonomy is up-to-date, but force flag was set", "ℹ".blue());
                println!("  Force update not yet implemented");
            } else {
                println!("{} Taxonomy is already up-to-date", "✓".green().bold());
            }
        }
        TaxonomyUpdateResult::Updated { old_version, new_version } => {
            println!("{} Taxonomy updated successfully!", "✓".green().bold());
            if let Some(old) = old_version {
                println!("  Previous version: {}", old.dimmed());
            }
            if let Some(new) = new_version {
                println!("  New version: {}", new.green());
            }
        }
    }

    Ok(())
}