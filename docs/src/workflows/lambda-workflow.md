# LAMBDA Workflow

LAMBDA is a high-performance protein aligner that benefits significantly from Talaria's database reduction techniques.

## Overview

LAMBDA (Local Aligner for Massive Biological Data) is designed for fast protein searches against large databases. Talaria optimizes LAMBDA workflows by reducing database size while maintaining search sensitivity.

## Workflow Integration

### Standard LAMBDA Workflow

```bash
# Traditional approach
lambda mkindexn -d proteins.fasta
lambda searchn -q queries.fasta -d proteins.fasta.lambda
```

### Talaria-Enhanced Workflow

```bash
# Step 1: Reduce database with LAMBDA optimization
talaria reduce \
    --input proteins.fasta \
    --output proteins.reduced.fasta \
    --aligner lambda \
    --threshold 0.85

# Step 2: Build LAMBDA index from reduced database
lambda mkindexn -d proteins.reduced.fasta

# Step 3: Search with delta expansion
talaria search \
    --query queries.fasta \
    --db proteins.reduced.fasta \
    --deltas proteins.deltas \
    --aligner lambda
```

## Optimization Strategies

### 1. Sequence Clustering

LAMBDA benefits from tight clustering of similar sequences:

```toml
[lambda]
clustering_threshold = 0.85
cluster_method = "cd-hit"
min_cluster_size = 3
```

### 2. Index Optimization

Reduce index size while maintaining sensitivity:

```bash
talaria reduce \
    --input proteins.fasta \
    --output proteins.reduced.fasta \
    --aligner lambda \
    --index-optimize \
    --max-index-size 1GB
```

### 3. Seed Optimization

Configure seed parameters for optimal performance:

```toml
[lambda.seeds]
seed_length = 10
seed_count = 5
spaced_seeds = true
seed_pattern = "111011011"
```

## Performance Tuning

### Memory Configuration

```toml
[lambda.performance]
threads = 16
memory_limit = "32GB"
chunk_size = 10000
cache_size = "4GB"
```

### Search Sensitivity

Balance speed vs sensitivity:

```bash
# High sensitivity (slower)
talaria search --lambda-mode sensitive \
    --e-value 1e-5 \
    --max-hits 500

# Fast mode (less sensitive)
talaria search --lambda-mode fast \
    --e-value 1e-3 \
    --max-hits 100
```

## Database Preparation

### 1. Protein Database Reduction

```bash
# Download and prepare UniProt
talaria download --database uniprot --dataset swissprot

# Reduce with LAMBDA optimization
talaria reduce \
    --input uniprot_sprot.fasta \
    --output sprot_lambda.fasta \
    --aligner lambda \
    --preserve-taxonomy \
    --min-length 30
```

### 2. Nucleotide Translation

For nucleotide queries against protein databases:

```bash
# Translate and reduce
talaria reduce \
    --input nucleotides.fasta \
    --output proteins.fasta \
    --translate \
    --genetic-code 1 \
    --aligner lambda
```

### 3. Domain Database

For domain-based searches:

```bash
# Extract and reduce domains
talaria reduce \
    --input proteins.fasta \
    --output domains.fasta \
    --extract-domains \
    --domain-db pfam \
    --aligner lambda
```

## Search Strategies

### 1. Standard Search

```bash
lambda searchn \
    -q queries.fasta \
    -d reduced.lambda \
    -o results.m8
```

### 2. Talaria-Enhanced Search

```bash
talaria search \
    --query queries.fasta \
    --db reduced.fasta \
    --deltas deltas.tal \
    --aligner lambda \
    --expand-hits \
    --output results.m8
```

### 3. Iterative Search

For maximum sensitivity:

```bash
# First pass: search reduced database
talaria search \
    --query queries.fasta \
    --db reduced.fasta \
    --aligner lambda \
    --output pass1.m8

# Second pass: expand and refine
talaria expand-search \
    --results pass1.m8 \
    --deltas deltas.tal \
    --refine \
    --output final.m8
```

## Output Processing

### 1. Standard BLAST Format

```bash
talaria search --output-format blast-m8
```

Output columns:
```
query_id subject_id %_identity alignment_length mismatches gap_opens q_start q_end s_start s_end e_value bit_score
```

### 2. Extended Format

```bash
talaria search --output-format extended
```

Additional fields:
- Original sequence ID (before reduction)
- Delta reconstruction info
- Taxonomic information

### 3. SAM Format

For compatibility with downstream tools:

```bash
talaria search --output-format sam
```

## Quality Metrics

### Search Sensitivity

Monitor search quality:

```bash
talaria benchmark \
    --query benchmark_queries.fasta \
    --truth ground_truth.txt \
    --db reduced.fasta \
    --aligner lambda
```

Metrics reported:
- True positive rate
- False positive rate
- ROC curve
- Precision-recall curve

### Compression Efficiency

```bash
talaria stats --db reduced.fasta --deltas deltas.tal
```

Reports:
- Compression ratio
- Index size reduction
- Search time comparison
- Memory usage

## Advanced Features

### 1. Adaptive Thresholds

Automatically adjust thresholds based on query:

```toml
[lambda.adaptive]
enable = true
min_threshold = 0.7
max_threshold = 0.95
adjust_by = "query_length"
```

### 2. Taxonomic Filtering

Search within specific taxonomic groups:

```bash
talaria search \
    --query queries.fasta \
    --db reduced.fasta \
    --taxonomy bacteria \
    --tax-id 2,1239,1783272
```

### 3. Profile Searches

Use HMM profiles with LAMBDA:

```bash
# Build profile database
talaria build-profiles \
    --input alignments.sto \
    --output profiles.hmm

# Search with profiles
talaria search \
    --profile profiles.hmm \
    --db reduced.fasta \
    --aligner lambda-hmm
```

## Benchmarks

### Performance Comparison

| Database | Original Size | Reduced Size | Index Size | Search Time | Memory |
|----------|--------------|--------------|------------|-------------|---------|
| UniProt SwissProt | 270 MB | 95 MB | 1.2 GB → 420 MB | 2.3s → 0.8s | 4 GB → 1.5 GB |
| UniProt TrEMBL | 100 GB | 28 GB | 450 GB → 126 GB | 180s → 50s | 64 GB → 18 GB |
| NR | 90 GB | 31 GB | 400 GB → 140 GB | 150s → 52s | 60 GB → 21 GB |

### Sensitivity Analysis

| E-value Threshold | Original Hits | Reduced DB Hits | Recovery Rate |
|------------------|---------------|-----------------|---------------|
| 1e-10 | 1,250 | 1,248 | 99.84% |
| 1e-5 | 3,420 | 3,398 | 99.36% |
| 1e-3 | 8,150 | 8,089 | 99.25% |
| 0.01 | 15,230 | 15,012 | 98.57% |

## Best Practices

### 1. Database Selection

- Use high-quality reference sequences
- Remove redundancy before reduction
- Maintain taxonomic diversity

### 2. Parameter Tuning

```bash
# Optimize for your dataset
talaria optimize \
    --input proteins.fasta \
    --test-queries queries.fasta \
    --aligner lambda \
    --auto-tune
```

### 3. Regular Updates

```bash
# Incremental updates
talaria update \
    --existing reduced.fasta \
    --new new_sequences.fasta \
    --aligner lambda \
    --incremental
```

## Troubleshooting

### Common Issues

1. **Low sensitivity**
   - Decrease clustering threshold
   - Increase reference coverage
   - Use profile searches

2. **High memory usage**
   - Increase reduction ratio
   - Use streaming mode
   - Partition large databases

3. **Slow searches**
   - Optimize index parameters
   - Use parallel search
   - Pre-filter by taxonomy

### Validation

Always validate reduced databases:

```bash
talaria validate \
    --original proteins.fasta \
    --reduced reduced.fasta \
    --deltas deltas.tal \
    --sample-queries queries.fasta
```

## Integration Examples

### 1. Pipeline Integration

```python
import subprocess

def lambda_pipeline(query_file, db_file):
    # Reduce database
    subprocess.run([
        "talaria", "reduce",
        "--input", db_file,
        "--output", "reduced.fasta",
        "--aligner", "lambda"
    ])
    
    # Build index
    subprocess.run([
        "lambda", "mkindexn",
        "-d", "reduced.fasta"
    ])
    
    # Search
    subprocess.run([
        "lambda", "searchn",
        "-q", query_file,
        "-d", "reduced.fasta.lambda",
        "-o", "results.m8"
    ])
```

### 2. Nextflow Workflow

```nextflow
process reduceDatabase {
    input:
    path fasta
    
    output:
    path "reduced.fasta"
    path "deltas.tal"
    
    script:
    """
    talaria reduce \
        --input ${fasta} \
        --output reduced.fasta \
        --aligner lambda
    """
}

process lambdaSearch {
    input:
    path query
    path database
    
    output:
    path "results.m8"
    
    script:
    """
    lambda searchn \
        -q ${query} \
        -d ${database} \
        -o results.m8
    """
}
```

## See Also

- [BLAST Workflow](blast-workflow.md) - Alternative search strategy
- [Diamond Workflow](diamond-workflow.md) - Fast protein aligner
- [Performance Optimization](../advanced/performance.md) - Tuning guide
- [LAMBDA Documentation](https://seqan.github.io/lambda/) - Official LAMBDA docs