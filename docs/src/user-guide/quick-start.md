# Quick Start - 3 Minutes to Success

Get Talaria running and see results immediately. No complex setup, just dive right in!

## Install (30 seconds)

```bash
# From source (recommended)
cargo build --release
./target/release/talaria --version

# Or install globally
cargo install talaria
```

## Dive Right In (2.5 minutes)

```bash
# 1. One-time setup (5 seconds)
talaria sequoia init

# 2. Download SwissProt database (small, ~200MB, perfect for testing)
talaria database download uniprot -d swissprot

# 3. Reduce it intelligently (auto-detects optimal size)
talaria reduce uniprot/swissprot -o reduced.fasta

# Done! You've just reduced a database. Use it with any aligner:
lambda3 mkindexp -d reduced.fasta
```

## What Just Happened?

- **SEQUOIA initialized**: Smart storage system that only downloads changes in the future
- **Downloaded SwissProt**: In chunks, ready for instant updates
- **Intelligently reduced**: Auto-detected optimal representatives using alignment analysis
- **Ready to use**: Works with LAMBDA, BLAST, Diamond, MMseqs2, etc.

## Next: Real Workflows

### For LAMBDA Users
```bash
# Auto-detection optimized for LAMBDA
talaria reduce uniprot/swissprot -a lambda -o lambda_db.fasta
lambda3 mkindexp -d lambda_db.fasta
lambda3 searchp -q queries.fasta -d lambda_db.fasta
```

### For Large Databases (NCBI nr)
```bash
# Download nr (warning: ~100GB, but only downloaded once!)
talaria database download ncbi -d nr

# Later, check for updates (same command, only downloads changes ~1GB)
talaria database download ncbi -d nr

# Reduce intelligently for your aligner
talaria reduce ncbi/nr -a diamond -o nr_reduced.fasta
# Or specify exact size if needed: -r 0.25
```

### Custom Databases
```bash
talaria database add -i mysequences.fasta --source mylab --dataset proteins
talaria reduce mylab/proteins -o my_reduced.fasta  # Auto-detects optimal reduction
```


## Common Commands

### View What You Have
```bash
# List databases
talaria database list

# View database info
talaria database info uniprot/swissprot

# Check SEQUOIA storage
talaria sequoia stats

# List sequences
talaria database list-sequences uniprot/swissprot --limit 10
```

### Optimize for Different Aligners
```bash
# Auto-detection adapts to each aligner's characteristics
talaria reduce uniprot/swissprot -a blast -o blast_db.fasta
talaria reduce uniprot/swissprot -a diamond -o diamond_db.fasta
talaria reduce uniprot/swissprot -a mmseqs2 -o mmseqs_db.fasta

# Or use fixed ratios for specific size requirements:
# talaria reduce uniprot/swissprot -r 0.3 -a blast -o blast_db.fasta
# talaria reduce uniprot/swissprot -r 0.25 -a diamond -o diamond_db.fasta

# For taxonomically diverse datasets, weight alignment scores by taxonomy:
# talaria reduce uniprot/trembl --use-taxonomy-weights -a diamond -o trembl_tax.fasta
```

## Tips for Success

### Start Small
- Use SwissProt (~200MB) for testing, not nr (~100GB)
- Let auto-detection find optimal reduction (no -r flag needed)
- Use `-a <aligner>` to optimize for your specific tool
- Add `-r 0.3` only if you need a specific target size

### Storage Location
```bash
# Default location
${TALARIA_HOME}/databases/

# Change it using environment variables
export TALARIA_DATABASES_DIR=/fast/ssd/talaria-databases
talaria sequoia init
```

### Use More Cores
```bash
# Use 16 threads
talaria -j 16 reduce ncbi/nr -o output.fasta  # Auto-detection with 16 threads
```

## Why SEQUOIA? The Update Problem Solved

Traditional approach downloads entire databases repeatedly:
- **Day 1**: Download 100GB nr database
- **Day 2**: Download 100GB again (99.9% unchanged!)
- **Year**: 36.5TB bandwidth wasted

CAGS approach:
- **Day 1**: Download 100GB (once)
- **Day 2**: Download 1GB of changes
- **Year**: ~100GB total (365× less!)

```bash
# This command is smart:
talaria database download ncbi -d nr
# First run: Downloads everything
# Future runs: Only downloads changes!
```

## Full Example: SwissProt to LAMBDA

```bash
# Complete workflow in 5 commands
talaria sequoia init
talaria database download uniprot -d swissprot
talaria reduce uniprot/swissprot -a lambda -o lambda_db.fasta  # Auto-detects optimal size
lambda3 mkindexp -d lambda_db.fasta
lambda3 searchp -q your_queries.fasta -d lambda_db.fasta -o results.m8

# Tomorrow, update with one command:
talaria database download uniprot -d swissprot  # Only downloads changes!
```

## Learn More

- [Basic Usage Guide](basic-usage.md) - Detailed explanations
- [CLI Reference](../api/cli-reference.md) - All commands and options
- [Troubleshooting](../sequoia/troubleshooting.md) - Common issues

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