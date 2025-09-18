/// Simplified download command that uses CASG

use crate::cli::commands::database::download::DownloadArgs;
use crate::download::{DatabaseSource, NCBIDatabase, UniProtDatabase};
use crate::utils::database_ref::{parse_database_ref, validate_source, validate_dataset};

pub fn run_direct_download(args: DownloadArgs) -> anyhow::Result<()> {
    // Parse the database reference
    let database_ref = args.database.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No database specified"))?;

    let (source, dataset) = parse_database_ref(database_ref)?;

    // Validate source and dataset
    let source = validate_source(&source)?;
    validate_dataset(source, &dataset)?;

    // Map to internal database source enum
    let database_source = match source {
        "uniprot" => match dataset.as_str() {
            "swissprot" => DatabaseSource::UniProt(UniProtDatabase::SwissProt),
            "trembl" => DatabaseSource::UniProt(UniProtDatabase::TrEMBL),
            "uniref50" => DatabaseSource::UniProt(UniProtDatabase::UniRef50),
            "uniref90" => DatabaseSource::UniProt(UniProtDatabase::UniRef90),
            "uniref100" => DatabaseSource::UniProt(UniProtDatabase::UniRef100),
            "idmapping" => DatabaseSource::UniProt(UniProtDatabase::IdMapping),
            _ => anyhow::bail!("Unknown UniProt dataset: '{}'", dataset),
        },
        "ncbi" => match dataset.as_str() {
            "nr" => DatabaseSource::NCBI(NCBIDatabase::NR),
            "nt" => DatabaseSource::NCBI(NCBIDatabase::NT),
            "refseq-protein" => DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein),
            "refseq-genomic" => DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic),
            "taxonomy" => DatabaseSource::NCBI(NCBIDatabase::Taxonomy),
            "prot-accession2taxid" => DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId),
            "nucl-accession2taxid" => DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId),
            _ => anyhow::bail!("Unknown NCBI dataset: '{}'", dataset),
        },
        "custom" => {
            // Custom databases require TaxIDs to be specified
            if args.taxids.is_none() && args.taxid_list.is_none() {
                anyhow::bail!(
                    "Custom databases require --taxids or --taxid-list to specify which sequences to fetch"
                );
            }
            DatabaseSource::Custom(dataset.clone())
        },
        "pdb" => anyhow::bail!("PDB database download not yet implemented"),
        "pfam" => anyhow::bail!("PFAM database download not yet implemented"),
        "silva" => anyhow::bail!("Silva database download not yet implemented"),
        "kegg" => anyhow::bail!("KEGG database download not yet implemented"),
        _ => anyhow::bail!("Unknown database source: '{}'", source),
    };

    // For custom databases with TaxIDs, use fetch logic
    if matches!(database_source, DatabaseSource::Custom(_)) {
        return run_custom_fetch(args, dataset);
    }

    // Always use content-addressed storage for standard downloads
    super::download_impl::run_database_download(args, database_source)
}

/// Run custom database fetch with TaxIDs using versioned manifest structure
fn run_custom_fetch(args: DownloadArgs, db_name: String) -> anyhow::Result<()> {
    use crate::cli::output::section_header;

    println!();
    section_header(&format!("Creating Custom Database: {}", db_name));

    // Use fetch logic but with proper versioned paths
    run_fetch_with_versioned_paths(args, db_name)
}

/// Run the fetch logic but ensure manifests are saved in versioned structure
fn run_fetch_with_versioned_paths(args: DownloadArgs, db_name: String) -> anyhow::Result<()> {
    use crate::core::database_manager::DatabaseManager;
    use crate::download::DatabaseSource;
    use crate::bio::uniprot::UniProtClient;
    use crate::cli::output::{success, info};

    // Get or parse TaxIDs
    let taxids = if let Some(taxid_list_path) = &args.taxid_list {
        // Read from file
        let content = std::fs::read_to_string(taxid_list_path)?;
        content.lines()
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
        taxids_str.split(',')
            .filter_map(|s| s.trim().parse::<u32>().ok())
            .collect()
    } else {
        anyhow::bail!("No TaxIDs specified");
    };

    if taxids.is_empty() {
        anyhow::bail!("No valid TaxIDs found");
    }

    info(&format!("Fetching sequences for {} TaxIDs", taxids.len()));

    // Initialize database manager with versioned paths
    let mut manager = DatabaseManager::new(None)?;
    let database_source = DatabaseSource::Custom(db_name.clone());

    // Create client and fetch sequences
    let client = UniProtClient::new("https://rest.uniprot.org")?;
    let mut all_sequences = Vec::new();
    let mut total_sequences = 0;

    for taxid in &taxids {
        info(&format!("Fetching TaxID {}...", taxid));

        // Fetch sequences using the client
        let sequences = client.fetch_by_taxid(*taxid)?;

        let count = sequences.len();
        total_sequences += count;
        info(&format!("  Found {} sequences", count));

        all_sequences.extend(sequences);
    }

    if all_sequences.is_empty() {
        anyhow::bail!("No sequences found for the specified TaxIDs");
    }

    info(&format!("Total sequences fetched: {}", total_sequences));

    // Now chunk and store using DatabaseManager's chunk_database method
    // This ensures everything goes through the versioned structure
    let temp_fasta = std::env::temp_dir().join(format!("talaria_fetch_{}.fasta", std::process::id()));

    // Write sequences to temporary FASTA
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp_fasta)?;
        for seq in &all_sequences {
            writeln!(file, ">{}", seq.id)?;
            if let Some(desc) = &seq.description {
                writeln!(file, "{}", desc)?;
            }
            writeln!(file, "{}", String::from_utf8_lossy(&seq.sequence))?;
        }
    }

    // Use DatabaseManager to chunk the database into CASG with versioned paths
    info("Processing into CASG chunks...");
    manager.chunk_database(&temp_fasta, &database_source)?;

    // Clean up temp file
    std::fs::remove_file(&temp_fasta).ok();

    success(&format!("Successfully created custom database: custom/{}", db_name));
    Ok(())
}