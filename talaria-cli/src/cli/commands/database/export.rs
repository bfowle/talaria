#![allow(dead_code)]

use anyhow::{Context, Result};
/// Export command for converting SEQUOIA-stored databases to standard FASTA format
///
/// This bridges the gap between our efficient SEQUOIA storage and traditional
/// bioinformatics tools that expect FASTA files
use clap::Args;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use talaria_bio::taxonomy_formatter::{TaxonomyFormatter, StandardTaxonomyFormatter};
use talaria_sequoia::assembler::FastaAssembler;
use talaria_sequoia::manifest::Manifest;
use crate::cli::output::{info as print_info, success as print_success};
use crate::core::database_manager::DatabaseManager;
use talaria_core::paths;
use crate::utils::database_ref::{parse_database_reference, DatabaseReference};
use crate::utils::progress::create_spinner;

#[derive(Args)]
pub struct ExportArgs {
    /// Database reference with optional version and profile
    /// Format: source/dataset[@version][:profile]
    /// Examples:
    ///   uniprot/swissprot                    (current version, auto-detect profile)
    ///   uniprot/swissprot@2024_04            (specific version)
    ///   uniprot/swissprot:50-percent         (current version, 50% profile)
    ///   uniprot/swissprot@2024_04:50-percent (specific version and profile)
    pub database: String,

    /// Output file path (defaults to cache location)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Force re-export even if cached version exists
    #[arg(short, long)]
    pub force: bool,

    /// Export format
    #[arg(long, value_enum, default_value = "fasta")]
    pub format: ExportFormat,

    /// Compress output with gzip
    #[arg(short = 'z', long)]
    pub compress: bool,

    /// Don't use cache, always export fresh
    #[arg(long)]
    pub no_cache: bool,

    /// Return cached path without exporting (fails if not cached)
    #[arg(long)]
    pub cached_only: bool,

    /// Include taxonomy information in headers
    #[arg(long)]
    pub with_taxonomy: bool,

    /// Quiet mode - only output the file path
    #[arg(short, long)]
    pub quiet: bool,

    /// Stream output (memory-efficient for large databases)
    #[arg(long)]
    pub stream: bool,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum ExportFormat {
    Fasta,
    Fastq,
    Tsv,
    Json,
}

pub fn run(args: ExportArgs) -> Result<()> {
    let db_ref = parse_database_reference(&args.database)?;

    // Determine output path
    let output_path = determine_output_path(&args, &db_ref)?;

    // Check if cached version exists
    if !args.force && !args.no_cache && output_path.exists() {
        if args.cached_only || !args.quiet {
            print_success(&format!("Using cached export: {}", output_path.display()));
        }

        if args.quiet {
            println!("{}", output_path.display());
        }

        return Ok(());
    }

    if args.cached_only {
        anyhow::bail!(
            "No cached export found for {}. Remove --cached-only to generate.",
            args.database
        );
    }

    // Export the database
    let spinner = if !args.quiet {
        Some(create_spinner(&format!(
            "Exporting {} to FASTA...",
            db_ref
        )))
    } else {
        None
    };

    let stats = perform_export(&args, &db_ref, &output_path)?;

    if let Some(s) = spinner {
        s.finish_and_clear();
    }

    // Output results
    if args.quiet {
        println!("{}", output_path.display());
    } else {
        print_success(&format!(
            "Exported {} sequences ({:.2} MB) to {}",
            stats.sequence_count,
            stats.file_size as f64 / 1_048_576.0,
            output_path.display()
        ));

        if !args.no_cache && args.output.is_none() {
            print_info("Export cached for future use");
        }
    }

    Ok(())
}

fn determine_output_path(args: &ExportArgs, db_ref: &DatabaseReference) -> Result<PathBuf> {
    if let Some(ref output) = args.output {
        return Ok(output.clone());
    }

    // Use cache directory
    let cache_dir = paths::talaria_databases_dir()
        .join("exports")
        .join(&db_ref.source)
        .join(&db_ref.dataset)
        .join(db_ref.version_or_default())
        .join(db_ref.profile_or_default());

    std::fs::create_dir_all(&cache_dir).context("Failed to create cache directory")?;

    let filename = format!(
        "export.{}{}",
        match args.format {
            ExportFormat::Fasta => "fasta",
            ExportFormat::Fastq => "fastq",
            ExportFormat::Tsv => "tsv",
            ExportFormat::Json => "json",
        },
        if args.compress { ".gz" } else { "" }
    );

    Ok(cache_dir.join(filename))
}

struct ExportStats {
    sequence_count: usize,
    file_size: u64,
}

fn perform_export(
    args: &ExportArgs,
    db_ref: &DatabaseReference,
    output_path: &Path,
) -> Result<ExportStats> {
    // Initialize database manager
    let base_path = paths::talaria_databases_dir();
    let _manager = DatabaseManager::new(Some(base_path.to_string_lossy().to_string()))?;

    // Find the manifest for the requested database
    let manifest_path = find_manifest(&base_path, db_ref)?;

    if !args.quiet {
        print_info(&format!("Using manifest: {}", manifest_path.display()));
    }

    // Load the manifest
    let manifest = Manifest::load_file(&manifest_path)?;
    let manifest_data = manifest
        .get_data()
        .ok_or_else(|| anyhow::anyhow!("No manifest data found"))?;

    // Check if we need a profile-specific manifest
    let temporal_manifest_owned;
    let final_manifest_data = if let Some(profile) = &db_ref.profile {
        // Find and load the reduction profile
        let profile_path = find_profile_manifest(&base_path, db_ref, profile)?;

        if !args.quiet {
            print_info(&format!(
                "Using reduction profile: {}",
                profile_path.display()
            ));
        }

        // Load the reduction manifest (handles both .tal and .json)
        use talaria_sequoia::reduction::ReductionManifest;
        let reduction_manifest = if profile_path.extension().and_then(|s| s.to_str()) == Some("tal")
        {
            // Load .tal format
            let data = std::fs::read(&profile_path)?;
            if data.len() < 4 || &data[0..4] != b"TAL\x01" {
                anyhow::bail!("Invalid .tal file format");
            }
            rmp_serde::from_slice::<ReductionManifest>(&data[4..])?
        } else {
            // Load JSON format
            let data = std::fs::read(&profile_path)?;
            serde_json::from_slice::<ReductionManifest>(&data)?
        };

        // Convert reduction manifest to temporal manifest for assembly
        // The reduction manifest contains the chunks we need
        temporal_manifest_owned = talaria_sequoia::types::TemporalManifest {
            version: manifest_data.version.clone(),
            created_at: reduction_manifest.created_at,
            sequence_version: manifest_data.sequence_version.clone(),
            taxonomy_version: manifest_data.taxonomy_version.clone(),
            temporal_coordinate: manifest_data.temporal_coordinate.clone(),
            taxonomy_root: manifest_data.taxonomy_root.clone(),
            sequence_root: manifest_data.sequence_root.clone(),
            chunk_merkle_tree: manifest_data.chunk_merkle_tree.clone(),
            taxonomy_manifest_hash: manifest_data.taxonomy_manifest_hash.clone(),
            taxonomy_dump_version: manifest_data.taxonomy_dump_version.clone(),
            source_database: manifest_data.source_database.clone(),
            chunk_index: {
                let mut chunks = Vec::new();
                // Add reference chunks
                for chunk in &reduction_manifest.reference_chunks {
                    chunks.push(talaria_sequoia::types::ChunkMetadata {
                        hash: chunk.chunk_hash.clone(),
                        sequence_count: chunk.sequence_count,
                        size: chunk.size,
                        compressed_size: chunk.compressed_size,
                        taxon_ids: chunk.taxon_ids.clone(),
                    });
                }
                // Note: Delta chunks are stored separately and would need special handling
                // for reconstruction. For simple export, we only use reference chunks.
                chunks
            },
            discrepancies: manifest_data.discrepancies.clone(),
            etag: manifest_data.etag.clone(),
            previous_version: manifest_data.previous_version.clone(),
        };

        &temporal_manifest_owned
    } else {
        manifest_data
    };

    // Create assembler using the SEQUOIA storage (use open to rebuild index)
    let sequoia_storage = talaria_sequoia::storage::SEQUOIAStorage::open(&base_path)?;
    let assembler = FastaAssembler::new(&sequoia_storage);

    // Export based on format and streaming preference
    let sequence_count = if args.stream {
        export_streamed(
            &assembler,
            final_manifest_data,
            output_path,
            &args.format,
            args.compress,
            args.with_taxonomy,
        )?
    } else {
        export_full(
            &assembler,
            final_manifest_data,
            output_path,
            &args.format,
            args.compress,
            args.with_taxonomy,
        )?
    };

    // Get file size
    let metadata = std::fs::metadata(output_path)?;
    let file_size = metadata.len();

    Ok(ExportStats {
        sequence_count,
        file_size,
    })
}

fn find_manifest(base_path: &Path, db_ref: &DatabaseReference) -> Result<PathBuf> {
    // Look in the versions directory structure
    let version_path = base_path
        .join("versions")
        .join(&db_ref.source)
        .join(&db_ref.dataset)
        .join(db_ref.version_or_default());

    // Try .tal first, then .json
    let tal_manifest = version_path.join("manifest.tal");
    if tal_manifest.exists() {
        return Ok(tal_manifest);
    }

    let json_manifest = version_path.join("manifest.json");
    if json_manifest.exists() {
        return Ok(json_manifest);
    }

    // Fallback to old manifests directory for compatibility
    let manifests_dir = base_path.join("manifests");
    if manifests_dir.exists() {
        // Try different naming conventions
        let candidates = vec![
            // With version in filename
            format!(
                "{}-{}-{}.json",
                db_ref.source,
                db_ref.dataset,
                db_ref.version_or_default()
            ),
            // Without version (current)
            format!("{}-{}.json", db_ref.source, db_ref.dataset),
            // Alternative naming
            format!("{}_{}.json", db_ref.source, db_ref.dataset),
        ];

        for candidate in candidates {
            let path = manifests_dir.join(&candidate);
            if path.exists() {
                return Ok(path);
            }
        }
    }

    // Check versions directory structure
    let versions_path = base_path
        .join("versions")
        .join(&db_ref.source)
        .join(&db_ref.dataset)
        .join(db_ref.version_or_default())
        .join("manifest.json");

    if versions_path.exists() {
        return Ok(versions_path);
    }

    anyhow::bail!("No manifest found for {}", db_ref.to_string())
}

fn find_profile_manifest(
    base_path: &Path,
    db_ref: &DatabaseReference,
    profile: &str,
) -> Result<PathBuf> {
    // Look in version-specific profiles directory
    let profiles_dir = base_path
        .join("versions")
        .join(&db_ref.source)
        .join(&db_ref.dataset)
        .join(db_ref.version_or_default())
        .join("profiles");

    // Try .tal format first (preferred)
    let tal_path = profiles_dir.join(format!("{}.tal", profile));
    if tal_path.exists() {
        return Ok(tal_path);
    }

    // Fall back to JSON format
    let json_path = profiles_dir.join(format!("{}.json", profile));
    if json_path.exists() {
        return Ok(json_path);
    }

    anyhow::bail!(
        "No profile manifest found for '{}' in database {}. Expected at: {}",
        profile,
        db_ref.to_string(),
        tal_path.display()
    )
}

fn export_streamed(
    assembler: &FastaAssembler,
    manifest: &talaria_sequoia::types::TemporalManifest,
    output_path: &Path,
    format: &ExportFormat,
    compress: bool,
    _with_taxonomy: bool,
) -> Result<usize> {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    // Create output writer
    let file = std::fs::File::create(output_path)?;
    let writer: Box<dyn Write> = if compress {
        Box::new(GzEncoder::new(file, Compression::default()))
    } else {
        Box::new(BufWriter::new(file))
    };

    let mut writer = BufWriter::new(writer);

    // Get chunk hashes
    let chunk_hashes: Vec<_> = manifest
        .chunk_index
        .iter()
        .map(|c| c.hash.clone())
        .collect();

    // Stream assembly directly to writer
    let total_sequences = match format {
        ExportFormat::Fasta => assembler.stream_assembly(&chunk_hashes, &mut writer)?,
        _ => {
            anyhow::bail!("Streaming export only supports FASTA format currently");
        }
    };

    writer.flush()?;
    Ok(total_sequences)
}

fn export_full(
    assembler: &FastaAssembler,
    manifest: &talaria_sequoia::types::TemporalManifest,
    output_path: &Path,
    format: &ExportFormat,
    compress: bool,
    with_taxonomy: bool,
) -> Result<usize> {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    // Get chunk hashes
    let chunk_hashes: Vec<_> = manifest
        .chunk_index
        .iter()
        .map(|c| c.hash.clone())
        .collect();

    // Assemble all sequences
    let sequences = assembler.assemble_from_chunks(&chunk_hashes)?;
    let sequence_count = sequences.len();

    // Create output writer
    let file = std::fs::File::create(output_path)?;
    let writer: Box<dyn Write> = if compress {
        Box::new(GzEncoder::new(file, Compression::default()))
    } else {
        Box::new(BufWriter::new(file))
    };

    let mut writer = BufWriter::new(writer);

    // Export based on format
    match format {
        ExportFormat::Fasta => {
            let formatter = StandardTaxonomyFormatter;
            for seq in &sequences {
                // Use TaxonomyFormatter to handle TaxID properly
                let header = if with_taxonomy {
                    formatter.format_header_with_taxid(
                        &seq.id,
                        seq.description.as_deref(),
                        seq.taxon_id,
                    )
                } else {
                    // Without taxonomy, just use id and description
                    if let Some(ref desc) = seq.description {
                        format!(">{} {}", seq.id, desc)
                    } else {
                        format!(">{}", seq.id)
                    }
                };
                writeln!(writer, "{}", header)?;
                writeln!(writer, "{}", String::from_utf8_lossy(&seq.sequence))?;
            }
        }
        ExportFormat::Fastq => {
            // Convert to FASTQ format (with dummy quality scores)
            for seq in &sequences {
                writeln!(writer, "@{}", seq.id)?;
                writeln!(writer, "{}", String::from_utf8_lossy(&seq.sequence))?;
                writeln!(writer, "+")?;
                // Use maximum quality score for all bases
                writeln!(writer, "{}", "I".repeat(seq.sequence.len()))?;
            }
        }
        ExportFormat::Tsv => {
            // Tab-separated values
            writeln!(writer, "id\tdescription\tsequence\ttaxon_id")?;
            for seq in &sequences {
                writeln!(
                    writer,
                    "{}\t{}\t{}\t{}",
                    seq.id,
                    seq.description.as_deref().unwrap_or(""),
                    String::from_utf8_lossy(&seq.sequence),
                    seq.taxon_id.map_or(String::new(), |t| t.to_string())
                )?;
            }
        }
        ExportFormat::Json => {
            // JSON format
            serde_json::to_writer_pretty(&mut writer, &sequences)?;
        }
    }

    writer.flush()?;
    Ok(sequence_count)
}
