#![allow(dead_code)]

use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct ListSequencesArgs {
    /// Database reference (e.g., "uniprot/swissprot")
    pub database: String,

    /// Show only sequence IDs (no descriptions)
    #[arg(long)]
    pub ids_only: bool,

    /// Show full sequence data
    #[arg(long)]
    pub full: bool,

    /// Filter by sequence ID pattern (supports wildcards)
    #[arg(long)]
    pub filter: Option<String>,

    /// Limit number of sequences shown
    #[arg(long, default_value = "100")]
    pub limit: usize,

    /// Output file (if not specified, prints to stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Output format
    #[arg(long, value_enum, default_value = "text")]
    pub format: OutputFormat,
}

// Use OutputFormat from talaria-core
use talaria_core::OutputFormat;

pub fn run(args: ListSequencesArgs) -> anyhow::Result<()> {
    use crate::cli::progress::create_spinner;
    use std::io::Write;
    use talaria_sequoia::database::DatabaseManager;
    use talaria_sequoia::ReductionManifest;

    // Show loading spinner while initializing
    let spinner = create_spinner("Loading database information...");

    // Initialize database manager
    let manager = DatabaseManager::new(None)?;

    spinner.finish_and_clear();

    // Parse database reference to check for profile
    let db_ref = talaria_utils::database::database_ref::parse_database_reference(&args.database)?;

    // Load appropriate manifest based on whether profile is specified
    let (chunk_metadata, total_sequences, db_display_name) = if let Some(profile) = &db_ref.profile
    {
        // Load reduction manifest for profile
        let versions_dir = talaria_core::system::paths::talaria_databases_dir().join("versions");
        let version = db_ref.version.as_deref().unwrap_or("current");
        let profile_path = versions_dir
            .join(&db_ref.source)
            .join(&db_ref.dataset)
            .join(version)
            .join("profiles")
            .join(format!("{}.tal", profile));

        if !profile_path.exists() {
            anyhow::bail!(
                "Profile '{}' not found for {}/{}",
                profile,
                db_ref.source,
                db_ref.dataset
            );
        }

        // Read and parse reduction manifest
        let mut content = std::fs::read(&profile_path)?;
        if content.starts_with(b"TAL") && content.len() > 4 {
            content = content[4..].to_vec();
        }

        let reduction_manifest: ReductionManifest = rmp_serde::from_slice(&content)?;

        // Convert reference chunks to chunk metadata format
        let mut chunk_metadata = Vec::new();
        for ref_chunk in &reduction_manifest.reference_chunks {
            chunk_metadata.push(talaria_sequoia::ManifestMetadata {
                hash: ref_chunk.chunk_hash.clone(),
                taxon_ids: ref_chunk.taxon_ids.clone(),
                sequence_count: ref_chunk.sequence_count,
                size: ref_chunk.size,
                compressed_size: ref_chunk.compressed_size,
            });
        }

        let total = reduction_manifest.statistics.reference_sequences;
        let display = format!("{}/{}:{}", db_ref.source, db_ref.dataset, profile);
        (chunk_metadata, total, display)
    } else {
        // Load regular database manifest
        let manifest = manager.get_manifest(&args.database)?;
        let total = manifest.chunk_index.iter().map(|c| c.sequence_count).sum();
        let display = format!("{}/{}", db_ref.source, db_ref.dataset);
        (manifest.chunk_index, total, display)
    };

    eprintln!("\u{25cf} Loading sequences from {}", db_display_name);
    eprintln!("  Total chunks: {}", chunk_metadata.len());
    eprintln!("  Total sequences: {}", total_sequences);

    // Collect sequence information from chunks
    let mut all_sequences = Vec::new();
    let pb =
        crate::cli::progress::create_progress_bar(chunk_metadata.len() as u64, "Reading chunks");

    let mut missing_chunks = 0;
    let mut sequences_loaded = 0;

    for chunk_info in &chunk_metadata {
        pb.inc(1);

        // Load using manifest-based approach (SEQUOIA way)
        match manager.load_manifest(&chunk_info.hash) {
            Ok(manifest) => {
                // Load sequences from canonical storage
                let remaining = args.limit.saturating_sub(sequences_loaded);
                match manager.load_sequences_from_manifest(
                    &manifest,
                    args.filter.as_deref(),
                    remaining,
                ) {
                    Ok(sequences) => {
                        for (seq_id, _fasta_data) in sequences {
                            all_sequences.push(talaria_sequoia::SequenceRef {
                                sequence_id: seq_id.clone(),
                                chunk_hash: chunk_info.hash.clone(),
                                offset: 0, // Not used in new format
                                length: 0, // Not used in new format
                            });
                            sequences_loaded += 1;

                            if sequences_loaded >= args.limit {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to load sequences from chunk {}: {}",
                            chunk_info.hash, e
                        );
                        missing_chunks += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to load manifest for chunk {}: {}",
                    chunk_info.hash, e
                );
                missing_chunks += 1;
            }
        }

        if sequences_loaded >= args.limit {
            all_sequences.truncate(args.limit);
            break;
        }
    }

    if missing_chunks > 0 {
        eprintln!(
            "\n⚠ Warning: {} chunks referenced in manifest are not present on disk",
            missing_chunks
        );
        eprintln!("  The database may need to be re-downloaded.");
        eprintln!("  Run: talaria database download {}", args.database);
    }

    pb.finish_with_message(format!("Found {} sequences", all_sequences.len()));

    // Format and output results
    let output: Box<dyn Write> = if let Some(path) = &args.output {
        Box::new(std::fs::File::create(path)?)
    } else {
        Box::new(std::io::stdout())
    };

    let mut writer = std::io::BufWriter::new(output);

    match args.format {
        OutputFormat::Text => {
            for seq_ref in &all_sequences {
                if args.ids_only {
                    writeln!(writer, "{}", seq_ref.sequence_id)?;
                } else if args.full {
                    writeln!(writer, "ID: {}", seq_ref.sequence_id)?;
                    writeln!(writer, "Chunk: {}", seq_ref.chunk_hash)?;
                    writeln!(
                        writer,
                        "Offset: {}, Length: {}",
                        seq_ref.offset, seq_ref.length
                    )?;
                    writeln!(writer, "---")?;
                } else {
                    writeln!(writer, "{}", seq_ref.sequence_id)?;
                }
            }
        }
        OutputFormat::Fasta => {
            eprintln!("\u{25cf} Note: FASTA output requires assembling full sequences");
            eprintln!("  This feature shows sequence headers only for now");
            for seq_ref in &all_sequences {
                writeln!(writer, ">{}", seq_ref.sequence_id)?;
                writeln!(writer, "SEQUENCE_DATA_NOT_ASSEMBLED")?;
            }
        }
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut writer, &all_sequences)?;
            writeln!(writer)?;
        }
        OutputFormat::Tsv => {
            writeln!(writer, "id\tchunk_hash\toffset\tlength")?;
            for seq_ref in &all_sequences {
                writeln!(
                    writer,
                    "{}\t{}\t{}\t{}",
                    seq_ref.sequence_id, seq_ref.chunk_hash, seq_ref.offset, seq_ref.length
                )?;
            }
        }
        OutputFormat::Yaml
        | OutputFormat::Csv
        | OutputFormat::Summary
        | OutputFormat::Detailed
        | OutputFormat::HashOnly => {
            // Default to text for unsupported formats
            for seq_ref in &all_sequences {
                writeln!(writer, "{}", seq_ref.sequence_id)?;
            }
        }
    }

    writer.flush()?;

    if args.output.is_some() {
        eprintln!("\u{2713} Output written to {:?}", args.output.unwrap());
    }

    Ok(())
}
