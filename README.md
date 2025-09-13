# Talaria

**Talaria** - Intelligent FASTA reduction for aligner index optimization

## Overview

Talaria is a high-performance Rust tool that intelligently reduces biological sequence databases (FASTA files) before indexing, optimizing them for use with various aligners like LAMBDA, BLAST, Kraken, Diamond, MMseqs2, and others.

## Features

- **3-5x faster** than traditional approaches through Rust and parallelization
- **60-70% size reduction** without sacrificing biological coverage
- **Taxonomy-aware** clustering and reference selection
- **Multi-aligner support**: Optimized for LAMBDA, BLAST, Kraken, Diamond, MMseqs2
- **Memory efficient**: Streaming architecture for databases of any size
- **Built-in validation**: Quality metrics and coverage analysis

## Quick Start

### Installation

```bash
# Clone and build from source
git clone https://github.com/brett/talaria
cd talaria
cargo build --release

# Install to PATH
cargo install --path .
```

### Basic Usage

```bash
# Reduce a FASTA file (default: 30% of original size)
talaria reduce -i input.fasta -o reduced.fasta

# Optimize for specific aligner
talaria reduce -i input.fasta -o reduced.fasta --target-aligner lambda

# Custom reduction ratio
talaria reduce -i input.fasta -o reduced.fasta -r 0.4

# Save delta metadata for reconstruction
talaria reduce -i input.fasta -o reduced.fasta -m deltas.dat
```

### Integration with LAMBDA

```bash
# Step 1: Reduce the database
talaria reduce \
  -i uniprot_sprot.fasta \
  -o uniprot_reduced.fasta \
  --target-aligner lambda \
  -r 0.3

# Step 2: Build LAMBDA index from reduced FASTA
lambda2 mkindexp \
  -d uniprot_reduced.fasta \
  --acc-tax-map idmapping.dat.gz \
  --tax-dump-dir tax-dump/

# Step 3: Search as normal
lambda2 searchp \
  -q queries.fasta \
  -i uniprot_reduced.lambda \
  -o results.m8
```

## Commands

### `reduce`
Reduce a FASTA file for optimal indexing

```bash
talaria reduce [OPTIONS] --input <FILE> --output <FILE>

Options:
  -i, --input <FILE>              Input FASTA file (supports .gz)
  -o, --output <FILE>             Output reduced FASTA (supports .gz)
  -a, --target-aligner <NAME>     Target aligner (lambda, blast, kraken, diamond, mmseqs2, generic)
  -r, --reduction-ratio <N>       Target size ratio (0.0-1.0) [default: 0.3]
  -m, --metadata <FILE>           Save delta metadata for reconstruction
  -c, --config <FILE>             Configuration file (or set TALARIA_CONFIG env var)
  --min-length <LENGTH>           Minimum sequence length [default: 50]
  --protein                       Force protein scoring
  --nucleotide                    Force nucleotide scoring
  --skip-validation               Skip validation step
```

### `stats`
Show statistics about a FASTA file or reduction

```bash
talaria stats -i <FILE> [-d <DELTAS>] [--format <FORMAT>]
```

### `validate`
Validate reduction quality against original

```bash
talaria validate -o <ORIGINAL> -r <REDUCED> -d <DELTAS>
```

### `reconstruct`
Reconstruct sequences from references and deltas

```bash
talaria reconstruct -r <REFERENCES> -d <DELTAS> -o <OUTPUT> [--sequences <ID>...]
```

### `download`
Download biological databases (currently supports UniProt and NCBI)

```bash
talaria download [DATABASE] [OPTIONS]

Supported databases:
  uniprot - UniProt/SwissProt/TrEMBL databases
  ncbi    - NCBI nr/nt/RefSeq databases
  
Note: PDB, PFAM, Silva, and KEGG databases are not yet implemented
```

## Algorithm

Talaria uses a multi-phase approach:

1. **Reference Selection**: Greedy selection of longest sequences as representatives
2. **Similarity Clustering**: Group similar sequences using k-mer overlap
3. **Delta Encoding**: Encode child sequences as compact deltas from references
4. **Optimization**: Target-specific optimizations for different aligners

## Configuration

Create a `talaria.toml` file:

```toml
[reduction]
target_ratio = 0.3
min_sequence_length = 50
similarity_threshold = 0.9
taxonomy_aware = true

[alignment]
gap_penalty = 20
gap_extension = 10

[performance]
chunk_size = 10000
cache_alignments = true
```

## Performance

Typical results on UniProt/SwissProt (565,928 sequences):

- **Input size**: 204 MB
- **Output size**: 61 MB (70% reduction)
- **References**: 169,778 sequences
- **Processing time**: 12 minutes (16 cores)
- **Memory usage**: 4.2 GB peak
- **Coverage**: 99.8% sequences, 98.5% taxa

## Documentation

Full documentation is available in the `docs/` directory. Build with mdbook:

```bash
cd docs
mdbook build
mdbook serve --open
```

## Development

This is a Rust rewrite of the original [db-reduce](https://github.com/brett/aegis-research/tree/main/db-reduce) C++ implementation, with significant improvements in performance, usability, and maintainability.

### Building from Source

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run with verbose output
RUST_LOG=debug cargo run -- reduce -i input.fasta -o output.fasta
```

## License

MIT

## Citation

If you use Talaria in your research, please cite:

```
Talaria: Intelligent FASTA Reduction for Aligner Index Optimization
https://github.com/brett/talaria
```

## Acknowledgments

Based on the original db-reduce implementation from the AEGIS research project.
