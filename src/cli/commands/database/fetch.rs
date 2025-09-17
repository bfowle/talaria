use clap::Args;
use std::path::PathBuf;
use crate::cli::output::*;

/// Magic bytes for Talaria manifest format
const TALARIA_MAGIC: &[u8] = b"TAL\x01";

#[derive(Args)]
pub struct FetchArgs {
    /// Comma-separated list of TaxIDs to fetch
    #[arg(long, value_name = "TAXIDS", conflicts_with = "taxid_list")]
    pub taxids: Option<String>,

    /// File containing list of TaxIDs (one per line)
    #[arg(long, value_name = "FILE", conflicts_with = "taxids")]
    pub taxid_list: Option<PathBuf>,

    /// Name for the custom database (e.g., "human_mouse")
    /// If not specified, uses "taxids_<ids>"
    #[arg(short, long)]
    pub name: Option<String>,

    /// Source category (default: "custom")
    #[arg(short, long, default_value = "custom")]
    pub source: String,

    /// Description of the database
    #[arg(long)]
    pub description: Option<String>,

    /// Version identifier (default: current date)
    #[arg(long)]
    pub version: Option<String>,

    /// Replace existing database if it exists
    #[arg(long)]
    pub replace: bool,

    /// UniProt API endpoint (for testing/mirrors)
    #[arg(long, default_value = "https://rest.uniprot.org", hide = true)]
    pub uniprot_api: String,

    /// Fetch reference proteomes instead of all sequences
    #[arg(long)]
    pub reference_proteomes: bool,

    /// Maximum sequences to fetch per TaxID (for testing)
    #[arg(long)]
    pub max_sequences: Option<usize>,
}

pub fn run(args: FetchArgs) -> anyhow::Result<()> {
    use crate::core::database_manager::DatabaseManager;
    use crate::bio::uniprot::{UniProtClient, parse_taxids, read_taxids_from_file};
    use crate::casg::chunker::TaxonomicChunker;
    use crate::casg::types::{ChunkingStrategy, ChunkMetadata, TemporalManifest, SHA256Hash, TaxonId, SpecialTaxon, ChunkStrategy};
    use crate::utils::progress::create_progress_bar;
    use crate::cli::formatter::{TaskList, TaskStatus, info_box};
    use crate::cli::output::*;
    use chrono::Utc;

    section_header("Database Fetch");

    // Initialize formatter
    crate::cli::formatter::init();

    // Validate that we have taxids from one source or another
    if args.taxids.is_none() && args.taxid_list.is_none() {
        anyhow::bail!("Must specify either --taxids or --taxid-list");
    }

    // Parse TaxIDs from input
    let taxids = if let Some(taxid_file) = &args.taxid_list {
        read_taxids_from_file(taxid_file)?
    } else if let Some(taxids_str) = &args.taxids {
        parse_taxids(taxids_str)?
    } else {
        vec![]
    };

    if taxids.is_empty() {
        anyhow::bail!("No valid TaxIDs provided");
    }

    // Determine database name
    let db_name = args.name.clone().unwrap_or_else(|| {
        let taxids_str = taxids.iter()
            .take(3)  // Limit to first 3 for readability
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join("_");

        if taxids.len() > 3 {
            format!("taxids_{}_and_{}_more", taxids_str, taxids.len() - 3)
        } else {
            format!("taxids_{}", taxids_str)
        }
    });

    // Initialize database manager
    use crate::core::paths;
    let base_path = paths::talaria_databases_dir();
    let manager = DatabaseManager::new(Some(base_path.to_string_lossy().to_string()))?;

    // Check if database already exists
    let db_path = base_path.join(&args.source).join(&db_name);
    if db_path.exists() && !args.replace {
        error(&format!("Database already exists: {}/{}", args.source, db_name));
        info("Use --replace to overwrite");
        anyhow::bail!("Database already exists");
    }

    // Create task list
    let mut task_list = TaskList::new();
    task_list.print_header(&format!("Fetch TaxID Database: {}/{}", args.source, db_name));

    info_box("Fetching from UniProt", &[
        &format!("TaxIDs: {} total", taxids.len()),
        "Automatic chunking with CASG",
        "Content-addressed storage",
        "Ready for reduction"
    ]);

    // Create directories
    std::fs::create_dir_all(&db_path)?;

    // Fetch sequences from UniProt
    let fetch_task = task_list.add_task("Fetch sequences from UniProt");
    task_list.update_task(fetch_task, TaskStatus::InProgress);

    // Pause task list updates during UniProt fetching to avoid display conflicts
    task_list.pause_updates();

    action(&format!("Fetching sequences for {} TaxIDs", taxids.len()));

    let client = UniProtClient::new(&args.uniprot_api)?;
    let mut total_sequences = 0;

    // Create progress bar for TaxID processing
    let progress = create_progress_bar(taxids.len() as u64, "Processing TaxIDs");

    let sequences = client.fetch_by_taxids_with_progress(&taxids, |_index, taxid, result| {
        if let Some(count) = result {
            if count > 0 {
                progress.set_message(format!("TaxID {}: Found {} sequences", taxid, count));
                total_sequences += count;
            } else {
                progress.set_message(format!("TaxID {}: No sequences found", taxid));
            }
            progress.inc(1);
        } else {
            progress.set_message(format!("Processing TaxID {}...", taxid));
        }
    })?;

    progress.finish_and_clear();

    success(&format!("Total sequences fetched: {}", format_number(total_sequences)));

    // Resume task list updates
    task_list.resume_updates();

    if sequences.is_empty() {
        task_list.update_task(fetch_task, TaskStatus::Failed);
        anyhow::bail!("No sequences found for the provided TaxIDs");
    }

    task_list.set_task_message(fetch_task, &format!("Fetched {} sequences", sequences.len()));
    task_list.update_task(fetch_task, TaskStatus::Complete);

    // Add taxonomy information to sequences
    let taxid_map = sequences.iter().enumerate()
        .filter_map(|(i, seq)| {
            // Try to find TaxID from the description
            if let Some(desc) = &seq.description {
                if desc.contains("OX=") {
                    // Parse TaxID from OX= tag
                    if let Some(ox_start) = desc.find("OX=") {
                        let ox_part = &desc[ox_start + 3..];
                        if let Some(end) = ox_part.find(' ').or_else(|| Some(ox_part.len())) {
                            if let Ok(taxid) = ox_part[..end].parse::<u32>() {
                                return Some((i, taxid));
                            }
                        }
                    }
                }
            }
            // If we can't parse from description, use the TaxID we fetched for
            // This is a simplification - in reality we'd need better mapping
            if taxids.len() == 1 {
                Some((i, taxids[0]))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    // Create chunker with taxonomy awareness
    let chunk_task = task_list.add_task("Chunk sequences");
    task_list.update_task(chunk_task, TaskStatus::InProgress);

    let strategy = ChunkingStrategy {
        target_chunk_size: 1024 * 1024, // 1MB target
        max_chunk_size: 10 * 1024 * 1024, // 10MB max
        min_sequences_per_chunk: 10, // At least 10 sequences
        taxonomic_coherence: 0.8, // High coherence for taxid-based fetch
        special_taxa: taxids.iter().map(|&t| SpecialTaxon {
            taxon_id: TaxonId(t),
            name: format!("TaxID_{}", t),
            strategy: ChunkStrategy::OwnChunks,
        }).collect(),
    };

    let mut chunker = TaxonomicChunker::new(strategy);

    // Add taxonomy mapping if available
    let mut taxonomy_map = std::collections::HashMap::new();
    for (seq_idx, taxid) in taxid_map {
        if seq_idx < sequences.len() {
            taxonomy_map.insert(sequences[seq_idx].id.clone(), TaxonId(taxid));
        }
    }
    chunker.load_taxonomy_mapping(taxonomy_map);

    let chunks = chunker.chunk_sequences(sequences)?;
    task_list.set_task_message(chunk_task, &format!("Created {} chunks", chunks.len()));
    task_list.update_task(chunk_task, TaskStatus::Complete);

    // Store chunks in CASG
    let store_task = task_list.add_task("Store chunks in CASG");
    task_list.update_task(store_task, TaskStatus::InProgress);

    let pb = create_progress_bar(chunks.len() as u64, "Storing chunks");
    let mut chunk_infos = Vec::new();

    for chunk in chunks {
        pb.inc(1);

        // Store chunk using the storage directly
        let hash = manager.get_storage().store_taxonomy_chunk(&chunk)?;

        // Create chunk metadata
        chunk_infos.push(ChunkMetadata {
            hash: hash.clone(),
            taxon_ids: chunk.taxon_ids.clone(),
            sequence_count: chunk.sequences.len(),
            size: chunk.size,
            compressed_size: chunk.compressed_size,
        });
    }
    pb.finish_with_message("All chunks stored");
    task_list.update_task(store_task, TaskStatus::Complete);

    // Create manifest
    let manifest_task = task_list.add_task("Create manifest");
    task_list.update_task(manifest_task, TaskStatus::InProgress);

    let version = args.version.unwrap_or_else(|| {
        Utc::now().format("%Y%m%d_%H%M%S").to_string()
    });

    let description = args.description.unwrap_or_else(|| {
        format!("Sequences fetched from UniProt for TaxIDs: {}",
                taxids.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", "))
    });

    // Compute Merkle root for the chunks
    let sequence_root = if !chunk_infos.is_empty() {
        // For now, use the hash of the first chunk as a simple root
        // In production, we'd properly compute the Merkle tree
        chunk_infos[0].hash.clone()
    } else {
        SHA256Hash::zero()
    };

    let temporal_manifest = TemporalManifest {
        version: version.clone(),
        created_at: Utc::now(),
        sequence_version: version.clone(),
        taxonomy_version: Utc::now().format("%Y-%m-%d").to_string(),
        taxonomy_root: SHA256Hash::zero(),
        sequence_root,
        taxonomy_manifest_hash: SHA256Hash::zero(),
        taxonomy_dump_version: "uniprot".to_string(),
        source_database: Some(format!("{}/{}", args.source, db_name)),
        chunk_index: chunk_infos.clone(),
        discrepancies: Vec::new(),
        etag: format!("{}-{}-{}", args.source, db_name, version),
        previous_version: None,
    };

    // Save manifest to the centralized manifests directory
    let manifests_dir = base_path.join("manifests");
    std::fs::create_dir_all(&manifests_dir)?;

    // Save Talaria format version (.tal) with magic header
    let manifest_tal_path = manifests_dir.join(format!("{}-{}.tal",
                                                        args.source.replace('/', "-"),
                                                        db_name));
    let mut tal_content = Vec::with_capacity(TALARIA_MAGIC.len() + 1024 * 512);
    tal_content.extend_from_slice(TALARIA_MAGIC);
    tal_content.extend_from_slice(&rmp_serde::to_vec(&temporal_manifest)?);
    std::fs::write(&manifest_tal_path, tal_content)?;

    // Also save JSON for debugging/compatibility
    let manifest_json_path = manifests_dir.join(format!("{}-{}.json",
                                                         args.source.replace('/', "-"),
                                                         db_name));
    let json_content = serde_json::to_string_pretty(&temporal_manifest)?;
    std::fs::write(&manifest_json_path, json_content)?;

    task_list.update_task(manifest_task, TaskStatus::Complete);

    success(&format!("Successfully created database: {}/{}", args.source, db_name));

    // Build tree of database details
    let total_sequences = chunk_infos.iter().map(|c| c.sequence_count).sum::<usize>();
    let details = vec![
        ("Description", description.clone()),
        ("Version", version.clone()),
        ("TaxIDs", format!("{} (fetched {} sequences)", format_number(taxids.len()), format_number(total_sequences))),
        ("Chunks", format_number(chunk_infos.len())),
        ("Location", db_path.display().to_string()),
    ];
    tree_section("Database Details", details, false);

    // Commands section
    subsection_header("Next Steps");
    info(&format!("View with: talaria database list"));
    info(&format!("Info: talaria database info {}/{}", args.source, db_name));
    info(&format!("Reduce: talaria reduce {}/{}", args.source, db_name));

    Ok(())
}