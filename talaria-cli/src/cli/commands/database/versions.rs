#![allow(dead_code)]

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use colored::*;
use std::path::PathBuf;

use crate::cli::formatting::output::{
    info as print_info, success as print_success, tree_section, warning as print_warning,
};
use talaria_core::system::paths;
use crate::core::database::database_ref::parse_database_reference;
use crate::core::versioning::version_detector::{VersionDetector, VersionManager};

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

    /// Show detailed information about a version
    Info(InfoVersionArgs),

    /// Import version info from downloaded database
    Import(ImportVersionArgs),
}

#[derive(Args)]
pub struct ListVersionsArgs {
    /// Database reference (e.g., "uniprot/swissprot")
    pub database: String,

    /// Show all details
    #[arg(long)]
    pub verbose: bool,

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
        VersionsCommand::Info(args) => show_info(args),
        VersionsCommand::Import(args) => import_version(args),
    }
}

fn list_versions(args: ListVersionsArgs) -> Result<()> {
    let db_ref = parse_database_reference(&args.database)?;
    let manager = VersionManager::new(paths::talaria_databases_dir());

    let versions = manager.list_versions(&db_ref.source, &db_ref.dataset)?;

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
        // Check system aliases
        let is_current = version.aliases.system.contains(&"current".to_string());
        let is_latest = version.aliases.system.contains(&"latest".to_string());
        let is_stable = version.aliases.system.contains(&"stable".to_string());

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

        // Add custom aliases
        for alias in &version.aliases.custom {
            markers.push(alias.dimmed().to_string());
        }

        let markers_str = if !markers.is_empty() {
            format!(" ({})", markers.join(", "))
        } else {
            String::new()
        };

        let date = version.created_at.format("%Y-%m-%d %H:%M");

        let display_name = if args.show_timestamps {
            format!(
                "{} ({})",
                version.display_name(),
                version.timestamp.dimmed()
            )
        } else {
            version.display_name().to_string()
        };

        if args.verbose {
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

            // Show all aliases by category
            if !version.aliases.upstream.is_empty() {
                println!(
                    "    Upstream aliases: {}",
                    version.aliases.upstream.join(", ").cyan()
                );
            }
            if !version.aliases.custom.is_empty() {
                println!(
                    "    Custom aliases:   {}",
                    version.aliases.custom.join(", ").dimmed()
                );
            }

            if !version.profiles.is_empty() {
                println!("    Profiles:   {}", version.profiles.join(", ").dimmed());
            }

            if !version.metadata.is_empty() {
                println!("    Metadata:");
                for (key, value) in &version.metadata {
                    println!("      {}: {}", key.dimmed(), value);
                }
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

    if !args.verbose {
        println!("\nUse --verbose for more details");
    }

    Ok(())
}

fn set_current(args: SetCurrentArgs) -> Result<()> {
    let db_ref = parse_database_reference(&args.database)?;
    let manager = VersionManager::new(paths::talaria_databases_dir());

    // Resolve version reference to timestamp
    let timestamp = manager
        .resolve_version(&db_ref.source, &db_ref.dataset, &args.version)
        .context(format!("Failed to resolve version '{}'", args.version))?;

    // Get version info for display
    let versions = manager.list_versions(&db_ref.source, &db_ref.dataset)?;
    let version = versions
        .iter()
        .find(|v| v.timestamp == timestamp)
        .context("Version metadata not found")?;

    // Set current symlink
    manager
        .set_current(&db_ref.source, &db_ref.dataset, &timestamp)
        .context("Failed to set current version")?;

    print_success(&format!(
        "Set {} ({}) as current version for {}",
        version.display_name().cyan(),
        timestamp.dimmed(),
        args.database
    ));

    // Optionally set as stable too
    if args.as_stable {
        // Create stable symlink
        let versions_dir = manager.get_versions_dir(&db_ref.source, &db_ref.dataset);
        let stable_link = versions_dir.join("stable");

        #[cfg(unix)]
        {
            use std::os::unix::fs;
            if stable_link.exists() {
                std::fs::remove_file(&stable_link)?;
            }
            fs::symlink(&timestamp, &stable_link)?;
        }

        print_success(&format!(
            "Also tagged {} as stable",
            version.display_name().cyan()
        ));
    }

    Ok(())
}

fn tag_version(args: TagVersionArgs) -> Result<()> {
    let db_ref = parse_database_reference(&args.database)?;
    let manager = VersionManager::new(paths::talaria_databases_dir());

    // Resolve version reference to timestamp
    let timestamp = manager
        .resolve_version(&db_ref.source, &db_ref.dataset, &args.version)
        .context(format!("Failed to resolve version '{}'", args.version))?;

    // Get version info for display
    let versions = manager.list_versions(&db_ref.source, &db_ref.dataset)?;
    let version = versions
        .iter()
        .find(|v| v.timestamp == timestamp)
        .context("Version metadata not found")?;

    // Check if alias already exists
    let alias_path = manager
        .get_versions_dir(&db_ref.source, &db_ref.dataset)
        .join(&args.alias);

    if alias_path.exists() && !args.force {
        print_warning(&format!(
            "Alias '{}' already exists. Use --force to overwrite.",
            args.alias
        ));
        return Ok(());
    }

    // Create alias
    manager
        .create_alias(&db_ref.source, &db_ref.dataset, &timestamp, &args.alias)
        .context("Failed to create alias")?;

    print_success(&format!(
        "Created alias '{}' → {} ({})",
        args.alias.cyan(),
        version.display_name(),
        timestamp.dimmed()
    ));

    Ok(())
}

fn show_info(args: InfoVersionArgs) -> Result<()> {
    let db_ref = parse_database_reference(&args.database)?;
    let manager = VersionManager::new(paths::talaria_databases_dir());

    let version_to_find = db_ref.version.as_deref().unwrap_or("current");

    // Resolve to timestamp first
    let timestamp = manager
        .resolve_version(&db_ref.source, &db_ref.dataset, version_to_find)
        .context(format!("Failed to resolve version '{}'", version_to_find))?;

    // Get the full version info
    let versions = manager.list_versions(&db_ref.source, &db_ref.dataset)?;
    let version = versions
        .iter()
        .find(|v| v.timestamp == timestamp)
        .context("Version metadata not found")?;

    println!(
        "\n{} {} {}",
        "●".cyan().bold(),
        "Version Information for".bold(),
        args.database.cyan()
    );

    let mut info = vec![];
    info.push(("Version", version.display_name().to_string()));
    info.push(("Timestamp", version.timestamp.clone()));

    if let Some(ref upstream) = version.upstream_version {
        info.push(("Upstream Version", upstream.clone()));
    }

    info.push((
        "Downloaded",
        version.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
    ));

    // Show aliases by category
    let all_aliases = version.all_aliases();
    if !all_aliases.is_empty() {
        info.push(("All Aliases", all_aliases.join(", ")));
    }

    if !version.profiles.is_empty() {
        info.push(("Profiles", version.profiles.join(", ")));
    }

    tree_section("Details", info, false);

    if !version.metadata.is_empty() {
        let metadata: Vec<(&str, String)> = version
            .metadata
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect();
        tree_section("Metadata", metadata, false);
    }

    // Check for manifest file
    let version_dir = manager
        .get_versions_dir(&db_ref.source, &db_ref.dataset)
        .join(&version.timestamp);

    if version_dir.join("manifest.json").exists() {
        println!(
            "\n{} Manifest found at: {}",
            "✓".green().bold(),
            version_dir.join("manifest.json").display()
        );
    }

    // Check for profiles
    let profiles_dir = version_dir.join("profiles");
    if profiles_dir.exists() {
        let profile_count = std::fs::read_dir(profiles_dir)?.count();
        println!(
            "{} {} reduction profiles available",
            "✓".green().bold(),
            profile_count
        );
    }

    Ok(())
}

fn import_version(args: ImportVersionArgs) -> Result<()> {
    let db_ref = parse_database_reference(&args.database)?;
    let detector = VersionDetector::new();

    // Try to detect version from file
    let content = std::fs::read(&args.path).context("Failed to read file")?;

    let mut version = if args.path.extension().is_some_and(|e| e == "json") {
        // Try as manifest
        detector.detect_from_manifest(args.path.to_str().unwrap())?
    } else {
        // Try as database file
        detector.detect_version(&db_ref.source, &db_ref.dataset, &content)?
    };

    // Override if specified
    if let Some(override_version) = args.version_override {
        version.upstream_version = Some(override_version.clone());
        version.aliases.upstream.push(override_version);
    }

    print_success(&format!(
        "Detected version: {} for {}",
        version.display_name().cyan(),
        args.database
    ));

    if let Some(ref upstream) = version.upstream_version {
        print_info(&format!("Upstream version: {}", upstream));
    }

    // Save version info
    let manager = VersionManager::new(paths::talaria_databases_dir());
    let version_dir = manager
        .get_versions_dir(&db_ref.source, &db_ref.dataset)
        .join(&version.timestamp);

    std::fs::create_dir_all(&version_dir)?;

    let version_file = version_dir.join("version.json");
    let version_json = serde_json::to_string_pretty(&version)?;
    std::fs::write(version_file, version_json)?;

    print_success("Version information saved");

    Ok(())
}
