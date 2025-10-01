#![allow(dead_code)]

use anyhow::{Context, Result};
/// Export command for converting SEQUOIA-stored databases to standard FASTA format
///
/// This bridges the gap between our efficient SEQUOIA storage and traditional
/// bioinformatics tools that expect FASTA files
use clap::Args;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::cli::formatting::output::{info as print_info, success as print_success};
use crate::cli::progress::create_spinner;
use talaria_bio::taxonomy::{StandardTaxonomyFormatter, TaxonomyFormatter};
use talaria_core::system::paths;
use talaria_sequoia::database::DatabaseManager;
use talaria_sequoia::manifest::Manifest;
use talaria_sequoia::operations::FastaAssembler;
use talaria_utils::database::database_ref::{parse_database_reference, DatabaseReference};

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

    /// Sequence data date for bi-temporal export (e.g., "2024-01-15")
    #[arg(long)]
    pub sequence_date: Option<String>,

    /// Taxonomy date for bi-temporal export (e.g., "2024-03-15")
    #[arg(long)]
    pub taxonomy_date: Option<String>,

    /// Filter by taxonomy expression (e.g., "Bacteria AND NOT Escherichia")
    #[arg(long)]
    pub taxonomy_filter: Option<String>,

    /// Reduce redundancy to specified percentage (0-100)
    /// Uses CD-HIT-like clustering to select representative sequences
    /// Example: --redundancy 90 keeps sequences with ≤90% similarity
    #[arg(long, value_name = "PERCENTAGE")]
    pub redundancy: Option<u8>,

    /// Maximum number of sequences to export (useful for testing)
    #[arg(long)]
    pub max_sequences: Option<usize>,

    /// Random sampling rate (0.0-1.0, e.g., 0.1 for 10% sample)
    #[arg(long)]
    pub sample: Option<f32>,
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
        Some(create_spinner(&format!("Exporting {} to FASTA...", db_ref)))
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
    // Check if we need bi-temporal export
    if args.sequence_date.is_some() || args.taxonomy_date.is_some() {
        return perform_bitemporal_export(args, db_ref, output_path);
    }

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
        use talaria_sequoia::ReductionManifest;
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
        temporal_manifest_owned = talaria_sequoia::TemporalManifest {
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
                    chunks.push(talaria_sequoia::ManifestMetadata {
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
    let sequoia_storage = talaria_sequoia::SequoiaStorage::open(&base_path)?;
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
            args,
        )?
    } else {
        export_full(
            &assembler,
            final_manifest_data,
            output_path,
            &args.format,
            args.compress,
            args.with_taxonomy,
            args,
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
    manifest: &talaria_sequoia::TemporalManifest,
    output_path: &Path,
    format: &ExportFormat,
    compress: bool,
    _with_taxonomy: bool,
    _args: &ExportArgs,
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
    manifest: &talaria_sequoia::TemporalManifest,
    output_path: &Path,
    format: &ExportFormat,
    compress: bool,
    with_taxonomy: bool,
    args: &ExportArgs,
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
    let mut sequences = assembler.assemble_from_chunks(&chunk_hashes)?;

    // Apply redundancy reduction if requested
    if let Some(redundancy) = args.redundancy {
        sequences = apply_redundancy_reduction(sequences, redundancy)?;
    }

    // Apply sampling if requested
    if let Some(sample_rate) = args.sample {
        if sample_rate > 0.0 && sample_rate < 1.0 {
            use rand::seq::SliceRandom;
            let mut rng = rand::thread_rng();
            let sample_size = (sequences.len() as f32 * sample_rate) as usize;
            sequences.shuffle(&mut rng);
            sequences.truncate(sample_size);
        }
    }

    // Apply max sequences limit if specified
    if let Some(max) = args.max_sequences {
        sequences.truncate(max);
    }

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

fn perform_bitemporal_export(
    args: &ExportArgs,
    db_ref: &DatabaseReference,
    output_path: &Path,
) -> Result<ExportStats> {
    use chrono::Utc;
    use std::sync::Arc;
    use talaria_sequoia::{BiTemporalDatabase, SequoiaStorage};

    // Parse times
    let sequence_time = if let Some(date_str) = &args.sequence_date {
        parse_time_input(date_str)?
    } else {
        Utc::now()
    };

    let taxonomy_time = if let Some(date_str) = &args.taxonomy_date {
        parse_time_input(date_str)?
    } else {
        sequence_time
    };

    if !args.quiet {
        print_info(&format!(
            "Bi-temporal export: sequence={}, taxonomy={}",
            sequence_time.format("%Y-%m-%d"),
            taxonomy_time.format("%Y-%m-%d")
        ));
    }

    // Get the database path
    let db_path = paths::talaria_databases_dir()
        .join(&db_ref.source)
        .join(&db_ref.dataset);

    if !db_path.exists() {
        anyhow::bail!(
            "Database not found at {:?}. Run 'talaria database download {}' first.",
            db_path,
            format!("{}/{}", db_ref.source, db_ref.dataset)
        );
    }

    // Open SEQUOIA storage and bi-temporal database
    let storage = Arc::new(SequoiaStorage::open(&db_path)?);
    let mut bi_temporal_db = BiTemporalDatabase::new(storage.clone())?;

    // Query at the specified times
    let snapshot = bi_temporal_db
        .query_at(sequence_time, taxonomy_time)
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to query database at specified times: {}. \
             The database may not have data for these dates.",
                e
            )
        })?;

    if !args.quiet {
        print_info(&format!(
            "Snapshot: {} sequences, {} chunks",
            snapshot.sequence_count(),
            snapshot.chunks().len()
        ));
    }

    // Create output file
    let file: Box<dyn std::io::Write> = if args.compress {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        Box::new(GzEncoder::new(
            std::fs::File::create(output_path)?,
            Compression::default(),
        ))
    } else {
        Box::new(std::fs::File::create(output_path)?)
    };

    let mut writer = BufWriter::new(file);

    // Write header with bi-temporal information
    match args.format {
        ExportFormat::Fasta => {
            writeln!(writer, "; SEQUOIA Bi-temporal Export")?;
            writeln!(
                writer,
                "; Sequence Date: {}",
                sequence_time.format("%Y-%m-%d %H:%M:%S UTC")
            )?;
            writeln!(
                writer,
                "; Taxonomy Date: {}",
                taxonomy_time.format("%Y-%m-%d %H:%M:%S UTC")
            )?;
            writeln!(
                writer,
                "; Sequence Root: {}",
                &snapshot.sequence_root().to_string()[..12]
            )?;
            writeln!(
                writer,
                "; Taxonomy Root: {}",
                &snapshot.taxonomy_root().to_string()[..12]
            )?;
            writeln!(writer, "; Total Sequences: {}", snapshot.sequence_count())?;
        }
        _ => {}
    }

    // Export actual sequences from chunks
    let mut sequence_count = 0;

    for chunk_meta in snapshot.chunks() {
        // Apply taxonomy filter if specified
        if let Some(filter) = &args.taxonomy_filter {
            if !matches_taxonomy_filter(&chunk_meta, filter)? {
                continue;
            }
        }

        // Try to load sequences from the chunk
        match storage.get_chunk(&chunk_meta.hash) {
            Ok(chunk_data) => {
                // Try to parse as ChunkManifest first
                if let Ok(manifest) =
                    rmp_serde::from_slice::<talaria_sequoia::ChunkManifest>(&chunk_data)
                {
                    // Load actual sequences from canonical storage
                    for seq_hash in &manifest.sequence_refs {
                        if let Ok(canonical) = storage.sequence_storage.load_canonical(seq_hash) {
                            let seq = talaria_bio::sequence::Sequence {
                                id: seq_hash.to_hex(),
                                description: None,
                                sequence: canonical.sequence.clone(),
                                taxon_id: None,
                                taxonomy_sources: Default::default(),
                            };

                            match args.format {
                                ExportFormat::Fasta => {
                                    writeln!(writer, ">{}", seq.id)?;
                                    writeln!(writer, "{}", String::from_utf8_lossy(&seq.sequence))?;
                                }
                                ExportFormat::Fastq => {
                                    writeln!(writer, "@{}", seq.id)?;
                                    writeln!(writer, "{}", String::from_utf8_lossy(&seq.sequence))?;
                                    writeln!(writer, "+")?;
                                    writeln!(writer, "{}", "I".repeat(seq.sequence.len()))?;
                                }
                                ExportFormat::Tsv => {
                                    writeln!(
                                        writer,
                                        "{}\t{}",
                                        seq.id,
                                        String::from_utf8_lossy(&seq.sequence)
                                    )?;
                                }
                                ExportFormat::Json => {
                                    let json = serde_json::json!({
                                        "id": seq.id,
                                        "sequence": String::from_utf8_lossy(&seq.sequence),
                                        "taxon_id": seq.taxon_id,
                                    });
                                    writeln!(writer, "{}", json)?;
                                }
                            }
                        }
                    }
                } else {
                    // Fall back to parsing as raw FASTA
                    let sequences = talaria_bio::parse_fasta_from_bytes(&chunk_data)?;

                    for seq in sequences {
                        match args.format {
                            ExportFormat::Fasta => {
                                writeln!(writer, ">{}", seq.id)?;
                                writeln!(writer, "{}", String::from_utf8_lossy(&seq.sequence))?;
                            }
                            ExportFormat::Tsv => {
                                writeln!(
                                    writer,
                                    "{}\t{}",
                                    seq.id,
                                    String::from_utf8_lossy(&seq.sequence)
                                )?;
                            }
                            ExportFormat::Json => {
                                let json = serde_json::json!({
                                    "id": seq.id,
                                    "sequence": String::from_utf8_lossy(&seq.sequence),
                                    "taxon_id": seq.taxon_id,
                                });
                                writeln!(writer, "{}", json)?;
                            }
                            ExportFormat::Fastq => {
                                writeln!(writer, "@{}", seq.id)?;
                                writeln!(writer, "{}", String::from_utf8_lossy(&seq.sequence))?;
                                writeln!(writer, "+")?;
                                writeln!(writer, "{}", "I".repeat(seq.sequence.len()))?;
                            }
                        }
                        sequence_count += 1;
                    }
                }
            }
            Err(e) => {
                if !args.quiet {
                    eprintln!(
                        "Warning: Failed to load chunk {}: {}",
                        &chunk_meta.hash.to_string()[..8],
                        e
                    );
                }
            }
        }
    }

    writer.flush()?;

    // Get file size
    let metadata = std::fs::metadata(output_path)?;

    Ok(ExportStats {
        sequence_count,
        file_size: metadata.len(),
    })
}

fn parse_time_input(input: &str) -> Result<chrono::DateTime<chrono::Utc>> {
    use chrono::{DateTime, NaiveDate, Utc};

    // Try parsing as full RFC3339 timestamp first
    if let Ok(dt) = DateTime::parse_from_rfc3339(input) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try parsing as date only (assume 00:00:00 UTC)
    if let Ok(dt) = NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        let time = dt
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid time"))?;
        return Ok(DateTime::from_naive_utc_and_offset(time, Utc));
    }

    Err(anyhow::anyhow!(
        "Invalid time format '{}'. Use YYYY-MM-DD or RFC3339 format.",
        input
    ))
}

fn matches_taxonomy_filter(
    chunk: &talaria_sequoia::ManifestMetadata,
    filter: &str,
) -> Result<bool> {
    // Use the new taxonomy filter with boolean expression support
    use talaria_sequoia::taxonomy::filter::TaxonomyFilter;

    let filter = TaxonomyFilter::parse(filter)?;
    Ok(filter.matches(&chunk.taxon_ids))
}

/// Apply redundancy reduction using simple sequence clustering
/// This is a basic implementation - for production use, consider integrating CD-HIT or MMseqs2
fn apply_redundancy_reduction(
    sequences: Vec<talaria_bio::Sequence>,
    redundancy_threshold: u8,
) -> Result<Vec<talaria_bio::Sequence>> {
    use std::collections::HashSet;

    if sequences.is_empty() {
        return Ok(sequences);
    }

    // Convert threshold to similarity fraction (e.g., 90 -> 0.9)
    let similarity_threshold = redundancy_threshold as f32 / 100.0;

    // Track which sequences are representatives
    let mut representatives = Vec::new();
    let mut clustered_indices = HashSet::new();

    for (i, seq) in sequences.iter().enumerate() {
        if clustered_indices.contains(&i) {
            continue;
        }

        // This sequence becomes a representative
        representatives.push(seq.clone());
        clustered_indices.insert(i);

        // Find all similar sequences and mark them as clustered
        for (j, other) in sequences.iter().enumerate().skip(i + 1) {
            if clustered_indices.contains(&j) {
                continue;
            }

            // Calculate similarity (basic implementation - use edit distance for small seqs)
            let similarity = calculate_sequence_similarity(&seq.sequence, &other.sequence);

            if similarity >= similarity_threshold {
                // This sequence is similar enough to be represented by seq[i]
                clustered_indices.insert(j);
            }
        }
    }

    println!(
        "Redundancy reduction: {} sequences -> {} representatives ({}% reduction)",
        sequences.len(),
        representatives.len(),
        ((sequences.len() - representatives.len()) as f32 / sequences.len() as f32 * 100.0) as i32
    );

    Ok(representatives)
}

/// Calculate similarity between two sequences (0.0 to 1.0)
/// This is a simplified implementation - for production, use proper alignment algorithms
fn calculate_sequence_similarity(seq1: &[u8], seq2: &[u8]) -> f32 {
    use std::collections::HashSet;

    // Quick length-based filter
    let len_diff = (seq1.len() as i32 - seq2.len() as i32).abs();
    let max_len = seq1.len().max(seq2.len()) as f32;

    if len_diff as f32 / max_len > 0.2 {
        // If lengths differ by more than 20%, consider them dissimilar
        return 0.0;
    }

    // Simple k-mer based similarity (for speed)
    let k = 3; // k-mer size
    if seq1.len() < k || seq2.len() < k {
        // For very short sequences, use exact match
        return if seq1 == seq2 { 1.0 } else { 0.0 };
    }

    let mut kmers1 = HashSet::new();
    let mut kmers2 = HashSet::new();

    // Extract k-mers from both sequences
    for window in seq1.windows(k) {
        kmers1.insert(window);
    }
    for window in seq2.windows(k) {
        kmers2.insert(window);
    }

    // Calculate Jaccard similarity
    let intersection = kmers1.intersection(&kmers2).count();
    let union = kmers1.union(&kmers2).count();

    if union == 0 {
        0.0
    } else {
        intersection as f32 / union as f32
    }
}
