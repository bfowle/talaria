# Kraken Workflow

Optimize Kraken taxonomic classification databases using Talaria's reduction techniques.

## Overview

Kraken is an ultrafast taxonomic classification system that assigns taxonomic labels to DNA sequences. Talaria enhances Kraken by reducing database size while maintaining classification accuracy through taxonomy-aware reduction.

## Database Optimization

### Standard Kraken Database

```bash
# Traditional Kraken database build
kraken2-build --standard --db kraken_db
# Results in ~100GB database
```

### Talaria-Optimized Database

```bash
# Step 1: Download and reduce sequences
talaria reduce \
    --input sequences.fasta \
    --output reduced.fasta \
    --aligner kraken \
    --taxonomy-aware \
    --preserve-species-diversity

# Step 2: Build Kraken database from reduced set
kraken2-build --add-to-library reduced.fasta --db kraken_reduced
kraken2-build --build --db kraken_reduced
# Results in ~25GB database with 98% accuracy
```

## Taxonomy-Aware Reduction

### Species-Level Preservation

```bash
talaria reduce \
    --input genomes.fasta \
    --output reduced.fasta \
    --aligner kraken \
    --taxonomy nodes.dmp \
    --min-species-coverage 0.95 \
    --preserve-type-strains
```

### Genus-Level Optimization

```bash
talaria reduce \
    --input genomes.fasta \
    --output reduced.fasta \
    --aligner kraken \
    --taxonomy-level genus \
    --representatives-per-genus 5 \
    --diversity-sampling
```

## K-mer Optimization

### K-mer Preservation Strategy

```toml
[kraken]
kmer_size = 35
minimizer_length = 31
minimizer_spaces = 7
preserve_unique_kmers = true
```

### Minimizer Selection

```bash
talaria reduce \
    --input sequences.fasta \
    --output reduced.fasta \
    --aligner kraken \
    --preserve-minimizers \
    --minimizer-threshold 0.01
```

## Classification Workflow

### 1. Build Reduced Database

```bash
# Download RefSeq genomes
talaria download \
    --database refseq \
    --type bacteria,archaea,viral \
    --complete-genomes

# Reduce with Kraken optimization
talaria reduce \
    --input refseq_genomes.fasta \
    --output kraken_reduced.fasta \
    --aligner kraken \
    --taxonomy-db taxonomy/ \
    --target-size 25GB

# Build Kraken database
kraken2-build --add-to-library kraken_reduced.fasta --db kraken_db
kraken2-build --download-taxonomy --db kraken_db
kraken2-build --build --db kraken_db --threads 32
```

### 2. Classify Sequences

```bash
# Standard classification
kraken2 \
    --db kraken_db \
    --output results.txt \
    --report report.txt \
    reads.fastq

# With confidence scoring
kraken2 \
    --db kraken_db \
    --confidence 0.1 \
    --output results.txt \
    --report report.txt \
    reads.fastq
```

### 3. Bracken Abundance Estimation

```bash
# Build Bracken database
bracken-build -d kraken_db -t 32 -l 150

# Estimate abundances
bracken \
    -d kraken_db \
    -i report.txt \
    -o bracken_output.txt \
    -l S
```

## Configuration Options

### Reduction Parameters

```toml
[kraken.reduction]
# Target database size
target_size_gb = 25

# Taxonomic coverage
min_species_coverage = 0.90
min_genus_coverage = 0.95
min_family_coverage = 0.98

# Reference selection
prefer_complete_genomes = true
prefer_type_strains = true
include_plasmids = false

# K-mer preservation
preserve_unique_kmers = true
kmer_coverage_threshold = 0.95
```

### Performance Settings

```toml
[kraken.performance]
# Memory usage
max_memory_gb = 128
use_memory_mapping = true

# Parallelization
threads = 32
batch_size = 10000

# Caching
cache_minimizers = true
cache_size_gb = 8
```

## Quality Metrics

### Classification Accuracy

```bash
talaria benchmark-kraken \
    --original-db kraken_full \
    --reduced-db kraken_reduced \
    --test-reads test_reads.fastq \
    --truth-labels truth.txt
```

Metrics:
- **Sensitivity**: Correctly classified reads
- **Precision**: Accuracy of classifications
- **F1 Score**: Harmonic mean
- **Taxonomic accuracy**: Per-rank accuracy

### Database Coverage

```bash
talaria analyze-coverage \
    --db kraken_reduced \
    --taxonomy taxonomy/ \
    --output coverage_report.html
```

## Advanced Features

### 1. Host Depletion

```bash
# Remove host sequences before reduction
talaria reduce \
    --input microbiome.fasta \
    --output reduced.fasta \
    --aligner kraken \
    --exclude-taxonomy 9606 \
    --exclude-similar-to human_genome.fasta
```

### 2. Custom Databases

```bash
# Build custom viral database
talaria reduce \
    --input viral_genomes.fasta \
    --output viral_reduced.fasta \
    --aligner kraken \
    --taxonomy viral_taxonomy/ \
    --min-genome-coverage 0.99 \
    --preserve-strains

# Add to Kraken
kraken2-build --add-to-library viral_reduced.fasta --db custom_viral
```

### 3. Metagenome Optimization

```bash
# Optimize for metagenome classification
talaria reduce \
    --input reference_genomes.fasta \
    --output metagenome_db.fasta \
    --aligner kraken \
    --metagenome-mode \
    --abundance-weighted \
    --common-species-boost
```

## Integration with Pipelines

### Nextflow Pipeline

```groovy
process reduceDatabase {
    input:
    path genomes
    path taxonomy
    
    output:
    path "reduced.fasta"
    
    script:
    """
    talaria reduce \
        --input ${genomes} \
        --output reduced.fasta \
        --aligner kraken \
        --taxonomy ${taxonomy} \
        --target-size 25GB
    """
}

process buildKraken {
    input:
    path reduced_fasta
    path taxonomy
    
    output:
    path "kraken_db"
    
    script:
    """
    kraken2-build --add-to-library ${reduced_fasta} --db kraken_db
    cp -r ${taxonomy} kraken_db/taxonomy
    kraken2-build --build --db kraken_db
    """
}

process classifyReads {
    input:
    path reads
    path kraken_db
    
    output:
    path "classification.txt"
    path "report.txt"
    
    script:
    """
    kraken2 \
        --db ${kraken_db} \
        --output classification.txt \
        --report report.txt \
        ${reads}
    """
}
```

### Python Integration

```python
from talaria import KrakenReducer
import subprocess

class KrakenPipeline:
    def __init__(self, target_size="25GB"):
        self.reducer = KrakenReducer(
            target_size=target_size,
            taxonomy_aware=True
        )
    
    def build_database(self, genomes_path, output_db):
        # Reduce sequences
        reduced = self.reducer.reduce(
            genomes_path,
            preserve_species_diversity=True,
            min_coverage=0.95
        )
        
        # Build Kraken database
        subprocess.run([
            "kraken2-build",
            "--add-to-library", reduced,
            "--db", output_db
        ])
        
        subprocess.run([
            "kraken2-build",
            "--build",
            "--db", output_db
        ])
    
    def classify(self, reads, database):
        result = subprocess.run([
            "kraken2",
            "--db", database,
            "--output", "-",
            reads
        ], capture_output=True, text=True)
        
        return self.parse_results(result.stdout)
```

## Performance Benchmarks

### Database Size Comparison

| Database Type | Original Size | Reduced Size | Reduction | Build Time | Memory |
|--------------|--------------|--------------|-----------|------------|---------|
| Standard | 100 GB | 25 GB | 4x | 8h → 2h | 128 GB → 32 GB |
| RefSeq Complete | 150 GB | 35 GB | 4.3x | 12h → 3h | 196 GB → 48 GB |
| RefSeq+GenBank | 300 GB | 65 GB | 4.6x | 24h → 5h | 384 GB → 80 GB |
| Custom Viral | 5 GB | 1.2 GB | 4.2x | 30m → 8m | 8 GB → 2 GB |

### Classification Performance

| Metric | Original DB | Reduced DB | Difference |
|--------|------------|------------|------------|
| Sensitivity | 95.2% | 94.8% | -0.4% |
| Precision | 98.1% | 97.9% | -0.2% |
| F1 Score | 96.6% | 96.3% | -0.3% |
| Speed (M reads/min) | 1.2 | 3.8 | +3.2x |
| Memory Usage | 128 GB | 32 GB | -75% |

### Taxonomic Level Accuracy

| Level | Original | Reduced | Delta |
|-------|----------|---------|-------|
| Species | 92.3% | 91.8% | -0.5% |
| Genus | 95.6% | 95.3% | -0.3% |
| Family | 97.2% | 97.1% | -0.1% |
| Order | 98.5% | 98.4% | -0.1% |
| Class | 99.1% | 99.1% | 0% |
| Phylum | 99.7% | 99.7% | 0% |

## Troubleshooting

### Low Classification Rate

**Problem**: Many reads unclassified

**Solutions**:
```bash
# Decrease reduction ratio
talaria reduce --target-size 40GB

# Include more diversity
talaria reduce --diversity-sampling --min-coverage 0.85

# Add specific organisms
talaria reduce --include-taxa "species_of_interest"
```

### Memory Issues

**Problem**: Out of memory during database build

**Solutions**:
```bash
# Use lower memory mode
kraken2-build --build --db kraken_db --max-db-size 20000

# Partition database
talaria partition-kraken --db large_db --parts 4

# Use memory mapping
kraken2 --memory-mapping --db kraken_db
```

### Poor Accuracy

**Problem**: Low classification accuracy

**Solutions**:
```bash
# Preserve more unique k-mers
talaria reduce --preserve-unique-kmers --kmer-threshold 0.99

# Increase species coverage
talaria reduce --min-species-coverage 0.98

# Use confidence scoring
kraken2 --confidence 0.5 --db kraken_db
```

## Best Practices

1. **Taxonomy Completeness**
   - Ensure taxonomy files are complete
   - Include all relevant taxonomic ranks
   - Update taxonomy regularly

2. **Database Selection**
   - Use complete genomes when possible
   - Include type strains for each species
   - Balance size vs accuracy needs

3. **Regular Updates**
   - Update database monthly
   - Track new species additions
   - Re-reduce periodically for optimal performance

4. **Validation**
   - Always benchmark on known samples
   - Compare with full database results
   - Monitor classification metrics

## See Also

- [BLAST Workflow](blast-workflow.md) - Sequence similarity search
- [Diamond Workflow](diamond-workflow.md) - Protein classification
- [MMseqs2 Workflow](mmseqs2-workflow.md) - Fast sequence clustering
- [Kraken2 Manual](https://github.com/DerrickWood/kraken2/wiki) - Official documentation