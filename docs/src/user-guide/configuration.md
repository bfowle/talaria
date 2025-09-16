# Configuration

Comprehensive guide to configuring Talaria for optimal performance and customization.

## Configuration Files

### File Locations

Talaria searches for configuration in the following order:

1. Command-line specified: `--config /path/to/config.toml`
2. Current directory: `./talaria.toml`
3. User config: `~/.config/talaria/config.toml`
4. System config: `/etc/talaria/config.toml`

### File Format

Configuration uses TOML format:

```toml
# Example talaria.toml
[general]
verbose = false
threads = 8
color_output = true

[reduction]
similarity_threshold = 0.0  # Default: disabled
target_ratio = 0.30
min_sequence_length = 50
taxonomy_aware = false  # Default: disabled

[alignment]
algorithm = "needleman-wunsch"
gap_penalty = -2
gap_extension = -1

[output]
format = "fasta"
compress = false
include_metadata = true
```

## Configuration Sections

### General Settings

```toml
[general]
# Logging verbosity (0-3)
verbose = 1

# Number of threads (0 = auto-detect)
threads = 0

# Enable colored terminal output
color_output = true

# Temporary directory for intermediate files
temp_dir = "/tmp/talaria"

# Maximum memory usage (in GB, 0 = unlimited)
max_memory = 0

# Progress bar display
show_progress = true
```

### Reduction Configuration

```toml
[reduction]
# Similarity threshold for clustering (0.0-1.0)
# Default: 0.0 (disabled - uses simple length-based selection)
# Optional: Set to 0.7-0.95 to enable similarity-based selection
similarity_threshold = 0.0

# Target reduction ratio (0.0-1.0)
# 0.3 means reduce to 30% of original size
target_ratio = 0.30

# Minimum sequence length to consider
min_sequence_length = 50

# Maximum sequence length (0 = no limit)
max_sequence_length = 0

# Maximum distance for delta encoding
max_delta_distance = 100

# Preserve taxonomic diversity
taxonomy_aware = false

# Minimum coverage per taxonomic group
min_taxonomy_coverage = 0.90

# Selection strategy
strategy = "greedy"  # Options: greedy, clustering, taxonomy-aware, hybrid

# Reference selection criteria
prefer_longer_sequences = true
prefer_complete_sequences = true
```

### Alignment Settings

```toml
[alignment]
# Algorithm selection
algorithm = "needleman-wunsch"  # Options: needleman-wunsch, smith-waterman, banded

# Scoring parameters
gap_penalty = -2
gap_extension = -1
match_score = 2
mismatch_score = -1

# Use scoring matrix for proteins
use_matrix = true
matrix_name = "BLOSUM62"  # Options: BLOSUM62, BLOSUM80, PAM250

# Banded alignment settings
use_banding = false
band_width = 100

# Approximation settings
use_approximation = false
kmer_size = 21
min_shared_kmers = 10
```

### Output Configuration

```toml
[output]
# Output format
format = "fasta"  # Options: fasta, fastq, genbank

# Compression
compress = false
compression_level = 6  # 1-9, higher = better compression

# Include metadata in output
include_metadata = true
metadata_format = "json"  # Options: json, yaml, xml

# Delta encoding settings
delta_format = "binary"  # Options: binary, text, json
include_checksums = true

# File naming
use_timestamps = false
output_suffix = "_reduced"

# Statistics output
generate_report = true
report_format = "html"  # Options: html, text, json
```

### Performance Settings

```toml
[performance]
# Chunk size for processing
chunk_size = 10000

# Batch size for parallel processing
batch_size = 1000

# Cache settings
cache_alignments = true
cache_size_mb = 1024

# Memory management
use_memory_mapping = true
preload_sequences = false

# I/O settings
buffer_size = 8192
use_async_io = true

# Parallel processing
parallel_chunks = true
work_stealing = true
```

### Aligner-Specific Settings

#### BLAST Configuration

```toml
[blast]
# BLAST-specific optimizations
word_size = 11
dust_filter = true
soft_masking = true
evalue_threshold = 1e-5
max_target_seqs = 500

# Database optimization
optimize_for_blastn = true
preserve_low_complexity = false
```

#### LAMBDA Configuration

```toml
[lambda]
# LAMBDA-specific settings
seed_length = 10
seed_count = 5
spaced_seeds = true
seed_pattern = "111011011"

# Index optimization
index_type = "fm-index"
sampling_rate = 10
```

#### Diamond Configuration

```toml
[diamond]
# Diamond-specific settings
sensitivity = "sensitive"  # Options: fast, mid-sensitive, sensitive, more-sensitive, very-sensitive, ultra-sensitive
block_size = 2.0
index_chunks = 4
```

#### Kraken Configuration

```toml
[kraken]
# Kraken-specific settings
kmer_size = 35
minimizer_length = 31
minimizer_spaces = 7
preserve_unique_kmers = true

# Taxonomy settings
taxonomy_dir = "/path/to/taxonomy"
min_species_coverage = 0.90
prefer_type_strains = true
```

#### MMseqs2 Configuration

```toml
[mmseqs2]
# MMseqs2-specific settings
sensitivity = 7.5
kmer_size = 14
kmer_pattern = 0
max_seqs = 300
clustering_mode = 0  # 0: Greedy set cover, 1: Connected component, 2: Greedy incremental
```

## Environment Variables

### Path Configuration

Configure where Talaria stores data using environment variables:

```bash
# Base directory for all Talaria data (default: ${TALARIA_HOME})
export TALARIA_HOME="/opt/talaria"

# Data directory (default: $TALARIA_HOME)
export TALARIA_DATA_DIR="/data/talaria"

# Database storage (default: $TALARIA_DATA_DIR/databases)
export TALARIA_DATABASES_DIR="/fast/ssd/talaria-databases"

# External tools (default: $TALARIA_DATA_DIR/tools)
export TALARIA_TOOLS_DIR="/usr/local/talaria-tools"

# Cache directory (default: $TALARIA_DATA_DIR/cache)
export TALARIA_CACHE_DIR="/tmp/talaria-cache"
```

### Remote Storage

Configure remote storage for distributed setups:

```bash
# Manifest server for remote updates
export TALARIA_MANIFEST_SERVER="https://manifests.example.com"

# Chunk server for remote storage (S3, GCS, Azure)
export TALARIA_CHUNK_SERVER="s3://my-bucket/talaria-chunks"

# Remote repository for sync
export TALARIA_REMOTE_REPO="https://github.com/org/talaria-databases"
```

### Performance and Behavior

Override configuration with environment variables:

```bash
# Logging
export TALARIA_LOG="debug"  # error, warn, info, debug, trace

# General settings
export TALARIA_THREADS=16
export TALARIA_VERBOSE=2
export TALARIA_COLOR=false

# Reduction settings
export TALARIA_THRESHOLD=0.85
export TALARIA_MIN_LENGTH=100

# Aligner selection
export TALARIA_ALIGNER=blast

# Output settings
export TALARIA_COMPRESS=true
export TALARIA_FORMAT=fasta

# Performance
export TALARIA_CHUNK_SIZE=5000
export TALARIA_CACHE_SIZE=2048
```

## Command-Line Override

Command-line arguments override both config files and environment variables:

```bash
# Override specific settings
talaria reduce \
    --config custom.toml \
    --threshold 0.95 \
    --threads 32 \
    --aligner lambda \
    -i input.fasta \
    -o output.fasta
```

## Profile-Based Configuration

### Creating Profiles

Create different profiles for various use cases:

```toml
# ~/.config/talaria/profiles/high-similarity.toml
[reduction]
threshold = 0.97
strategy = "greedy"
min_sequence_length = 200

[performance]
chunk_size = 5000
use_approximation = false
```

```toml
# ~/.config/talaria/profiles/fast-mode.toml
[reduction]
threshold = 0.85
strategy = "greedy"

[alignment]
use_approximation = true
use_banding = true
band_width = 50

[performance]
chunk_size = 20000
cache_alignments = false
```

### Using Profiles

```bash
# Use a specific profile
talaria reduce --profile high-similarity -i input.fa -o output.fa

# Combine profiles
talaria reduce \
    --profile fast-mode \
    --profile high-memory \
    -i input.fa -o output.fa
```

## Validation

### Configuration Validation

```bash
# Validate configuration file
talaria config validate --config talaria.toml

# Show effective configuration
talaria config show --config talaria.toml

# Generate default configuration
talaria config generate > my_config.toml
```

### Configuration Testing

```bash
# Test configuration with sample data
talaria config test \
    --config talaria.toml \
    --sample-input test.fasta

# Benchmark different configurations
talaria config benchmark \
    --configs config1.toml,config2.toml \
    --input benchmark.fasta
```

## Advanced Configuration

### Dynamic Configuration

```toml
[dynamic]
# Adjust threshold based on sequence length
adaptive_threshold = true
threshold_min = 0.70
threshold_max = 0.95
threshold_length_factor = 0.0001

# Adjust chunk size based on available memory
adaptive_chunk_size = true
min_chunk_size = 1000
max_chunk_size = 50000

# Auto-tune performance settings
auto_tune = true
auto_tune_samples = 100
```

### Conditional Configuration

```toml
[[conditionals]]
# Use different settings for large files
condition = "file_size > 1GB"
[conditionals.settings]
chunk_size = 50000
use_memory_mapping = true
streaming_mode = true

[[conditionals]]
# Adjust for protein sequences
condition = "sequence_type == 'protein'"
[conditionals.settings]
threshold = 0.70
use_matrix = true
matrix_name = "BLOSUM62"
```

### Plugin Configuration

```toml
[plugins]
# Enable plugins
enabled = true
plugin_dir = "~/.config/talaria/plugins"

# Plugin-specific settings
[plugins.custom_aligner]
enabled = true
path = "/usr/local/lib/talaria/custom_aligner.so"
config = { param1 = "value1", param2 = 42 }
```

## Configuration Examples

### High-Performance Configuration

```toml
# Optimized for speed on high-memory systems
[general]
threads = 0  # Use all available
max_memory = 64  # GB

[reduction]
threshold = 0.85
strategy = "greedy"

[alignment]
use_approximation = true
use_banding = true
band_width = 50

[performance]
chunk_size = 50000
batch_size = 5000
cache_size_mb = 8192
use_memory_mapping = true
preload_sequences = true
parallel_chunks = true
```

### Memory-Constrained Configuration

```toml
# Optimized for low-memory systems
[general]
threads = 4
max_memory = 4  # GB

[reduction]
threshold = 0.90
strategy = "greedy"

[alignment]
use_banding = true
band_width = 30

[performance]
chunk_size = 1000
batch_size = 100
cache_alignments = false
use_memory_mapping = true
preload_sequences = false
streaming_mode = true
```

### Quality-Focused Configuration

```toml
# Optimized for maximum quality
[general]
threads = 0

[reduction]
threshold = 0.95
strategy = "hybrid"
taxonomy_aware = true

[alignment]
algorithm = "needleman-wunsch"
use_approximation = false

[output]
include_metadata = true
include_checksums = true
generate_report = true

[performance]
cache_alignments = true
cache_size_mb = 4096
```

## Troubleshooting

### Common Configuration Issues

1. **Invalid TOML syntax**
   ```bash
   # Validate syntax
   talaria config validate --config talaria.toml
   ```

2. **Conflicting settings**
   ```bash
   # Check for conflicts
   talaria config check --config talaria.toml
   ```

3. **Performance issues**
   ```bash
   # Auto-tune configuration
   talaria config tune --input sample.fasta --output optimized.toml
   ```

### Configuration Debugging

```bash
# Enable debug output
export TALARIA_DEBUG_CONFIG=1

# Show configuration loading process
talaria --debug-config reduce -i input.fa -o output.fa

# Log configuration values
talaria --log-config reduce -i input.fa -o output.fa
```

## Best Practices

1. **Start with defaults**: Begin with default settings and adjust as needed
2. **Profile your workload**: Use different profiles for different data types
3. **Version control**: Keep configuration files in version control
4. **Document changes**: Comment your configuration files
5. **Test incrementally**: Change one setting at a time and test
6. **Monitor performance**: Track metrics when adjusting settings
7. **Use validation**: Always validate configuration before production use

## See Also

- [Basic Usage](basic-usage.md) - Getting started guide
- [Performance Optimization](../advanced/performance.md) - Performance tuning
- [API Reference](../api/configuration.md) - Configuration API
- [Environment Variables](../api/cli.md#environment-variables) - Complete list