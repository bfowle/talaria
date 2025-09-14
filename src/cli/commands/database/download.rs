use clap::Args;
use std::path::PathBuf;
use chrono::Local;

#[derive(Args)]
pub struct DownloadArgs {
    /// Database to download (uniprot, ncbi, etc.)
    #[arg(value_enum)]
    pub database: Option<Database>,
    
    /// Output directory for downloaded files
    #[arg(short, long, default_value = ".")]
    pub output: PathBuf,
    
    /// Specific dataset to download
    /// UniProt: swissprot, trembl, uniref50, uniref90, uniref100, idmapping
    /// NCBI: nr, nt, refseq-protein, refseq-genomic, taxonomy, prot-accession2taxid, nucl-accession2taxid
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
    if args.list_datasets {
        list_available_datasets();
        return Ok(());
    }

    if args.interactive || args.database.is_none() {
        run_interactive_download(args)
    } else {
        run_direct_download(args)
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

fn download_uniprot_interactive(_output_dir: &PathBuf) -> anyhow::Result<()> {
    use dialoguer::{Select, Confirm, theme::ColorfulTheme};
    use crate::cli::interactive::{show_info, show_success};
    
    let datasets = vec![
        ("SwissProt", "Manually reviewed sequences (~570K, ~200MB)", "uniprot_sprot.fasta.gz"),
        ("TrEMBL", "Unreviewed sequences (~250M, ~100GB)", "uniprot_trembl.fasta.gz"),
        ("UniRef90", "Clustered at 90% identity (~100M)", "uniref90.fasta.gz"),
        ("UniRef50", "Clustered at 50% identity (~50M)", "uniref50.fasta.gz"),
        ("UniRef100", "Clustered at 100% identity (~300M)", "uniref100.fasta.gz"),
    ];
    
    let items: Vec<String> = datasets
        .iter()
        .map(|(name, desc, _)| format!("{} - {}", name, desc))
        .collect();
    
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select UniProt dataset")
        .items(&items)
        .default(0)
        .interact()?;
    
    let (name, desc, _filename) = datasets[selection];
    
    show_info(&format!("Selected: {} ({})", name, desc));
    
    let download_taxonomy = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Download taxonomy mapping (recommended)?")
        .default(true)
        .interact()?;
    
    if download_taxonomy {
        show_info("Would download taxonomy mapping...");
    }
    
    show_info(&format!("Would download {}...", name));
    show_success(&format!("{} download configured!", name));
    
    Ok(())
}

fn download_ncbi_interactive(_output_dir: &PathBuf) -> anyhow::Result<()> {
    use dialoguer::{Select, theme::ColorfulTheme};
    use crate::cli::interactive::{show_info, show_success};

    let datasets = vec![
        ("nr", "Non-redundant protein sequences (~90GB)", "nr.gz"),
        ("nt", "Nucleotide sequences (~70GB)", "nt.gz"),
        ("RefSeq Proteins", "RefSeq protein database (~30GB)", "refseq_protein.fasta.gz"),
        ("RefSeq Genomes", "RefSeq complete genomes (~150GB)", "refseq_genomic.fasta.gz"),
        ("Taxonomy", "NCBI taxonomy dump (~50MB)", "taxdump.tar.gz"),
        ("Protein Accession2TaxId", "Protein accession mappings (~15GB)", "prot.accession2taxid.gz"),
        ("Nucleotide Accession2TaxId", "Nucleotide accession mappings (~8GB)", "nucl.accession2taxid.gz"),
    ];
    
    let items: Vec<String> = datasets
        .iter()
        .map(|(name, desc, _)| format!("{} - {}", name, desc))
        .collect();
    
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select NCBI dataset")
        .items(&items)
        .default(0)
        .interact()?;
    
    let (name, desc, _filename) = datasets[selection];
    
    show_info(&format!("Selected: {} ({})", name, desc));
    show_info(&format!("Would download {}...", name));
    show_success(&format!("{} download configured!", name));
    
    Ok(())
}

fn run_direct_download(args: DownloadArgs) -> anyhow::Result<()> {
    use crate::download::{DatabaseSource, NCBIDatabase, UniProtDatabase, DownloadProgress};
    use crate::core::database_manager::{DatabaseManager, DatabaseMetadata};
    use crate::core::config::load_config;
    use chrono::Utc;
    
    // Load config to get database settings
    let config = load_config("talaria.toml").unwrap_or_default();
    
    // Initialize database manager
    let db_manager = DatabaseManager::new(config.database.database_dir)?
        .with_retention(config.database.retention_count);
    
    let runtime = tokio::runtime::Runtime::new()?;
    
    let database_source = match args.database {
        Some(Database::UniProt) => {
            let dataset = args.dataset.as_deref().unwrap_or("swissprot");
            match dataset {
                "swissprot" => DatabaseSource::UniProt(UniProtDatabase::SwissProt),
                "trembl" => DatabaseSource::UniProt(UniProtDatabase::TrEMBL),
                "uniref50" => DatabaseSource::UniProt(UniProtDatabase::UniRef50),
                "uniref90" => DatabaseSource::UniProt(UniProtDatabase::UniRef90),
                "uniref100" => DatabaseSource::UniProt(UniProtDatabase::UniRef100),
                "idmapping" => DatabaseSource::UniProt(UniProtDatabase::IdMapping),
                _ => anyhow::bail!("Unknown UniProt dataset: '{}'. Valid options are: swissprot, trembl, uniref50, uniref90, uniref100, idmapping", dataset),
            }
        }
        Some(Database::NCBI) => {
            let dataset = args.dataset.as_deref().unwrap_or("nr");
            match dataset {
                "nr" => DatabaseSource::NCBI(NCBIDatabase::NR),
                "nt" => DatabaseSource::NCBI(NCBIDatabase::NT),
                "refseq-protein" => DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein),
                "refseq-genomic" => DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic),
                "taxonomy" => DatabaseSource::NCBI(NCBIDatabase::Taxonomy),
                "prot-accession2taxid" => DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId),
                "nucl-accession2taxid" => DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId),
                _ => anyhow::bail!("Unknown NCBI dataset: '{}'. Valid options are: nr, nt, refseq-protein, refseq-genomic, taxonomy, prot-accession2taxid, nucl-accession2taxid", dataset),
            }
        }
        Some(Database::PDB) => {
            anyhow::bail!("PDB database download not yet implemented. Please download manually from https://www.rcsb.org/");
        }
        Some(Database::PFAM) => {
            anyhow::bail!("PFAM database download not yet implemented. Please download manually from https://www.ebi.ac.uk/interpro/entry/pfam/");
        }
        Some(Database::Silva) => {
            anyhow::bail!("Silva database download not yet implemented. Please download manually from https://www.arb-silva.de/");
        }
        Some(Database::KEGG) => {
            anyhow::bail!("KEGG database download not yet implemented. Please download manually from https://www.genome.jp/kegg/");
        }
        _ => {
            anyhow::bail!("No database specified");
        }
    };
    
    // Determine source and dataset names for directory structure
    let (source_name, dataset_name, filename) = match &database_source {
        DatabaseSource::UniProt(db) => {
            // Use simple lowercase names for directories
            let dataset = match db {
                UniProtDatabase::SwissProt => "swissprot".to_string(),
                UniProtDatabase::TrEMBL => "trembl".to_string(),
                UniProtDatabase::UniRef50 => "uniref50".to_string(),
                UniProtDatabase::UniRef90 => "uniref90".to_string(),
                UniProtDatabase::UniRef100 => "uniref100".to_string(),
                UniProtDatabase::IdMapping => "idmapping".to_string(),
            };
            let filename = if matches!(db, UniProtDatabase::IdMapping) {
                "idmapping.dat.gz".to_string()
            } else {
                format!("{}.fasta", dataset)
            };
            ("uniprot".to_string(), dataset, filename)
        }
        DatabaseSource::NCBI(db) => {
            // Use simple names for directories, not the Display format
            let dataset = match db {
                NCBIDatabase::NR => "nr".to_string(),
                NCBIDatabase::NT => "nt".to_string(),
                NCBIDatabase::RefSeqProtein => "refseq-protein".to_string(),
                NCBIDatabase::RefSeqGenomic => "refseq-genomic".to_string(),
                NCBIDatabase::Taxonomy => "taxonomy".to_string(),
                NCBIDatabase::ProtAccession2TaxId => "prot-accession2taxid".to_string(),
                NCBIDatabase::NuclAccession2TaxId => "nucl-accession2taxid".to_string(),
            };
            let filename = match db {
                NCBIDatabase::Taxonomy => "taxdump".to_string(),
                NCBIDatabase::ProtAccession2TaxId => "prot.accession2taxid.gz".to_string(),
                NCBIDatabase::NuclAccession2TaxId => "nucl.accession2taxid.gz".to_string(),
                _ => format!("{}.fasta", dataset)
            };
            ("ncbi".to_string(), dataset, filename)
        }
        DatabaseSource::Custom(path) => {
            ("custom".to_string(), "custom".to_string(), PathBuf::from(path).file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("custom.fasta")
                .to_string())
        }
    };
    
    // Prepare the versioned download directory
    let (version_dir, version_date) = if args.output == PathBuf::from(".") {
        // Use centralized directory if no custom output specified
        db_manager.prepare_download(&source_name, &dataset_name)?
    } else {
        // Use user-specified directory
        (args.output.clone(), Local::now().format("%Y-%m-%d").to_string())
    };

    let output_file = version_dir.join(&filename);
    let is_temp_dir = version_dir.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with(".tmp_"))
        .unwrap_or(false);

    // Check if file already exists and is complete
    if output_file.exists() && !is_temp_dir {
        let file_size = std::fs::metadata(&output_file)?.len();
        if file_size > 0 {
            println!("✅ {} already downloaded ({}MB)", filename, file_size / 1_048_576);
            println!("Use --force to re-download");
            return Ok(());
        }
    }

    println!("Downloading {} to {}", database_source, output_file.display());

    let download_result = runtime.block_on(async {
        let mut progress = DownloadProgress::new();
        crate::download::download_database_with_full_options(
            database_source.clone(),
            &output_file,
            &mut progress,
            args.skip_verify,
            args.resume
        ).await
    });

    // Handle download errors with helpful messages
    if let Err(e) = download_result {
        // Check if temp file exists for resume
        let temp_file = output_file.with_extension("tmp");
        if temp_file.exists() && !args.resume {
            let temp_size = std::fs::metadata(&temp_file)?.len();
            eprintln!("\n❌ Download failed: {}", e);
            eprintln!("💡 Partial download exists ({:.2} GB). Try resuming with:",
                     temp_size as f64 / 1_073_741_824.0);
            eprintln!("   talaria database download {} -d {} -r",
                     source_name, dataset_name);
            return Err(e);
        }
        return Err(e);
    }

    // Calculate checksum for integrity (skip for taxonomy which extracts to directory)
    let (checksum, file_size) = if matches!(&database_source, DatabaseSource::NCBI(NCBIDatabase::Taxonomy)) {
        // For taxonomy, we don't have a single file to checksum
        println!("Taxonomy extracted to directory");
        (None, 0)
    } else if output_file.exists() {
        println!("Calculating checksum...");
        let checksum = DatabaseManager::calculate_checksum(&output_file)
            .ok()
            .or(None);
        let size = std::fs::metadata(&output_file)?.len();
        (checksum, size)
    } else {
        (None, 0)
    };

    // Save metadata to versioned directory
    let metadata = DatabaseMetadata {
        source: source_name.clone(),
        dataset: dataset_name.clone(),
        version: version_date.clone(),
        download_date: Utc::now(),
        file_size,
        checksum,
        url: None, // TODO: Track source URL
    };

    let metadata_path = version_dir.join("metadata.json");
    metadata.save(&metadata_path)?;

    // Finalize download if using temp directory
    if args.output == PathBuf::from(".") {
        if is_temp_dir {
            println!("Finalizing download...");
            db_manager.finalize_download(&source_name, &dataset_name, &version_date)?;
        }

        db_manager.update_current_link(&source_name, &dataset_name, &version_date)?;

        // Clean old versions if needed
        let removed = db_manager.clean_old_versions(&source_name, &dataset_name)?;
        if !removed.is_empty() {
            println!("Cleaned up old versions: {:?}", removed);
        }
    }
    
    println!("Download complete: {}", output_file.display());
    Ok(())
}