#![allow(dead_code)]

/// Add a custom database from a FASTA file
use clap::Args;
use std::collections::HashMap;
use std::path::PathBuf;

/// Magic bytes for Talaria manifest format
const TALARIA_MAGIC: &[u8] = b"TAL\x01";

#[derive(Args)]
pub struct AddArgs {
    /// Path to the FASTA file to add as a custom database
    #[arg(short, long, value_name = "FILE")]
    pub input: PathBuf,

    /// Name for the custom database (e.g., "team-proteins")
    /// If not specified, uses the filename without extension
    #[arg(short, long)]
    pub name: Option<String>,

    /// Source category (default: "custom")
    #[arg(short, long, default_value = "custom")]
    pub source: String,

    /// Dataset name within the source
    /// If not specified, uses --name or filename
    #[arg(short, long)]
    pub dataset: Option<String>,

    /// Description of the database
    #[arg(long)]
    pub description: Option<String>,

    /// Version identifier (default: current date)
    #[arg(long)]
    pub version: Option<String>,

    /// Replace existing database if it exists
    #[arg(long)]
    pub replace: bool,

    /// Copy file instead of moving (keeps original in place)
    #[arg(long)]
    pub copy: bool,

    /// Automatically download taxonomy prerequisites if missing
    #[arg(long)]
    pub download_prerequisites: bool,

    /// Show deduplication statistics
    #[arg(long)]
    pub show_dedup_stats: bool,
}

pub fn run(args: AddArgs) -> anyhow::Result<()> {
    use crate::cli::formatting::output::*;
    use crate::cli::progress::create_progress_bar;
    use chrono::Utc;
    use std::sync::Arc;
    use talaria_bio::parse_fasta;
    use talaria_sequoia::chunker::{ChunkingStrategy, TaxonomicChunker};
    use talaria_sequoia::database::DatabaseManager;
    use talaria_sequoia::MerkleDAG;
    use talaria_sequoia::{
        BiTemporalCoordinate, DatabaseSource, ManifestMetadata, NCBIDatabase, SHA256Hash,
        SHA256HashExt, SerializedMerkleTree, TemporalManifest, UniProtDatabase,
    };

    // Validate input file
    if !args.input.exists() {
        anyhow::bail!("Input file does not exist: {:?}", args.input);
    }

    // Determine database name
    let db_name = args
        .name
        .clone()
        .or_else(|| {
            args.input
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .ok_or_else(|| anyhow::anyhow!("Could not determine database name"))?;

    let dataset = args.dataset.clone().unwrap_or_else(|| db_name.clone());

    // Initialize paths
    use talaria_core::system::paths;
    let base_path = paths::talaria_databases_dir();

    // Create DatabaseManager first (it will initialize SequoiaStorage which creates a SequenceStorage)
    let manager = DatabaseManager::new(Some(base_path.to_string_lossy().to_string()))?;

    // Reuse the existing SequenceStorage from the DatabaseManager's repository
    // This avoids double-initialization of RocksDB
    let sequence_storage = Arc::clone(&manager.get_repository().storage.sequence_storage);

    // Get initial stats for deduplication tracking
    let initial_stats = sequence_storage.get_stats()?;

    // Generate version timestamp
    let version = args
        .version
        .clone()
        .unwrap_or_else(talaria_core::system::paths::generate_utc_timestamp);

    // Check if database already exists
    let db_base = base_path.join("versions").join(&args.source).join(&dataset);
    let db_path = db_base.join(&version);

    if db_base.exists() && !args.replace {
        anyhow::bail!(
            "Database already exists: {}/{}. Use --replace to overwrite.",
            args.source,
            dataset
        );
    }

    info(&format!("Adding database: {}/{}", args.source, dataset));
    println!();

    // Create directories
    std::fs::create_dir_all(&db_path)?;

    // Determine database source enum
    // Extract just the database name from dataset (handles "uniprot/uniref50" → "uniref50")
    let db_type = dataset.split('/').last().unwrap_or(&dataset);

    let database_source_enum = if args.source == "custom" {
        DatabaseSource::Custom(dataset.clone())
    } else if args.source == "uniprot" {
        // Parse the dataset name to determine which UniProt database
        match db_type.to_lowercase().as_str() {
            "swissprot" => DatabaseSource::UniProt(UniProtDatabase::SwissProt),
            "trembl" => DatabaseSource::UniProt(UniProtDatabase::TrEMBL),
            "uniref50" => DatabaseSource::UniProt(UniProtDatabase::UniRef50),
            "uniref90" => DatabaseSource::UniProt(UniProtDatabase::UniRef90),
            "uniref100" => DatabaseSource::UniProt(UniProtDatabase::UniRef100),
            _ => {
                eprintln!("Warning: Unknown UniProt database '{}', defaulting to SwissProt", db_type);
                DatabaseSource::UniProt(UniProtDatabase::SwissProt)
            }
        }
    } else if args.source == "ncbi" {
        DatabaseSource::NCBI(NCBIDatabase::NR)
    } else {
        DatabaseSource::Custom(format!("{}/{}", args.source, dataset))
    };

    // Check taxonomy prerequisites
    use talaria_sequoia::taxonomy::TaxonomyPrerequisites;
    let prereqs = TaxonomyPrerequisites::new();
    prereqs.display_status();

    if args.download_prerequisites {
        prereqs.ensure_prerequisites(true)?;
    }

    // Check file size to determine processing path
    let file_size = std::fs::metadata(&args.input)?.len();
    const STREAMING_THRESHOLD: u64 = 1_000_000_000; // 1GB

    if file_size > STREAMING_THRESHOLD {
        // LARGE FILE PATH: Use streaming mode to avoid OOM
        action(&format!(
            "Large file detected ({:.2} GB), using streaming mode...",
            file_size as f64 / 1_073_741_824.0
        ));
        println!();

        // Create progress callback
        let progress_callback = |msg: &str| {
            action(msg);
        };

        // Use manager.chunk_database() which automatically handles:
        // 1. Streaming processing (no OOM!)
        // 2. Automatic deduplication
        // 3. Manifest storage
        let mut manager_mut = manager;
        manager_mut.chunk_database(
            &args.input,
            &database_source_enum,
            Some(&progress_callback)
        )?;

        // Flush storage
        manager_mut.get_repository().storage.sequence_storage.flush()?;

        println!();
        success(&format!(
            "Large file processed successfully using streaming mode (avoided OOM)"
        ));
        println!();
        info("Note: Use 'database list' to see the stored database");

        return Ok(());
    }

    // SMALL FILE PATH: Use original in-memory path with detailed stats
    // Read FASTA file
    action(&format!("Reading FASTA file: {:?}", args.input));
    let sequences = parse_fasta(&args.input)?;
    let sequence_count = sequences.len();

    // Check for empty file
    if sequence_count == 0 {
        anyhow::bail!("Input file contains no sequences. Please provide a valid FASTA file.");
    }

    tree_item(
        false,
        "Sequences read",
        Some(&format_number(sequence_count)),
    );

    // Create chunker with sequence storage
    let strategy = ChunkingStrategy::default();
    let mut chunker =
        TaxonomicChunker::new(strategy, sequence_storage, database_source_enum.clone());

    // Process sequences with canonical storage
    action("Processing sequences with deduplication...");
    println!();

    // Create progress bar for sequence processing
    let pb = create_progress_bar(sequence_count as u64, "Processing sequences");
    let pb_clone = pb.clone();

    // Create progress callback to update the UI
    let progress_callback = Box::new(move |count: usize, msg: &str| {
        pb_clone.set_position(count as u64);
        pb_clone.set_message(msg.to_string());
    });

    // Process sequences with automatic deduplication and progress tracking
    let chunk_manifests = chunker.chunk_sequences_canonical_with_progress(
        sequences,
        Some(progress_callback)
    )?;

    pb.finish_and_clear();

    // IMPORTANT: Flush sequence storage after processing
    // This ensures the packed storage index is persisted to disk
    chunker.sequence_storage.flush()?;

    // Get final stats for deduplication report
    let final_stats = chunker.sequence_storage.get_stats()?;

    // Calculate deduplication results
    let initial_seq = initial_stats.total_sequences.unwrap_or(0);
    let final_seq = final_stats.total_sequences.unwrap_or(0);
    let new_sequences = final_seq.saturating_sub(initial_seq);
    let deduplicated = sequence_count.saturating_sub(new_sequences);
    let dedup_percentage = if sequence_count > 0 {
        (deduplicated as f32 / sequence_count as f32) * 100.0
    } else {
        0.0
    };

    // Report deduplication results
    if args.show_dedup_stats || deduplicated > 0 {
        println!();
        subsection_header("Deduplication Statistics");
        tree_item(
            false,
            "Total sequences in file",
            Some(&format_number(sequence_count)),
        );
        tree_item(
            false,
            "New unique sequences",
            Some(&format_number(new_sequences)),
        );
        tree_item(
            false,
            "Deduplicated (already existed)",
            Some(&format_number(deduplicated)),
        );
        tree_item(
            true,
            "Space saved",
            Some(&format!("{:.1}%", dedup_percentage)),
        );

        if deduplicated > 0 {
            println!();
            success(&format!(
                "✨ Saved storage by deduplicating {} sequences that already existed!",
                format_number(deduplicated)
            ));
        }
    }

    println!();
    action("Creating chunk manifests...");
    tree_item(
        false,
        "Manifests created",
        Some(&format_number(chunk_manifests.len())),
    );

    // Convert ChunkManifests to ManifestMetadata for compatibility
    let pb = create_progress_bar(chunk_manifests.len() as u64, "Storing manifests");
    let mut chunk_infos = Vec::new();

    for manifest in &chunk_manifests {
        pb.inc(1);

        // Store the manifest itself (very small, just references)
        let manifest_data = rmp_serde::to_vec(&manifest)?;
        let hash = manager.get_storage().store_chunk(&manifest_data, true)?;

        // Create metadata
        chunk_infos.push(ManifestMetadata {
            hash: hash.clone(),
            taxon_ids: manifest.taxon_ids.clone(),
            sequence_count: manifest.sequence_count,
            size: manifest.total_size,
            compressed_size: Some(manifest_data.len()),
        });
    }
    pb.finish_with_message("All manifests stored");

    // Build Merkle tree
    let chunk_merkle_tree = if !chunk_infos.is_empty() {
        let dag = MerkleDAG::build_from_items(chunk_infos.clone())?;
        let root_hash = dag
            .root_hash()
            .ok_or_else(|| anyhow::anyhow!("Failed to get Merkle root"))?;

        let serialized = rmp_serde::to_vec(&dag)?;
        Some(SerializedMerkleTree {
            root_hash,
            node_count: chunk_infos.len(),
            serialized_nodes: serialized,
        })
    } else {
        None
    };

    // Create temporal manifest
    let temporal_coordinate = Some(BiTemporalCoordinate {
        sequence_time: Utc::now(),
        taxonomy_time: Utc::now(),
    });

    let temporal_manifest = TemporalManifest {
        version: version.clone(),
        created_at: Utc::now(),
        sequence_version: version.clone(),
        taxonomy_version: "none".to_string(),
        temporal_coordinate,
        taxonomy_root: SHA256Hash::zero(),
        sequence_root: SHA256Hash::zero(),
        chunk_merkle_tree,
        taxonomy_manifest_hash: SHA256Hash::zero(),
        taxonomy_dump_version: "none".to_string(),
        source_database: Some(format!("{}/{}", args.source, dataset)),
        chunk_index: chunk_infos.clone(),
        discrepancies: Vec::new(),
        etag: format!("custom-{}-{}", dataset, version),
        previous_version: None,
    };

    // Save manifest to RocksDB using shared function (NO filesystem writes)
    let chunk_count = temporal_manifest.chunk_index.len();
    let sequence_count = temporal_manifest.chunk_index.iter().map(|c| c.sequence_count).sum();
    let total_size = temporal_manifest.chunk_index.iter().map(|c| c.size).sum();

    manager.save_manifest_to_repository(
        &args.source,
        &dataset,
        &version,
        &temporal_manifest,
        chunk_count,
        sequence_count,
        total_size,
    )?;

    // Flush RocksDB to ensure data is persisted
    manager.get_repository().storage.sequence_storage.get_rocksdb().flush()?;

    // Create symlink for "current" version (legacy filesystem)
    let current_link = db_base.join("current");
    if current_link.exists() {
        std::fs::remove_file(&current_link).ok();
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&db_path, &current_link)?;
    #[cfg(windows)]
    std::fs::write(&current_link, db_path.to_string_lossy().as_bytes())?;

    // Final summary
    println!();
    println!("{}", "═".repeat(60));
    success(&format!(
        "Database {}/{} added successfully!",
        args.source, dataset
    ));
    println!("{}", "═".repeat(60));

    println!();
    subsection_header("Summary");
    tree_item(false, "Version", Some(&version));
    tree_item(
        false,
        "Total sequences",
        Some(&format_number(sequence_count)),
    );
    tree_item(
        false,
        "Unique sequences stored",
        Some(&format_number(new_sequences)),
    );
    tree_item(
        false,
        "Chunk manifests",
        Some(&format_number(chunk_infos.len())),
    );
    tree_item(true, "Location", Some(&db_path.display().to_string()));

    // Show global repository stats
    let global_stats = chunker.sequence_storage.get_stats()?;
    println!();
    subsection_header("Global Repository Statistics");
    tree_item(
        false,
        "Total unique sequences",
        Some(&format_number(global_stats.total_sequences.unwrap_or(0))),
    );
    tree_item(
        false,
        "Total representations",
        Some(&format_number(
            global_stats.total_representations.unwrap_or(0),
        )),
    );
    tree_item(
        false,
        "Average representations per sequence",
        Some(&format!("{:.2}", global_stats.deduplication_ratio)),
    );
    tree_item(
        true,
        "Total storage used",
        Some(&format_bytes(global_stats.total_size as u64)),
    );

    println!();
    info("💡 Tip: Sequences are now stored canonically and deduplicated across ALL databases!");
    info(
        "    Any identical sequences in future imports will automatically reference existing data.",
    );

    Ok(())
}

// Helper function to load taxonomy mappings using unified TaxonomyProvider
fn load_taxonomy_mappings() -> anyhow::Result<HashMap<String, talaria_sequoia::TaxonId>> {
    use talaria_utils::taxonomy::{
        load_taxonomy_mappings_with_fallback, TaxonomyMappingSource,
    };

    // Prefer NCBI mappings for the add command, but fallback to UniProt if needed
    let mappings: HashMap<String, u32> =
        load_taxonomy_mappings_with_fallback(TaxonomyMappingSource::NCBI)?;

    // Convert to TaxonId type
    Ok(mappings
        .into_iter()
        .map(|(k, v)| (k, talaria_sequoia::TaxonId(v)))
        .collect())
}

use talaria_utils::display::format::format_bytes;
