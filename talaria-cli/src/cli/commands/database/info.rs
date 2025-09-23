#![allow(dead_code)]

use clap::Args;

#[derive(Args)]
pub struct InfoArgs {
    /// Database reference (e.g., "uniprot/swissprot") or file path
    pub database: String,

    /// Show sequence statistics
    #[arg(long)]
    pub stats: bool,

    /// Show taxonomic distribution
    #[arg(long)]
    pub taxonomy: bool,

    /// Output format
    #[arg(long, value_enum, default_value = "text")]
    pub format: OutputFormat,

    /// Show reduction profiles if available
    #[arg(long)]
    pub show_reductions: bool,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

pub fn run(args: InfoArgs) -> anyhow::Result<()> {
    use crate::cli::output::*;
    use crate::core::database_manager::DatabaseManager;
    use crate::utils::database_ref::parse_database_reference;
    use crate::utils::progress::create_spinner;
    use humansize::{format_size, BINARY};

    // Parse the database reference to separate database and profile
    let db_ref = parse_database_reference(&args.database)?;
    let base_name = db_ref.base_ref();

    // Initialize database manager with spinner
    let spinner = create_spinner("Loading database information...");
    let manager = DatabaseManager::new(None)?;
    let databases = manager.list_databases()?;
    spinner.finish_and_clear();

    section_header("Database Information");

    // Check if a profile was specified
    if let Some(profile) = &db_ref.profile {
        // Show profile-specific information
        return show_profile_info(&manager, &db_ref, profile, &databases);
    }

    // Find the requested database (handle both slash and hyphen formats)
    let db_info = databases
        .iter()
        .find(|db| {
            // Exact match or partial match at the end
            db.name == base_name || db.name.ends_with(&base_name)
        })
        .ok_or_else(|| anyhow::anyhow!("Database '{}' not found in repository", base_name))?;

    // Build tree structure for database info
    tree_item(false, "Name", Some(&db_info.name));
    tree_item(false, "Version", Some(&db_info.version));
    tree_item(
        false,
        "Created",
        Some(&db_info.created_at.format("%Y-%m-%d %H:%M:%S").to_string()),
    );

    // Storage section
    let storage_items = vec![
        ("Chunks", db_info.chunk_count.to_string()),
        ("Size", format_size(db_info.total_size, BINARY)),
    ];
    tree_section("Storage", storage_items, false);

    // Reductions section
    if !db_info.reduction_profiles.is_empty() {
        tree_item(false, "Reductions", None);
        for (i, profile) in db_info.reduction_profiles.iter().enumerate() {
            let is_last = i == db_info.reduction_profiles.len() - 1;
            if is_last {
                tree_item_continued_last(profile, None);
            } else {
                tree_item_continued(profile, None);
            }
        }

        // Show detailed reduction info if requested
        if args.show_reductions {
            subsection_header("Reduction Details");
            let storage = manager.get_storage();
            for (idx, profile) in db_info.reduction_profiles.iter().enumerate() {
                if let Ok(Some(manifest)) = storage.get_reduction_by_profile(profile) {
                    let is_last_profile = idx == db_info.reduction_profiles.len() - 1;
                    let reduction_items = vec![
                        (
                            "Reduction Ratio",
                            format!("{:.1}%", manifest.statistics.actual_reduction_ratio * 100.0),
                        ),
                        (
                            "Reference Sequences",
                            format_number(manifest.statistics.reference_sequences),
                        ),
                        (
                            "Child Sequences",
                            format_number(manifest.statistics.child_sequences),
                        ),
                        (
                            "Coverage",
                            format!("{:.1}%", manifest.statistics.sequence_coverage * 100.0),
                        ),
                        (
                            "Size",
                            format!(
                                "{} → {}",
                                format_size(manifest.statistics.original_size as usize, BINARY),
                                format_size(manifest.statistics.reduced_size as usize, BINARY)
                            ),
                        ),
                    ];
                    tree_section(profile, reduction_items, is_last_profile);
                }
            }
        }
    } else {
        empty("No reduced versions available");
    }

    if args.stats {
        stats_header("Statistics");
        // We'd need to assemble and analyze to get full stats
        info("Full statistics require assembling chunks");
        info("This will be implemented in a future update");
    }

    // Show storage benefits as tree
    let stats = manager.get_stats()?;
    let benefits_items = vec![
        (
            "Deduplication ratio",
            format!("{:.2}x", stats.deduplication_ratio),
        ),
        (
            "Storage saved",
            format!(
                "~{}%",
                ((1.0 - 1.0 / stats.deduplication_ratio) * 100.0) as i32
            ),
        ),
        ("Incremental updates", "Enabled".to_string()),
        ("Cryptographic verification", "SHA256".to_string()),
    ];
    tree_section("Storage Benefits", benefits_items, true);

    Ok(())
}

fn show_profile_info(
    manager: &crate::core::database_manager::DatabaseManager,
    db_ref: &crate::utils::database_ref::DatabaseReference,
    profile: &str,
    databases: &[crate::core::database_manager::DatabaseInfo],
) -> anyhow::Result<()> {
    use crate::cli::output::*;
    use humansize::{format_size, BINARY};

    let base_name = db_ref.base_ref();

    // Find the database info
    let db_info = databases
        .iter()
        .find(|db| db.name == base_name)
        .ok_or_else(|| anyhow::anyhow!("Database '{}' not found in repository", base_name))?;

    // Parse source and dataset from the database name
    let parts: Vec<&str> = db_info.name.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid database name format: {}", db_info.name);
    }
    let source = parts[0];
    let dataset = parts[1];

    // Load the reduction profile manifest
    let storage = manager.get_storage();
    let profile_manifest = storage
        .get_database_reduction_by_profile(profile, source, dataset, Some(&db_info.version))?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Profile '{}' not found for database '{}'",
                profile,
                base_name
            )
        })?;

    // Build tree structure for profile info
    tree_item(false, "Database", Some(&db_info.name));
    tree_item(false, "Version", Some(&db_info.version));
    tree_item(false, "Profile", Some(profile));
    tree_item(
        false,
        "Created",
        Some(
            &profile_manifest
                .created_at
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        ),
    );

    // Show reduction parameters
    let params_items = vec![
        (
            "Reduction ratio",
            format!(
                "{:.1}%",
                profile_manifest.parameters.reduction_ratio * 100.0
            ),
        ),
        (
            "Target aligner",
            profile_manifest
                .parameters
                .target_aligner
                .as_ref()
                .map(|a| format!("{:?}", a))
                .unwrap_or_else(|| "Generic".to_string()),
        ),
        (
            "Min sequence length",
            profile_manifest.parameters.min_length.to_string(),
        ),
        (
            "Similarity threshold",
            format!(
                "{:.1}%",
                profile_manifest.parameters.similarity_threshold * 100.0
            ),
        ),
        (
            "Taxonomy-aware",
            if profile_manifest.parameters.taxonomy_aware {
                "Yes"
            } else {
                "No"
            }
            .to_string(),
        ),
    ];
    tree_section("Parameters", params_items, false);

    // Show storage information
    let storage_items = vec![
        (
            "Reference chunks",
            profile_manifest.reference_chunks.len().to_string(),
        ),
        (
            "Delta chunks",
            profile_manifest.delta_chunks.len().to_string(),
        ),
        (
            "Total references",
            profile_manifest
                .reference_chunks
                .iter()
                .map(|c| c.sequence_count)
                .sum::<usize>()
                .to_string(),
        ),
    ];
    tree_section("Storage", storage_items, false);

    // Show reduction statistics if available
    let stats = &profile_manifest.statistics;
    let original_count = stats.original_sequences;
    let reference_count = stats.reference_sequences;
    let delta_count = stats.child_sequences;
    let coverage = (reference_count as f64 + delta_count as f64) / original_count as f64 * 100.0;

    let stats_items = vec![
        ("Original sequences", original_count.to_string()),
        ("Reference sequences", reference_count.to_string()),
        ("Delta sequences", delta_count.to_string()),
        ("Coverage", format!("{:.1}%", coverage)),
        ("Original size", format_size(stats.original_size, BINARY)),
        ("Reduced size", format_size(stats.reduced_size, BINARY)),
        (
            "Compression ratio",
            format!(
                "{:.2}x",
                stats.original_size as f64 / stats.reduced_size as f64
            ),
        ),
        (
            "Size reduction",
            format!(
                "{:.1}%",
                (1.0 - stats.reduced_size as f64 / stats.original_size as f64) * 100.0
            ),
        ),
    ];
    tree_section("Statistics", stats_items, false);

    // Show benefits compared to original
    let benefits_items = vec![
        ("Memory usage", "Optimized for aligner indexing".to_string()),
        ("Query coverage", format!("{:.1}%", coverage)),
        (
            "Reconstruction",
            "Full sequences recoverable via deltas".to_string(),
        ),
        (
            "Verification",
            format!(
                "SHA256 hash: {}",
                profile_manifest
                    .reduction_id
                    .to_hex()
                    .chars()
                    .take(16)
                    .collect::<String>()
            ),
        ),
    ];
    tree_section("Benefits", benefits_items, true);

    Ok(())
}
