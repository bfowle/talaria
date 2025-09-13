use clap::Args;

#[derive(Args)]
pub struct CleanArgs {
    /// Specific database to clean (e.g., "uniprot/swissprot")
    /// If not specified, cleans all databases
    #[arg(value_name = "DATABASE")]
    pub database: Option<String>,
    
    /// Number of versions to keep (overrides config)
    #[arg(long)]
    pub keep: Option<usize>,
    
    /// Remove all versions except current
    #[arg(long)]
    pub all: bool,
    
    /// Dry run - show what would be deleted without actually deleting
    #[arg(long)]
    pub dry_run: bool,
    
    /// Force deletion without confirmation
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: CleanArgs) -> anyhow::Result<()> {
    use crate::core::database_manager::DatabaseManager;
    use crate::core::config::load_config;
    use dialoguer::Confirm;
    use humansize::{format_size, BINARY};
    
    // Load config to get database settings
    let config = load_config("talaria.toml").unwrap_or_default();
    
    // Determine retention count
    let retention_count = if args.all {
        0
    } else if let Some(keep) = args.keep {
        keep
    } else {
        config.database.retention_count
    };
    
    // Initialize database manager
    let db_manager = DatabaseManager::new(config.database.database_dir)?
        .with_retention(retention_count);
    
    // Get databases to clean
    let databases_to_clean = if let Some(database_ref) = args.database {
        // Clean specific database
        let reference = db_manager.parse_reference(&database_ref)?;
        vec![(reference.source, reference.dataset)]
    } else {
        // Clean all databases
        let all_dbs = db_manager.list_all_databases()?;
        all_dbs.into_iter()
            .map(|db| (db.source, db.dataset))
            .collect()
    };
    
    if databases_to_clean.is_empty() {
        println!("No databases found to clean.");
        return Ok(());
    }
    
    println!("Analyzing {} database(s) for cleanup...\n", databases_to_clean.len());
    
    let mut total_space_to_free = 0u64;
    let mut versions_to_remove = Vec::new();
    
    // Analyze what would be cleaned
    for (source, dataset) in &databases_to_clean {
        let versions = db_manager.list_versions(source, dataset)?;
        
        if versions.len() <= 1 {
            continue; // Nothing to clean
        }
        
        let current_version = versions.iter()
            .find(|v| v.is_current)
            .or_else(|| versions.first());
        
        let mut removable = Vec::new();
        let mut keep_count = 0;
        
        for version in &versions {
            // Always keep current version
            if version.is_current || current_version.map(|cv| cv.version == version.version).unwrap_or(false) {
                continue;
            }
            
            if args.all || keep_count >= retention_count {
                removable.push(version.clone());
                total_space_to_free += version.size;
            } else {
                keep_count += 1;
            }
        }
        
        if !removable.is_empty() {
            println!("Database: {}/{}", source, dataset);
            println!("  Total versions: {}", versions.len());
            println!("  Versions to remove: {}", removable.len());
            
            for version in &removable {
                println!("    - {} ({}, {})", 
                         version.version,
                         format_size(version.size, BINARY),
                         version.modified.format("%Y-%m-%d"));
            }
            
            versions_to_remove.push((source.clone(), dataset.clone(), removable));
            println!();
        }
    }
    
    if versions_to_remove.is_empty() {
        println!("No old versions to clean up!");
        return Ok(());
    }
    
    println!("Summary:");
    println!("  Total versions to remove: {}", 
             versions_to_remove.iter().map(|(_, _, v)| v.len()).sum::<usize>());
    println!("  Total space to free: {}", format_size(total_space_to_free, BINARY));
    println!();
    
    if args.dry_run {
        println!("Dry run mode - no files were deleted.");
        println!("Run without --dry-run to actually clean up.");
        return Ok(());
    }
    
    // Confirm deletion
    if !args.force {
        let confirm = Confirm::new()
            .with_prompt("Do you want to proceed with cleanup?")
            .default(false)
            .interact()?;
        
        if !confirm {
            println!("Cleanup cancelled.");
            return Ok(());
        }
    }
    
    // Perform cleanup
    println!("\nCleaning up old versions...");
    
    let mut total_removed = 0;
    let mut total_freed = 0u64;
    
    for (source, dataset, versions) in versions_to_remove {
        for version in versions {
            print!("  Removing {}/{}/{}... ", source, dataset, version.version);
            
            match std::fs::remove_dir_all(&version.path) {
                Ok(_) => {
                    println!("✅");
                    total_removed += 1;
                    total_freed += version.size;
                }
                Err(e) => {
                    println!("❌ Error: {}", e);
                }
            }
        }
    }
    
    println!("\nCleanup complete!");
    println!("  Removed {} version(s)", total_removed);
    println!("  Freed {} of disk space", format_size(total_freed, BINARY));
    
    Ok(())
}