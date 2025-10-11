use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use dialoguer::Confirm;
use humansize::{format_size, BINARY};

use talaria_sequoia::database::DatabaseManager;
use talaria_utils::database::database_ref::parse_database_reference;

#[derive(Args)]
pub struct DeleteArgs {
    /// Database reference: source/dataset[@version][:profile]
    ///
    /// Examples:
    ///
    ///   uniprot/swissprot               - Delete entire database (all versions)
    ///
    ///   uniprot/swissprot@2024_04       - Delete specific version
    ///
    ///   uniprot/swissprot@current       - Delete current version
    ///
    ///   uniprot/swissprot@my-test-tag   - Delete version by alias
    pub database: String,

    /// Skip confirmation prompt
    #[arg(long, short)]
    pub force: bool,

    /// Show what would be deleted without actually deleting
    #[arg(long)]
    pub dry_run: bool,

    /// When deleting current version, reassign to latest
    #[arg(long)]
    pub reassign_current: bool,

    /// Keep the current version when deleting entire database
    #[arg(long, conflicts_with = "reassign_current")]
    pub keep_current: bool,
}

pub fn run(args: DeleteArgs) -> Result<()> {
    let db_ref = parse_database_reference(&args.database)?;
    let manager = DatabaseManager::new(None)?;

    // Determine what we're deleting
    if db_ref.version.is_none() {
        // Delete entire database
        delete_entire_database(&manager, &db_ref, &args)
    } else {
        // Delete specific version
        delete_single_version(&manager, &db_ref, &args)
    }
}

fn delete_single_version(
    manager: &DatabaseManager,
    db_ref: &talaria_core::types::DatabaseReference,
    args: &DeleteArgs,
) -> Result<()> {
    let version_ref = db_ref.version.as_ref().expect("Version should be present");

    // Get version info
    let version_info = manager
        .get_version_info(&db_ref.source, &db_ref.dataset, version_ref)
        .context("Failed to get version information")?;

    // Check if this is the current version
    let is_current = version_info.aliases.contains(&"current".to_string());

    // Display what will be deleted
    println!();
    println!(
        "{} {}",
        "⚠".yellow().bold(),
        "You are about to delete:".bold()
    );
    println!();
    println!(
        "  {}  {}/{}",
        "Database:".dimmed(),
        db_ref.source,
        db_ref.dataset
    );
    println!(
        "  {}   {}",
        "Version:".dimmed(),
        version_info
            .upstream_version
            .as_ref()
            .unwrap_or(&version_info.timestamp)
            .cyan()
    );
    println!(
        "  {}    {} ({})",
        "Timestamp:".dimmed(),
        version_info.timestamp.dimmed(),
        version_info.created_at.format("%Y-%m-%d %H:%M UTC")
    );
    println!(
        "  {}    {}",
        "Chunks:".dimmed(),
        version_info.chunk_count.to_string().cyan()
    );
    println!(
        "  {}      {}",
        "Size:".dimmed(),
        format_size(version_info.total_size, BINARY).cyan()
    );
    println!(
        "  {} {}",
        "Sequences:".dimmed(),
        version_info.sequence_count.to_string().cyan()
    );
    println!();

    if !version_info.aliases.is_empty() {
        let aliases_str = version_info.aliases.join(", ");
        println!("  {}   {}", "Aliases:".dimmed(), aliases_str.yellow());
    } else {
        println!("  {}   {}", "Aliases:".dimmed(), "none".dimmed());
    }
    println!();

    // Warn if current version
    if is_current {
        println!(
            "{} {}",
            "⚠".yellow().bold(),
            "This is the current version!".yellow().bold()
        );

        if args.reassign_current {
            println!("  The 'current' alias will be removed.");
            println!("  You will need to manually set a new current version.");
        } else {
            println!("  Use --reassign-current to automatically set the next version as current.");
        }
        println!();
    }

    // Check for other versions
    let all_versions = manager.list_database_versions(&db_ref.source, &db_ref.dataset)?;
    let remaining_count = all_versions.len() - 1;

    if remaining_count > 0 {
        println!(
            "{} {} other version(s) will remain.",
            "ℹ".blue(),
            remaining_count.to_string().cyan()
        );
    } else {
        println!(
            "{} {}",
            "⚠".yellow().bold(),
            "This is the last version of this database!".yellow()
        );
    }
    println!();

    // Note about chunks
    println!("{}", "Note:".bold());
    println!("  Chunks may be shared with other versions or databases.");
    println!(
        "  Run {} to remove orphaned chunks.",
        format!(
            "'talaria database clean {}/{}'",
            db_ref.source, db_ref.dataset
        )
        .cyan()
    );
    println!();

    if args.dry_run {
        println!(
            "{} {} (dry run)",
            "✓".green(),
            "Would delete this version".dimmed()
        );
        return Ok(());
    }

    // Confirm deletion
    if !args.force {
        let prompt = if is_current {
            format!("Delete current version {}?", version_ref)
        } else {
            format!("Delete version {}?", version_ref)
        };

        let confirmed = Confirm::new()
            .with_prompt(prompt)
            .default(false)
            .interact()?;

        if !confirmed {
            println!("Deletion cancelled");
            return Ok(());
        }
    }

    // Perform deletion
    manager
        .delete_database_version(&db_ref.source, &db_ref.dataset, version_ref)
        .context("Failed to delete version")?;

    println!();
    println!(
        "{} Deleted version {}",
        "✓".green().bold(),
        version_ref.cyan()
    );
    println!(
        "  {}",
        format!(
            "Removed manifest: manifest:{}:{}:{}",
            db_ref.source, db_ref.dataset, version_info.timestamp
        )
        .dimmed()
    );

    // Handle current version reassignment
    if is_current && args.reassign_current && remaining_count > 0 {
        // Find the latest remaining version
        let remaining_versions: Vec<_> = all_versions
            .into_iter()
            .filter(|v| v.timestamp != version_info.timestamp)
            .collect();

        if let Some(latest) = remaining_versions.first() {
            manager.set_version_alias(
                &db_ref.source,
                &db_ref.dataset,
                &latest.timestamp,
                "current",
            )?;
            println!(
                "{} Reassigned 'current' to {}",
                "✓".green(),
                latest
                    .upstream_version
                    .as_ref()
                    .unwrap_or(&latest.timestamp)
                    .cyan()
            );
        }
    }

    println!();

    Ok(())
}

fn delete_entire_database(
    manager: &DatabaseManager,
    db_ref: &talaria_core::types::DatabaseReference,
    args: &DeleteArgs,
) -> Result<()> {
    // Get all versions
    let versions = manager.list_database_versions(&db_ref.source, &db_ref.dataset)?;

    if versions.is_empty() {
        println!(
            "{} No versions found for {}/{}",
            "ℹ".blue(),
            db_ref.source,
            db_ref.dataset
        );
        return Ok(());
    }

    // Calculate totals
    let total_chunks: usize = versions.iter().map(|v| v.chunk_count).sum();
    let total_size: u64 = versions.iter().map(|v| v.total_size).sum();
    let total_sequences: usize = versions.iter().map(|v| v.sequence_count).sum();

    // Find current version
    let current_version = versions
        .iter()
        .find(|v| v.aliases.contains(&"current".to_string()));

    // Display what will be deleted
    println!();
    println!(
        "{} {}",
        "⚠".yellow().bold(),
        "You are about to delete ENTIRE DATABASE:".bold()
    );
    println!();
    println!(
        "  {}  {}/{}",
        "Database:".dimmed(),
        db_ref.source,
        db_ref.dataset
    );
    println!(
        "  {}  {}",
        "Versions:".dimmed(),
        versions.len().to_string().cyan()
    );
    println!(
        "  {}    {} (total across all versions)",
        "Chunks:".dimmed(),
        total_chunks.to_string().cyan()
    );
    println!(
        "  {}      {} (total)",
        "Size:".dimmed(),
        format_size(total_size, BINARY).cyan()
    );
    println!(
        "  {} {} (total)",
        "Sequences:".dimmed(),
        total_sequences.to_string().cyan()
    );
    println!();

    if let Some(current) = current_version {
        println!(
            "  {} {}",
            "Current:".dimmed(),
            current
                .upstream_version
                .as_ref()
                .unwrap_or(&current.timestamp)
                .yellow()
        );
        println!();
    }

    println!("{}", "Versions to be deleted:".bold());
    for version in &versions {
        let aliases_str = if !version.aliases.is_empty() {
            format!(" ({})", version.aliases.join(", "))
        } else {
            String::new()
        };
        println!(
            "  • {}{}",
            version
                .upstream_version
                .as_ref()
                .unwrap_or(&version.timestamp),
            aliases_str.dimmed()
        );
    }
    println!();

    // Handle keep-current option
    if args.keep_current && current_version.is_some() {
        println!(
            "{} {} will be preserved",
            "ℹ".blue(),
            "Current version".cyan()
        );
        println!();
    }

    // Note about chunks
    println!("{}", "Note:".bold());
    println!("  This removes all version metadata from RocksDB.");
    println!("  Chunks will remain in storage (may be shared with other databases).");
    println!(
        "  Run {} to remove orphaned chunks.",
        format!(
            "'talaria database clean {}/{}'",
            db_ref.source, db_ref.dataset
        )
        .cyan()
    );
    println!();

    if args.dry_run {
        println!(
            "{} {} (dry run)",
            "✓".green(),
            "Would delete entire database".dimmed()
        );
        return Ok(());
    }

    // Confirm deletion
    if !args.force {
        let prompt = format!(
            "Delete ALL {} versions of {}/{}?",
            versions.len(),
            db_ref.source,
            db_ref.dataset
        );

        let confirmed = Confirm::new()
            .with_prompt(prompt)
            .default(false)
            .interact()?;

        if !confirmed {
            println!("Deletion cancelled");
            return Ok(());
        }
    }

    // Perform deletion
    let deleted_versions = manager
        .delete_entire_database(&db_ref.source, &db_ref.dataset)
        .context("Failed to delete database")?;

    println!();
    println!(
        "{} Deleted {} versions of {}/{}",
        "✓".green().bold(),
        deleted_versions.len().to_string().cyan(),
        db_ref.source,
        db_ref.dataset
    );

    for version in &deleted_versions {
        println!(
            "  {}",
            format!(
                "• Removed manifest:{}:{}:{}",
                db_ref.source, db_ref.dataset, version
            )
            .dimmed()
        );
    }
    println!();

    Ok(())
}
