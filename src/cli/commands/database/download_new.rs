/// Updated download command that uses CASG by default

use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct DownloadArgs {
    /// Database to download (uniprot, ncbi, etc.)
    #[arg(value_enum)]
    pub database: Option<Database>,

    /// Output directory for downloaded files
    #[arg(short, long, default_value = ".")]
    pub output: PathBuf,

    /// Specific dataset to download
    #[arg(short = 'd', long)]
    pub dataset: Option<String>,

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

    /// Use legacy versioned download (creates dated directories)
    #[arg(long)]
    pub legacy: bool,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum Database {
    #[value(name = "uniprot")]
    UniProt,
    #[value(name = "ncbi")]
    NCBI,
    #[value(name = "pdb")]
    PDB,
    #[value(name = "pfam")]
    PFAM,
    #[value(name = "silva")]
    Silva,
    #[value(name = "kegg")]
    KEGG,
}

pub fn run(args: DownloadArgs) -> anyhow::Result<()> {
    use crate::download::{DatabaseSource, NCBIDatabase, UniProtDatabase};

    if args.list_datasets {
        list_available_datasets();
        return Ok(());
    }

    if args.interactive || args.database.is_none() {
        return run_interactive_download(args);
    }

    // Parse database source
    let database_source = match args.database.as_ref().unwrap() {
        Database::UniProt => {
            let dataset = args.dataset.as_deref().unwrap_or("swissprot");
            match dataset {
                "swissprot" => DatabaseSource::UniProt(UniProtDatabase::SwissProt),
                "trembl" => DatabaseSource::UniProt(UniProtDatabase::TrEMBL),
                "uniref50" => DatabaseSource::UniProt(UniProtDatabase::UniRef50),
                "uniref90" => DatabaseSource::UniProt(UniProtDatabase::UniRef90),
                "uniref100" => DatabaseSource::UniProt(UniProtDatabase::UniRef100),
                "idmapping" => DatabaseSource::UniProt(UniProtDatabase::IdMapping),
                _ => anyhow::bail!("Unknown UniProt dataset: '{}'", dataset),
            }
        }
        Database::NCBI => {
            let dataset = args.dataset.as_deref().unwrap_or("nr");
            match dataset {
                "nr" => DatabaseSource::NCBI(NCBIDatabase::NR),
                "nt" => DatabaseSource::NCBI(NCBIDatabase::NT),
                "refseq-protein" => DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein),
                "refseq-genomic" => DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic),
                "taxonomy" => DatabaseSource::NCBI(NCBIDatabase::Taxonomy),
                "prot-accession2taxid" => DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId),
                "nucl-accession2taxid" => DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId),
                _ => anyhow::bail!("Unknown NCBI dataset: '{}'", dataset),
            }
        }
        _ => anyhow::bail!("Database not yet implemented"),
    };

    // Route to CASG or legacy
    if args.legacy {
        super::download_casg::run_legacy_download(args, database_source)
    } else {
        super::download_casg::run_casg_download(args, database_source)
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
        Cell::new("Nucleotide sequences"),
        Cell::new("~70 GB compressed"),
    ]);
    table.add_row(vec![
        Cell::new(""),
        Cell::new("taxonomy"),
        Cell::new("Taxonomic classification database"),
        Cell::new("~50 MB compressed"),
    ]);

    println!("\nAvailable Databases and Datasets:");
    println!("{}", table);
    println!("\n[TIP] CASG mode (default) only downloads changes after initial sync!");
    println!("         Use --legacy for old versioned directory behavior");
}

fn run_interactive_download(args: DownloadArgs) -> anyhow::Result<()> {
    use dialoguer::{Select, theme::ColorfulTheme};
    use crate::cli::interactive::print_header;

    print_header("Database Download Manager");

    println!("► Using CASG for efficient database management");
    println!("   (Use --legacy flag for old versioned downloads)");
    println!();

    let databases = vec![
        "UniProt - Protein sequences",
        "NCBI - Comprehensive sequence databases",
        "Exit",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select database source")
        .items(&databases)
        .default(0)
        .interact()?;

    match selection {
        0 => download_uniprot_interactive(args),
        1 => download_ncbi_interactive(args),
        _ => Ok(()),
    }
}

fn download_uniprot_interactive(mut args: DownloadArgs) -> anyhow::Result<()> {
    use dialoguer::{Select, theme::ColorfulTheme};

    let datasets = vec![
        ("swissprot", "SwissProt - Manually reviewed (~570K sequences)"),
        ("trembl", "TrEMBL - Unreviewed (~250M sequences)"),
        ("uniref90", "UniRef90 - Clustered at 90% identity"),
    ];

    let items: Vec<_> = datasets.iter().map(|(_, desc)| *desc).collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select UniProt dataset")
        .items(&items)
        .default(0)
        .interact()?;

    args.database = Some(Database::UniProt);
    args.dataset = Some(datasets[selection].0.to_string());

    run(args)
}

fn download_ncbi_interactive(mut args: DownloadArgs) -> anyhow::Result<()> {
    use dialoguer::{Select, theme::ColorfulTheme};

    let datasets = vec![
        ("nr", "NR - Non-redundant protein sequences"),
        ("nt", "NT - Nucleotide sequences"),
        ("taxonomy", "Taxonomy - Classification database"),
    ];

    let items: Vec<_> = datasets.iter().map(|(_, desc)| *desc).collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select NCBI dataset")
        .items(&items)
        .default(0)
        .interact()?;

    args.database = Some(Database::NCBI);
    args.dataset = Some(datasets[selection].0.to_string());

    run(args)
}