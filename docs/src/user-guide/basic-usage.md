# Basic Usage

A practical guide to using Talaria for common sequence database reduction tasks.

## Quick Start

### Basic Reduction

Reduce a FASTA file using intelligent auto-detection:

```bash
talaria reduce -i sequences.fasta -o reduced.fasta -a diamond
```

This command:
- **Automatically determines optimal reduction** using alignment-based selection
- Uses LAMBDA aligner if available (most accurate) or k-mer analysis as fallback
- Considers taxonomic relationships and sequence similarity
- Outputs reference sequences and auto-generates delta file for reconstruction
- Achieves optimal balance between size reduction and sequence coverage

### View Statistics

Analyze your FASTA files:

```bash
# Basic statistics
talaria stats -i sequences.fasta

# Visual statistics with charts
talaria stats -i sequences.fasta --visual

# Compare original vs reduced
talaria stats -i reduced.fasta -d deltas.tal
```

### Interactive Mode

Launch the interactive TUI:

```bash
talaria interactive
```

Navigate menus to:
- Download databases
- Run reduction wizard
- View statistics
- Configure settings

## Reduction Methods

### Default: Intelligent Auto-Detection (Recommended)

When no reduction ratio (-r) is specified, Talaria uses intelligent auto-detection:
- **LAMBDA-based selection**: If LAMBDA is installed, uses accurate alignment scoring
- **K-mer fallback**: Uses efficient k-mer analysis if LAMBDA unavailable
- **Taxonomy-aware**: Automatically considers taxonomic relationships
- **Coverage optimization**: Stops adding references when coverage plateaus
- **Dynamic sizing**: Adapts to your specific dataset characteristics

### Fixed Ratio Reduction

For specific size requirements, use the `-r` flag:

```bash
# Reduce to exactly 30% of original
talaria reduce -i input.fasta -o output.fasta -r 0.3
```

### Advanced Options

| Feature | Flag | Description | When to Use |
|---------|------|-------------|-------------|
| Fixed ratio | `-r <0.0-1.0>` | Exact reduction target | Known size constraints |
| Similarity threshold | `--similarity-threshold <value>` | K-mer similarity clustering | Highly similar sequences |
| Alignment selection | `--align-select` | Force alignment-based selection | Maximum accuracy needed |
| Taxonomy awareness | `--taxonomy-aware` | Enhanced taxonomic grouping | Diverse taxonomic data |
| Low complexity filter | `--low-complexity-filter` | Remove repetitive sequences | Genomic data with repeats |
| Skip deltas | `--no-deltas` | No reconstruction file | Speed over recoverability |

## Common Use Cases

### 1. Reducing a Protein Database

```bash
# Download UniProt SwissProt
talaria download uniprot --dataset swissprot

# Reduce with intelligent auto-detection (recommended)
talaria reduce \
    -i uniprot_sprot.fasta \
    -o sprot_reduced.fasta \
    -a diamond
# Automatically selects optimal references based on sequence alignments

# Alternative: Fixed 30% reduction for specific size requirements
talaria reduce \
    -i uniprot_sprot.fasta \
    -o sprot_reduced.fasta \
    -r 0.3 \
    -a diamond

# Advanced: High-similarity clustering for redundant datasets
talaria reduce \
    -i uniprot_sprot.fasta \
    -o sprot_reduced.fasta \
    --similarity-threshold 0.90 \
    -a diamond
```

### 2. Preparing BLAST Database

```bash
# Reduce nucleotide database with auto-detection (recommended)
talaria reduce \
    -i genomes.fasta \
    -o genomes_reduced.fasta \
    -a blast
# Intelligently selects references covering maximum sequence diversity

# Alternative: For highly similar genomes (e.g., bacterial strains)
talaria reduce \
    -i genomes.fasta \
    -o genomes_reduced.fasta \
    -a blast \
    --similarity-threshold 0.95
# Groups nearly identical sequences together

# Create BLAST database from reduced set
makeblastdb -in genomes_reduced.fasta -dbtype nucl
```

### 3. Optimizing Kraken Database

```bash
# Auto-detection for Kraken (recommended)
talaria reduce \
    -i refseq_bacteria.fasta \
    -o bacteria_reduced.fasta \
    -a kraken
# Automatically balances taxonomic representation

# Enhanced: Explicit taxonomy-aware reduction
talaria reduce \
    -i refseq_bacteria.fasta \
    -o bacteria_reduced.fasta \
    -a kraken \
    --taxonomy-aware
# Ensures each taxonomic group is well-represented

# Build Kraken database from reduced set
kraken2-build --add-to-library bacteria_reduced.fasta --db kraken_db
```

### 4. Clustering Similar Sequences

```bash
# Auto-detect representatives (recommended for unknown datasets)
talaria reduce \
    -i amplicons.fasta \
    -o representatives.fasta
# Automatically finds optimal number of representatives

# Fixed reduction for specific needs
talaria reduce \
    -i amplicons.fasta \
    -o representatives.fasta \
    -r 0.1  # Keep exactly 10% as representatives

# High-similarity clustering for amplicon data
talaria reduce \
    -i amplicons.fasta \
    -o representatives.fasta \
    --similarity-threshold 0.97 \
    --min-length 200
# Groups sequences with >97% similarity
```

### 5. Fast Processing Without Deltas

```bash
# Maximum speed, no reconstruction needed
talaria reduce \
    -i large_database.fasta \
    -o reduced.fasta \
    --no-deltas \
    --skip-validation
# Uses auto-detection but skips delta encoding

# With fixed ratio for predictable output size
talaria reduce \
    -i large_database.fasta \
    -o reduced.fasta \
    -r 0.3 \
    --no-deltas \
    --skip-validation
```

### 6. Handling Long Sequences

```bash
# Auto-detection with alignment length limit
talaria reduce \
    -i whole_genomes.fasta \
    -o genomes_reduced.fasta \
    --max-align-length 5000
# Prevents memory issues with very long sequences

# Fixed reduction with length limit
talaria reduce \
    -i whole_genomes.fasta \
    -o genomes_reduced.fasta \
    --max-align-length 5000 \
    -r 0.4
```

## Input and Output

### Input Formats

Talaria accepts:
- **FASTA** (.fa, .fasta, .fna, .faa)
- **Compressed FASTA** (.fa.gz, .fasta.gz)
- **Multi-FASTA** (multiple sequences per file)

### Output Files

Default output includes:

1. **Reduced FASTA** (`output.fasta`)
   - Contains reference sequences
   - Full sequence data preserved
   - Original headers maintained

2. **Delta File** (`output.deltas.fasta` or as specified with `-m`)
   - Auto-generated based on output filename
   - Contains delta-encoded sequences
   - Required for reconstruction

3. **Statistics** (shown in terminal)
   - Reduction statistics
   - Sequence coverage
   - Size reduction achieved

## Configuration

### Using Config Files

Create `talaria.toml`:

```toml
[reduction]
target_ratio = 0.3
min_sequence_length = 100
similarity_threshold = 0.0  # Disabled by default
taxonomy_aware = false       # Disabled by default

[alignment]
gap_penalty = 20
gap_extension = 10
algorithm = "needleman-wunsch"

[output]
format = "fasta"
compress_output = false
include_metadata = true

[performance]
chunk_size = 10000
batch_size = 1000
cache_alignments = true
```

Use with:

```bash
talaria reduce -c talaria.toml -i input.fa -o output.fa
```

### Environment Variables

```bash
# Set default threads
export TALARIA_THREADS=16

# Set config location
export TALARIA_CONFIG=$HOME/.talaria/config.toml
```

## Command Reference

### Global Options

```bash
talaria [GLOBAL OPTIONS] <COMMAND> [ARGS]

Global Options:
  -v, --verbose     Increase verbosity (can repeat)
  -j, --threads N   Number of threads (0=auto)
  -h, --help        Show help message
```

### Reduce Command

```bash
talaria reduce [OPTIONS] -i INPUT -o OUTPUT
talaria reduce [OPTIONS] [DATABASE]  # For database reduction

Required (file mode):
  -i, --input FILE          Input FASTA file
  -o, --output FILE         Output FASTA file

Required (database mode):
  [DATABASE]                Database to reduce (e.g., "uniprot/swissprot")

Selection Methods:
  (none)                    Auto-detect optimal reduction (recommended)
  -r, --reduction-ratio N   Fixed reduction ratio (0.0-1.0)

Target Optimization:
  -a, --target-aligner NAME Target aligner (blast|lambda|kraken|diamond|mmseqs2|generic)
                           Optimizes for specific search tool [default: generic]

Common Options:
  --min-length N            Minimum sequence length [default: 50]
  -m, --metadata FILE       Delta metadata file (auto-generated if not specified)
  -j, --threads N           Number of threads (0 = all available) [default: 0]
  --skip-validation         Skip validation step
  -v, --verbose            Increase verbosity (can repeat)

Advanced Selection:
  --similarity-threshold N  Enable similarity clustering (0.0-1.0)
  --align-select           Force alignment-based selection
  --taxonomy-aware         Enhanced taxonomy-aware clustering
  --low-complexity-filter  Filter low complexity sequences
  --all-vs-all            Use all-vs-all alignment (Lambda only)

Performance Options:
  --no-deltas             Skip delta encoding (faster, no reconstruction)
  --max-align-length N    Max sequence length for alignment [default: 10000]
  --store                 Store result in database structure

Sequence Type:
  --protein               Use amino acid scoring (auto-detected by default)
  --nucleotide           Use nucleotide scoring (auto-detected by default)
```

### Stats Command

```bash
talaria stats [OPTIONS] -i INPUT

Options:
  -i, --input FILE          Input FASTA file
  -d, --deltas FILE         Delta file (if analyzing reduction)
  --detailed                Show detailed statistics
  --format FORMAT           Output format (text|json|csv)
  --visual                  Show visual charts
  --interactive             Launch interactive viewer
```

### Download Command

```bash
talaria download [DATABASE] [OPTIONS]

Arguments:
  DATABASE                  Database source (uniprot|ncbi|pdb|pfam|silva|kegg)

Options:
  -d, --dataset NAME        Specific dataset to download
  -o, --output DIR          Output directory [default: .]
  -t, --taxonomy            Download taxonomy data
  -r, --resume              Resume incomplete download
  -i, --interactive         Interactive selection mode
  --skip-verify             Skip checksum verification
```

### Reconstruct Command

```bash
talaria reconstruct [OPTIONS] -r REFERENCES -d DELTAS -o OUTPUT

Options:
  -r, --references FILE     Reference FASTA file
  -d, --deltas FILE         Delta metadata file
  -o, --output FILE         Reconstructed output file
  --sequences ID...         Reconstruct specific sequences only
```

## Performance Tips

### Memory Optimization

```bash
# Use fewer threads for lower memory
talaria reduce -i large.fasta -o reduced.fasta -j 4

# Skip delta encoding to reduce memory usage
talaria reduce -i huge.fasta -o reduced.fasta --no-deltas

# Limit alignment length
talaria reduce -i input.fasta -o output.fasta --max-align-length 1000
```

### Speed Optimization

```bash
# Maximum threads
talaria reduce -i input.fasta -o output.fasta -j 0

# Skip delta encoding for speed
talaria reduce -i input.fasta -o output.fasta --no-deltas

# Skip validation
talaria reduce -i input.fasta -o output.fasta --skip-validation
```

## Troubleshooting

### Common Issues

#### Out of Memory

```bash
# Solution 1: Use fewer threads
talaria reduce -i input.fasta -o output.fasta -j 4

# Solution 2: Skip delta encoding
talaria reduce -i input.fasta -o output.fasta --no-deltas

# Solution 3: Reduce max alignment length
talaria reduce -i input.fasta -o output.fasta --max-align-length 500
```

#### Poor Compression

```bash
# Solution 1: Adjust similarity threshold
talaria reduce -i input.fasta -o output.fasta --similarity-threshold 0.8

# Solution 2: Check sequence diversity
talaria stats -i input.fasta --detailed

# Solution 3: Try alignment-based selection
talaria reduce -i input.fasta -o output.fasta --align-select
```

#### Slow Performance

```bash
# Solution 1: Skip delta encoding
talaria reduce -i input.fasta -o output.fasta --no-deltas

# Solution 2: Use more threads
talaria reduce -i input.fasta -o output.fasta -j 0

# Solution 3: Reduce max alignment length
talaria reduce -i input.fasta -o output.fasta --max-align-length 1000
```

## Examples

### Example 1: Bacterial Genome Database

```bash
# Download bacterial genomes
talaria download ncbi --dataset bacteria

# Reduce with taxonomy preservation
talaria reduce \
    -i bacteria.fasta \
    -o bacteria_reduced.fasta \
    --similarity-threshold 0.95 \
    --taxonomy-aware

# Create BLAST database
makeblastdb -in bacteria_reduced.fasta -dbtype nucl

# Search
blastn -query my_sequences.fasta -db bacteria_reduced.fasta
```

### Example 2: Protein Family Analysis

```bash
# Reduce protein family
talaria reduce \
    -i protein_family.fasta \
    -o representatives.fasta \
    --similarity-threshold 0.6

# Analyze results
talaria stats -i representatives.fasta --detailed
```

### Example 3: Metagenome Processing

```bash
# Reduce reference database
talaria reduce \
    -i reference_genomes.fasta \
    -o reference_reduced.fasta \
    -a kraken \
    --taxonomy-aware

# Map reads to reduced database
minimap2 -ax sr reference_reduced.fasta reads.fastq > alignments.sam
```

## Best Practices

1. **Always Validate**: Run validation on a subset before production use
2. **Choose Appropriate Thresholds**: Higher for similar sequences, lower for diverse
3. **Monitor Metrics**: Track compression ratio and search sensitivity
4. **Regular Updates**: Re-reduce databases periodically as they grow
5. **Backup Originals**: Keep original files until validated
6. **Document Settings**: Record parameters used for reproducibility

## See Also

- [Installation](installation.md) - Setup instructions
- [Configuration](configuration.md) - Detailed configuration options
- [Advanced Usage](../advanced/performance.md) - Performance optimization
- [API Reference](../api/cli.md) - Complete command reference