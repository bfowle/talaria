use clap::Args;

#[derive(Args)]
pub struct UpdateArgs {
    /// Specific database to update (e.g., "uniprot/swissprot")
    /// If not specified, checks all databases
    #[arg(value_name = "DATABASE")]
    pub database: Option<String>,
    
    /// Actually download updates (default is check-only)
    #[arg(long)]
    pub download: bool,
    
    /// Force update even if current version is recent
    #[arg(long)]
    pub force: bool,
    
    /// Skip checksum verification
    #[arg(long)]
    pub skip_verify: bool,
    
    /// Days threshold for considering a database outdated
    #[arg(long, default_value = "30")]
    pub outdated_days: u64,
}

pub fn run(args: UpdateArgs) -> anyhow::Result<()> {
    use crate::core::database_manager::DatabaseManager;
    use crate::core::config::load_config;
    use chrono::{Duration, Local};
    
    // Load config to get database settings
    let config = load_config("talaria.toml").unwrap_or_default();
    
    // Initialize database manager
    let db_manager = DatabaseManager::new(config.database.database_dir.clone())?
        .with_retention(config.database.retention_count);
    
    let outdated_threshold = Duration::days(args.outdated_days as i64);
    let now = Local::now();
    
    // Get databases to check
    let databases_to_check = if let Some(database_ref) = args.database {
        // Check specific database
        let reference = db_manager.parse_reference(&database_ref)?;
        vec![(reference.source, reference.dataset)]
    } else {
        // Check all databases
        let all_dbs = db_manager.list_all_databases()?;
        all_dbs.into_iter()
            .map(|db| (db.source, db.dataset))
            .collect()
    };
    
    if databases_to_check.is_empty() {
        println!("No databases found to update.");
        println!("Use 'talaria database download' to download databases first.");
        return Ok(());
    }
    
    println!("Checking {} database(s) for updates...\n", databases_to_check.len());
    
    let mut updates_available = Vec::new();
    
    for (source, dataset) in &databases_to_check {
        let versions = db_manager.list_versions(source, dataset)?;
        
        if let Some(current) = versions.iter().find(|v| v.is_current).or_else(|| versions.first()) {
            let age = now.signed_duration_since(current.modified);
            let is_outdated = age > outdated_threshold;
            
            println!("Database: {}/{}", source, dataset);
            println!("  Current version: {} ({})", 
                     current.version, 
                     current.modified.format("%Y-%m-%d"));
            println!("  Age: {} days", age.num_days());
            
            if is_outdated || args.force {
                println!("  Status: {} Update available", 
                         if is_outdated { "⚠️ " } else { "ℹ️ " });
                updates_available.push((source.clone(), dataset.clone()));
                
                // Check remote for actual updates (simplified version check)
                if args.download {
                    println!("  Action: Downloading update...");
                    if let Err(e) = download_update(&db_manager, source, dataset, args.skip_verify) {
                        println!("  Error: Failed to download update: {}", e);
                    } else {
                        println!("  ✅ Successfully updated!");
                    }
                }
            } else {
                println!("  Status: ✅ Up to date");
            }
            println!();
        }
    }
    
    if !updates_available.is_empty() && !args.download {
        println!("\n{} update(s) available!", updates_available.len());
        println!("Run with --download to update databases:");
        for (source, dataset) in &updates_available {
            println!("  talaria database update {}/{} --download", source, dataset);
        }
    } else if updates_available.is_empty() {
        println!("All databases are up to date!");
    }
    
    Ok(())
}

fn download_update(
    db_manager: &crate::core::database_manager::DatabaseManager,
    source: &str,
    dataset: &str,
    skip_verify: bool,
) -> anyhow::Result<()> {
    use crate::download::{download_database_with_full_options, DatabaseSource, UniProtDatabase, NCBIDatabase, DownloadProgress};
    use crate::core::database_manager::DatabaseMetadata;
    use chrono::Utc;
    
    // Map source/dataset to DatabaseSource
    let database_source = match (source, dataset) {
        ("uniprot", "swissprot") => DatabaseSource::UniProt(UniProtDatabase::SwissProt),
        ("uniprot", "trembl") => DatabaseSource::UniProt(UniProtDatabase::TrEMBL),
        ("uniprot", "uniref50") => DatabaseSource::UniProt(UniProtDatabase::UniRef50),
        ("uniprot", "uniref90") => DatabaseSource::UniProt(UniProtDatabase::UniRef90),
        ("uniprot", "uniref100") => DatabaseSource::UniProt(UniProtDatabase::UniRef100),
        ("ncbi", "nr") => DatabaseSource::NCBI(NCBIDatabase::NR),
        ("ncbi", "nt") => DatabaseSource::NCBI(NCBIDatabase::NT),
        ("ncbi", "refseqprotein") => DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein),
        ("ncbi", "refseqgenomic") => DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic),
        ("ncbi", "taxonomy") => DatabaseSource::NCBI(NCBIDatabase::Taxonomy),
        _ => anyhow::bail!("Unknown database: {}/{}", source, dataset),
    };
    
    // Prepare download directory
    let (version_dir, version_date) = db_manager.prepare_download(source, dataset)?;
    let filename = format!("{}.fasta", dataset);
    let output_file = version_dir.join(&filename);
    
    // Download the database
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let mut progress = DownloadProgress::new();
        download_database_with_full_options(
            database_source.clone(),
            &output_file,
            &mut progress,
            skip_verify,
            false, // don't resume for updates
        ).await
    })?;
    
    // Save metadata
    let metadata = DatabaseMetadata {
        source: source.to_string(),
        dataset: dataset.to_string(),
        version: version_date.clone(),
        download_date: Utc::now(),
        file_size: std::fs::metadata(&output_file)?.len(),
        checksum: None,
        url: None,
    };
    
    let metadata_path = version_dir.join("metadata.json");
    metadata.save(&metadata_path)?;
    
    // Update current symlink
    db_manager.update_current_link(source, dataset, &version_date)?;
    
    // Clean old versions
    let removed = db_manager.clean_old_versions(source, dataset)?;
    if !removed.is_empty() {
        println!("  Cleaned up old versions: {:?}", removed);
    }
    
    Ok(())
}