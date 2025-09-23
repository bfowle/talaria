# Database Management Quick Reference

## Custom Databases

### Add Local FASTA Files
```bash
# Add a custom database
talaria database add -i /path/to/sequences.fasta --name "team-proteins"

# Use custom source and dataset
talaria database add -i sequences.fasta \
  --source "myteam" \
  --dataset "proteins-v2" \
  --description "Team protein database v2"

# Keep original file (copy instead of move)
talaria database add -i valuable.fasta --copy

# Replace existing database
talaria database add -i updated.fasta --name "team-proteins" --replace
```

Custom databases work with all standard commands:
- `talaria reduce custom/team-proteins -r 0.3`
- `talaria validate custom/team-proteins:30-percent`
- `talaria reconstruct custom/team-proteins:30-percent`
- `talaria database info custom/team-proteins`

## Download Commands

### UniProt Databases
```bash
# Download SwissProt (manually reviewed)
talaria database download uniprot -d swissprot

# Download TrEMBL (automatically annotated)
talaria database download uniprot -d trembl

# Download UniRef clusters
talaria database download uniprot -d uniref50
talaria database download uniprot -d uniref90
talaria database download uniprot -d uniref100
```

### NCBI Databases
```bash
# Download NR (non-redundant proteins)
talaria database download ncbi -d nr

# Download NT (nucleotides)
talaria database download ncbi -d nt

# Download RefSeq
talaria database download ncbi -d refseq-protein
talaria database download ncbi -d refseq-genomic

# Download Taxonomy
talaria database download ncbi -d taxonomy
```

### Download Options
```bash
# Skip checksum verification (faster)
talaria database download uniprot -d swissprot --skip-verify

# Resume interrupted download
talaria database download ncbi -d nr --resume

# Specify custom output directory
talaria database download uniprot -d swissprot --output /custom/path
```

## List Commands

### Basic Listing
```bash
# List all databases
talaria database list

# Show reduced versions
talaria database list --show-reduced

# Sort by different fields
talaria database list --sort size
talaria database list --sort date
talaria database list --sort name
```

### Detailed Views
```bash
# Detailed information
talaria database list --detailed

# Show all versions (not just current)
talaria database list --all-versions

# Specific database
talaria database list --database uniprot/swissprot

# Combined options
talaria database list --detailed --all-versions --show-reduced
```

## Diff Commands

### Basic Comparisons
```bash
# Compare with previous version
talaria database diff uniprot/swissprot

# Compare two specific versions
talaria database diff uniprot/swissprot@2025-09-10 uniprot/swissprot@2025-09-12

# Compare reduced with original
talaria database diff uniprot/swissprot uniprot/swissprot:30-percent
```

### Advanced Comparisons
```bash
# Headers only (fast)
talaria database diff uniprot/swissprot --headers-only

# Detailed sequence changes
talaria database diff uniprot/swissprot --detailed

# With taxonomic analysis
talaria database diff uniprot/swissprot --taxonomy

# Custom similarity threshold
talaria database diff uniprot/swissprot --similarity-threshold 0.95
```

### Output Formats
```bash
# Text report (default)
talaria database diff uniprot/swissprot

# HTML report with visuals
talaria database diff uniprot/swissprot --format html --visual -o report.html

# JSON for programmatic access
talaria database diff uniprot/swissprot --format json -o diff.json

# CSV for spreadsheets
talaria database diff uniprot/swissprot --format csv -o changes.csv
```

## Reduce Commands

### Basic Reduction
```bash
# Simple reduction to 30%
talaria reduce -i input.fasta -o reduced.fasta -r 0.3

# Target specific aligner
talaria reduce -i input.fasta -o reduced.fasta -r 0.3 --target-aligner blast
```

### Store as Reduction Profile
```bash
# Reduce a database from SEQUOIA repository (creates a profile, not a new database)
talaria reduce custom/cholera -a lambda
# Creates profile: auto-detect

# Reduce with specific ratio (profile name: "30-percent")
talaria reduce uniprot/swissprot -r 0.3
# Creates profile: 30-percent

# Reduce with custom profile name
talaria reduce uniprot/swissprot \
  --profile blast-optimized \
  --reduction-ratio 0.3
# Creates profile: blast-optimized

# List databases shows profiles under "Reductions" column
talaria database list
# Output:
# Database          | Reductions
# custom/cholera    | auto-detect
# uniprot/swissprot | 30-percent, blast-optimized
```

### Advanced Reduction Options
```bash
# Similarity-based clustering
talaria reduce -i input.fasta -o reduced.fasta \
  --similarity-threshold 0.95 \
  --reduction-ratio 0.3

# Taxonomy-aware reduction
talaria reduce -i input.fasta -o reduced.fasta \
  --taxonomy-aware \
  --reduction-ratio 0.5

# Skip delta encoding (faster, no reconstruction)
talaria reduce -i input.fasta -o reduced.fasta \
  --no-deltas \
  --reduction-ratio 0.3

# Filter sequences
talaria reduce -i input.fasta -o reduced.fasta \
  --min-length 100 \
  --low-complexity-filter \
  --reduction-ratio 0.4
```

## Clean Commands

```bash
# Clean old versions (keeps 3 by default)
talaria database clean uniprot/swissprot

# Keep specific number of versions
talaria database clean uniprot/swissprot --keep 5

# Dry run (show what would be deleted)
talaria database clean uniprot/swissprot --dry-run

# Clean all databases
talaria database clean --all
```

## Reference Format

### Database References
```
source/dataset[@version][:reduction]

Examples:
- uniprot/swissprot                    # Current version
- uniprot/swissprot@2025-09-12        # Specific version
- uniprot/swissprot:30-percent        # Reduced version
- uniprot/swissprot@2025-09-12:blast  # Specific version's reduction
```

### Directory Structure
```
${TALARIA_HOME}/databases/data/
├── custom/                    # Custom databases
│   └── team-proteins/
│       └── YYYY-MM-DD/
├── myteam/                    # Custom source
│   └── proteins-v2/
│       └── YYYY-MM-DD/
└── source/                    # Public databases
    └── dataset/
        ├── current -> YYYY-MM-DD
        └── YYYY-MM-DD/
            ├── dataset.fasta
            ├── metadata.json
            └── reduced/
                └── profile-name/
                    ├── dataset.fasta
                    ├── dataset.deltas.tal
                    └── metadata.json
```

## Environment Variables

```bash
# Set database directory
export TALARIA_DATABASE_DIR="/shared/databases"

# Set retention count
export TALARIA_RETENTION_COUNT=5

# Set default config file
export TALARIA_CONFIG="/etc/talaria/config.toml"

# Set thread count
export TALARIA_THREADS=8
```

## Configuration File

```toml
# talaria.toml
[database]
database_dir = "/shared/team/talaria/databases/data"
retention_count = 3
auto_clean = true

[reduction]
min_sequence_length = 50
similarity_threshold = 0.95
taxonomy_aware = false

[download]
verify_checksums = true
resume_on_failure = true
max_retries = 3
timeout_seconds = 300
```

## Common Workflows

### Weekly Database Update
```bash
#!/bin/bash
# Update databases and create standard reductions

# Download latest versions
talaria database download uniprot -d swissprot
talaria database download ncbi -d nr

# Create standard reductions
talaria reduce --store --database uniprot/swissprot \
  --profile blast-30 --reduction-ratio 0.3 \
  --target-aligner blast

talaria reduce --store --database ncbi/nr \
  --profile fast-25 --reduction-ratio 0.25 \
  --target-aligner diamond

# Clean old versions
talaria database clean --all --keep 3

# Generate comparison report
talaria database diff uniprot/swissprot \
  --format html --visual \
  -o weekly-changes.html
```

### Team Setup
```bash
#!/bin/bash
# Initialize shared database directory for team

SHARED_DIR="/nfs/team/talaria/databases"

# Create directory structure
mkdir -p $SHARED_DIR/data
mkdir -p $SHARED_DIR/metadata

# Set permissions
chmod 755 $SHARED_DIR
chmod 775 $SHARED_DIR/data

# Configure talaria
cat > $HOME/.talaria/config.toml << EOF
[database]
database_dir = "$SHARED_DIR/data"
retention_count = 5
EOF

# Initial download
talaria database download uniprot -d swissprot
talaria database download uniprot -d trembl

# Create team-standard reductions
for ratio in 0.1 0.3 0.5 0.7; do
  percent=$(echo "$ratio * 100" | bc | cut -d. -f1)
  talaria reduce --store \
    --database uniprot/swissprot \
    --profile "standard-${percent}" \
    --reduction-ratio $ratio
done
```