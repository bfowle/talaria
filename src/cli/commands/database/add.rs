use clap::Args;
use std::path::PathBuf;
use crate::cli::output::*;

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
}

pub fn run(args: AddArgs) -> anyhow::Result<()> {
    use crate::core::database_manager::DatabaseManager;
    use crate::casg::chunker::TaxonomicChunker;
    use crate::casg::types::{ChunkingStrategy, ChunkMetadata, TemporalManifest, SHA256Hash};
    use crate::bio::fasta::parse_fasta;
    use crate::utils::progress::create_progress_bar;
    use crate::cli::output::*;
    use chrono::Utc;

    // Validate input file
    if !args.input.exists() {
        anyhow::bail!("Input file does not exist: {:?}", args.input);
    }

    // Determine database name
    let db_name = args.name.clone().or_else(|| {
        args.input.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    }).ok_or_else(|| anyhow::anyhow!("Could not determine database name"))?;

    let dataset = args.dataset.clone().unwrap_or_else(|| db_name.clone());

    // Initialize database manager
    use crate::core::paths;
    let base_path = paths::talaria_databases_dir();

    let manager = DatabaseManager::new(Some(base_path.to_string_lossy().to_string()))?;

    // Check if database already exists
    let db_path = base_path.join(&args.source).join(&dataset);
    if db_path.exists() && !args.replace {
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
    tree_item(false, "Sequences read", Some(&format_number(sequence_count)));

    // Create chunker
    let strategy = ChunkingStrategy {
        target_chunk_size: 5 * 1024 * 1024, // 5MB target
        max_chunk_size: 10 * 1024 * 1024, // 10MB max
        min_sequences_per_chunk: 10, // At least 10 sequences
        taxonomic_coherence: 0.0, // No taxonomy optimization for custom DBs
        special_taxa: Vec::new(),
    };

    let chunker = TaxonomicChunker::new(strategy);

    // Chunk the sequences
    action("Chunking sequences...");
    let chunks = chunker.chunk_sequences(sequences)?;
    tree_item(false, "Chunks created", Some(&format_number(chunks.len())));

    // Store chunks in CASG
    let pb = create_progress_bar(chunks.len() as u64, "Storing chunks");
    let mut chunk_infos = Vec::new();

    for chunk in chunks {
        pb.inc(1);

        // Store chunk
        let hash = manager.get_storage().store_chunk(
            &serde_json::to_vec(&chunk)?,
            true // compress
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
    let version = args.version.unwrap_or_else(|| {
        Utc::now().format("%Y%m%d").to_string()
    });

    let _description = args.description.unwrap_or_else(|| {
        format!("Custom database imported from {:?}", args.input.file_name().unwrap_or_default())
    });

    let temporal_manifest = TemporalManifest {
        version: version.clone(),
        created_at: Utc::now(),
        sequence_version: version.clone(),
        taxonomy_version: "none".to_string(),
        taxonomy_root: SHA256Hash::zero(),
        sequence_root: SHA256Hash::zero(),
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

    success(&format!("Successfully added custom database: {}/{}", args.source, dataset));

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