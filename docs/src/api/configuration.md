# Configuration API Reference

Talaria uses TOML format configuration files to customize behavior for reduction algorithms, alignment parameters, output formats, and performance settings. This document provides complete reference for all configuration options, validation rules, and usage patterns.

## Configuration File Location

Talaria searches for configuration files in the following order:

1. **Command line specified:** `-c/--config` flag
2. **Environment variable:** `TALARIA_CONFIG`  
3. **User config directory:** `~/.config/talaria/config.toml`
4. **System config directory:** `/etc/talaria/config.toml`
5. **Current directory:** `./talaria.toml`

## Configuration Structure

The configuration file is organized into four main sections:

```toml
[reduction]     # Sequence reduction parameters
[alignment]     # Alignment scoring and algorithms  
[output]        # Output format and metadata options
[performance]   # Performance tuning and caching
```

---

## [reduction] Section

Controls the core sequence reduction algorithms and thresholds.

### `target_ratio`
**Type:** Float  
**Range:** 0.0 to 1.0  
**Default:** `0.3`  
**Description:** Target reduction ratio where 0.3 means retain 30% of original sequences.

```toml
[reduction]
target_ratio = 0.25    # Reduce to 25% of original size
```

**Validation:**
- Must be greater than 0.0 and less than or equal to 1.0
- Values below 0.1 may result in significant information loss
- Values above 0.8 provide minimal compression benefit

### `min_sequence_length`
**Type:** Integer  
**Range:** 1 to 100,000  
**Default:** `50`  
**Description:** Minimum sequence length (amino acids/nucleotides) to include in reduction.

```toml
[reduction]
min_sequence_length = 100    # Only consider sequences ≥100 residues
```

**Validation:**
- Must be a positive integer
- Typical range: 30-500 for proteins, 100-10000 for nucleotides
- Very low values (<20) may include low-quality sequences

### `max_delta_distance`
**Type:** Integer  
**Range:** 1 to 10,000  
**Default:** `100`  
**Description:** Maximum edit distance for delta encoding between similar sequences.

```toml
[reduction]
max_delta_distance = 150    # Allow larger deltas for more compression
```

**Validation:**
- Must be positive integer
- Higher values increase compression but reduce reconstruction speed
- Should be less than typical sequence length / 4

### `similarity_threshold`
**Type:** Float  
**Range:** 0.0 to 1.0  
**Default:** `0.9`  
**Description:** Similarity threshold for clustering sequences (0.9 = 90% similarity).

```toml
[reduction]
similarity_threshold = 0.95    # More stringent clustering
```

**Validation:**
- Must be between 0.0 and 1.0
- Higher values create smaller clusters (less compression)
- Values below 0.5 may cluster dissimilar sequences

### `taxonomy_aware`
**Type:** Boolean  
**Default:** `true`  
**Description:** Preserve taxonomic diversity during reduction.

```toml
[reduction]
taxonomy_aware = false    # Ignore taxonomic information
```

**Effect:**
- `true`: Ensures representative sequences from each taxonomic group
- `false`: Purely similarity-based reduction (may lose taxonomic coverage)

### Complete Reduction Example

```toml
[reduction]
target_ratio = 0.2
min_sequence_length = 75
max_delta_distance = 120
similarity_threshold = 0.92
taxonomy_aware = true
```

---

## [alignment] Section

Configuration for sequence alignment algorithms and scoring matrices.

### `gap_penalty`
**Type:** Integer  
**Range:** -100 to 0  
**Default:** `-11`  
**Description:** Gap opening penalty for sequence alignments (negative values).

```toml
[alignment]
gap_penalty = -15    # More stringent gap penalty
```

**Guidelines:**
- More negative values discourage gaps
- Typical protein values: -8 to -15
- Typical nucleotide values: -5 to -12

### `gap_extension`
**Type:** Integer  
**Range:** -50 to 0  
**Default:** `-1`  
**Description:** Gap extension penalty for continuing existing gaps.

```toml
[alignment]
gap_extension = -2    # Higher penalty for long gaps
```

**Guidelines:**
- Usually less penalized than gap opening
- Typical values: -1 to -4
- Must be less negative than gap_penalty

### `algorithm`
**Type:** String  
**Values:** `needleman-wunsch`, `smith-waterman`, `banded`, `diagonal`  
**Default:** `needleman-wunsch`  
**Description:** Alignment algorithm to use for similarity calculations.

```toml
[alignment]
algorithm = "smith-waterman"    # Local alignment
```

**Algorithm Details:**
- **`needleman-wunsch`**: Global alignment, best for full-length sequences
- **`smith-waterman`**: Local alignment, good for partial matches
- **`banded`**: Faster global alignment with restricted search space
- **`diagonal`**: Fastest, approximate alignment for large datasets

### Matrix Selection (Advanced)

For protein sequences, you can specify scoring matrices:

```toml
[alignment]
algorithm = "needleman-wunsch"
gap_penalty = -11
gap_extension = -1
matrix = "BLOSUM62"    # Optional: BLOSUM45, BLOSUM80, PAM250
```

**Matrix Options:**
- **`BLOSUM62`**: Default, good general purpose
- **`BLOSUM45`**: Distant homologs  
- **`BLOSUM80`**: Close homologs
- **`PAM250`**: Evolutionary distances

### Complete Alignment Example

```toml
[alignment]
gap_penalty = -12
gap_extension = -2
algorithm = "needleman-wunsch"
matrix = "BLOSUM62"
```

---

## [output] Section

Controls output file formats, metadata inclusion, and compression options.

### `format`
**Type:** String  
**Values:** `fasta`, `fastq`, `phylip`, `nexus`  
**Default:** `fasta`  
**Description:** Output file format for reduced sequences.

```toml
[output]
format = "fasta"    # Standard FASTA format
```

**Format Details:**
- **`fasta`**: Standard sequence format, widely compatible
- **`fastq`**: Includes quality scores (if available)
- **`phylip`**: Phylogenetic analysis format
- **`nexus`**: Nexus format for phylogenetic software

### `include_metadata`
**Type:** Boolean  
**Default:** `true`  
**Description:** Include metadata in output headers (taxonomy, source database, etc.).

```toml
[output]
include_metadata = false    # Minimal headers
```

**Effect:**
- `true`: Rich headers with taxonomy, source, etc.
- `false`: Simple sequence ID only

### `compress_output`
**Type:** Boolean  
**Default:** `false`  
**Description:** Compress output files using gzip.

```toml
[output]
compress_output = true    # Automatic compression
```

**Benefits:**
- Reduces file size by 70-90%
- Supported by most bioinformatics tools
- Slight performance overhead

### `line_length`
**Type:** Integer  
**Range:** 50 to 200  
**Default:** `80`  
**Description:** Number of characters per line in FASTA output.

```toml
[output]
line_length = 60    # Shorter lines for readability
```

### `header_format`
**Type:** String  
**Values:** `standard`, `ncbi`, `uniprot`, `custom`  
**Default:** `standard`  
**Description:** Header format style for output sequences.

```toml
[output]
header_format = "uniprot"    # UniProt-style headers
```

**Header Formats:**
- **`standard`**: `>ID description`
- **`ncbi`**: `>gi|number|db|accession| description`
- **`uniprot`**: `>sp|accession|name description`
- **`custom`**: User-defined template

### Custom Header Template

```toml
[output]
header_format = "custom"
header_template = ">{id}|{taxonomy}|{length} {description}"
```

**Template Variables:**
- `{id}`: Sequence identifier
- `{description}`: Sequence description  
- `{taxonomy}`: Taxonomic classification
- `{length}`: Sequence length
- `{source}`: Source database

### Complete Output Example

```toml
[output]
format = "fasta"
include_metadata = true
compress_output = true
line_length = 80
header_format = "uniprot"
```

---

## [performance] Section

Performance tuning options for large-scale processing.

### `chunk_size`
**Type:** Integer  
**Range:** 1,000 to 1,000,000  
**Default:** `10000`  
**Description:** Number of sequences to process in parallel chunks.

```toml
[performance]
chunk_size = 50000    # Larger chunks for big datasets
```

**Guidelines:**
- Larger chunks: Better throughput, more memory usage
- Smaller chunks: More responsive progress, less memory
- Optimal range: 5,000-50,000 sequences

### `batch_size`
**Type:** Integer  
**Range:** 100 to 100,000  
**Default:** `1000`  
**Description:** Number of sequences per alignment batch.

```toml
[performance]
batch_size = 5000    # Larger batches for efficiency
```

**Guidelines:**
- Affects memory usage during alignment
- Larger batches improve vectorization
- Should be smaller than chunk_size

### `cache_alignments`
**Type:** Boolean  
**Default:** `true`  
**Description:** Cache alignment results to avoid recomputation.

```toml
[performance]
cache_alignments = false    # Disable caching to save memory
```

**Trade-offs:**
- `true`: Faster repeated operations, uses more memory
- `false`: Lower memory usage, may recompute alignments

### `parallel_io`
**Type:** Boolean  
**Default:** `true`  
**Description:** Enable parallel file I/O operations.

```toml
[performance]
parallel_io = false    # Sequential I/O for slow storage
```

**Guidelines:**
- `true`: Faster on SSDs and high-bandwidth storage
- `false`: Better for spinning disks or network storage

### `memory_limit`
**Type:** String  
**Format:** `<number><unit>` (e.g., "4GB", "512MB")  
**Default:** `"auto"`  
**Description:** Maximum memory usage limit.

```toml
[performance]
memory_limit = "8GB"    # Limit to 8 gigabytes
```

**Units:**
- `MB`: Megabytes
- `GB`: Gigabytes  
- `auto`: Automatic based on system memory

### `temp_directory`
**Type:** String  
**Default:** System temporary directory  
**Description:** Directory for temporary files during processing.

```toml
[performance]
temp_directory = "/fast/scratch/talaria"    # Use fast storage
```

**Guidelines:**
- Use fastest available storage (SSD, ramdisk)
- Ensure sufficient space (2-3x input file size)
- Clean up automatically on completion

### Complete Performance Example

```toml
[performance]
chunk_size = 25000
batch_size = 2000
cache_alignments = true
parallel_io = true
memory_limit = "16GB"
temp_directory = "/tmp/talaria"
```

---

## Configuration Templates

### High-Performance Template

Optimized for large databases and powerful hardware:

```toml
[reduction]
target_ratio = 0.2
min_sequence_length = 50
max_delta_distance = 150
similarity_threshold = 0.9
taxonomy_aware = true

[alignment]
gap_penalty = -11
gap_extension = -1
algorithm = "banded"

[output]
format = "fasta"
include_metadata = true
compress_output = true
line_length = 80
header_format = "standard"

[performance]
chunk_size = 50000
batch_size = 5000
cache_alignments = true
parallel_io = true
memory_limit = "auto"
```

### Memory-Constrained Template

Optimized for limited memory environments:

```toml
[reduction]
target_ratio = 0.3
min_sequence_length = 75
max_delta_distance = 100
similarity_threshold = 0.9
taxonomy_aware = true

[alignment]
gap_penalty = -11
gap_extension = -1
algorithm = "diagonal"

[output]
format = "fasta"
include_metadata = false
compress_output = true
line_length = 80
header_format = "standard"

[performance]
chunk_size = 5000
batch_size = 500
cache_alignments = false
parallel_io = false
memory_limit = "4GB"
```

### Taxonomic Classification Template

Optimized for maintaining taxonomic diversity:

```toml
[reduction]
target_ratio = 0.4
min_sequence_length = 100
max_delta_distance = 80
similarity_threshold = 0.95
taxonomy_aware = true

[alignment]
gap_penalty = -12
gap_extension = -2
algorithm = "needleman-wunsch"

[output]
format = "fasta"
include_metadata = true
compress_output = false
line_length = 80
header_format = "uniprot"
header_template = ">{id}|taxid:{taxonomy} {description}"

[performance]
chunk_size = 10000
batch_size = 1000
cache_alignments = true
parallel_io = true
memory_limit = "auto"
```

---

## Environment Variable Overrides

Configuration values can be overridden using environment variables with the pattern `TALARIA_<SECTION>_<OPTION>`:

```bash
# Override reduction target ratio
export TALARIA_REDUCTION_TARGET_RATIO=0.25

# Override performance chunk size  
export TALARIA_PERFORMANCE_CHUNK_SIZE=20000

# Override alignment algorithm
export TALARIA_ALIGNMENT_ALGORITHM=smith-waterman

# Override output compression
export TALARIA_OUTPUT_COMPRESS_OUTPUT=true
```

### Boolean Values
Use `true`/`false` or `1`/`0`:

```bash
export TALARIA_REDUCTION_TAXONOMY_AWARE=false
export TALARIA_PERFORMANCE_CACHE_ALIGNMENTS=0
```

### Precedence Order

Configuration values are resolved in this order:

1. **Command line arguments** (highest priority)
2. **Environment variables**
3. **Configuration file**  
4. **Default values** (lowest priority)

---

## Validation and Error Handling

### Configuration Validation

Talaria validates all configuration values on startup:

```bash
# Validate configuration without processing
talaria reduce --config my_config.toml --dry-run
```

### Common Validation Errors

#### Invalid Range Values
```toml
[reduction]
target_ratio = 1.5    # ERROR: Must be ≤ 1.0
```

**Error Message:**
```
Configuration error: reduction.target_ratio must be between 0.0 and 1.0, got 1.5
```

#### Incompatible Settings
```toml
[alignment]
gap_penalty = -5
gap_extension = -10   # ERROR: Extension must be less negative than opening
```

**Error Message:**
```
Configuration error: alignment.gap_extension (-10) must be greater than gap_penalty (-5)
```

#### Missing Required Dependencies
```toml
[performance]
memory_limit = "invalid_format"    # ERROR: Invalid memory format
```

**Error Message:**
```
Configuration error: performance.memory_limit must be in format '<number><unit>' (e.g., '4GB')
```

### Configuration Testing

Test configuration changes with dry-run mode:

```bash
# Test configuration without processing data
talaria --config test_config.toml reduce \
    --input small_test.fasta \
    --output /dev/null \
    --dry-run
```

---

## Advanced Configuration

### Custom Scoring Matrices

Define custom scoring matrices for specialized applications:

```toml
[alignment]
algorithm = "needleman-wunsch"
gap_penalty = -11
gap_extension = -1

# Custom matrix definition
[alignment.matrix]
type = "custom"
file = "path/to/custom_matrix.txt"

# Or inline definition
[alignment.matrix.scores]
AA = 4
AC = -2
AG = 0
AT = -2
# ... more amino acid pairs
```

### Conditional Configuration

Use different settings based on input characteristics:

```toml
# Default settings
[reduction]
target_ratio = 0.3

# Override for large databases (>1M sequences)
[reduction.large_database]
target_ratio = 0.2
chunk_size = 100000

# Override for small databases (<10K sequences)  
[reduction.small_database]
target_ratio = 0.5
chunk_size = 1000
```

### Plugin Configuration

Configure external plugins and algorithms:

```toml
[plugins]
enabled = ["custom_clusterer", "taxonomy_enhancer"]

[plugins.custom_clusterer]
algorithm = "graph_based"
min_cluster_size = 5
max_cluster_size = 1000

[plugins.taxonomy_enhancer]
database_path = "/opt/taxonomy/nodes.dmp"
prefer_species_level = true
```

---

## Configuration Management

### Version Control

Store configuration files in version control with your analysis workflows:

```bash
# Project structure
project/
├── configs/
│   ├── production.toml
│   ├── development.toml  
│   └── testing.toml
├── scripts/
│   └── run_reduction.sh
└── data/
    └── input.fasta
```

### Configuration Profiles

Manage multiple configurations with profiles:

```bash
# Production profile
talaria --config configs/production.toml reduce ...

# Development profile with more verbose output
talaria -vv --config configs/development.toml reduce ...

# Testing profile with validation
talaria --config configs/testing.toml reduce ... --validate
```

### Configuration Generation

Generate configuration files from command line:

```bash
# Generate default configuration
talaria config --generate > default.toml

# Generate optimized configuration for specific use case
talaria config --optimize-for lambda --target-ratio 0.25 > lambda.toml

# Generate configuration template with comments
talaria config --template > template.toml
```

---

## Troubleshooting Configuration

### Common Issues

#### Configuration Not Found
```bash
ERROR: Configuration file not found: /path/to/config.toml
```

**Solution:**
- Verify file path and permissions
- Use absolute paths
- Check environment variables

#### Invalid TOML Syntax
```bash
ERROR: Failed to parse configuration: expected '=' at line 15
```

**Solution:**
- Validate TOML syntax online
- Check for missing quotes, brackets
- Ensure proper indentation

#### Performance Issues
If configuration causes performance problems:

```bash
# Reset to default configuration
talaria config --reset

# Generate minimal configuration
talaria config --minimal > minimal.toml

# Profile with different settings
time talaria --config test.toml reduce ...
```

### Debug Configuration Loading

Enable configuration debugging:

```bash
# Show configuration loading process
TALARIA_LOG=debug talaria --config my.toml reduce ...

# Show final resolved configuration
talaria --config my.toml --show-config reduce ...
```

---

## Migration Guide

### Upgrading from Version 0.1

Configuration format changes in version 0.2:

**Old Format:**
```toml
threshold = 0.9
target_size = 0.3
use_taxonomy = true
```

**New Format:**
```toml
[reduction]
similarity_threshold = 0.9
target_ratio = 0.3
taxonomy_aware = true
```

**Migration Command:**
```bash
talaria config --migrate-from v0.1 old_config.toml > new_config.toml
```

### Configuration Schema Updates

Check configuration schema compatibility:

```bash
# Validate against current schema
talaria config --validate my_config.toml

# Update to latest schema
talaria config --update-schema my_config.toml > updated_config.toml
```

---

## Best Practices

### Configuration Organization

1. **Use descriptive filenames:** `lambda_aggressive.toml`, `blast_conservative.toml`
2. **Include comments:** Document why specific settings were chosen
3. **Version configurations:** Track changes in version control
4. **Test configurations:** Validate on small datasets first
5. **Profile performance:** Measure impact of configuration changes

### Security Considerations

1. **File permissions:** Restrict access to configuration files containing sensitive paths
2. **Path validation:** Use absolute paths to prevent directory traversal
3. **Environment isolation:** Use separate configurations for different environments

### Performance Optimization

1. **Start conservative:** Begin with higher ratios and proven settings
2. **Benchmark systematically:** Test one parameter at a time
3. **Monitor resources:** Watch memory and CPU usage during tuning
4. **Document results:** Keep records of what works for different datasets

### Configuration Documentation

Always document your configuration choices:

```toml
# Lambda-optimized configuration for bacterial proteomes
# Tested with datasets up to 50M sequences
# Last updated: 2024-01-15
# Performance: ~4 hours for 10M sequences on 32-core machine

[reduction]
target_ratio = 0.2        # Aggressive reduction for fast indexing
similarity_threshold = 0.9 # Balanced clustering
taxonomy_aware = true     # Preserve species diversity
```
