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

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Fasta,
    Json,
    Tsv,
}

pub fn run(args: ListSequencesArgs) -> anyhow::Result<()> {
    use talaria_sequoia::assembler::FastaAssembler;
    use crate::core::database_manager::DatabaseManager;
    use crate::utils::progress::create_spinner;
    use std::io::Write;

    // Show loading spinner while initializing
    let spinner = create_spinner("Loading database information...");

    // Initialize database manager
    use talaria_core::paths;
    let base_path = paths::talaria_databases_dir();

    let manager = DatabaseManager::new(Some(base_path.to_string_lossy().to_string()))?;

    spinner.finish_and_clear();

    // Parse database reference
    let parts: Vec<&str> = args.database.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!(
            "Invalid database reference. Use format: source/database (e.g., uniprot/swissprot)"
        );
    }

    let source = parts[0];
    let db_name = parts[1];

    // Find the database manifest in the manifests directory
    // Try both naming conventions
    let manifest_name = format!("{}-{}.json", source, db_name);
    let manifest_path = base_path.join("manifests").join(&manifest_name);

    let manifest_path = if manifest_path.exists() {
        manifest_path
    } else {
        // Try alternative naming
        let alt_manifest_path = base_path
            .join("manifests")
            .join(format!("{}_{}.json", source, db_name));
        if alt_manifest_path.exists() {
            alt_manifest_path
        } else {
            anyhow::bail!(
                "Database not found: {}\nLooked for: {}\nand: {}",
                args.database,
                manifest_path.display(),
                alt_manifest_path.display()
            );
        }
    };

    // Load manifest
    let manifest = talaria_sequoia::manifest::Manifest::load_file(&manifest_path)?;
    let manifest_data = manifest
        .get_data()
        .ok_or_else(|| anyhow::anyhow!("No manifest data found"))?;

    eprintln!(
        "\u{25cf} Loading sequences from {} (version {})",
        args.database, manifest_data.version
    );
    eprintln!("  Total chunks: {}", manifest_data.chunk_index.len());

    // Create assembler
    let _assembler = FastaAssembler::new(manager.get_storage());

    // Collect sequence references from chunks
    let mut all_sequences = Vec::new();
    let pb = crate::utils::progress::create_progress_bar(
        manifest_data.chunk_index.len() as u64,
        "Reading chunks",
    );

    let mut missing_chunks = 0;
    for chunk_info in &manifest_data.chunk_index {
        pb.inc(1);

        // Load chunk
        match manager.get_storage().get_chunk(&chunk_info.hash) {
            Ok(chunk_data) => {
                if let Ok(chunk) =
                    serde_json::from_slice::<talaria_sequoia::types::TaxonomyAwareChunk>(&chunk_data)
                {
                    // Apply filter if specified
                    let sequences: Vec<_> = if let Some(filter) = &args.filter {
                        chunk
                            .sequences
                            .into_iter()
                            .filter(|seq| seq.sequence_id.contains(filter))
                            .collect()
                    } else {
                        chunk.sequences
                    };

                    all_sequences.extend(sequences);

                    // Check limit
                    if all_sequences.len() >= args.limit {
                        all_sequences.truncate(args.limit);
                        break;
                    }
                }
            }
            Err(_) => {
                missing_chunks += 1;
                // Continue processing other chunks
            }
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
    }

    writer.flush()?;

    if args.output.is_some() {
        eprintln!("\u{2713} Output written to {:?}", args.output.unwrap());
    }

    Ok(())
}
