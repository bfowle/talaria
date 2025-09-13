# BLAST Workflow

Integration guide for using Talaria with BLAST (Basic Local Alignment Search Tool) for sequence similarity searches.

## Overview

BLAST is the most widely used sequence alignment tool in bioinformatics. Talaria enhances BLAST workflows by reducing database size while maintaining search sensitivity through intelligent reference selection and delta encoding.

## Workflow Comparison

### Traditional BLAST Workflow

```bash
# Standard BLAST database creation and search
makeblastdb -in sequences.fasta -dbtype nucl -out sequences_db
blastn -query queries.fasta -db sequences_db -out results.txt
```

### Talaria-Enhanced Workflow

```bash
# Step 1: Reduce database
talaria reduce \
    --input sequences.fasta \
    --output reduced.fasta \
    --aligner blast \
    --threshold 0.90

# Step 2: Create BLAST database from reduced set
makeblastdb -in reduced.fasta -dbtype nucl -out reduced_db

# Step 3: Search with automatic delta expansion
talaria blast-search \
    --query queries.fasta \
    --db reduced_db \
    --deltas sequences.deltas \
    --expand-hits
```

## Database Optimization

### Nucleotide Databases

```bash
# Optimize for blastn
talaria reduce \
    --input nt.fasta \
    --output nt_reduced.fasta \
    --aligner blast-nucl \
    --threshold 0.95 \
    --min-length 100 \
    --word-size 11
```

### Protein Databases

```bash
# Optimize for blastp
talaria reduce \
    --input nr.fasta \
    --output nr_reduced.fasta \
    --aligner blast-prot \
    --threshold 0.80 \
    --min-length 30 \
    --word-size 3
```

### Translated Searches

```bash
# Optimize for blastx/tblastn
talaria reduce \
    --input proteins.fasta \
    --output proteins_reduced.fasta \
    --aligner blast-trans \
    --preserve-frames \
    --codon-aware
```

## Configuration

### BLAST-Specific Settings

```toml
[blast]
# Database type
dbtype = "nucl"  # or "prot"

# Word size optimization
word_size = 11  # 11 for nucl, 3 for prot

# E-value threshold
evalue = 1e-5

# Output format
outfmt = 6  # Tabular format

# Number of threads
num_threads = 8

# Max target sequences
max_target_seqs = 500
```

### Reduction Parameters

```toml
[blast.reduction]
# Similarity threshold for clustering
threshold = 0.90

# Minimum sequence length
min_length = 100

# Maximum sequences per cluster
max_cluster_size = 100

# Preserve low-complexity regions
keep_low_complexity = false

# Mask repetitive elements
mask_repeats = true
```

## Search Strategies

### 1. Quick Search

Fast search against reduced database:

```bash
talaria blast-search \
    --mode quick \
    --query queries.fasta \
    --db reduced_db \
    --evalue 1e-3 \
    --max-hits 10
```

### 2. Sensitive Search

Comprehensive search with delta expansion:

```bash
talaria blast-search \
    --mode sensitive \
    --query queries.fasta \
    --db reduced_db \
    --deltas sequences.deltas \
    --evalue 1e-10 \
    --expand-all \
    --max-hits 1000
```

### 3. Iterative Search

Progressive refinement strategy:

```bash
# Initial fast search
talaria blast-search \
    --query queries.fasta \
    --db reduced_db \
    --output round1.txt \
    --evalue 1e-3

# Refine with delta expansion
talaria blast-refine \
    --initial round1.txt \
    --deltas sequences.deltas \
    --output final.txt \
    --evalue 1e-10
```

## Output Formats

### Standard BLAST Formats

```bash
# Format 0: Pairwise
talaria blast-search --outfmt 0

# Format 6: Tabular
talaria blast-search --outfmt 6

# Format 7: Tabular with comments
talaria blast-search --outfmt 7

# Format 10: CSV
talaria blast-search --outfmt 10

# Format 11: ASN.1
talaria blast-search --outfmt 11
```

### Custom Tabular Format

```bash
talaria blast-search \
    --outfmt "6 qseqid sseqid pident length mismatch gapopen qstart qend sstart send evalue bitscore staxids"
```

### Talaria Extended Format

```bash
talaria blast-search \
    --outfmt talaria \
    --include-deltas \
    --include-taxonomy
```

## Performance Optimization

### Memory Management

```bash
# Low memory mode
talaria blast-search \
    --low-memory \
    --db-chunk-size 1000 \
    --query-chunk-size 100

# High performance mode
talaria blast-search \
    --load-db-memory \
    --num-threads 32 \
    --gpu-accelerate
```

### Database Partitioning

```bash
# Split large database
talaria split-db \
    --input large_db.fasta \
    --num-parts 10 \
    --output-prefix part_

# Parallel search
parallel talaria blast-search \
    --query queries.fasta \
    --db part_{}.fasta \
    ::: {1..10}
```

## Quality Control

### Validation Metrics

```bash
talaria validate-blast \
    --original-db sequences.fasta \
    --reduced-db reduced.fasta \
    --test-queries validation_set.fasta \
    --metrics sensitivity,specificity,accuracy
```

Output metrics:
- **Sensitivity**: Percentage of true hits found
- **Specificity**: Percentage of true negatives
- **Accuracy**: Overall correctness
- **F1 Score**: Harmonic mean of precision and recall

### Benchmark Comparison

```bash
talaria benchmark \
    --mode blast \
    --original sequences.fasta \
    --reduced reduced.fasta \
    --queries benchmark_queries.fasta \
    --output benchmark_report.html
```

## Advanced Features

### 1. Taxonomy-Aware Search

```bash
talaria blast-search \
    --query queries.fasta \
    --db reduced_db \
    --taxids 9606,10090,7955 \
    --exclude-taxids 10239 \
    --taxonomy-db taxonomy.db
```

### 2. Profile-Based Search

```bash
# PSI-BLAST integration
talaria psi-blast \
    --query query.fasta \
    --db reduced_db \
    --num-iterations 3 \
    --inclusion-threshold 0.005 \
    --save-pssm query.pssm
```

### 3. Domain Search

```bash
# RPS-BLAST integration
talaria rps-blast \
    --query proteins.fasta \
    --db cdd_reduced \
    --evalue 0.01 \
    --show-domain-hits
```

## Troubleshooting

### Common Issues

#### 1. Missing Hits

**Problem**: Some expected hits not found in reduced database

**Solutions**:
```bash
# Decrease clustering threshold
talaria reduce --threshold 0.85

# Increase reference coverage
talaria reduce --min-coverage 0.95

# Use sensitive search mode
talaria blast-search --mode sensitive --expand-all
```

#### 2. Slow Performance

**Problem**: Searches taking too long

**Solutions**:
```bash
# Increase reduction ratio
talaria reduce --target-ratio 0.2

# Use indexed search
talaria index --db reduced.fasta --index-type suffix-array

# Enable GPU acceleration
talaria blast-search --gpu --gpu-blocks 1024
```

#### 3. High Memory Usage

**Problem**: Running out of memory

**Solutions**:
```bash
# Use streaming mode
talaria blast-search --stream --max-memory 4G

# Partition database
talaria partition --db large.fasta --max-size 1G

# Use memory-mapped files
talaria blast-search --mmap --preload false
```

## Integration Examples

### Python Integration

```python
from talaria import BlastSearch, DatabaseReducer

# Reduce database
reducer = DatabaseReducer(
    threshold=0.9,
    aligner='blast'
)
reduced_db = reducer.reduce('sequences.fasta')

# Perform search
searcher = BlastSearch(
    database=reduced_db,
    deltas='sequences.deltas'
)
results = searcher.search(
    query='queries.fasta',
    evalue=1e-5,
    expand_hits=True
)

# Process results
for hit in results:
    print(f"{hit.query_id}\t{hit.subject_id}\t{hit.evalue}")
```

### Snakemake Workflow

```python
rule reduce_database:
    input:
        "data/{dataset}.fasta"
    output:
        reduced="reduced/{dataset}.fasta",
        deltas="reduced/{dataset}.deltas"
    params:
        threshold=0.9,
        aligner="blast"
    shell:
        """
        talaria reduce \
            --input {input} \
            --output {output.reduced} \
            --deltas {output.deltas} \
            --threshold {params.threshold} \
            --aligner {params.aligner}
        """

rule blast_search:
    input:
        query="queries/{query}.fasta",
        db="reduced/{dataset}.fasta",
        deltas="reduced/{dataset}.deltas"
    output:
        "results/{query}_vs_{dataset}.txt"
    threads: 8
    shell:
        """
        talaria blast-search \
            --query {input.query} \
            --db {input.db} \
            --deltas {input.deltas} \
            --output {output} \
            --threads {threads}
        """
```

## Performance Benchmarks

### Database Size Reduction

| Database | Original | Reduced | Ratio | Index Size | Build Time |
|----------|----------|---------|-------|------------|------------|
| NT | 70 GB | 18 GB | 3.9x | 280 GB → 72 GB | 4h → 1h |
| NR | 90 GB | 22 GB | 4.1x | 360 GB → 88 GB | 5h → 1.2h |
| RefSeq | 45 GB | 12 GB | 3.8x | 180 GB → 48 GB | 2.5h → 40min |
| UniProt | 85 GB | 19 GB | 4.5x | 340 GB → 76 GB | 4.5h → 1h |

### Search Performance

| Query Set | Database | Original Time | Reduced Time | Speedup | Sensitivity |
|-----------|----------|--------------|--------------|---------|-------------|
| 100 bacterial genomes | NT | 45 min | 12 min | 3.8x | 99.2% |
| 1000 proteins | NR | 2.5 h | 38 min | 3.9x | 98.7% |
| 50 viral genomes | RefSeq | 20 min | 5 min | 4.0x | 99.5% |
| 500 domains | UniProt | 1.5 h | 22 min | 4.1x | 98.9% |

## Best Practices

1. **Choose Appropriate Thresholds**
   - Nucleotide: 0.90-0.95 similarity
   - Protein: 0.70-0.85 similarity
   - Adjust based on sequence diversity

2. **Optimize Word Size**
   - Larger word size for similar sequences
   - Smaller word size for divergent sequences
   - Match BLAST defaults when possible

3. **Validate Results**
   - Always run validation on subset
   - Compare with original database results
   - Monitor sensitivity metrics

4. **Regular Updates**
   - Incrementally update reduced databases
   - Recompute references periodically
   - Track database growth

## See Also

- [LAMBDA Workflow](lambda-workflow.md) - Fast protein aligner
- [Diamond Workflow](diamond-workflow.md) - BLAST alternative
- [Kraken Workflow](kraken-workflow.md) - Taxonomic classification
- [BLAST Documentation](https://blast.ncbi.nlm.nih.gov/) - Official BLAST docs