# MMseqs2 Workflow

MMseqs2 (Many-against-Many sequence searching) is a software suite for fast and sensitive sequence searches and clustering of large sequence datasets.

## Overview

Talaria optimizes FASTA files for MMseqs2's cascaded clustering and profile search capabilities, maintaining the k-mer prefiltering efficiency while preserving search sensitivity.

## Quick Start

```bash
# Reduce FASTA optimized for MMseqs2
talaria reduce \
  -i uniprot_sprot.fasta \
  -o uniprot_mmseqs2.fasta \
  --target-aligner mmseqs2 \
  -r 0.3

# Create MMseqs2 database
mmseqs createdb uniprot_mmseqs2.fasta uniprot_db

# Create index
mmseqs createindex uniprot_db tmp --sensitivity 5.7

# Run search
mmseqs search uniprot_db query_db result_db tmp -s 5.7

# Convert to readable format
mmseqs convertalis uniprot_db query_db result_db result.m8
```

## Optimization Strategy

### 1. Cascaded Clustering
MMseqs2 uses cascaded clustering at multiple identity levels. Talaria:
- Pre-clusters at 90%, 70%, 50%, 30% identity levels
- Selects representatives from each level
- Maintains clustering hierarchy

### 2. K-mer Prefiltering
MMseqs2 uses k-mer matching for prefiltering. Talaria:
- Optimizes k-mer diversity (k=6,7,8 based on sensitivity)
- Prioritizes sequences with rare k-mers
- Ensures comprehensive k-mer coverage

### 3. Profile Search Support
For profile searches, Talaria:
- Groups sequences by length bins
- Maintains sequence diversity within groups
- Preserves profile-building representatives

### 4. Sensitivity Levels
MMseqs2 has sensitivity levels from 1 to 7.5. Talaria adjusts:
- s1-s3: Aggressive reduction (60-70%)
- s4-s5.7: Balanced reduction (40-50%)
- s6-s7.5: Conservative reduction (20-30%)

## Configuration

### Talaria Configuration

```toml
[mmseqs2]
clustering_steps = [0.9, 0.7, 0.5, 0.3]  # Cascaded thresholds
sensitivity = 5.7                         # Default sensitivity
profile_mode = false                      # Enable for profile searches
kmer_size = 7                            # K-mer size for prefiltering
```

### Command-Line Options

```bash
# Basic reduction for MMseqs2
talaria reduce -i input.fasta -o output.fasta --target-aligner mmseqs2

# Optimize for profile searches
talaria reduce -i input.fasta -o output.fasta \
  --target-aligner mmseqs2 \
  --mmseqs2-profile

# Custom sensitivity level
talaria reduce -i input.fasta -o output.fasta \
  --target-aligner mmseqs2 \
  --mmseqs2-sensitivity 7.5
```

## MMseqs2 Workflows

### Standard Search Workflow

```bash
# 1. Reduce database
talaria reduce -i target.fasta -o target_reduced.fasta \
  --target-aligner mmseqs2 \
  -r 0.4

# 2. Create databases
mmseqs createdb target_reduced.fasta targetDB
mmseqs createdb queries.fasta queryDB

# 3. Search with standard sensitivity
mmseqs search queryDB targetDB resultDB tmp -s 5.7

# 4. Convert results
mmseqs convertalis queryDB targetDB resultDB result.m8
```

### Clustering Workflow

```bash
# 1. Reduce for clustering
talaria reduce -i sequences.fasta -o sequences_reduced.fasta \
  --target-aligner mmseqs2 \
  --mmseqs2-clustering

# 2. Create database
mmseqs createdb sequences_reduced.fasta seqDB

# 3. Cluster at multiple thresholds
mmseqs cluster seqDB clusterDB tmp \
  --min-seq-id 0.3 \
  --cluster-mode 2 \
  --cov-mode 0

# 4. Extract representatives
mmseqs createsubdb clusterDB seqDB clusterDB_rep
mmseqs convert2fasta clusterDB_rep representatives.fasta
```

### Profile Search Workflow

```bash
# 1. Reduce with profile optimization
talaria reduce -i database.fasta -o database_reduced.fasta \
  --target-aligner mmseqs2 \
  --mmseqs2-profile

# 2. Create profile database
mmseqs createdb database_reduced.fasta targetDB
mmseqs createdb queries.fasta queryDB

# 3. Build profiles
mmseqs result2profile queryDB targetDB resultDB profileDB

# 4. Iterative profile search
mmseqs search profileDB targetDB resultDB tmp \
  -s 7.5 \
  --num-iterations 3
```

## Sensitivity vs Speed Trade-offs

| Sensitivity | K-mer | Talaria Reduction | Search Speed | Use Case |
|------------|-------|-------------------|--------------|----------|
| 1.0 | 6 | 70% | Very fast | Quick screening |
| 4.0 | 6 | 50% | Fast | Default searches |
| 5.7 | 7 | 40% | Balanced | Standard analysis |
| 7.0 | 7 | 30% | Slower | Sensitive searches |
| 7.5 | 8 | 20% | Slowest | Maximum sensitivity |

## Advanced Usage

### Taxonomy-Aware Searching

```bash
# Download taxonomy
talaria download --database ncbi --dataset taxonomy

# Reduce with taxonomy preservation
talaria reduce -i nr.fasta -o nr_reduced.fasta \
  --target-aligner mmseqs2 \
  --preserve-taxonomy

# Create taxonomy-annotated database
mmseqs createdb nr_reduced.fasta nrDB
mmseqs createtaxdb nrDB tmp \
  --ncbi-tax-dump taxonomy/ \
  --tax-mapping-file prot.accession2taxid

# Taxonomic search
mmseqs taxonomy nrDB queryDB taxonomyDB tmp \
  --lca-mode 2
```

### Metagenome Analysis Pipeline

```bash
# 1. Prepare reference database
talaria reduce -i uniprot.fasta -o uniprot_meta.fasta \
  --target-aligner mmseqs2 \
  --mmseqs2-sensitivity 5.7 \
  --preserve-taxonomy

# 2. Create MMseqs2 database
mmseqs createdb uniprot_meta.fasta uniprotDB

# 3. Process metagenome
mmseqs createdb metagenome.fasta metaDB

# 4. Search against reference
mmseqs search metaDB uniprotDB resultDB tmp \
  -s 5.7 \
  --max-seqs 100

# 5. Assign taxonomy
mmseqs taxonomy metaDB uniprotDB taxonomyDB tmp \
  --lca-mode 3 \
  --tax-lineage 1

# 6. Create report
mmseqs taxonomyreport uniprotDB taxonomyDB report.tsv
```

### Comparative Genomics

```bash
# Reduce multiple genomes
for genome in genomes/*.fasta; do
  name=$(basename $genome .fasta)
  talaria reduce -i $genome -o reduced/${name}_reduced.fasta \
    --target-aligner mmseqs2
  mmseqs createdb reduced/${name}_reduced.fasta ${name}DB
done

# All-vs-all comparison
mmseqs easy-search genomeDB genomeDB result.m8 tmp \
  --min-seq-id 0.3 \
  -c 0.8 \
  --cov-mode 0
```

## Performance Metrics

### Benchmark: UniProt/SwissProt

```
Original Database:
- Size: 200 MB
- Sequences: 570,000
- Index creation: 3 minutes
- Search time (1000 queries): 180 seconds
- Memory: 6 GB

After Talaria Reduction (40%):
- Size: 80 MB
- Sequences: 228,000
- Index creation: 1.2 minutes
- Search time (1000 queries): 75 seconds
- Memory: 2.5 GB
- Sensitivity: 98.5% of original hits
```

## Integration Examples

### MMseqs2 + Pfam

```bash
# Download Pfam
talaria download --database pfam

# Reduce for HMM searches
talaria reduce -i Pfam-A.fasta -o Pfam-A_reduced.fasta \
  --target-aligner mmseqs2 \
  --mmseqs2-profile

# Search against Pfam
mmseqs search queryDB pfamDB resultDB tmp \
  --num-iterations 3 \
  -s 7.5
```

### MMseqs2 + AlphaFold

```bash
# Reduce AlphaFold database
talaria reduce -i alphafold.fasta -o alphafold_reduced.fasta \
  --target-aligner mmseqs2

# Structure-aware search
mmseqs search queryDB alphafoldDB resultDB tmp \
  --alignment-mode 3 \
  -s 7.5
```

## Best Practices

1. **Choose appropriate sensitivity**: Higher sensitivity = less reduction
2. **Use cascaded clustering**: Efficient for large-scale analysis
3. **Enable profile mode**: For HMM and iterative searches
4. **Preserve taxonomy**: Essential for metagenomics
5. **Monitor k-mer coverage**: Critical for prefiltering efficiency

## Troubleshooting

### Insufficient Sensitivity

```bash
# Increase sensitivity level
mmseqs search queryDB targetDB resultDB tmp -s 7.5

# Or reduce less aggressively
talaria reduce -i input.fasta -o output.fasta \
  --target-aligner mmseqs2 \
  -r 0.5
```

### Memory Issues

```bash
# Use split strategy
mmseqs createdb large.fasta largeDB --split 4
mmseqs createindex largeDB tmp --split 4

# Or reduce more aggressively
talaria reduce -i large.fasta -o smaller.fasta \
  --target-aligner mmseqs2 \
  -r 0.2
```

### Slow Profile Searches

```bash
# Optimize for profiles
talaria reduce -i database.fasta -o database_opt.fasta \
  --target-aligner mmseqs2 \
  --mmseqs2-profile \
  --mmseqs2-length-binning

# Use fewer iterations
mmseqs search profileDB targetDB resultDB tmp \
  --num-iterations 2
```

## See Also

- [MMseqs2 GitHub](https://github.com/soedinglab/MMseqs2)
- [MMseqs2 User Guide](https://mmseqs.com/latest/userguide.pdf)
- [Cascaded Clustering](../algorithms/clustering.md)
- [K-mer Optimization](../algorithms/kmer-optimization.md)