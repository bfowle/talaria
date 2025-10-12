#![allow(dead_code)]

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use colored::*;
use std::path::PathBuf;

use crate::cli::formatting::output::{
    success as print_success, tree_section, warning as print_warning,
};
use talaria_herald::database::DatabaseManager;
use talaria_utils::database::database_ref::parse_database_reference;

#[derive(Args)]
pub struct VersionsArgs {
    #[command(subcommand)]
    pub command: VersionsCommand,
}

#[derive(Subcommand)]
pub enum VersionsCommand {
    /// List all available versions for a database
    List(ListVersionsArgs),

    /// Set the current version for a database
    SetCurrent(SetCurrentArgs),

    /// Create an alias for a database version
    Tag(TagVersionArgs),

    /// Remove an alias from a database version
    Untag(UntagVersionArgs),

    /// Show detailed information about a version
    Info(InfoVersionArgs),

    /// Import version info from downloaded database
    Import(ImportVersionArgs),
}

#[derive(Args)]
pub struct ListVersionsArgs {
    /// Database reference (e.g., "uniprot/swissprot")
    pub database: String,

    /// Show detailed information
    #[arg(long)]
    pub detailed: bool,

    /// Show internal timestamps
    #[arg(long)]
    pub show_timestamps: bool,
}

#[derive(Args)]
pub struct SetCurrentArgs {
    /// Database reference
    pub database: String,

    /// Version to set as current (e.g., "2024_04", "stable", or timestamp)
    pub version: String,

    /// Also set as stable
    #[arg(long)]
    pub as_stable: bool,
}

#[derive(Args)]
pub struct TagVersionArgs {
    /// Database reference
    pub database: String,

    /// Version to tag
    pub version: String,

    /// Alias name (e.g., "stable", "paper-2024", "production")
    pub alias: String,

    /// Force overwrite if alias exists
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Args)]
pub struct UntagVersionArgs {
    /// Database reference
    pub database: String,

    /// Alias name to remove (e.g., "stable", "my-test-tag")
    ///
    /// Note: Cannot remove protected aliases 'current' or 'latest'
    pub alias: String,

    /// Skip confirmation prompt
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Args)]
pub struct InfoVersionArgs {
    /// Full database reference including version (e.g., "uniprot/swissprot@2024_04")
    pub database: String,
}

#[derive(Args)]
pub struct ImportVersionArgs {
    /// Database reference
    pub database: String,

    /// Path to database file or manifest
    pub path: PathBuf,

    /// Override detected version
    #[arg(long)]
    pub version_override: Option<String>,
}

pub fn run(args: VersionsArgs) -> Result<()> {
    match args.command {
        VersionsCommand::List(args) => list_versions(args),
        VersionsCommand::SetCurrent(args) => set_current(args),
        VersionsCommand::Tag(args) => tag_version(args),
        VersionsCommand::Untag(args) => untag_version(args),
        VersionsCommand::Info(args) => show_info(args),
        VersionsCommand::Import(args) => import_version(args),
    }
}

fn list_versions(args: ListVersionsArgs) -> Result<()> {
    use humansize::{format_size, BINARY};

    let db_ref = parse_database_reference(&args.database)?;
    let manager = DatabaseManager::new(None)?;

    let versions = manager.list_database_versions(&db_ref.source, &db_ref.dataset)?;

    if versions.is_empty() {
        print_warning(&format!("No versions found for {}", args.database));
        println!("\nHint: Download a database first with:");
        println!("  talaria database download {}", args.database);
        return Ok(());
    }

    println!(
        "\n{} {} for {}\n",
        "●".cyan().bold(),
        "Available versions".bold(),
        args.database.cyan()
    );

    for version in &versions {
        // Check for standard aliases
        let is_current = version.aliases.contains(&"current".to_string());
        let is_latest = version.aliases.contains(&"latest".to_string());
        let is_stable = version.aliases.contains(&"stable".to_string());

        let mut markers = vec![];
        if is_current {
            markers.push("current".green().bold().to_string());
        }
        if is_latest {
            markers.push("latest".blue().to_string());
        }
        if is_stable {
            markers.push("stable".yellow().to_string());
        }

        // Add custom aliases (those not in standard set)
        for alias in &version.aliases {
            if !matches!(alias.as_str(), "current" | "latest" | "stable") {
                markers.push(alias.dimmed().to_string());
            }
        }

        let markers_str = if !markers.is_empty() {
            format!(" ({})", markers.join(", "))
        } else {
            String::new()
        };

        let date = version.created_at.format("%Y-%m-%d %H:%M");

        // Use upstream_version if available, otherwise timestamp
        let display_name = if args.show_timestamps {
            format!(
                "{} ({})",
                version
                    .upstream_version
                    .as_ref()
                    .unwrap_or(&version.timestamp),
                version.timestamp.dimmed()
            )
        } else {
            version
                .upstream_version
                .as_ref()
                .unwrap_or(&version.timestamp)
                .to_string()
        };

        if args.detailed {
            println!(
                "  {} {}{}",
                if is_current {
                    "▶".green().bold()
                } else {
                    "●".dimmed()
                },
                display_name.bold(),
                markers_str
            );
            println!("    Timestamp:  {}", version.timestamp.dimmed());
            println!("    Downloaded: {}", date.to_string().dimmed());

            if let Some(ref upstream) = version.upstream_version {
                println!("    Upstream:   {}", upstream.cyan());
            }

            println!("    Chunks:     {}", version.chunk_count);
            println!("    Sequences:  {}", version.sequence_count);
            println!(
                "    Size:       {}",
                format_size(version.total_size, BINARY)
            );

            if !version.aliases.is_empty() {
                println!("    Aliases:    {}", version.aliases.join(", ").dimmed());
            }

            println!();
        } else {
            println!(
                "  {} {:<30} {} - downloaded {}",
                if is_current {
                    "▶".green().bold()
                } else {
                    "●".dimmed()
                },
                display_name.bold(),
                markers_str,
                date.to_string().dimmed()
            );
        }
    }

    if !args.detailed {
        println!("\nUse --detailed for more information");
    }

    Ok(())
}

fn set_current(args: SetCurrentArgs) -> Result<()> {
    let db_ref = parse_database_reference(&args.database)?;
    let manager = DatabaseManager::new(None)?;

    // Resolve version reference to timestamp
    let timestamp = manager
        .resolve_version_reference(&db_ref.source, &db_ref.dataset, &args.version)
        .context(format!("Failed to resolve version '{}'", args.version))?;

    // Get version info for display
    let versions = manager.list_database_versions(&db_ref.source, &db_ref.dataset)?;
    let version = versions
        .iter()
        .find(|v| v.timestamp == timestamp)
        .context("Version metadata not found")?;

    // Set current alias in RocksDB
    manager
        .set_version_alias(&db_ref.source, &db_ref.dataset, &timestamp, "current")
        .context("Failed to set current version")?;

    let display_name = version
        .upstream_version
        .as_ref()
        .unwrap_or(&version.timestamp);
    print_success(&format!(
        "Set {} ({}) as current version for {}",
        display_name.cyan(),
        timestamp.dimmed(),
        args.database
    ));

    // Optionally set as stable too
    if args.as_stable {
        manager.set_version_alias(&db_ref.source, &db_ref.dataset, &timestamp, "stable")?;
        print_success(&format!("Also tagged {} as stable", display_name.cyan()));
    }

    Ok(())
}

fn tag_version(args: TagVersionArgs) -> Result<()> {
    let db_ref = parse_database_reference(&args.database)?;
    let manager = DatabaseManager::new(None)?;

    // Resolve version reference to timestamp
    let timestamp = manager
        .resolve_version_reference(&db_ref.source, &db_ref.dataset, &args.version)
        .context(format!("Failed to resolve version '{}'", args.version))?;

    // Get version info for display
    let versions = manager.list_database_versions(&db_ref.source, &db_ref.dataset)?;
    let version = versions
        .iter()
        .find(|v| v.timestamp == timestamp)
        .context("Version metadata not found")?;

    // Check if alias already exists (by checking if it resolves to anything)
    if !args.force {
        if let Ok(existing) =
            manager.resolve_version_reference(&db_ref.source, &db_ref.dataset, &args.alias)
        {
            if existing != timestamp {
                print_warning(&format!(
                    "Alias '{}' already points to a different version. Use --force to overwrite.",
                    args.alias
                ));
                return Ok(());
            }
        }
    }

    // Create custom alias
    manager
        .set_version_alias(&db_ref.source, &db_ref.dataset, &timestamp, &args.alias)
        .context("Failed to create alias")?;

    let display_name = version
        .upstream_version
        .as_ref()
        .unwrap_or(&version.timestamp);
    print_success(&format!(
        "Created alias '{}' → {} ({})",
        args.alias.cyan(),
        display_name,
        timestamp.dimmed()
    ));

    Ok(())
}

fn untag_version(args: UntagVersionArgs) -> Result<()> {
    let db_ref = parse_database_reference(&args.database)?;
    let manager = DatabaseManager::new(None)?;

    // Verify the alias exists and get its target
    let target_timestamp = manager
        .resolve_version_reference(&db_ref.source, &db_ref.dataset, &args.alias)
        .context(format!("Alias '{}' not found", args.alias))?;

    // Get version info for display
    let versions = manager.list_database_versions(&db_ref.source, &db_ref.dataset)?;
    let version = versions.iter().find(|v| v.timestamp == target_timestamp);

    // Display what will be removed
    println!();
    println!("Removing alias '{}'", args.alias.cyan());
    if let Some(v) = version {
        let display_name = v.upstream_version.as_ref().unwrap_or(&v.timestamp);
        println!("  Currently points to: {}", display_name.cyan());
        println!("  Timestamp: {}", v.timestamp.dimmed());
    } else {
        println!("  Currently points to: {}", target_timestamp.dimmed());
    }
    println!();

    // Confirm deletion
    if !args.force {
        use dialoguer::Confirm;
        let confirmed = Confirm::new()
            .with_prompt(format!("Remove alias '{}'?", args.alias))
            .default(false)
            .interact()?;

        if !confirmed {
            println!("Cancelled");
            return Ok(());
        }
    }

    // Delete the alias
    manager
        .delete_version_alias(&db_ref.source, &db_ref.dataset, &args.alias)
        .context("Failed to remove alias")?;

    print_success(&format!("Removed alias '{}'", args.alias.cyan()));

    Ok(())
}

fn show_info(args: InfoVersionArgs) -> Result<()> {
    use humansize::{format_size, BINARY};

    let db_ref = parse_database_reference(&args.database)?;
    let manager = DatabaseManager::new(None)?;

    let version_to_find = db_ref.version.as_deref().unwrap_or("current");

    // Resolve to timestamp first
    let timestamp = manager
        .resolve_version_reference(&db_ref.source, &db_ref.dataset, version_to_find)
        .context(format!("Failed to resolve version '{}'", version_to_find))?;

    // Get the full version info
    let versions = manager.list_database_versions(&db_ref.source, &db_ref.dataset)?;
    let version = versions
        .iter()
        .find(|v| v.timestamp == timestamp)
        .context("Version metadata not found")?;

    // Get the manifest for additional details
    let manifest = manager.get_version_manifest(&db_ref.source, &db_ref.dataset, &timestamp)?;

    println!(
        "\n{} {} {}",
        "●".cyan().bold(),
        "Version Information for".bold(),
        args.database.cyan()
    );

    let mut info = vec![];
    let display_name = version
        .upstream_version
        .as_ref()
        .unwrap_or(&version.timestamp);
    info.push(("Version", display_name.to_string()));
    info.push(("Timestamp", version.timestamp.clone()));

    if let Some(ref upstream) = version.upstream_version {
        info.push(("Upstream Version", upstream.clone()));
    }

    info.push((
        "Downloaded",
        version.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
    ));

    info.push(("Chunks", version.chunk_count.to_string()));
    info.push(("Sequences", version.sequence_count.to_string()));
    info.push(("Total Size", format_size(version.total_size, BINARY)));

    // Show aliases
    if !version.aliases.is_empty() {
        info.push(("Aliases", version.aliases.join(", ")));
    }

    info.push(("Sequence Version", manifest.sequence_version.clone()));
    info.push(("Taxonomy Version", manifest.taxonomy_version.clone()));

    tree_section("Details", info, false);

    println!("\n{} Manifest stored in RocksDB", "✓".green().bold());

    Ok(())
}

fn import_version(_args: ImportVersionArgs) -> Result<()> {
    // Import functionality is deprecated with RocksDB-based storage
    // Versions are automatically tracked when databases are downloaded
    print_warning("The 'import' command is deprecated with HERALD RocksDB storage.");
    println!("\nDatabase versions are automatically tracked when downloaded.");
    println!("Use 'talaria database download' to add databases to the repository.");
    Ok(())
}
