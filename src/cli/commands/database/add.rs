use clap::Args;
use std::path::PathBuf;

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

    eprintln!("\u{25cf} Adding custom database: {}/{}", args.source, dataset);

    // Create directories
    std::fs::create_dir_all(&db_path)?;

    // Read FASTA file
    eprintln!("\u{25cf} Reading FASTA file: {:?}", args.input);
    let sequences = parse_fasta(&args.input)?;
    let sequence_count = sequences.len();
    eprintln!("  Read {} sequences", sequence_count);

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
    eprintln!("\u{25cf} Chunking sequences...");
    let chunks = chunker.chunk_sequences(sequences)?;
    eprintln!("  Created {} chunks", chunks.len());

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

    // Save manifest
    let manifest_path = db_path.join("manifest.json");
    let manifest_content = serde_json::to_string_pretty(&temporal_manifest)?;
    std::fs::write(&manifest_path, manifest_content)?;

    eprintln!("\u{2713} Successfully added custom database: {}/{}", args.source, dataset);
    eprintln!("  Version: {}", version);
    eprintln!("  Sequences: {}", sequence_count);
    eprintln!("  Chunks: {}", chunk_infos.len());
    eprintln!("  Location: {:?}", db_path);

    // Optionally remove original file if moving (not copying)
    if !args.copy {
        eprintln!("\u{25cf} Note: Original file kept at {:?}", args.input);
        eprintln!("  Use --copy=false to move the file instead");
    }

    Ok(())
}