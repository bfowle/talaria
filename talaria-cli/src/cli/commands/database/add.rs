#![allow(dead_code)]

use clap::Args;
use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
// use crate::cli::output::*;  // TODO: Remove if not needed

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
}

pub fn run(args: AddArgs) -> anyhow::Result<()> {
    use talaria_bio::fasta::parse_fasta;
    use talaria_sequoia::chunker::TaxonomicChunker;
    use talaria_sequoia::merkle::MerkleDAG;
    use talaria_sequoia::types::{
        BiTemporalCoordinate, ChunkMetadata, ChunkingStrategy, SHA256Hash, SerializedMerkleTree,
        TemporalManifest,
    };
    use crate::cli::output::*;
    use crate::core::database_manager::DatabaseManager;
    use crate::utils::progress::create_progress_bar;
    use chrono::Utc;

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

    // Initialize database manager
    use talaria_core::paths;
    let base_path = paths::talaria_databases_dir();

    let manager = DatabaseManager::new(Some(base_path.to_string_lossy().to_string()))?;

    // Generate version timestamp (UTC for consistency)
    let version = args
        .version
        .clone()
        .unwrap_or_else(talaria_core::paths::generate_utc_timestamp);

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

    // Create directories
    std::fs::create_dir_all(&db_path)?;

    // Read FASTA file
    action(&format!("Reading FASTA file: {:?}", args.input));
    let sequences = parse_fasta(&args.input)?;
    let sequence_count = sequences.len();
    tree_item(
        false,
        "Sequences read",
        Some(&format_number(sequence_count)),
    );

    // Create chunker with default strategy (same as database download)
    // This ensures consistency across all database operations
    let strategy = ChunkingStrategy::default();
    let mut chunker = TaxonomicChunker::new(strategy);

    // Check taxonomy prerequisites
    use crate::core::taxonomy_prerequisites::TaxonomyPrerequisites;
    let prereqs = TaxonomyPrerequisites::new();
    prereqs.display_status();

    // Ensure prerequisites if requested
    if args.download_prerequisites {
        prereqs.ensure_prerequisites(true)?;
    }

    // Try to load accession-to-taxid mappings for better taxonomy resolution
    action("Loading taxonomy mappings...");
    let taxonomy_map = load_taxonomy_mappings()?;
    if !taxonomy_map.is_empty() {
        chunker.load_taxonomy_mapping(taxonomy_map.clone());
        tree_item(
            false,
            "Accession mappings loaded",
            Some(&format_number(taxonomy_map.len())),
        );
    } else {
        tree_item(
            false,
            "No accession mappings found",
            Some("Will use TaxID from headers"),
        );
    }

    // Chunk the sequences
    action("Chunking sequences...");
    let chunks = chunker.chunk_sequences_into_taxonomy_aware(sequences)?;
    tree_item(false, "Chunks created", Some(&format_number(chunks.len())));

    // Store chunks in SEQUOIA
    let pb = create_progress_bar(chunks.len() as u64, "Storing chunks");
    let mut chunk_infos = Vec::new();

    for chunk in chunks {
        pb.inc(1);

        // Store chunk
        let hash = manager.get_storage().store_chunk(
            &serde_json::to_vec(&chunk)?,
            true, // compress
        )?;

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

    // Create manifest
    let _description = args.description.unwrap_or_else(|| {
        format!(
            "Custom database imported from {:?}",
            args.input.file_name().unwrap_or_default()
        )
    });

    // Build Merkle tree from chunks
    let chunk_merkle_tree = if !chunk_infos.is_empty() {
        let dag = MerkleDAG::build_from_items(chunk_infos.clone())?;
        let root_hash = dag
            .root_hash()
            .ok_or_else(|| anyhow::anyhow!("Failed to get Merkle root"))?;

        // Serialize the Merkle tree
        let serialized = rmp_serde::to_vec(&dag)?;
        Some(SerializedMerkleTree {
            root_hash,
            node_count: chunk_infos.len(),
            serialized_nodes: serialized,
        })
    } else {
        None
    };

    // Create bi-temporal coordinate
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

    // Save manifest in Talaria format (.tal) with magic header
    let manifest_path_tal = db_path.join("manifest.tal");
    let mut tal_content = Vec::with_capacity(TALARIA_MAGIC.len() + 1024 * 512);
    tal_content.extend_from_slice(TALARIA_MAGIC);
    tal_content.extend_from_slice(&rmp_serde::to_vec(&temporal_manifest)?);
    std::fs::write(&manifest_path_tal, tal_content)?;

    // Also save JSON for debugging/compatibility
    let manifest_path = db_path.join("manifest.json");
    let json_content = serde_json::to_string_pretty(&temporal_manifest)?;
    std::fs::write(&manifest_path, json_content)?;

    // Create current symlink
    let current_link = db_base.join("current");
    if current_link.exists() {
        std::fs::remove_file(&current_link)?;
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&version, &current_link)?;
    }
    #[cfg(windows)]
    {
        std::fs::write(&current_link, &version)?;
    }

    success(&format!(
        "Successfully added custom database: {}/{}",
        args.source, dataset
    ));

    // Build tree of database details
    let details = vec![
        ("Version", version.clone()),
        ("Sequences", format_number(sequence_count)),
        ("Chunks", format_number(chunk_infos.len())),
        ("Location", db_path.display().to_string()),
    ];
    tree_section("Database Details", details, false);

    // Note about file handling
    if !args.copy {
        info(&format!("Original file kept at: {}", args.input.display()));
        info("Use --copy=false to move the file instead");
    }

    Ok(())
}

/// Load taxonomy mappings from available accession2taxid files
fn load_taxonomy_mappings() -> anyhow::Result<HashMap<String, talaria_sequoia::types::TaxonId>> {
    use talaria_sequoia::types::TaxonId;
    use talaria_core::paths;

    let mut mapping = HashMap::new();
    let mappings_dir = paths::talaria_taxonomy_current_dir().join("mappings");

    // Try NCBI prot.accession2taxid first (most common for protein sequences)
    let ncbi_file = mappings_dir.join("prot.accession2taxid.gz");
    if ncbi_file.exists() {
        use crate::cli::output::info;
        info(&format!(
            "Loading NCBI accession2taxid from: {}",
            ncbi_file.display()
        ));
        mapping.extend(load_ncbi_accession2taxid(&ncbi_file)?);
        if !mapping.is_empty() {
            return Ok(mapping);
        }
    }

    // Try UniProt idmapping if NCBI not found
    let uniprot_file = mappings_dir.join("uniprot_idmapping.dat.gz");
    if uniprot_file.exists() {
        use crate::cli::output::info;
        info(&format!(
            "Loading UniProt idmapping from: {}",
            uniprot_file.display()
        ));
        mapping.extend(load_uniprot_idmapping(&uniprot_file)?);
    }

    // Check for a simple accession2taxid file in taxonomy root
    let simple_file = paths::talaria_taxonomy_current_dir().join("accession2taxid.txt");
    if simple_file.exists() {
        use crate::cli::output::info;
        info(&format!(
            "Loading custom accession2taxid from: {}",
            simple_file.display()
        ));
        let file = File::open(&simple_file)?;
        let reader = BufReader::new(file);

        for line in reader.lines().skip(1) {
            // Skip header
            let line = line?;
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let accession = parts[0].to_string();
                if let Ok(taxid) = parts[2].parse::<u32>() {
                    mapping.insert(accession, TaxonId(taxid));
                }
            }
        }
    }

    Ok(mapping)
}

/// Load NCBI prot.accession2taxid format
fn load_ncbi_accession2taxid(
    path: &PathBuf,
) -> anyhow::Result<HashMap<String, talaria_sequoia::types::TaxonId>> {
    use talaria_sequoia::types::TaxonId;

    let mut mapping = HashMap::new();
    let file = File::open(path)?;
    let decoder = GzDecoder::new(file);
    let reader = BufReader::new(decoder);

    // Format: accession<tab>accession.version<tab>taxid<tab>gi
    for (idx, line) in reader.lines().enumerate() {
        if idx == 0 {
            continue;
        } // Skip header
        if idx > 1000000 {
            break;
        } // Limit for performance

        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let accession = parts[0].to_string();
            if let Ok(taxid) = parts[2].parse::<u32>() {
                mapping.insert(accession, TaxonId(taxid));
            }
        }
    }

    Ok(mapping)
}

/// Load UniProt idmapping format
fn load_uniprot_idmapping(
    path: &PathBuf,
) -> anyhow::Result<HashMap<String, talaria_sequoia::types::TaxonId>> {
    use talaria_sequoia::types::TaxonId;

    let mut mapping = HashMap::new();
    let file = File::open(path)?;
    let decoder = GzDecoder::new(file);
    let reader = BufReader::new(decoder);

    // Format: UniProtKB-AC<tab>ID-type<tab>ID-value
    // We're looking for NCBI-taxon entries
    for (idx, line) in reader.lines().enumerate() {
        if idx > 1000000 {
            break;
        } // Limit for performance

        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 && parts[1] == "NCBI-taxon" {
            let accession = parts[0].to_string();
            if let Ok(taxid) = parts[2].parse::<u32>() {
                mapping.insert(accession, TaxonId(taxid));
            }
        }
    }

    Ok(mapping)
}
