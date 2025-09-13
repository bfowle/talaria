# Quick Start

Get up and running with Talaria in minutes! This guide shows you how to use Talaria's powerful database management and reduction features.

## Installation

```bash
# Quick install via cargo
cargo install talaria

# Or build from source
git clone https://github.com/yourusername/talaria
cd talaria
cargo build --release
sudo cp target/release/talaria /usr/local/bin/
```

## Database Management Workflow (Recommended)

Talaria provides a centralized database management system that makes working with biological databases effortless.

### 1. Download a Database

```bash
# Download UniProt SwissProt (automatically versioned and stored)
talaria database download --database uniprot -d swissprot

# Download NCBI NR with resume support
talaria database download --database ncbi -d nr --resume

# List available databases
talaria database download --list-datasets
```

### 2. Reduce the Database

```bash
# Reduce stored database (auto-saves in database structure)
talaria reduce uniprot/swissprot --profile blast-optimized -r 0.3

# Create multiple reduction profiles for different use cases
talaria reduce uniprot/swissprot --profile diamond-fast -a diamond -r 0.25
talaria reduce uniprot/swissprot --profile sensitive-search -r 0.7
```

### 3. List and Manage Databases

```bash
# View all databases and their reductions
talaria database list --show-reduced

# Example output:
# ┌────────────────────┬──────────┬──────────┬──────────────────────┬──────────┐
# │ Database           │ Version  │ Size     │ Modified             │ Versions │
# ├────────────────────┼──────────┼──────────┼──────────────────────┼──────────┤
# │ uniprot/swissprot  │ 2025-09-12 │ 268 MiB  │ 2025-09-12 14:30:00 │ 3        │
# │   └─ blast-optimized │ 30%    │ 80 MiB   │                      │ 45K seqs │
# │   └─ diamond-fast   │ 25%      │ 67 MiB   │                      │ 38K seqs │
# └────────────────────┴──────────┴──────────┴──────────────────────┴──────────┘
```

### 4. Validate Reduction Quality

```bash
# Validate a stored reduction
talaria validate uniprot/swissprot:blast-optimized

# Generate detailed report
talaria validate uniprot/swissprot:blast-optimized --report validation.json
```

### 5. Use in Alignment

```bash
# The reduced database is at a predictable location
REDUCED_DB=~/.talaria/databases/data/uniprot/swissprot/current/reduced/blast-optimized/swissprot.fasta

# Build BLAST database
makeblastdb -in $REDUCED_DB -dbtype prot -out swissprot_blast

# Run BLAST search
blastp -db swissprot_blast -query queries.fasta -out results.txt
```

### 6. Reconstruct When Needed

```bash
# Reconstruct full sequences from reduction
talaria reconstruct uniprot/swissprot:blast-optimized -o full_sequences.fasta

# Reconstruct specific sequences only
talaria reconstruct uniprot/swissprot:blast-optimized --sequences P12345,Q67890
```

## Traditional File-Based Workflow

You can still use Talaria with individual files if preferred:

### 1. Basic Reduction

```bash
# Simple 30% reduction
talaria reduce -i database.fasta -o reduced.fasta

# With metadata for reconstruction
talaria reduce -i database.fasta -o reduced.fasta -m deltas.tal
```

### 2. Validate Quality

```bash
# Validate the reduction
talaria validate \
    -o database.fasta \
    -r reduced.fasta \
    -d deltas.tal
```

### 3. Reconstruct

```bash
# Reconstruct original sequences
talaria reconstruct \
    -r reduced.fasta \
    -d deltas.tal \
    -o reconstructed.fasta
```

## Common Use Cases

### Team Collaboration

Set up a shared database directory for your team:

```bash
# Configure shared directory
export TALARIA_DATABASE_DIR=/shared/team/databases

# Team member 1: Download and reduce
talaria database download --database uniprot -d trembl
talaria reduce uniprot/trembl --profile team-standard -r 0.3

# Team member 2: Use the same reduction
talaria search \
    -d /shared/team/databases/uniprot/trembl/current/reduced/team-standard/trembl.fasta \
    -q my_queries.fasta
```

### Multiple Aligner Support

Create optimized reductions for different aligners:

```bash
# BLAST optimization
talaria reduce ncbi/nr --profile blast-30 -a blast -r 0.3

# DIAMOND optimization  
talaria reduce ncbi/nr --profile diamond-25 -a diamond -r 0.25

# MMseqs2 optimization
talaria reduce ncbi/nr --profile mmseqs-40 -a mmseqs2 -r 0.4

# List all reductions
talaria database list --database ncbi/nr --show-reduced
```

### Version Management

Track database changes over time:

```bash
# Compare current with previous version
talaria database diff uniprot/swissprot

# Compare specific versions
talaria database diff \
    uniprot/swissprot@2024-01-01 \
    uniprot/swissprot@2024-02-01 \
    --format html -o changes.html

# Clean old versions (keeps 3 by default)
talaria database clean uniprot/swissprot
```

## Interactive Mode

For a guided experience, use interactive mode:

```bash
# Launch interactive interface
talaria interactive
```

Navigate with:
- `↑/↓` or `j/k` - Move selection
- `Enter` - Select option
- `Tab` - Switch tabs
- `Esc` or `q` - Exit

## Performance Tips

1. **Use stored databases**: Avoid re-reading large files
   ```bash
   talaria reduce uniprot/swissprot --profile fast -r 0.2
   ```

2. **Parallel processing**: Use all available cores
   ```bash
   talaria -j 0 reduce ncbi/nr --profile parallel -r 0.3
   ```

3. **Skip validation for speed**: When you trust the process
   ```bash
   talaria reduce uniprot/trembl --profile quick --skip-validation
   ```

4. **No deltas for one-way reduction**: When reconstruction isn't needed
   ```bash
   talaria reduce ncbi/nt --profile one-way --no-deltas -r 0.2
   ```

## Configuration

Create a configuration file for consistent settings:

```toml
# ~/.talaria/config.toml
[database]
database_dir = "/data/talaria/databases"
retention_count = 5

[reduction]
min_sequence_length = 100
similarity_threshold = 0.95

[performance]
max_memory_gb = 16
parallel_io = true
```

## Next Steps

- Read the [Basic Usage Guide](basic-usage.md) for detailed explanations
- Explore [Workflow Examples](../workflows/) for specific aligners
- Learn about [Team Collaboration](../../team-collaboration.md)
- Check the [CLI Reference](../api/cli.md) for all options

## Getting Help

```bash
# View help for any command
talaria help
talaria reduce --help
talaria database --help

# Check version
talaria --version

# Enable verbose output for debugging
talaria -vv reduce uniprot/swissprot --profile debug
```