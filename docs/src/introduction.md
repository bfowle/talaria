# Talaria

**Talaria** is a high-performance tool for intelligently reducing biological sequence databases (FASTA files) to optimize them for indexing with various aligners like LAMBDA, BLAST, Kraken, Diamond, MMseqs2, and others.

## What is Talaria?

Talaria reduces redundancy in protein and nucleotide databases by:

1. **Selecting representative sequences** as references using intelligent algorithms
2. **Encoding similar sequences** as compact deltas from references  
3. **Outputting reduced FASTA files** that maintain biological coverage while minimizing size
4. **Enabling reconstruction** of full sequences when needed

## Key Features

- **High Performance**: 3-5x faster than traditional approaches through Rust and parallelization
- **Significant Size Reduction**: Achieve 60-70% smaller indices without sacrificing coverage
- **Biology-Aware**: Taxonomy-aware clustering and reference selection
- **Multi-Aligner Support**: Optimized for LAMBDA, BLAST, Kraken, Diamond, MMseqs2, and more
- **Memory Efficient**: Streaming architecture handles databases of any size
- **Quality Validation**: Built-in tools to validate reduction quality
- **Comprehensive Metrics**: Detailed statistics and benchmarking

## Why Use Talaria?

Modern biological databases are growing exponentially. UniProt/SwissProt, RefSeq, and other databases contain millions of sequences with significant redundancy. This creates challenges:

- **Storage costs** for maintaining large indices
- **Memory requirements** for loading indices
- **Query time** increases with database size
- **Update complexity** when refreshing indices

Talaria solves these problems by intelligently reducing database size while preserving the biological information needed for accurate alignment and classification.

## Quick Example

```bash
# Reduce a FASTA file optimized for LAMBDA
talaria reduce -i uniprot_sprot.fasta -o reduced.fasta --target-aligner lambda

# Build LAMBDA index from reduced file
lambda2 mkindexp -d reduced.fasta --acc-tax-map idmapping.dat.gz

# Query works normally with the reduced index
lambda2 searchp -q queries.fasta -i reduced.lambda
```

## Supported Aligners

Talaria provides optimized reduction strategies for:

- **LAMBDA**: Fast protein search with taxonomy support
- **BLAST**: The standard for sequence alignment
- **Kraken**: Taxonomic classification using k-mers
- **Diamond**: Fast protein aligner
- **MMseqs2**: Sensitive protein search with clustering
- **Generic**: Configurable for any aligner

## Getting Started

Ready to reduce your database size and speed up your alignments? Head to the [Quick Start](./user-guide/quick-start.md) guide!
