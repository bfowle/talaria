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
}

pub fn run(args: DownloadArgs) -> anyhow::Result<()> {
    if args.list_datasets {
        list_available_datasets();
        return Ok(());
    }

    if args.interactive || args.database.is_none() {
        run_interactive_download(args)
    } else {
        // Parse and validate the database reference
        use crate::utils::database_ref::parse_database_ref;
        let (source, dataset) = parse_database_ref(args.database.as_ref().unwrap())?;

        // Print header and CASG info
        use crate::cli::output::section_header;
        use crate::cli::formatter::info_box;
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
        section_header(&format!("▶ Database Download: {}: {}", source_name, dataset_name));
        println!("{}", "═".repeat(80).dimmed());
        println!();

        info_box("Content-Addressed Storage (CASG)", &[
            "Automatic deduplication",
            "Incremental updates",
            "Cryptographic verification",
            "Bandwidth-efficient downloads"
        ]);
        println!();

        // Use CASG for all downloads
        super::download_simple::run_direct_download(args)
    }
}

fn list_available_datasets() {
    use comfy_table::{Table, Cell, Attribute, ContentArrangement, Color};
    use comfy_table::presets::UTF8_FULL;
    use comfy_table::modifiers::UTF8_ROUND_CORNERS;

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(vec![
        Cell::new("Database").add_attribute(Attribute::Bold).fg(Color::Green),
        Cell::new("Dataset").add_attribute(Attribute::Bold).fg(Color::Green),
        Cell::new("Description").add_attribute(Attribute::Bold).fg(Color::Green),
        Cell::new("Typical Size").add_attribute(Attribute::Bold).fg(Color::Green),
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
        Cell::new("pdb").add_attribute(Attribute::Bold).fg(Color::DarkGrey),
        Cell::new("(not implemented)").fg(Color::DarkGrey),
        Cell::new("Protein structure sequences").fg(Color::DarkGrey),
        Cell::new("").fg(Color::DarkGrey),
    ]);
    table.add_row(vec![
        Cell::new("pfam").add_attribute(Attribute::Bold).fg(Color::DarkGrey),
        Cell::new("(not implemented)").fg(Color::DarkGrey),
        Cell::new("Protein families").fg(Color::DarkGrey),
        Cell::new("").fg(Color::DarkGrey),
    ]);
    table.add_row(vec![
        Cell::new("silva").add_attribute(Attribute::Bold).fg(Color::DarkGrey),
        Cell::new("(not implemented)").fg(Color::DarkGrey),
        Cell::new("Ribosomal RNA sequences").fg(Color::DarkGrey),
        Cell::new("").fg(Color::DarkGrey),
    ]);
    table.add_row(vec![
        Cell::new("kegg").add_attribute(Attribute::Bold).fg(Color::DarkGrey),
        Cell::new("(not implemented)").fg(Color::DarkGrey),
        Cell::new("Metabolic pathways").fg(Color::DarkGrey),
        Cell::new("").fg(Color::DarkGrey),
    ]);

    println!("\nAvailable Databases and Datasets:");
    println!("{}", table);
    println!("\nUsage: talaria database download --database <DATABASE> --dataset <DATASET>");
    println!("Example: talaria database download --database uniprot --dataset swissprot");
}

fn run_interactive_download(args: DownloadArgs) -> anyhow::Result<()> {
    use dialoguer::{Select, theme::ColorfulTheme};
    use crate::cli::interactive::print_header;
    
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
    use dialoguer::{Select, Confirm, theme::ColorfulTheme};
    use crate::cli::interactive::{show_info, show_success};

    let datasets = vec![
        ("swissprot", "SwissProt", "Manually reviewed sequences (~570K, ~200MB)"),
        ("trembl", "TrEMBL", "Unreviewed sequences (~250M, ~100GB)"),
        ("uniref90", "UniRef90", "Clustered at 90% identity (~100M)"),
        ("uniref50", "UniRef50", "Clustered at 50% identity (~50M)"),
        ("uniref100", "UniRef100", "Clustered at 100% identity (~300M)"),
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
    };

    // Print header and CASG info
    use crate::cli::output::section_header;
    use crate::cli::formatter::info_box;
    use colored::Colorize;

    println!();
    section_header(&format!("▶ Database Download: UniProt: {}", name));
    println!("{}", "═".repeat(80).dimmed());
    println!();

    info_box("Content-Addressed Storage (CASG)", &[
        "Automatic deduplication",
        "Incremental updates",
        "Cryptographic verification",
        "Bandwidth-efficient downloads"
    ]);
    println!();

    // Call the actual download function
    super::download_simple::run_direct_download(args)?;

    show_success(&format!("{} download complete!", name));

    Ok(())
}

fn download_ncbi_interactive(output_dir: &PathBuf) -> anyhow::Result<()> {
    use dialoguer::{Select, theme::ColorfulTheme};
    use crate::cli::interactive::{show_info, show_success};

    let datasets = vec![
        ("nr", "NR", "Non-redundant protein sequences (~90GB)"),
        ("nt", "NT", "Nucleotide sequences (~70GB)"),
        ("refseq-protein", "RefSeq Proteins", "RefSeq protein database (~30GB)"),
        ("refseq-genomic", "RefSeq Genomes", "RefSeq complete genomes (~150GB)"),
        ("taxonomy", "Taxonomy", "NCBI taxonomy dump (~50MB)"),
        ("prot-accession2taxid", "Protein Accession2TaxId", "Protein accession mappings (~15GB)"),
        ("nucl-accession2taxid", "Nucleotide Accession2TaxId", "Nucleotide accession mappings (~8GB)"),
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
    };

    // Print header and CASG info
    use crate::cli::output::section_header;
    use crate::cli::formatter::info_box;
    use colored::Colorize;

    println!();
    section_header(&format!("▶ Database Download: NCBI: {}", name));
    println!("{}", "═".repeat(80).dimmed());
    println!();

    info_box("Content-Addressed Storage (CASG)", &[
        "Automatic deduplication",
        "Incremental updates",
        "Cryptographic verification",
        "Bandwidth-efficient downloads"
    ]);
    println!();

    // Call the actual download function
    super::download_simple::run_direct_download(args)?;

    show_success(&format!("{} download complete!", name));

    Ok(())
}

// Legacy download implementation moved to download_casg::run_legacy_download
// This function is no longer needed as we route through download_simple module