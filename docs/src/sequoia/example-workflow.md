# SEQUOIA Example Workflows

Complete examples demonstrating the SEQUOIA (Sequence Query Optimization with Indexed Architecture) system in action.

## Quick Start

### 1. Initialize SEQUOIA Repository

```bash
# Create a new SEQUOIA repository
talaria sequoia init --path ~/.talaria/sequoia

# Or use default location
export TALARIA_HOME=~/my-data
talaria sequoia init
```

### 2. Add Database to SEQUOIA

```bash
# Download and store UniProt SwissProt
talaria database download uniprot/swissprot

# Or add custom FASTA file
talaria database add --input my_sequences.fasta
```

### 3. Reduce Database with SEQUOIA Storage

```bash
# Reduce database using SEQUOIA's content-addressed storage
talaria reduce uniprot/swissprot

# With specific parameters
talaria reduce uniprot/swissprot \
  --target-aligner lambda \
  --reduction-ratio 0.3
```

## Advanced Workflows

### Bi-Temporal Versioning

Track both sequence and taxonomy evolution independently:

```bash
# View version history
talaria sequoia history --path ~/.talaria/sequoia

# Show detailed history with filters
talaria sequoia history \
  --detailed \
  --since 2024-01-01 \
  --until 2024-12-31
```

### Content-Addressed Storage

SEQUOIA uses SHA256 hashing for deduplication:

```bash
# Check repository statistics
talaria sequoia stats

# Output shows:
# - Total chunks stored
# - Deduplication ratio
# - Storage efficiency
# - Merkle DAG statistics
```

### Taxonomy-Aware Operations

Extract sequences by taxonomic group:

```bash
# Extract all E. coli sequences
talaria database fetch-taxids 562 \
  --output ecoli_sequences.fasta

# Extract human proteins
talaria database fetch-taxids 9606 \
  --output human_proteins.fasta
```

### Cloud Synchronization

Sync SEQUOIA repository with cloud storage:

```bash
# Configure remote repository
export TALARIA_MANIFEST_SERVER="s3://my-bucket/sequoia/manifests"
export TALARIA_CHUNK_SERVER="s3://my-bucket/sequoia/chunks"

# Sync with remote
talaria sequoia sync

# Check for updates only
talaria sequoia sync --check-only
```

## Complete Example: Research Workflow

### Setup New Project

```bash
# 1. Initialize SEQUOIA repository
mkdir ~/myproject
cd ~/myproject
talaria sequoia init --path ./sequoia_data

# 2. Download reference database
talaria database download uniprot/swissprot

# 3. Add custom sequences
talaria database add --input experimental_sequences.fasta
```

### Perform Analysis

```bash
# 4. Create reduced database for faster searching
talaria reduce uniprot/swissprot \
  --output swissprot_reduced.fasta \
  --target-aligner lambda \
  --reduction-ratio 0.2

# 5. Verify reduction quality
talaria validate \
  swissprot_reduced.fasta \
  --original ~/.talaria/databases/uniprot/swissprot/sequences.fasta

# 6. Check storage efficiency
talaria sequoia stats
```

### Track Changes Over Time

```bash
# 7. Update database (downloads only changed chunks)
talaria database update uniprot/swissprot

# 8. View evolution history
talaria sequoia history --detailed

# 9. Compare versions
talaria database diff \
  uniprot/swissprot@2024-01-01 \
  uniprot/swissprot@2024-06-01
```

## Performance Optimization

### Parallel Processing

```bash
# Use all available cores
export TALARIA_THREADS=0

# Or specify thread count
talaria reduce uniprot/trembl --threads 16
```

### Memory Management

```bash
# Limit memory usage for large databases
export TALARIA_MAX_MEMORY="8G"

# Enable memory-mapped I/O
export TALARIA_USE_MMAP=1
```

### Storage Optimization

```bash
# Enable aggressive compression
export TALARIA_COMPRESSION="zstd:max"

# Use SSD for chunk storage
export TALARIA_CHUNK_DIR="/fast-ssd/sequoia/chunks"

# Keep frequently accessed chunks in cache
export TALARIA_CACHE_SIZE="4G"
```

## Verification and Integrity

### Merkle Proof Verification

```bash
# Verify entire repository integrity
talaria verify --database uniprot/swissprot

# Generate cryptographic proof for specific chunk
talaria chunk inspect ABC123 --proof

# Verify bi-temporal consistency
talaria temporal verify --date 2024-01-01
```

### Detect Discrepancies

```bash
# Find taxonomy mismatches
talaria database check-discrepancies uniprot/swissprot

# List detailed discrepancy report
talaria database check-discrepancies \
  --output discrepancies.json \
  --format json
```

## Integration with Tools

### LAMBDA Aligner

```bash
# Install LAMBDA if needed
talaria tools install lambda

# Use SEQUOIA-reduced database with LAMBDA
lambda3 searchp \
  swissprot_reduced.fasta \
  query.fasta \
  -o results.txt
```

### Export for Other Tools

```bash
# Export as standard FASTA
talaria database export uniprot/swissprot \
  --format fasta \
  --output swissprot_full.fasta

# Export with custom filters
talaria database export uniprot/swissprot \
  --taxids 9606,10090 \
  --min-length 100 \
  --output filtered.fasta
```

## Troubleshooting

### Common Issues

```bash
# Check SEQUOIA repository health
talaria sequoia stats --verify

# Repair corrupted chunks
talaria sequoia repair

# Clear cache if experiencing issues
rm -rf ~/.talaria/cache/*

# Rebuild manifest
talaria sequoia rebuild-manifest
```

### Debug Mode

```bash
# Enable debug logging
export TALARIA_LOG=debug

# Trace SEQUOIA operations
export TALARIA_LOG=trace

# Save debug output
talaria reduce uniprot/swissprot 2> debug.log
```

## Best Practices

1. **Regular Updates**: Run `talaria database update` weekly to stay current
2. **Version Control**: Track important versions with `talaria sequoia history`
3. **Storage Location**: Use fast SSDs for SEQUOIA chunk storage
4. **Compression**: Enable zstd compression for better storage efficiency
5. **Verification**: Run `talaria verify` after major operations
6. **Backup**: Sync to cloud storage regularly with `talaria sequoia sync`

## See Also

- [SEQUOIA Architecture](overview.md) - Technical details
- [Performance Tuning](performance.md) - Optimization guide
- [API Reference](api-reference.md) - Complete command reference
- [Troubleshooting](troubleshooting.md) - Common problems and solutions