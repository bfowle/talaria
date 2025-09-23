# Database Management Guide

Talaria provides comprehensive database management using the Sequence Query Optimization with Indexed Architecture (SEQUOIA) system for efficient incremental updates.

## How SEQUOIA Works

| Aspect | SEQUOIA Benefit |
|--------|-------------|
| Initial Download | 100GB split into chunks |
| Daily Updates | ~1GB (only changed chunks) |
| Storage (1 year) | ~100GB (deduplicated) |
| Update Check | 100KB manifest |
| Deduplication | Automatic 30-50% |
| Verification | Cryptographic proofs |

## Key Features

- **Content-Addressed Storage**: Immutable chunks with SHA256 addressing
- **Incremental Updates**: Only download changed chunks
- **Bi-Temporal Versioning**: Track sequence and taxonomy changes independently
- **Cryptographic Verification**: Merkle DAG ensures integrity
- **Smart Chunking**: Group sequences by taxonomy for better compression

## Directory Structure

### SEQUOIA Directory Structure
```
${TALARIA_HOME}/databases/
├── manifests/                      # Database-specific manifest files
│   ├── uniprot-swissprot.json     # SwissProt manifest (filename uses -)
│   ├── ncbi-nr.json                # NR database manifest
│   └── custom-mydb.json            # Custom database manifests
├── profiles/                       # Reduction profiles
│   ├── 30-percent                 # Hash reference to 30% reduction manifest
│   ├── 50-percent                 # Hash reference to 50% reduction manifest
│   ├── auto-detect                # Auto-detected reduction profile
│   └── blast-optimized            # Custom named profile
├── chunks/                         # Content-addressed chunk storage
│   ├── ab/                         # Two-letter prefix directories
│   │   └── abc123...               # SHA256-named chunk files
│   └── de/
│       └── def456...
├── taxonomy/                       # Unified taxonomy directory
│   ├── 20250917_202728/            # Versioned taxonomy snapshot
│   │   ├── tree/                   # Core taxonomy tree (NCBI taxdump)
│   │   │   ├── nodes.dmp
│   │   │   ├── names.dmp
│   │   │   └── ...
│   │   ├── mappings/               # Accession-to-taxid mappings
│   │   │   ├── prot.accession2taxid.gz
│   │   │   ├── nucl.accession2taxid.gz
│   │   │   └── uniprot_idmapping.dat.gz
│   │   └── manifest.json
│   └── current -> 20250917_202728  # Symlink to current version
└── versions/                       # Versioned database data
    ├── uniprot/
    │   └── swissprot/
    └── ncbi/
        └── nr/
```

**Note on Naming Conventions:**
- Database references use "/" separator: `uniprot/swissprot`, `custom/mydb`
- Manifest filenames use "-" separator: `uniprot-swissprot.json`
- Reduction profiles are stored separately, not as new databases


## Database Download

### How It Works

The `database download` command intelligently handles both initial downloads and updates:

```bash
# First time - downloads entire database
talaria database download uniprot -d swissprot
# Downloads all chunks, creates manifest

# Run again - automatically checks for updates
talaria database download uniprot -d swissprot
# Output: "Database is already up to date!" or "Updated: 5 new chunks"
```

### What Happens Behind the Scenes

1. **First Download**:
   - Downloads manifest (~100KB)
   - Downloads all chunks (e.g., 200MB as ~50 chunks)
   - Stores with deduplication and compression
   - Creates database-specific manifest

2. **Subsequent Runs**:
   - Checks local manifest
   - Compares with source (if available)
   - Downloads only changed chunks
   - Updates manifest

For large databases, the savings are massive:
- SwissProt update: ~5MB instead of 200MB
- NR update: ~1GB instead of 100GB

### Database Commands

```bash
# Download database (initial or update)
talaria database download uniprot -d swissprot

# Add custom FASTA to SEQUOIA
talaria database add -i sequences.fasta --source mylab --dataset proteins

# List downloaded databases
talaria database list

# Show database information
talaria database info uniprot/swissprot

# List sequences from a database
talaria database list-sequences uniprot/swissprot --limit 100

# Update taxonomy data
talaria database update-taxonomy
```


## Supported Databases

### UniProt

| Dataset | Size | Description | Command |
|---------|------|-------------|---------|
| SwissProt | ~200MB | Manually reviewed sequences | `--dataset swissprot` |
| TrEMBL | ~100GB | Unreviewed sequences | `--dataset trembl` |
| UniRef100 | ~50GB | Clustered at 100% identity | `--dataset uniref100` |
| UniRef90 | ~20GB | Clustered at 90% identity | `--dataset uniref90` |
| UniRef50 | ~8GB | Clustered at 50% identity | `--dataset uniref50` |

**Example: Download SwissProt with taxonomy mapping**
```bash
talaria database download uniprot \
  -d swissprot \
  --taxonomy
# Downloads to: ${TALARIA_HOME}/databases/data/uniprot/swissprot/YYYY-MM-DD/
```

### NCBI

| Dataset | Size | Description | Command |
|---------|------|-------------|---------|
| nr | ~90GB | Non-redundant proteins | `--dataset nr` |
| nt | ~70GB | Nucleotide sequences | `--dataset nt` |
| RefSeq Proteins | ~20GB | RefSeq protein database | `--dataset refseq-protein` |
| RefSeq Genomes | Varies | Complete genomes | `--dataset refseq-genomic` |
| Taxonomy | ~50MB | NCBI taxonomy dump | `--dataset taxonomy` |

**Example: Download nr with taxonomy**
```bash
# Download nr database
talaria database download ncbi -d nr

# Download taxonomy separately
talaria database download ncbi -d taxonomy
```

### PDB, PFAM, Silva, KEGG

These databases are recognized but not yet fully implemented. Coming in future versions.

## Advanced Download Options

### Resume Interrupted Downloads

```bash
talaria database download uniprot \
  -d trembl \
  --resume
```

### Parallel Downloads

*Note: Parallel download of multiple datasets is planned for a future version.*

### Checksum Verification

```bash
# Skip checksum verification (faster but less safe)
talaria download \
  --database uniprot \
  --dataset swissprot \
  --skip-verify
```

## Automatic Processing

*Note: Automatic processing pipelines are planned for a future version. For now, download and process in separate steps:*

```bash
# Step 1: Download
talaria download --database uniprot --dataset swissprot

# Step 2: Reduce
talaria reduce -i swissprot.fasta -o swissprot_reduced.fasta -a lambda
```

## Configuration

Database download settings are currently hardcoded. Custom configuration support is planned for a future version.

## Database URLs

### Current UniProt URLs (auto-updated)
- SwissProt: `https://ftp.uniprot.org/pub/databases/uniprot/current_release/knowledgebase/complete/uniprot_sprot.fasta.gz`
- TrEMBL: `https://ftp.uniprot.org/pub/databases/uniprot/current_release/knowledgebase/complete/uniprot_trembl.fasta.gz`
- Taxonomy mapping: `https://ftp.uniprot.org/pub/databases/uniprot/current_release/knowledgebase/idmapping/idmapping.dat.gz`

### Current NCBI URLs
- nr: `https://ftp.ncbi.nlm.nih.gov/blast/db/FASTA/nr.gz`
- nt: `https://ftp.ncbi.nlm.nih.gov/blast/db/FASTA/nt.gz`
- Taxonomy: `https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/taxdump.tar.gz`
- Accession2Taxid: `https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/accession2taxid/prot.accession2taxid.gz`

## Unified Taxonomy System

Talaria uses a unified taxonomy directory structure that consolidates all taxonomy-related data into a single versioned location:

```bash
# Download complete NCBI taxonomy (includes taxdump)
talaria database download ncbi/taxonomy

# The taxonomy is stored in:
# ~/.talaria/databases/taxonomy/
#   ├── 20250917_202728/            # Versioned snapshot
#   │   ├── tree/                   # NCBI taxdump files
#   │   │   ├── nodes.dmp
#   │   │   ├── names.dmp
#   │   │   └── ...
#   │   └── mappings/               # Accession mappings
#   │       ├── ncbi_prot.accession2taxid.gz
#   │       └── uniprot_idmapping.dat.gz
#   └── current -> 20250917_202728  # Symlink to current version
```

### Benefits of Unified Taxonomy

- **Consistency**: All databases use the same taxonomy version
- **Efficiency**: No duplicate taxonomy files
- **Versioning**: Track taxonomy updates independently
- **Tool Compatibility**: Works with LAMBDA, DIAMOND, Kraken2, etc.

### For LAMBDA

LAMBDA automatically detects taxonomy in the unified location:

```bash
# LAMBDA will use:
# ~/.talaria/databases/taxonomy/current/tree/       # taxdump
# ~/.talaria/databases/taxonomy/current/mappings/   # accessions

# Build LAMBDA index with taxonomy
lambda2 mkindexp \
  -d reduced.fasta \
  --acc-tax-map ~/.talaria/databases/taxonomy/current/mappings/prot.accession2taxid.gz \
  --tax-dump-dir ~/.talaria/databases/taxonomy/current/tree/
```

### For DIAMOND

```bash
# Point DIAMOND to unified taxonomy
diamond makedb --in reduced.fasta --db reduced \
  --taxonmap ~/.talaria/databases/taxonomy/current/mappings/prot.accession2taxid \
  --taxonnodes ~/.talaria/databases/taxonomy/current/tree/nodes.dmp \
  --taxonnames ~/.talaria/databases/taxonomy/current/tree/names.dmp

# Build Diamond database with taxonomy
diamond makedb --in sequences.fasta --db sequences \
  --taxonmap prot.accession2taxid \
  --taxonnodes nodes.dmp \
  --taxonnames names.dmp
```

### For Kraken2

```bash
# Kraken2 has its own database download system
kraken2-build --download-taxonomy --db kraken2_db
kraken2-build --download-library bacteria --db kraken2_db

# Or use Talaria to download and convert
talaria download --database ncbi --dataset nr.gz
talaria convert --input nr.gz --output kraken2_format --format kraken2
```

## Database Management Commands

### List Databases

```bash
# List all downloaded databases
talaria database list

# Show detailed information
talaria database list --detailed

# Show all versions (not just current)
talaria database list --all-versions

# List specific database versions
talaria database list --database uniprot/swissprot
```

### Update Databases

```bash
# Check for updates and download if available (same as initial download)
talaria database download uniprot -d swissprot

# The download command automatically:
# - Detects if database exists
# - Checks for updates
# - Downloads only changes
# - Reports status clearly

# Resume interrupted download
talaria database download uniprot -d swissprot --resume
```

### Storage Management

```bash
# View SEQUOIA repository statistics
talaria sequoia stats

# Initialize SEQUOIA if not already done
talaria sequoia init

# Future: Garbage collection for unused chunks
# talaria sequoia gc  # Not yet implemented
```

### Compare Database Versions

```bash
# Compare current with previous version
talaria database diff uniprot/swissprot

# Compare specific versions
talaria database diff uniprot/swissprot@2024-01-01 uniprot/swissprot@2024-02-01

# Compare with file path
talaria database diff uniprot/swissprot /path/to/other.fasta

# Generate detailed report
talaria database diff uniprot/swissprot --detailed --output report.html
```

### Get Database Info

```bash
# Show database statistics
talaria database info uniprot/swissprot/current/swissprot.fasta

# Include taxonomic distribution
talaria database info database.fasta --taxonomy

# Output as JSON
talaria database info database.fasta --format json
```

## Configuration

### Database Settings

Configure database management in `talaria.toml`:

```toml
[database]
# Base directory for databases (default: ${TALARIA_HOME}/databases/data/)
database_dir = "/data/talaria/databases"

# Number of old versions to keep (0 = keep all)
retention_count = 3

# Automatically check for updates
auto_update_check = false

# Preferred mirror for downloads
preferred_mirror = "ebi"  # or "uniprot", "ncbi"
```

### Environment Variables

```bash
# Override database directory
export TALARIA_DB_DIR=/custom/path/databases

# Set retention policy
export TALARIA_RETENTION=5
```

## Storage Recommendations

### Disk Space Planning

| Database | Original | After Reduction (30%) | With Index |
|----------|----------|----------------------|------------|
| SwissProt | 200 MB | 60 MB | 150 MB |
| nr | 90 GB | 27 GB | 40 GB |
| nt | 70 GB | 21 GB | 35 GB |
| UniRef90 | 20 GB | 6 GB | 10 GB |

## Troubleshooting

### Slow Downloads

```bash
# Downloads use default settings
talaria download --database uniprot --dataset swissprot
```

### Checksum Failures

```bash
# Re-download (overwrites existing)
talaria download --database uniprot --dataset swissprot

# Checksums are automatically verified when available
```

### Disk Space Issues

```bash
# Download to external drive
talaria download --database ncbi --dataset nr \
  --output /mnt/external/databases/

# For very large files, ensure sufficient disk space
# Streaming/chunked processing is planned for future versions
```

## See Also

- [UniProt Guide](./uniprot-guide.md)
- [NCBI Guide](./ncbi-guide.md)
- [Taxonomy Setup](./taxonomy-setup.md)
- [Database Management](./management.md)