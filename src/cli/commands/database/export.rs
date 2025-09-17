/// Export command for converting CASG-stored databases to standard FASTA format
///
/// This bridges the gap between our efficient CASG storage and traditional
/// bioinformatics tools that expect FASTA files

use clap::Args;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::io::{Write, BufWriter};

use crate::cli::output::{success as print_success, info as print_info};
use crate::utils::progress::create_spinner;
use crate::utils::database_ref::{parse_database_reference, DatabaseReference};
use crate::core::database_manager::DatabaseManager;
use crate::core::paths;
use crate::casg::assembler::FastaAssembler;
use crate::casg::manifest::Manifest;

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
            print_success(&format!(
                "Using cached export: {}",
                output_path.display()
            ));
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
            db_ref.to_string()
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

fn determine_output_path(
    args: &ExportArgs,
    db_ref: &DatabaseReference,
) -> Result<PathBuf> {
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

    std::fs::create_dir_all(&cache_dir)
        .context("Failed to create cache directory")?;

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
    let manager = DatabaseManager::new(Some(base_path.to_string_lossy().to_string()))?;

    // Find the manifest for the requested database
    let manifest_path = find_manifest(&base_path, db_ref)?;

    if !args.quiet {
        print_info(&format!("Using manifest: {}", manifest_path.display()));
    }

    // Load the manifest
    let manifest = Manifest::load_file(&manifest_path)?;
    let manifest_data = manifest.get_data()
        .ok_or_else(|| anyhow::anyhow!("No manifest data found"))?;

    // Check if we need a profile-specific manifest
    let final_manifest = if db_ref.profile.is_some() {
        let profile = db_ref.profile_or_default();
        find_profile_manifest(&base_path, db_ref, profile)?
    } else {
        manifest_path.clone()
    };

    // Load the appropriate manifest data
    let final_manifest_obj;
    let final_manifest_data = if final_manifest != manifest_path {
        final_manifest_obj = Manifest::load_file(&final_manifest)?;
        final_manifest_obj.get_data()
            .ok_or_else(|| anyhow::anyhow!("No profile manifest data found"))?
    } else {
        manifest_data
    };

    // Create assembler
    let assembler = FastaAssembler::new(manager.get_storage());

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

fn find_manifest(
    base_path: &Path,
    db_ref: &DatabaseReference,
) -> Result<PathBuf> {
    let manifests_dir = base_path.join("manifests");

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

    anyhow::bail!(
        "No manifest found for {}",
        db_ref.to_string()
    )
}

fn find_profile_manifest(
    base_path: &Path,
    db_ref: &DatabaseReference,
    profile: &str,
) -> Result<PathBuf> {
    // Look for profile-specific manifest
    let profile_path = base_path
        .join("profiles")
        .join(&db_ref.source)
        .join(&db_ref.dataset)
        .join(profile)
        .join("manifest.json");

    if profile_path.exists() {
        return Ok(profile_path);
    }

    // Check in versions directory
    let versions_profile = base_path
        .join("versions")
        .join(&db_ref.source)
        .join(&db_ref.dataset)
        .join(db_ref.version_or_default())
        .join("profiles")
        .join(profile)
        .join("manifest.json");

    if versions_profile.exists() {
        return Ok(versions_profile);
    }

    anyhow::bail!(
        "No manifest found for profile '{}' of {}",
        profile,
        db_ref.to_string()
    )
}

fn export_streamed(
    assembler: &FastaAssembler,
    manifest: &crate::casg::types::TemporalManifest,
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
    let chunk_hashes: Vec<_> = manifest.chunk_index
        .iter()
        .map(|c| c.hash.clone())
        .collect();

    // Stream assembly directly to writer
    let total_sequences = match format {
        ExportFormat::Fasta => {
            assembler.stream_assembly(&chunk_hashes, &mut writer)?
        }
        _ => {
            anyhow::bail!("Streaming export only supports FASTA format currently");
        }
    };

    writer.flush()?;
    Ok(total_sequences)
}

fn export_full(
    assembler: &FastaAssembler,
    manifest: &crate::casg::types::TemporalManifest,
    output_path: &Path,
    format: &ExportFormat,
    compress: bool,
    with_taxonomy: bool,
) -> Result<usize> {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    // Get chunk hashes
    let chunk_hashes: Vec<_> = manifest.chunk_index
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
            for seq in &sequences {
                write!(writer, ">{}", seq.id)?;

                if let Some(ref desc) = seq.description {
                    write!(writer, " {}", desc)?;
                }

                if with_taxonomy {
                    if let Some(taxon_id) = seq.taxon_id {
                        write!(writer, " TaxID={}", taxon_id)?;
                    }
                }

                writeln!(writer)?;
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