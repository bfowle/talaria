# Database Management Guide

Talaria provides comprehensive database management with automatic versioning, centralized storage, and lifecycle management. All databases are stored in a versioned directory structure under `~/.talaria/databases/data/` by default.

## Key Features

- **Centralized Storage**: All databases in one configurable location
- **Automatic Versioning**: Date-based version tracking  
- **Smart References**: Use `source/dataset` or `source/dataset@version`
- **Lifecycle Management**: Update checking and old version cleanup
- **Metadata Tracking**: Download dates, checksums, and source URLs

## Directory Structure

```
~/.talaria/
├── databases/
│   ├── metadata/          # JSON metadata files
│   └── data/              # Actual database files
│       ├── uniprot/
│       │   ├── swissprot/
│       │   │   ├── 2024-01-15/
│       │   │   │   ├── swissprot.fasta
│       │   │   │   └── metadata.json
│       │   │   └── current -> 2024-01-15/  # Symlink to latest
│       └── ncbi/
│           └── nr/
```

## Interactive Download Mode

The easiest way to download databases is using the interactive mode:

```bash
talaria database download --interactive
# or
talaria interactive  # Then select "Download databases"
```

This will guide you through:
1. Selecting a database source (UniProt, NCBI, etc.)
2. Choosing specific datasets
3. Configuring download options
4. Automatic decompression and verification

## Command-Line Download

### Basic Usage

```bash
# Download UniProt SwissProt (automatically versioned)
talaria database download uniprot -d swissprot

# Download NCBI nr database
talaria database download ncbi -d nr

# Download with taxonomy
talaria database download uniprot -d swissprot --taxonomy

# Specify custom output directory (overrides centralized storage)
talaria database download ncbi -d nr --output /data/databases/
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
# Downloads to: ~/.talaria/databases/data/uniprot/swissprot/YYYY-MM-DD/
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

## Taxonomy Setup

### For LAMBDA

```bash
# Download required files
talaria download --database ncbi --dataset taxdump.tar.gz
talaria download --database uniprot --dataset idmapping.dat.gz

# Extract taxonomy files
tar -xzf taxdump.tar.gz nodes.dmp names.dmp

# Build LAMBDA index with taxonomy
lambda2 mkindexp \
  -d reduced.fasta \
  --acc-tax-map idmapping.dat.gz \
  --tax-dump-dir ./
```

### For Diamond

```bash
# Download NCBI taxonomy
talaria download --database ncbi --dataset taxdump.tar.gz
talaria download --database ncbi --dataset prot.accession2taxid.gz

# Extract files
tar -xzf taxdump.tar.gz
gunzip prot.accession2taxid.gz

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
# Check all databases for updates
talaria database update

# Check specific database
talaria database update uniprot/swissprot

# Download updates if available
talaria database update --download

# Force update even if recent
talaria database update uniprot/swissprot --download --force
```

### Clean Old Versions

```bash
# Clean old versions (keeps 3 by default)
talaria database clean

# Keep only 1 old version
talaria database clean --keep 1

# Remove all old versions except current
talaria database clean --all

# Dry run to see what would be deleted
talaria database clean --dry-run

# Clean specific database
talaria database clean uniprot/swissprot
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
# Base directory for databases (default: ~/.talaria/databases/data/)
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