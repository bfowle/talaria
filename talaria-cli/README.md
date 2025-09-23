# talaria-cli

Command-line interface for the Talaria sequence reduction system.

## Overview

This crate provides the CLI application for Talaria, offering:

- **Database Reduction**: Intelligent FASTA reduction for faster indexing
- **Database Management**: Download, update, and manage biological databases
- **SEQUOIA Operations**: Content-addressed storage management
- **Visualization**: Rich terminal UI and HTML reports
- **Tool Integration**: Seamless integration with aligners

## Installation

```bash
# From source
cargo install --path talaria-cli

# Or build locally
cargo build --release
cp target/release/talaria /usr/local/bin/
```

## Commands

### reduce
Reduce FASTA files for optimal indexing:

```bash
# Basic reduction (30% of original)
talaria reduce -i input.fasta -o reduced.fasta

# Target specific aligner
talaria reduce -i input.fasta -o reduced.fasta --target-aligner lambda

# Custom reduction ratio
talaria reduce -i input.fasta -o reduced.fasta -r 0.4

# With HTML report
talaria reduce -i input.fasta -o reduced.fasta --html-report report.html

# Fetch and reduce by TaxID
talaria reduce --taxids "9606,10090" -o human_mouse.fasta

# From TaxID file
talaria reduce --taxid-list organisms.txt -o reduced.fasta
```

### database
Manage biological databases:

```bash
# List available databases
talaria database list

# Download database
talaria database download uniprot/swissprot

# Update databases
talaria database update --all

# Add custom database
talaria database add mydb /path/to/fasta.gz

# Get database info
talaria database info uniprot/swissprot

# Export sequences by TaxID
talaria database export uniprot/swissprot --taxids 9606 -o human.fasta
```

### chunk
SEQUOIA chunking operations:

```bash
# Chunk a FASTA file
talaria chunk create -i sequences.fasta -o chunks/

# Inspect chunks
talaria chunk inspect chunks/manifest.json

# Verify integrity
talaria chunk verify chunks/

# List chunks
talaria chunk list chunks/
```

### reconstruct
Reconstruct original sequences:

```bash
# From reduced FASTA and metadata
talaria reconstruct -i reduced.fasta -m deltas.dat -o original.fasta

# From SEQUOIA chunks
talaria reconstruct --from-chunks chunks/ -o reconstructed.fasta
```

### stats
Analyze sequences and reductions:

```bash
# Basic statistics
talaria stats input.fasta

# Compare original and reduced
talaria stats compare original.fasta reduced.fasta

# Detailed analysis
talaria stats analyze reduced.fasta --verbose
```

### interactive
Interactive terminal UI:

```bash
# Launch interactive mode
talaria interactive

# Features:
# - Visual database browser
# - Real-time reduction preview
# - Progress monitoring
# - Configuration editor
```

## Configuration

### Environment Variables
```bash
export TALARIA_HOME=$HOME/.talaria
export TALARIA_CONFIG=$HOME/.talaria/config.toml
export TALARIA_LOG=debug
export TALARIA_THREADS=8
```

### Configuration File
Create `~/.talaria/config.toml`:

```toml
[reduction]
target_ratio = 0.3
min_sequence_length = 50
taxonomy_aware = true

[alignment]
algorithm = "lambda"
gap_penalty = 11
gap_extension = 1

[performance]
chunk_size = 1000000
batch_size = 10000
cache_alignments = true

[database]
retention_count = 3
auto_update_check = false
preferred_mirror = "ebi"
```

## Output Formats

### Reduction Report
HTML reports include:
- Sequence statistics
- Size reduction metrics
- Coverage analysis
- Taxonomy distribution
- Interactive visualizations

### Progress Display
Rich terminal output with:
- Progress bars
- Real-time statistics
- Memory usage
- Time estimates

## Integration Examples

### With LAMBDA
```bash
# Step 1: Reduce database
talaria reduce -i uniprot_sprot.fasta -o reduced.fasta --target-aligner lambda

# Step 2: Build index
lambda3 mkindexp -d reduced.fasta

# Step 3: Search
lambda3 searchp -q queries.fasta -i reduced.lambda -o results.m8
```

### With DIAMOND
```bash
# Reduce for DIAMOND
talaria reduce -i nr.fasta -o nr_reduced.fasta --target-aligner diamond

# Build DIAMOND database
diamond makedb --in nr_reduced.fasta --db nr_reduced

# Search
diamond blastp --db nr_reduced --query queries.fasta --out results.tsv
```

## Performance Tips

1. **Use appropriate reduction ratios**: 0.3 for general, 0.5 for sensitive
2. **Enable taxonomy-aware mode** for better clustering
3. **Adjust batch size** for memory constraints
4. **Use SSD storage** for SEQUOIA chunks
5. **Enable parallel processing** with TALARIA_THREADS

## Troubleshooting

```bash
# Enable debug logging
TALARIA_LOG=debug talaria reduce -i input.fasta -o output.fasta

# Preserve workspace for inspection
TALARIA_PRESERVE_ON_FAILURE=1 talaria reduce -i input.fasta -o output.fasta

# Check version and configuration
talaria --version
talaria config show
```

## Usage

This is the main CLI application. Install it as shown above.

## License

MIT