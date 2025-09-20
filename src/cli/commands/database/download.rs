use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct DownloadArgs {
    /// Database to download (e.g., uniprot/swissprot, ncbi/nr)
    pub database: Option<String>,

    /// Output directory for downloaded files
    #[arg(short, long, default_value = ".")]
    pub output: PathBuf,

    /// Download taxonomy data as well
    #[arg(short = 't', long)]
    pub taxonomy: bool,

    /// Download complete taxonomy dataset (all components)
    #[arg(
        long,
        help = "Download all taxonomy components (taxdump + all mappings)"
    )]
    pub complete: bool,

    /// Resume incomplete download
    #[arg(short = 'r', long)]
    pub resume: bool,

    /// Interactive mode
    #[arg(short = 'i', long)]
    pub interactive: bool,

    /// Skip checksum verification
    #[arg(long)]
    pub skip_verify: bool,

    /// List available datasets for each database
    #[arg(long)]
    pub list_datasets: bool,

    /// Save manifest in JSON format instead of binary .tal format
    #[arg(long)]
    pub json: bool,

    /// Manifest server URL (overrides TALARIA_MANIFEST_SERVER env var)
    #[arg(long)]
    pub manifest_server: Option<String>,

    /// Home directory for Talaria (overrides TALARIA_HOME env var)
    #[arg(long)]
    pub talaria_home: Option<String>,

    /// Preserve LAMBDA tool on failure (overrides TALARIA_PRESERVE_LAMBDA_ON_FAILURE env var)
    #[arg(long)]
    pub preserve_lambda_on_failure: bool,

    /// Perform a dry run (only check what would be downloaded, don't actually download)
    #[arg(short = 'd', long)]
    pub dry_run: bool,

    /// Force download even if current version is up-to-date
    #[arg(short = 'f', long)]
    pub force: bool,

    // Fetch-specific options for creating custom databases
    /// Comma-separated list of TaxIDs to fetch (for custom databases)
    #[arg(long, value_name = "TAXIDS", conflicts_with = "taxid_list")]
    pub taxids: Option<String>,

    /// File containing list of TaxIDs, one per line (for custom databases)
    #[arg(long, value_name = "FILE", conflicts_with = "taxids")]
    pub taxid_list: Option<PathBuf>,

    /// Fetch reference proteomes instead of all sequences
    #[arg(long)]
    pub reference_proteomes: bool,

    /// Maximum sequences to fetch per TaxID (for testing)
    #[arg(long)]
    pub max_sequences: Option<usize>,

    /// Description of the custom database
    #[arg(long)]
    pub description: Option<String>,

    // Bi-temporal versioning options
    /// Download database at specific point in time (ISO 8601 format)
    /// Example: --at-time "2024-01-15T10:00:00Z"
    #[arg(long, value_name = "TIMESTAMP")]
    pub at_time: Option<String>,

    /// Download specific sequence version (hash or timestamp)
    #[arg(long, value_name = "VERSION")]
    pub sequence_version: Option<String>,

    /// Download specific taxonomy version (hash or timestamp)
    #[arg(long, value_name = "VERSION")]
    pub taxonomy_version: Option<String>,

    /// Show available versions for the database
    #[arg(long)]
    pub show_versions: bool,
}

pub fn run(args: DownloadArgs) -> anyhow::Result<()> {
    if args.list_datasets {
        list_available_datasets();
        return Ok(());
    }

    // Handle --complete flag for taxonomy
    if args.complete {
        return run_complete_taxonomy_download(args);
    }

    if args.interactive || args.database.is_none() {
        run_interactive_download(args)
    } else {
        // Parse and validate the database reference
        use crate::utils::database_ref::parse_database_ref;
        let (source, dataset) = parse_database_ref(args.database.as_ref().unwrap())?;

        // Print header and CASG info
        use crate::cli::formatter::info_box;
        use crate::cli::output::section_header;
        use colored::Colorize;

        // Format the display name nicely
        let source_name = match source.as_str() {
            "uniprot" => "UniProt",
            "ncbi" => "NCBI",
            _ => &source,
        };
        let dataset_name = match dataset.as_str() {
            "swissprot" => "SwissProt",
            "trembl" => "TrEMBL",
            "uniref50" => "UniRef50",
            "uniref90" => "UniRef90",
            "uniref100" => "UniRef100",
            "idmapping" => "IdMapping",
            "nr" => "NR",
            "nt" => "NT",
            "refseq-protein" => "RefSeq Proteins",
            "refseq-genomic" => "RefSeq Genomes",
            "taxonomy" => "Taxonomy",
            "prot-accession2taxid" => "Protein Accession2TaxId",
            "nucl-accession2taxid" => "Nucleotide Accession2TaxId",
            _ => &dataset,
        };

        println!();
        section_header(&format!(
            "▶ Database Download: {}: {}",
            source_name, dataset_name
        ));
        println!("{}", "═".repeat(80).dimmed());
        println!();

        info_box(
            "Content-Addressed Storage (CASG)",
            &[
                "Automatic deduplication",
                "Incremental updates",
                "Cryptographic verification",
                "Bandwidth-efficient downloads",
            ],
        );
        println!();

        // Handle custom databases (with taxids) vs regular databases
        if source == "custom" {
            run_custom_download(args, dataset)
        } else {
            // Use CASG for regular database downloads
            use super::download_impl::run_database_download;
            use crate::download::DatabaseSource;

            let database_source = DatabaseSource::from_string(&format!("{}/{}", source, dataset))?;
            run_database_download(args, database_source)
        }
    }
}

fn run_complete_taxonomy_download(args: DownloadArgs) -> anyhow::Result<()> {
    use super::download_impl::run_database_download;
    use crate::download::DatabaseSource;
    use colored::Colorize;

    println!();
    println!("{}  Complete Taxonomy Download", "▶".cyan().bold());
    println!(
        "{}  This will download all taxonomy components:",
        "ℹ".blue()
    );
    println!("    • NCBI Taxonomy (taxdump)");
    println!("    • Protein Accession to TaxID mapping");
    println!("    • Nucleotide Accession to TaxID mapping");
    println!("    • UniProt ID mapping");
    println!();

    let components = vec![
        ("ncbi/taxonomy", "NCBI Taxonomy"),
        ("ncbi/prot-accession2taxid", "Protein Accession2TaxID"),
        ("ncbi/nucl-accession2taxid", "Nucleotide Accession2TaxID"),
        ("uniprot/idmapping", "UniProt ID Mapping"),
    ];

    let mut success_count = 0;
    let mut failed = Vec::new();

    for (source_str, name) in components {
        println!("{}  Downloading {}...", "►".cyan().bold(), name);

        match DatabaseSource::from_string(source_str) {
            Ok(database_source) => {
                // Clone args for each component
                let component_args = DownloadArgs {
                    database: Some(source_str.to_string()),
                    output: args.output.clone(),
                    taxonomy: args.taxonomy,
                    complete: false, // Don't recurse
                    resume: args.resume,
                    interactive: false, // Force non-interactive for batch
                    skip_verify: args.skip_verify,
                    list_datasets: false,
                    json: args.json,
                    manifest_server: args.manifest_server.clone(),
                    talaria_home: args.talaria_home.clone(),
                    preserve_lambda_on_failure: args.preserve_lambda_on_failure,
                    dry_run: args.dry_run,
                    force: args.force,
                    taxids: None,
                    taxid_list: None,
                    reference_proteomes: false,
                    max_sequences: None,
                    description: None,
                    at_time: args.at_time.clone(),
                    sequence_version: args.sequence_version.clone(),
                    taxonomy_version: args.taxonomy_version.clone(),
                    show_versions: args.show_versions,
                };

                match run_database_download(component_args, database_source) {
                    Ok(_) => {
                        println!("{}  {} downloaded successfully", "✓".green().bold(), name);
                        success_count += 1;
                    }
                    Err(e) => {
                        println!("{}  Failed to download {}: {}", "✗".red().bold(), name, e);
                        failed.push((name, e));
                    }
                }
            }
            Err(e) => {
                println!(
                    "{}  Failed to parse {}: {}",
                    "✗".red().bold(),
                    source_str,
                    e
                );
                failed.push((name, e));
            }
        }
        println!();
    }

    // Summary
    println!("{}", "═".repeat(60).dimmed());
    println!("{}  Complete Taxonomy Download Summary", "◆".cyan().bold());
    println!(
        "    • {} components downloaded successfully",
        success_count.to_string().green()
    );
    if !failed.is_empty() {
        println!(
            "    • {} components failed:",
            failed.len().to_string().red()
        );
        for (name, err) in &failed {
            println!("      - {}: {}", name, err);
        }
    }
    println!();

    if failed.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Some components failed to download"))
    }
}

fn list_available_datasets() {
    use comfy_table::modifiers::UTF8_ROUND_CORNERS;
    use comfy_table::presets::UTF8_FULL;
    use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table};

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(vec![
        Cell::new("Database")
            .add_attribute(Attribute::Bold)
            .fg(Color::Green),
        Cell::new("Dataset")
            .add_attribute(Attribute::Bold)
            .fg(Color::Green),
        Cell::new("Description")
            .add_attribute(Attribute::Bold)
            .fg(Color::Green),
        Cell::new("Typical Size")
            .add_attribute(Attribute::Bold)
            .fg(Color::Green),
    ]);

    // UniProt datasets
    table.add_row(vec![
        Cell::new("uniprot").add_attribute(Attribute::Bold),
        Cell::new("swissprot"),
        Cell::new("Manually reviewed protein sequences"),
        Cell::new("~100 MB compressed"),
    ]);
    table.add_row(vec![
        Cell::new(""),
        Cell::new("trembl"),
        Cell::new("Automatically annotated protein sequences"),
        Cell::new("~50 GB compressed"),
    ]);
    table.add_row(vec![
        Cell::new(""),
        Cell::new("uniref50"),
        Cell::new("Clustered sequences at 50% identity"),
        Cell::new("~10 GB compressed"),
    ]);
    table.add_row(vec![
        Cell::new(""),
        Cell::new("uniref90"),
        Cell::new("Clustered sequences at 90% identity"),
        Cell::new("~20 GB compressed"),
    ]);
    table.add_row(vec![
        Cell::new(""),
        Cell::new("uniref100"),
        Cell::new("Clustered sequences at 100% identity"),
        Cell::new("~60 GB compressed"),
    ]);
    table.add_row(vec![
        Cell::new(""),
        Cell::new("idmapping"),
        Cell::new("UniProt accession to taxonomy mapping"),
        Cell::new("~15 GB compressed"),
    ]);

    // NCBI datasets
    table.add_row(vec![
        Cell::new("ncbi").add_attribute(Attribute::Bold),
        Cell::new("nr"),
        Cell::new("Non-redundant protein sequences"),
        Cell::new("~90 GB compressed"),
    ]);
    table.add_row(vec![
        Cell::new(""),
        Cell::new("nt"),
        Cell::new("Nucleotide sequences from multiple sources"),
        Cell::new("~70 GB compressed"),
    ]);
    table.add_row(vec![
        Cell::new(""),
        Cell::new("refseq-protein"),
        Cell::new("Curated protein sequences"),
        Cell::new("~30 GB compressed"),
    ]);
    table.add_row(vec![
        Cell::new(""),
        Cell::new("refseq-genomic"),
        Cell::new("Complete genomic sequences"),
        Cell::new("~150 GB compressed"),
    ]);
    table.add_row(vec![
        Cell::new(""),
        Cell::new("taxonomy"),
        Cell::new("Taxonomic classification database (taxdump)"),
        Cell::new("~50 MB compressed"),
    ]);
    table.add_row(vec![
        Cell::new(""),
        Cell::new("prot-accession2taxid"),
        Cell::new("Protein accession to taxonomy ID mapping"),
        Cell::new("~15 GB compressed"),
    ]);
    table.add_row(vec![
        Cell::new(""),
        Cell::new("nucl-accession2taxid"),
        Cell::new("Nucleotide accession to taxonomy ID mapping"),
        Cell::new("~8 GB compressed"),
    ]);

    // Not yet implemented databases
    table.add_row(vec![
        Cell::new("pdb")
            .add_attribute(Attribute::Bold)
            .fg(Color::DarkGrey),
        Cell::new("(not implemented)").fg(Color::DarkGrey),
        Cell::new("Protein structure sequences").fg(Color::DarkGrey),
        Cell::new("").fg(Color::DarkGrey),
    ]);
    table.add_row(vec![
        Cell::new("pfam")
            .add_attribute(Attribute::Bold)
            .fg(Color::DarkGrey),
        Cell::new("(not implemented)").fg(Color::DarkGrey),
        Cell::new("Protein families").fg(Color::DarkGrey),
        Cell::new("").fg(Color::DarkGrey),
    ]);
    table.add_row(vec![
        Cell::new("silva")
            .add_attribute(Attribute::Bold)
            .fg(Color::DarkGrey),
        Cell::new("(not implemented)").fg(Color::DarkGrey),
        Cell::new("Ribosomal RNA sequences").fg(Color::DarkGrey),
        Cell::new("").fg(Color::DarkGrey),
    ]);
    table.add_row(vec![
        Cell::new("kegg")
            .add_attribute(Attribute::Bold)
            .fg(Color::DarkGrey),
        Cell::new("(not implemented)").fg(Color::DarkGrey),
        Cell::new("Metabolic pathways").fg(Color::DarkGrey),
        Cell::new("").fg(Color::DarkGrey),
    ]);

    println!("\nAvailable Databases and Datasets:");
    println!("{}", table);
    println!("\nUsage: talaria database download --database <DATABASE> --dataset <DATASET>");
    println!("Example: talaria database download --database uniprot --dataset swissprot");
}

fn run_custom_download(args: DownloadArgs, db_name: String) -> anyhow::Result<()> {
    use crate::bio::taxonomy::SequenceProvider;
    use crate::bio::uniprot::CustomDatabaseProvider;
    use crate::cli::output::{info, section_header, success};
    use crate::core::database_manager::DatabaseManager;
    use crate::download::DatabaseSource;

    // Parse TaxIDs
    let taxids = if let Some(taxid_list_path) = &args.taxid_list {
        // Read from file
        let content = std::fs::read_to_string(taxid_list_path)?;
        content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    trimmed.parse::<u32>().ok()
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    } else if let Some(taxids_str) = &args.taxids {
        // Parse comma-separated list
        taxids_str
            .split(',')
            .filter_map(|s| s.trim().parse::<u32>().ok())
            .collect()
    } else {
        anyhow::bail!("Custom databases require --taxids or --taxid-list");
    };

    if taxids.is_empty() {
        anyhow::bail!("No valid TaxIDs found");
    }

    println!();
    section_header(&format!("Creating Custom Database: {}", db_name));
    info(&format!("Fetching sequences for {} TaxIDs", taxids.len()));

    // Initialize database manager
    let mut manager = DatabaseManager::new(None)?;
    let database_source = DatabaseSource::Custom(db_name.clone());

    // Create provider and fetch sequences
    let provider = CustomDatabaseProvider::new(db_name.clone(), taxids)?;
    let sequences = provider.fetch_sequences()?;

    info(&format!("Total sequences fetched: {}", sequences.len()));

    // Use the unified pipeline - chunk sequences directly
    info("Processing into CASG chunks...");
    manager.chunk_sequences_direct(sequences, &database_source)?;

    success(&format!(
        "Successfully created custom database: custom/{}",
        db_name
    ));
    Ok(())
}

fn run_interactive_download(args: DownloadArgs) -> anyhow::Result<()> {
    use crate::cli::interactive::print_header;
    use dialoguer::{theme::ColorfulTheme, Select};

    print_header("Database Download Manager");

    let databases = vec![
        "UniProt - Protein sequences",
        "NCBI - Comprehensive sequence databases",
        "PDB - Protein structure sequences",
        "PFAM - Protein families",
        "Silva - Ribosomal RNA sequences",
        "KEGG - Metabolic pathways",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select database source")
        .items(&databases)
        .default(0)
        .interact()?;

    match selection {
        0 => download_uniprot_interactive(&args.output)?,
        1 => download_ncbi_interactive(&args.output)?,
        _ => anyhow::bail!("Database not yet implemented"),
    }

    Ok(())
}

fn download_uniprot_interactive(output_dir: &PathBuf) -> anyhow::Result<()> {
    use crate::cli::interactive::{show_info, show_success};
    use dialoguer::{theme::ColorfulTheme, Confirm, Select};

    let datasets = vec![
        (
            "swissprot",
            "SwissProt",
            "Manually reviewed sequences (~570K, ~200MB)",
        ),
        ("trembl", "TrEMBL", "Unreviewed sequences (~250M, ~100GB)"),
        ("uniref90", "UniRef90", "Clustered at 90% identity (~100M)"),
        ("uniref50", "UniRef50", "Clustered at 50% identity (~50M)"),
        (
            "uniref100",
            "UniRef100",
            "Clustered at 100% identity (~300M)",
        ),
    ];

    let items: Vec<String> = datasets
        .iter()
        .map(|(_, name, desc)| format!("{} - {}", name, desc))
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select UniProt dataset")
        .items(&items)
        .default(0)
        .interact()?;

    let (dataset_id, name, desc) = datasets[selection];

    show_info(&format!("Selected: {} ({})", name, desc));

    let download_taxonomy = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Download taxonomy mapping (recommended)?")
        .default(true)
        .interact()?;

    // Create the database reference in the new format
    let database_ref = format!("uniprot/{}", dataset_id);

    // Create args with the new format
    let args = DownloadArgs {
        database: Some(database_ref.clone()),
        output: output_dir.clone(),
        taxonomy: download_taxonomy,
        complete: false,
        resume: false,
        interactive: false,
        skip_verify: false,
        list_datasets: false,
        json: false,
        manifest_server: None,
        talaria_home: None,
        preserve_lambda_on_failure: false,
        dry_run: false,
        force: false,
        taxids: None,
        taxid_list: None,
        reference_proteomes: false,
        max_sequences: None,
        description: None,
        at_time: None,
        sequence_version: None,
        taxonomy_version: None,
        show_versions: false,
    };

    // Print header and CASG info
    use crate::cli::formatter::info_box;
    use crate::cli::output::section_header;
    use colored::Colorize;

    println!();
    section_header(&format!("▶ Database Download: UniProt: {}", name));
    println!("{}", "═".repeat(80).dimmed());
    println!();

    info_box(
        "Content-Addressed Storage (CASG)",
        &[
            "Automatic deduplication",
            "Incremental updates",
            "Cryptographic verification",
            "Bandwidth-efficient downloads",
        ],
    );
    println!();

    // Use the unified CASG download
    use super::download_impl::run_database_download;
    use crate::download::DatabaseSource;

    let database_source = DatabaseSource::from_string(&format!("uniprot/{}", dataset_id))?;
    run_database_download(args, database_source)?;

    show_success(&format!("{} download complete!", name));

    Ok(())
}

fn download_ncbi_interactive(output_dir: &PathBuf) -> anyhow::Result<()> {
    use crate::cli::interactive::{show_info, show_success};
    use dialoguer::{theme::ColorfulTheme, Select};

    let datasets = vec![
        ("nr", "NR", "Non-redundant protein sequences (~90GB)"),
        ("nt", "NT", "Nucleotide sequences (~70GB)"),
        (
            "refseq-protein",
            "RefSeq Proteins",
            "RefSeq protein database (~30GB)",
        ),
        (
            "refseq-genomic",
            "RefSeq Genomes",
            "RefSeq complete genomes (~150GB)",
        ),
        ("taxonomy", "Taxonomy", "NCBI taxonomy dump (~50MB)"),
        (
            "prot-accession2taxid",
            "Protein Accession2TaxId",
            "Protein accession mappings (~15GB)",
        ),
        (
            "nucl-accession2taxid",
            "Nucleotide Accession2TaxId",
            "Nucleotide accession mappings (~8GB)",
        ),
    ];

    let items: Vec<String> = datasets
        .iter()
        .map(|(_, name, desc)| format!("{} - {}", name, desc))
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select NCBI dataset")
        .items(&items)
        .default(0)
        .interact()?;

    let (dataset_id, name, desc) = datasets[selection];

    show_info(&format!("Selected: {} ({})", name, desc));

    // Create the database reference in the new format
    let database_ref = format!("ncbi/{}", dataset_id);

    // Create args with the new format
    let args = DownloadArgs {
        database: Some(database_ref.clone()),
        output: output_dir.clone(),
        taxonomy: false,
        complete: false,
        resume: false,
        interactive: false,
        skip_verify: false,
        list_datasets: false,
        json: false,
        manifest_server: None,
        talaria_home: None,
        preserve_lambda_on_failure: false,
        dry_run: false,
        force: false,
        taxids: None,
        taxid_list: None,
        reference_proteomes: false,
        max_sequences: None,
        description: None,
        at_time: None,
        sequence_version: None,
        taxonomy_version: None,
        show_versions: false,
    };

    // Print header and CASG info
    use crate::cli::formatter::info_box;
    use crate::cli::output::section_header;
    use colored::Colorize;

    println!();
    section_header(&format!("▶ Database Download: NCBI: {}", name));
    println!("{}", "═".repeat(80).dimmed());
    println!();

    info_box(
        "Content-Addressed Storage (CASG)",
        &[
            "Automatic deduplication",
            "Incremental updates",
            "Cryptographic verification",
            "Bandwidth-efficient downloads",
        ],
    );
    println!();

    // Use the unified CASG download
    use super::download_impl::run_database_download;
    use crate::download::DatabaseSource;

    let database_source = DatabaseSource::from_string(&format!("ncbi/{}", dataset_id))?;
    run_database_download(args, database_source)?;

    show_success(&format!("{} download complete!", name));

    Ok(())
}
