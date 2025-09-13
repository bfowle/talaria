# Diamond Workflow

Diamond is an accelerated BLAST-like tool for protein and translated DNA searches, achieving up to 10,000x the speed of BLAST.

## Overview

Talaria optimizes FASTA files specifically for Diamond's double-indexing strategy and block-aligning algorithm.

## Quick Start

```bash
# Reduce FASTA optimized for Diamond
talaria reduce \
  -i uniprot_sprot.fasta \
  -o uniprot_diamond.fasta \
  --target-aligner diamond \
  -r 0.3

# Build Diamond database
diamond makedb --in uniprot_diamond.fasta --db uniprot_diamond

# Run Diamond search
diamond blastp \
  --db uniprot_diamond \
  --query queries.fasta \
  --out results.m8 \
  --sensitive \
  --threads 16
```

## Optimization Strategy

### 1. Seed Diversity
Diamond uses spaced seeds of length 12-15 for initial matching. Talaria ensures:
- Maximum seed coverage across the reduced database
- Preservation of rare seeds for sensitivity
- Optimal distribution of seed patterns

### 2. Clustering at 90% Identity
Diamond's default clustering threshold is 90%. Talaria:
- Pre-clusters sequences at 90% identity
- Selects longest sequences as cluster representatives
- Maintains one representative per cluster

### 3. Taxonomic Diversity
For metagenomic applications, Talaria:
- Preserves representatives from all taxonomic groups
- Interleaves sequences from different taxa
- Ensures balanced taxonomic representation

### 4. Sequence Complexity
Diamond performs better with complex sequences first:
- Sorts by Shannon entropy
- Places low-complexity sequences at the end
- Optimizes memory access patterns

## Configuration

### Talaria Configuration

```toml
[diamond]
clustering_threshold = 0.9  # Diamond's default
min_seed_coverage = 0.95    # Maintain seed diversity
preserve_taxonomy = true    # For metagenomics
complexity_sorting = true   # Sort by entropy
```

### Command-Line Options

```bash
# Basic reduction for Diamond
talaria reduce -i input.fasta -o output.fasta --target-aligner diamond

# Custom clustering threshold
talaria reduce -i input.fasta -o output.fasta \
  --target-aligner diamond \
  --diamond-clustering 0.85

# Optimize for ultra-sensitive mode
talaria reduce -i input.fasta -o output.fasta \
  --target-aligner diamond \
  --diamond-sensitivity ultra-sensitive
```

## Diamond Sensitivity Modes

Talaria adjusts optimization based on Diamond's sensitivity:

| Mode | Talaria Optimization | Use Case |
|------|---------------------|----------|
| Fast | Aggressive reduction (70%) | Quick searches |
| Default | Balanced (50% reduction) | General use |
| Sensitive | Moderate (40% reduction) | Better sensitivity |
| More-sensitive | Conservative (30% reduction) | High sensitivity |
| Very-sensitive | Minimal (20% reduction) | Maximum sensitivity |
| Ultra-sensitive | Preserve most (10% reduction) | Critical searches |

## Performance Comparison

### Before Reduction
```
Database size: 200 MB
Sequences: 570,000
Index build: 5 minutes
Search time: 120 seconds
Memory usage: 8 GB
```

### After Talaria Reduction (30%)
```
Database size: 60 MB
Sequences: 171,000
Index build: 1.5 minutes
Search time: 40 seconds
Memory usage: 2.5 GB
Sensitivity loss: <2%
```

## Advanced Usage

### Metagenomic Workflow

```bash
# Download and reduce nr database
talaria download --database ncbi --dataset nr
talaria reduce -i nr.fasta -o nr_reduced.fasta \
  --target-aligner diamond \
  --preserve-taxonomy \
  --min-taxon-coverage 0.95

# Build Diamond database with taxonomy
diamond makedb --in nr_reduced.fasta --db nr_reduced \
  --taxonmap prot.accession2taxid \
  --taxonnodes nodes.dmp \
  --taxonnames names.dmp

# Run taxonomic search
diamond blastp --db nr_reduced --query metagenome.fasta \
  --out results.tsv \
  --outfmt 6 qseqid sseqid pident length mismatch gapopen qstart qend sstart send evalue bitscore staxids \
  --sensitive \
  --top 10
```

### Iterative Search Strategy

```bash
# First pass: Fast search on heavily reduced database
talaria reduce -i nr.fasta -o nr_fast.fasta -r 0.1 --target-aligner diamond
diamond makedb --in nr_fast.fasta --db nr_fast
diamond blastp --db nr_fast --query queries.fasta --out hits_fast.m8 --fast

# Extract unmatched queries
talaria filter-unmatched -i queries.fasta -m hits_fast.m8 -o unmatched.fasta

# Second pass: Sensitive search on moderately reduced database
talaria reduce -i nr.fasta -o nr_sensitive.fasta -r 0.4 --target-aligner diamond
diamond makedb --in nr_sensitive.fasta --db nr_sensitive
diamond blastp --db nr_sensitive --query unmatched.fasta --out hits_sensitive.m8 --very-sensitive
```

## Integration with Other Tools

### Diamond + MEGAN (Taxonomic Analysis)

```bash
# Reduce with taxonomy preservation
talaria reduce -i nr.fasta -o nr_megan.fasta \
  --target-aligner diamond \
  --preserve-taxonomy

# Diamond search with taxonomic output
diamond blastp --db nr_megan --query samples.fasta \
  --daa samples.daa \
  --sensitive

# Convert for MEGAN
diamond view --daa samples.daa \
  --outfmt 100 \
  --out samples.megan
```

### Diamond + Krona (Visualization)

```bash
# Run Diamond with taxonomic classification
diamond blastp --db nr_reduced --query input.fasta \
  --out results.m8 \
  --outfmt 6 qseqid staxids bitscore \
  --sensitive

# Process for Krona
ktImportBLAST results.m8 -o krona.html
```

## Best Practices

1. **Choose appropriate sensitivity**: Higher sensitivity requires less aggressive reduction
2. **Preserve taxonomy for metagenomics**: Use `--preserve-taxonomy` flag
3. **Monitor seed coverage**: Ensure >95% seed coverage for good sensitivity
4. **Use iterative strategy**: Fast search first, then sensitive on unmatched
5. **Validate results**: Compare hits before and after reduction

## Troubleshooting

### Low Sensitivity After Reduction

**Problem**: Missing expected hits after reduction

**Solution**:
```bash
# Use less aggressive reduction
talaria reduce -i input.fasta -o output.fasta \
  --target-aligner diamond \
  -r 0.5  # Keep 50% instead of 30%

# Or use higher sensitivity mode
diamond blastp --db reduced --query queries.fasta \
  --out results.m8 \
  --ultra-sensitive
```

### Memory Issues with Large Databases

**Problem**: Out of memory during Diamond search

**Solution**:
```bash
# Use Diamond's block size parameter
diamond blastp --db large_db --query queries.fasta \
  --out results.m8 \
  --block-size 0.5  # Smaller blocks use less memory

# Or further reduce the database
talaria reduce -i large.fasta -o smaller.fasta \
  --target-aligner diamond \
  -r 0.2  # More aggressive reduction
```

## See Also

- [Diamond GitHub Repository](https://github.com/bbuchfink/diamond)
- [Diamond Manual](https://github.com/bbuchfink/diamond/wiki)
- [Talaria Reduction Algorithm](../algorithms/reduction.md)
- [Performance Benchmarks](../benchmarks/performance.md)