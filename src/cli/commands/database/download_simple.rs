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
        "pdb" => anyhow::bail!("PDB database download not yet implemented"),
        "pfam" => anyhow::bail!("PFAM database download not yet implemented"),
        "silva" => anyhow::bail!("Silva database download not yet implemented"),
        "kegg" => anyhow::bail!("KEGG database download not yet implemented"),
        _ => anyhow::bail!("Unknown database source: '{}'", source),
    };

    // Always use content-addressed storage
    super::download_impl::run_database_download(args, database_source)
}