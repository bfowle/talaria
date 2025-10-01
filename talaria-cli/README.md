# Talaria CLI - Command-Line Interface for Sequence Reduction System

## Overview

Talaria CLI is the primary command-line interface for the Talaria sequence reduction system, providing comprehensive tools for managing, reducing, and optimizing biological sequence databases. It serves as the main entry point for users to interact with the Talaria ecosystem, orchestrating operations across multiple subsystems including SEQUOIA storage, bioinformatics tools, and cloud services.

### Key Capabilities

- **Intelligent FASTA Reduction**: Reduces sequence databases by 30-70% while maintaining search sensitivity
- **Database Management**: Download, update, and manage biological databases from UniProt, NCBI, and custom sources
- **Content-Addressed Storage**: SEQUOIA-based storage with deduplication and verification
- **Multi-Aligner Optimization**: Tailored reduction strategies for LAMBDA, BLAST, DIAMOND, Kraken, and MMseqs2
- **Bi-Temporal Querying**: Query databases at specific points in sequence and taxonomy time
- **Interactive Terminal UI**: Rich TUI for visual database exploration and management
- **Cloud Integration**: Support for S3, GCS, and Azure blob storage backends
- **Taxonomy-Aware Processing**: Intelligent sequence grouping based on taxonomic classification
- **Unified Packed Storage**: Revolutionary 5,500× reduction in file count through pack files

## Recent Improvements

### Unified Packed Storage Architecture (v2.0.0)

The entire storage system has been revolutionized with unified packed storage:

- **5,500× Fewer Files**: From 2.2M individual files to just 400 pack files
- **Instant Startup**: <1 second startup (was 10-30 seconds)
- **10× Import Speed**: Now 50,000+ sequences/second
- **Cloud-Ready**: Pack files perfect for S3/GCS storage
- **No Backwards Compatibility**: Clean architecture, no migration complexity

This isn't a minor optimization - it's a fundamental architectural breakthrough that makes previously impossible workflows practical.

### Intelligent Resume Downloads (v1.2.0)

- **Automatic Workspace Discovery**: The `--resume` flag now automatically finds existing downloads
- **Smart Resume Logic**: Detects the most recent matching download for any database source
- **Clear Progress Messages**: Shows exactly what's being resumed and from what stage
- **Complete Download Handling**: Resumes SEQUOIA processing even if download finished but processing didn't
- **Multi-Stage Resume Support**: Can resume from downloading, decompressing, or processing stages

#### Resume Examples

```bash
# Download a large database (e.g., UniRef50 is 14GB compressed, 28GB decompressed)
talaria database download uniprot/uniref50

# If interrupted or fails, resume with:
talaria database download uniprot/uniref50 --resume

# Output shows resume status:
# ● Searching for existing downloads of UniProt/UniRef50...
# ✓ Found existing download at: ~/.talaria/downloads/uniprot_uniref50_20250927_c7c5c593
# ├─ Found download from 2 hours ago
# └─ Download complete, resuming SEQUOIA processing...
# ✓ Using existing downloaded file

# For debugging, preserve download workspace:
TALARIA_PRESERVE_DOWNLOADS=1 talaria database download uniprot/uniref50

# Force fresh download (ignore existing):
talaria database download uniprot/uniref50 --force
```

#### Workspace Management

Each download creates an isolated workspace:

```
~/.talaria/downloads/
├── uniprot_uniref50_20250927_c7c5c593/
│   ├── state.json              # Resume state
│   ├── uniref50.fasta.gz       # Downloaded file
│   ├── uniref50.fasta          # Decompressed file
│   └── chunks/                 # Processing artifacts
└── ncbi_nr_20250926_a1b2c3d4/
    └── ...
```

Environment variables for workspace control:
- `TALARIA_PRESERVE_DOWNLOADS=1` - Keep workspace after successful processing
- `TALARIA_PRESERVE_ON_FAILURE=1` - Keep workspace on errors (default)
- `TALARIA_PRESERVE_ALWAYS=1` - Never clean up workspaces

### Enhanced Test Coverage (v1.1.0)

- **4x increase in test coverage**: From 20 to 81+ comprehensive tests
- **Added property-based testing**: Automatic edge case discovery
- **Integration test suite**: End-to-end workflow validation
- **Error scenario testing**: 21 edge cases and failure modes
- **Performance benchmarks**: 10 benchmark groups for optimization
- **Mock framework integration**: Isolated unit testing
- **Snapshot testing**: Output validation and regression prevention

### Testing Enhancements

- **Command Testing**: Full coverage of all CLI commands with assert_cmd
- **Parallel Test Execution**: Faster test runs with proper isolation
- **Test Helpers Library**: Reusable utilities for test data generation
- **CI/CD Integration**: Automated testing on all commits
- **Coverage Reporting**: HTML reports with line-by-line analysis

## Architecture

### Module Organization

```
talaria-cli/
├── src/
│   ├── main.rs                       # Entry point and initialization
│   ├── cli/                          # CLI interface layer
│   │   ├── commands/                 # Command implementations
│   │   │   ├── chunk/                # SEQUOIA chunk operations
│   │   │   ├── database/             # Database management (20+ subcommands)
│   │   │   ├── sequoia/              # SEQUOIA repository management
│   │   │   └── tools/                # Tool installation and management
│   │   ├── formatting/               # Output formatting
│   │   │   ├── formatter.rs          # Terminal formatting utilities
│   │   │   ├── output.rs             # Output handling
│   │   │   └── stats_display.rs      # Statistics display
│   │   ├── interactive/              # Terminal UI components
│   │   │   ├── wizard.rs             # Interactive configuration wizard
│   │   │   ├── docs_viewer.rs        # Built-in documentation viewer
│   │   │   └── themes.rs             # UI theming
│   │   └── progress/                 # Progress tracking
│   ├── core/                         # Core business logic
│   │   ├── database/                 # Database management
│   │   │   ├── database_manager.rs   # Central database operations
│   │   │   ├── taxonomy_manager.rs   # Taxonomy handling
│   │   │   └── database_diff.rs      # Database comparison
│   │   ├── execution/                # Execution strategies
│   │   │   ├── parallel.rs           # Parallel processing
│   │   │   └── memory_estimator.rs   # Memory usage estimation
│   │   ├── selection/                # Reference selection algorithms
│   │   │   ├── traits.rs             # Selection interfaces
│   │   │   └── impls.rs              # Algorithm implementations
│   │   ├── traits/                   # Core trait definitions
│   │   ├── versioning/               # Version management
│   │   └── workspace/                # Workspace handling
│   ├── download/                     # Database downloaders
│   │   ├── ncbi.rs                   # NCBI database downloads
│   │   └── uniprot.rs                # UniProt downloads
│   ├── index/                        # Aligner index builders
│   │   ├── lambda.rs                 # LAMBDA index
│   │   ├── blast.rs                  # BLAST database
│   │   ├── diamond.rs                # DIAMOND index
│   │   └── kraken.rs                 # Kraken database
│   ├── processing/                   # Processing pipelines
│   │   └── pipeline.rs               # Main processing pipeline
│   ├── report/                       # Report generation
│   │   ├── html.rs                   # HTML reports
│   │   └── json.rs                   # JSON output
│   └── utils/                        # Utility functions
│       └── format.rs                 # Formatting utilities
```

### Design Principles

1. **Separation of Concerns**: Clear separation between CLI interface, core logic, and external integrations
2. **Trait-Based Architecture**: Extensible through trait implementations for new algorithms and tools
3. **Progressive Enhancement**: Basic functionality works offline, enhanced features with network access
4. **Fail-Safe Operations**: Workspace preservation on failure, automatic recovery mechanisms
5. **Resource Awareness**: Memory estimation, adaptive batch sizing, parallel execution control

## Command Reference

### Global Options

```bash
talaria [OPTIONS] <COMMAND>

OPTIONS:
    -v, --verbose      Verbosity level (can be repeated for more detail)
    -j, --threads <N>  Number of threads to use (0 = all available)
    -h, --help         Print help information
    -V, --version      Print version information
```

### Main Commands

#### `reduce` - Intelligent FASTA Reduction

```bash
talaria reduce [OPTIONS] <DATABASE>

ARGUMENTS:
    <DATABASE>    Database to reduce (e.g., "uniprot/swissprot", "custom/mydb")

OPTIONS:
    -o, --output <FILE>           Output reduced FASTA file
    -a, --target-aligner <TOOL>   Target aligner [lambda|blast|diamond|kraken|mmseqs2|generic]
    -r, --reduction-ratio <RATIO> Target reduction ratio (0.0-1.0)
    --min-length <N>              Minimum sequence length [default: 50]
    -m, --metadata <FILE>         Output metadata file for deltas
    -c, --config <FILE>           Configuration file
    --protein                     Use amino acid scoring
    --nucleotide                  Use nucleotide scoring
    --batch-size <N>              Batch size for processing
    --lambda-path <PATH>          Path to LAMBDA executable
    --no-cache                    Disable alignment caching
    --preserve-order              Maintain input sequence order
    --html-report <FILE>          Generate HTML report
    --json-output                 Output JSON format
    --dry-run                     Preview without execution
```

#### `database` - Database Management

```bash
talaria database <SUBCOMMAND>

SUBCOMMANDS:
    list                List available databases
    info <DB>           Show database information
    download <DB>       Download database from remote sources
    update [DB]         Update existing databases
    check-updates       Check for available updates
    add <NAME> <PATH>   Add custom database from FASTA
    export <DB>         Export database to FASTA
    versions <DB>       Manage database versions
    diff <DB1> <DB2>    Compare two databases (comprehensive analysis)
    verify <DB>         Verify database integrity
    clean               Clean unused data
    backup <DB>         Create database backup
    taxa-coverage <DB>  Show taxonomy coverage
    list-sequences <DB> List sequences in database
    check-discrepancies Check for taxonomy discrepancies
    update-taxonomy     Update taxonomy data
    gc                  Run garbage collection
    mirror <SOURCE>     Mirror remote database
    optimize <DB>       Optimize database storage
```

##### Database Download Examples

```bash
# Download UniProt SwissProt
talaria database download uniprot/swissprot

# Download NCBI nr with specific date
talaria database download ncbi/nr --date 2024-01

# Download with custom mirror
talaria database download uniprot/trembl --mirror ebi

# Download taxonomy data
talaria database download ncbi/taxonomy

# Download with automatic resume on failure
talaria database download ncbi/nr --resume  # Resume if previous download was interrupted

# Force fresh download (ignore existing)
talaria database download uniprot/swissprot --force
```

##### Download Recovery

```bash
# List interrupted downloads that can be resumed
talaria database download --list-resumable

# Example output:
# Resumable Downloads:
#
#   a1b2c3d4 - uniprot_swissprot (downloading)
#     Started: 2024-01-15 14:23:45
#   e5f6g7h8 - ncbi_nr (decompressing)
#     Started: 2024-01-15 11:15:30
#
# Use 'talaria database download --resume-id <id>' to resume a specific download
# Use 'talaria database download --resume' to resume the most recent download

# Resume most recent download
talaria database download --resume

# Resume specific download by ID
talaria database download --resume-id a1b2c3d4

# Clean old download workspaces (default: 7 days old)
talaria database clean --downloads

# Clean downloads older than 24 hours
talaria database clean --downloads --max-age-hours 24

# Dry run to see what would be cleaned
talaria database clean --downloads --dry-run
```

##### Database Management Examples

```bash
# Add custom database
talaria database add mydb /path/to/sequences.fasta \
    --description "Custom viral database" \
    --taxonomy-mapping mapping.tsv

# Export by taxonomy
talaria database export uniprot/swissprot \
    --taxids 9606,10090 \
    --output human_mouse.fasta

# Version management
talaria database versions uniprot/swissprot --list
talaria database versions uniprot/swissprot --rollback 2024_03
talaria database versions uniprot/swissprot --prune --keep 3
```

##### Database Comparison (diff)

The `diff` command provides comprehensive comparison between databases:

```bash
# Basic comparison
talaria database diff uniprot/swissprot uniprot/uniref50

# Show all analysis types
talaria database diff db1 db2 --all

# Specific analysis types
talaria database diff db1 db2 --sequences   # Sequence-level comparison
talaria database diff db1 db2 --chunks      # Chunk-level comparison
talaria database diff db1 db2 --taxonomy    # Taxonomy distribution

# Export results
talaria database diff db1 db2 --export comparison.json

# Summary mode
talaria database diff db1 db2 --summary

# Detailed mode
talaria database diff db1 db2 --detailed
```

Output includes:
- **Chunk Analysis**: Shared/unique chunks, percentages
- **Sequence Analysis**: Total sequences, overlap statistics, sample IDs
- **Taxonomy Distribution**: Shared/unique taxa, top organisms by sequence count
- **Storage Metrics**: Database sizes, deduplication savings

Example output:
```
DATABASE COMPARISON: uniprot/swissprot vs uniprot/uniref50
════════════════════════════════════════════════════════════

CHUNK-LEVEL ANALYSIS
────────────────────
Total chunks in first:        1,234
Total chunks in second:       5,678
Shared chunks:                  456 (36.9% / 8.0%)
Unique to first:                778 (63.1%)
Unique to second:             5,222 (92.0%)

SEQUENCE-LEVEL ANALYSIS
──────────────────────
Total sequences in first:   569,667
Total sequences in second: 52,324,151
Shared sequences:           234,567 (41.2% / 0.4%)

TAXONOMY DISTRIBUTION
────────────────────
Taxa in first:               14,562
Taxa in second:             234,789
Shared taxa:                 12,345 (84.8% / 5.3%)

Top shared taxa:
1. Homo sapiens (9606): 45,234 / 1,234,567 seqs
2. Mus musculus (10090): 23,456 / 987,654 seqs
3. E. coli (562): 12,345 / 543,210 seqs

STORAGE METRICS
──────────────
Size first:                  267.8 MB
Size second:                  45.2 GB
Deduplication savings:        12.3 GB (shared content)
```

#### `sequoia` - SEQUOIA Repository Management

```bash
talaria sequoia <SUBCOMMAND>

SUBCOMMANDS:
    init <PATH>           Initialize SEQUOIA repository
    stats [PATH]          Show repository statistics
    sync <REMOTE>         Synchronize with remote repository
    history               Show operation history
    time-travel <TIME>    Query at specific time point
    compact               Compact storage
    verify                Verify integrity
```

#### `chunk` - Chunk Operations

```bash
talaria chunk <SUBCOMMAND>

SUBCOMMANDS:
    inspect <HASH>     Inspect chunk by hash (displays ChunkDisplayInfo)
    lookup <ID>        Find chunk containing sequence ID
    verify <PATH>      Verify chunk integrity
    export <HASH>      Export chunk contents
```

Note: These commands use `ChunkDisplayInfo` for formatted output of chunk metadata.

#### `reconstruct` - Sequence Reconstruction

```bash
talaria reconstruct [OPTIONS]

OPTIONS:
    -i, --input <FILE>        Reduced FASTA file
    -m, --metadata <FILE>     Delta metadata file
    -o, --output <FILE>       Output reconstructed FASTA
    --from-sequoia            Reconstruct from SEQUOIA storage
    --verify                  Verify reconstruction accuracy
```

#### `stats` - Statistics and Analysis

```bash
talaria stats [OPTIONS] <INPUT>

OPTIONS:
    --compare <FILE>      Compare with another file
    --detailed           Show detailed statistics
    --by-taxonomy        Group statistics by taxonomy
    --export <FORMAT>    Export format [json|csv|html]
```

#### `tools` - Tool Management

```bash
talaria tools <SUBCOMMAND>

SUBCOMMANDS:
    list              List available tools
    install <TOOL>    Install aligner tool
    update <TOOL>     Update tool
    check             Verify tool installations
```

#### `interactive` - Interactive Mode

```bash
talaria interactive [OPTIONS]

OPTIONS:
    --theme <THEME>    UI theme [dark|light|auto]
    --no-mouse         Disable mouse support
```

Features in interactive mode:
- Visual database browser with search
- Real-time reduction preview
- Configuration editor
- Documentation viewer
- Progress monitoring dashboard
- Database comparison tools

#### `temporal` - Temporal Queries

```bash
talaria temporal [OPTIONS] <DATABASE>

OPTIONS:
    --sequence-time <TIME>    Sequence version time
    --taxonomy-time <TIME>    Taxonomy version time
    --as-of <TIME>            Query as of specific time
    --between <T1> <T2>       Changes between times
```

#### `verify` - Verification Operations

```bash
talaria verify [OPTIONS] <TARGET>

OPTIONS:
    --merkle-proof        Generate Merkle proof
    --check-integrity     Full integrity check
    --repair              Attempt to repair issues
```

#### `validate` - Validation Operations

```bash
talaria validate [OPTIONS] <ORIGINAL> <REDUCED>

OPTIONS:
    --coverage-threshold <N>   Minimum coverage required
    --identity-threshold <N>   Minimum identity required
    --sample-size <N>          Number of sequences to sample
```

## Core Components

### Database Management System

The database management system (`core/database/`) provides:

- **DatabaseManager**: Central orchestrator for all database operations
- **TaxonomyManager**: NCBI taxonomy integration and mapping
- **DatabaseReference** (from talaria-core): Structured database references with version and profile support
- **DatabaseDiff**: Efficient comparison of database versions
- **TaxonomyPrerequisites**: Automatic dependency resolution for taxonomy data

Key features:
- Content-addressed storage through SEQUOIA
- Automatic deduplication across databases
- Incremental updates with delta downloads
- Multi-source support (UniProt, NCBI, custom)
- Taxonomy-aware organization

### Reduction Pipeline

The reduction pipeline (`core/reducer.rs`) implements:

1. **Sequence Loading**: Streaming FASTA parser with taxonomy extraction
2. **Reference Selection**: Multiple algorithms for representative selection
3. **Delta Encoding**: Efficient encoding of similar sequences
4. **Chunk Generation**: Taxonomy-aware chunking for storage
5. **Index Building**: Aligner-specific index generation

Supported algorithms:
- Greedy coverage maximization
- Phylogenetic clustering
- MinHash-based selection
- Graph-based community detection
- LAMBDA-guided selection

### Reference Selection Strategies

Reference selection (`core/selection/`) provides:

- **Trait-based abstraction**: Common interface for all algorithms
- **Quality metrics**: Coverage, identity, and redundancy scoring
- **Adaptive selection**: Dynamic threshold adjustment
- **Taxonomy-aware**: Respects taxonomic boundaries

Implementations:
- `GreedySelector`: Fast, coverage-optimized
- `PhylogeneticSelector`: Evolution-aware selection
- `MinHashSelector`: Similarity-based clustering
- `GraphSelector`: Community detection approach
- `HybridSelector`: Combines multiple strategies

### Workspace Management

Workspace management (`core/workspace/`) handles:

- **TempWorkspace**: Automatic cleanup temporary workspaces
- **SequoiaWorkspace**: SEQUOIA-specific workspace with preservation
- **Workspace preservation**: Debug mode for failure analysis
- **Atomic operations**: Safe concurrent access

Structure:
```
workspace/
├── input/        # Original input files
├── sanitized/    # Cleaned sequences
├── alignments/   # Alignment results
├── references/   # Selected references
├── deltas/       # Delta encodings
├── output/       # Final output
└── metadata/     # Processing metadata
```

### Version Management

Version management (`core/versioning/`) provides:

- **VersionDetector**: Automatic version detection for databases
- **VersionStore**: Persistent version storage and retrieval
- **Semantic versioning**: Support for major.minor.patch
- **Date-based versions**: YYYY-MM-DD format support
- **Version comparison**: Efficient diff generation

### Progress Tracking

Progress tracking (`cli/progress/`) implements:

- **Multi-level progress**: Nested progress bars for complex operations
- **Memory tracking**: Real-time memory usage monitoring
- **ETA calculation**: Adaptive time estimation
- **Throughput display**: MB/s, sequences/s metrics
- **Graceful degradation**: Fallback for non-terminal environments

## Integration Architecture

### Talaria-Core Integration

Links with talaria-core for:
- Core types (`SHA256Hash`, `TaxonId`)
- Error handling (`TalariaError`)
- System paths and configuration
- Logging infrastructure

### Talaria-Bio Integration

Uses talaria-bio for:
- FASTA parsing and writing
- Sequence manipulation
- Delta encoding algorithms
- Taxonomy operations
- Alignment scoring

### Talaria-Sequoia Integration

Integrates talaria-sequoia for:
- Content-addressed storage
- Merkle DAG verification
- Bi-temporal versioning
- Chunk management
- Canonical sequence storage

### Talaria-Tools Integration

Leverages talaria-tools for:
- Aligner installations
- Tool configuration
- Index building
- Search operations

### Talaria-Utils Integration

Utilizes talaria-utils for:
- Display formatting
- Progress indicators
- Workspace utilities
- Database references

## Configuration System

### Environment Variables

```bash
# Core Configuration
TALARIA_HOME                 # Base directory (default: ~/.talaria)
TALARIA_CONFIG               # Config file path (default: $TALARIA_HOME/config.toml)
TALARIA_LOG                  # Log level [error|warn|info|debug|trace]
TALARIA_THREADS              # Thread count (0 = auto)

# Database Configuration
TALARIA_DATABASES_DIR        # Database storage (default: $TALARIA_HOME/databases)

# Download Management
TALARIA_PRESERVE_ON_FAILURE  # Keep download files on failure (default: 1)
TALARIA_PRESERVE_ALWAYS      # Never clean download workspace (debugging)
TALARIA_PRESERVE_DOWNLOADS   # Keep downloaded files after processing
TALARIA_SESSION              # Override session ID for downloads (testing)
TALARIA_TAXONOMY_DIR         # Taxonomy data (default: $TALARIA_HOME/taxonomy)
TALARIA_CACHE_DIR            # Cache directory (default: $TALARIA_HOME/cache)

# SEQUOIA Configuration
TALARIA_SEQUOIA_DIR          # SEQUOIA storage (default: $TALARIA_HOME/sequoia)
TALARIA_CHUNK_SIZE           # Target chunk size (default: 5MB)
TALARIA_COMPRESSION_LEVEL    # Zstd level 1-22 (default: 19)

# Tool Configuration
TALARIA_TOOLS_DIR            # Tool installations (default: $TALARIA_HOME/tools)
TALARIA_LAMBDA_PATH          # LAMBDA executable path
TALARIA_DIAMOND_PATH         # DIAMOND executable path

# Workspace Configuration
TALARIA_WORKSPACE_DIR        # Temp workspace (default: /tmp/talaria)
TALARIA_PRESERVE_ON_FAILURE  # Keep workspace on error
TALARIA_PRESERVE_ALWAYS      # Always keep workspace

# Network Configuration
TALARIA_MIRROR               # Preferred mirror [ncbi|ebi|expasy]
TALARIA_TIMEOUT              # Network timeout in seconds
TALARIA_RETRY_COUNT          # Download retry attempts
TALARIA_PROXY                # HTTP proxy URL

# Performance Configuration
TALARIA_BATCH_SIZE           # Processing batch size
TALARIA_MEMORY_LIMIT         # Memory limit in GB
TALARIA_CACHE_ALIGNMENTS     # Cache alignment results
TALARIA_PARALLEL_DOWNLOADS   # Parallel download streams
```

### Configuration File Format

```toml
# ~/.talaria/config.toml

[general]
threads = 8
log_level = "info"
color_output = true
progress_style = "fancy"

[reduction]
default_ratio = 0.3
min_sequence_length = 50
max_sequence_length = 50000
batch_size = 10000
cache_alignments = true
preserve_order = false

[reduction.algorithms]
default = "greedy"
protein = "phylogenetic"
nucleotide = "minhash"

[database]
home = "~/.talaria/databases"
retention_count = 3
auto_update_check = true
preferred_mirror = "ebi"
download_timeout = 3600
parallel_downloads = 4

[database.sources]
uniprot = "https://ftp.uniprot.org/pub/databases/uniprot"
ncbi = "https://ftp.ncbi.nlm.nih.gov"
ebi = "https://ftp.ebi.ac.uk"

[sequoia]
chunk_size = 5242880  # 5MB
compression_level = 19
enable_deduplication = true
verify_on_write = true

[taxonomy]
auto_download = true
update_interval = "monthly"
ncbi_taxdump = "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy"

[tools]
auto_install = false
check_updates = true

[tools.lambda]
version = "3.0.0"
index_params = "-p blastp"

[tools.diamond]
version = "2.1.8"
makedb_params = "--quiet"

[performance]
memory_limit = 16  # GB
io_buffer_size = 8192
compression_threads = 4

[output]
html_theme = "dark"
json_pretty = true
csv_delimiter = ","
report_verbosity = 2

[interactive]
theme = "dark"
mouse_enabled = true
unicode_borders = true
syntax_highlighting = true
```

## Data Flow & Processing

### Reduction Pipeline Flow

```
Input FASTA
    ↓
[Sequence Parser]
    ↓
[Taxonomy Extractor] ←── [Taxonomy Manager]
    ↓
[Sanitizer]
    ↓
[Reference Selector] ←── [Alignment Cache]
    ↓
[Delta Encoder]
    ↓
[Chunk Generator] ←── [SEQUOIA Storage]
    ↓
[Index Builder] ←── [Tool Manager]
    ↓
Output (Reduced FASTA + Metadata)
```

### Database Download Flow

```
Remote Source (UniProt/NCBI)
    ↓
[Version Detector]
    ↓
[Incremental Downloader] ←── [Progress Tracker]
    ↓
[Decompressor]
    ↓
[FASTA Parser]
    ↓
[Sequence Processor] ←── [Taxonomy Mapper]
    ↓
[Chunk Generator]
    ↓
[SEQUOIA Storage] ←── [Deduplication Engine]
    ↓
[Manifest Generator]
    ↓
Local Database
```

### Query Processing Flow

```
Query Request
    ↓
[Query Parser]
    ↓
[Temporal Resolver] ←── [Version Store]
    ↓
[Index Lookup] ←── [Bloom Filter]
    ↓
[Chunk Retrieval] ←── [Cache Layer]
    ↓
[Delta Reconstruction]
    ↓
[Sequence Assembly]
    ↓
Query Results
```

## Storage Layout

### Directory Structure

```
$TALARIA_HOME/
├── config.toml             # Main configuration
├── databases/              # Database storage
│   ├── catalog.json        # Database catalog
│   ├── uniprot/
│   │   ├── swissprot/
│   │   │   ├── manifest.json
│   │   │   ├── current/    # Current version
│   │   │   └── versions/   # Historical versions
│   │   └── trembl/
│   └── ncbi/
│       ├── nr/
│       ├── nt/
│       └── taxonomy/
├── sequoia/                # SEQUOIA storage
│   ├── chunks/             # Content-addressed chunks
│   │   ├── 00/
│   │   ├── 01/
│   │   └── ff/
│   ├── manifests/          # Database manifests
│   ├── indices/            # Fast lookup indices
│   └── packs/              # Packed storage
├── taxonomy/               # Taxonomy data
│   ├── current/            # Current taxonomy
│   │   ├── nodes.dmp
│   │   ├── names.dmp
│   │   └── merged.dmp
│   └── versions/           # Historical versions
├── tools/                  # Installed tools
│   ├── lambda/
│   ├── diamond/
│   └── mmseqs2/
├── cache/                  # Temporary cache
│   ├── alignments/
│   ├── downloads/
│   └── indices/
├── logs/                   # Application logs
└── workspace/              # Temporary workspaces
```

### Database Manifest Format

```json
{
  "version": "2024_04",
  "created": "2024-04-15T00:00:00Z",
  "source": "uniprot",
  "dataset": "swissprot",
  "statistics": {
    "total_sequences": 571282,
    "total_size": 256789012,
    "unique_taxa": 15234,
    "sequence_types": {
      "protein": 571282,
      "nucleotide": 0
    }
  },
  "sequoia": {
    "manifest_hash": "abc123...",
    "chunk_count": 1234,
    "total_chunks_size": 125678901,
    "compression_ratio": 0.49
  },
  "taxonomy": {
    "version": "2024_03",
    "root_hash": "def456..."
  },
  "chunks": [
    {
      "hash": "sha256:abcd...",
      "taxon_ids": [9606, 10090],
      "sequence_count": 1250,
      "size": 524288,
      "compressed_size": 251234
    }
  ]
}
```

## Output Formats

### HTML Report Structure

Generated HTML reports include:

```html
<!DOCTYPE html>
<html>
<head>
    <title>Talaria Reduction Report</title>
    <style>/* Embedded CSS for offline viewing */</style>
    <script>/* Interactive visualizations */</script>
</head>
<body>
    <div class="summary">
        <!-- Overall statistics -->
    </div>
    <div class="charts">
        <!-- Size reduction chart -->
        <!-- Coverage distribution -->
        <!-- Taxonomy breakdown -->
    </div>
    <div class="details">
        <!-- Sequence-level details -->
        <!-- Reference selection rationale -->
        <!-- Delta encoding statistics -->
    </div>
    <div class="performance">
        <!-- Processing time -->
        <!-- Memory usage -->
        <!-- I/O statistics -->
    </div>
</body>
</html>
```

### JSON Output Format

```json
{
  "metadata": {
    "version": "1.0.0",
    "timestamp": "2024-04-15T12:00:00Z",
    "command": "reduce",
    "parameters": {}
  },
  "input": {
    "file": "input.fasta",
    "sequences": 100000,
    "size": 50000000
  },
  "output": {
    "file": "reduced.fasta",
    "sequences": 30000,
    "size": 15000000
  },
  "statistics": {
    "reduction_ratio": 0.3,
    "coverage": 0.95,
    "average_identity": 0.89
  },
  "performance": {
    "duration_seconds": 120,
    "memory_peak_mb": 2048,
    "sequences_per_second": 833
  }
}
```

## Error Handling

### Error Types and Exit Codes

```rust
// Exit codes by error category
Configuration Error: 2 // Invalid configuration
I/O Error: 3           // File/network I/O issues
Parse Error: 4         // Invalid input format
Database Error: 5      // Database operations
Tool Error: 6          // External tool failures
Validation Error: 7    // Validation failures
Permission Error: 8    // Access denied
Network Error: 9       // Network issues
Resource Error: 10     // Out of memory/disk
Internal Error: 11     // Unexpected errors
```

### Error Recovery

The CLI implements multiple recovery strategies:

1. **Workspace Preservation**: Keep workspace on failure for debugging
2. **Partial Downloads**: Resume interrupted downloads
3. **Transaction Rollback**: Atomic database operations
4. **Automatic Retry**: Network operations with exponential backoff
5. **Graceful Degradation**: Fallback to basic functionality

### Debug Logging

```bash
# Enable debug output
TALARIA_LOG=debug talaria reduce ...

# Trace-level logging
TALARIA_LOG=trace talaria reduce ...

# Log to file
TALARIA_LOG=debug talaria reduce ... 2>debug.log

# Structured logging
TALARIA_LOG=json,debug talaria reduce ...
```

## Development Guide

### Building from Source

```bash
# Clone repository
git clone https://github.com/user/talaria
cd talaria

# Build all components
cargo build --release

# Run tests
cargo test --all

# Run comprehensive test suite
cargo test --all-features    # Test with all features
cargo test --workspace       # Test entire workspace
cargo clippy --all-targets   # Lint checks
cargo fmt --check            # Format checks

# Install locally
cargo install --path talaria-cli
```

### Test Development Guidelines

1. **Test Organization**:
   - Unit tests in `src/` with `#[cfg(test)]` modules
   - Integration tests in `tests/` directory
   - Benchmarks in `benches/` directory
   - Test helpers in `tests/common/mod.rs`

2. **Test Naming**:
   - Unit: `test_function_behavior`
   - Integration: `test_workflow_description`
   - Error: `test_error_condition`
   - Benchmark: `bench_operation_variant`

3. **Test Coverage Goals**:
   - Critical paths: 100% coverage
   - Core algorithms: Comprehensive property tests
   - Error handling: All error conditions tested
   - CLI commands: Integration test per command

4. **Performance Testing**:
   - Benchmark critical operations
   - Compare against baselines
   - Test with realistic data sizes
   - Profile memory usage

### Adding a New Command

1. Create command module in `src/cli/commands/`:
```rust
// src/cli/commands/mycommand.rs
use clap::Args;

#[derive(Args)]
pub struct MyCommandArgs {
    // Command arguments
}

pub fn run(args: MyCommandArgs) -> anyhow::Result<()> {
    // Implementation
}
```

2. Register in `src/cli/mod.rs`:
```rust
#[derive(Subcommand)]
pub enum Commands {
    // ...
    MyCommand(commands::mycommand::MyCommandArgs),
}
```

3. Add handler in `src/main.rs`:
```rust
match cli.command {
    // ...
    Commands::MyCommand(args) => commands::mycommand::run(args),
}
```

### Extending Reference Selection

Implement the `ReferenceSelector` trait:

```rust
use crate::core::traits::ReferenceSelector;

pub struct MySelector {
    // Configuration
}

impl ReferenceSelector for MySelector {
    fn select_references(
        &self,
        sequences: &[Sequence],
        target_count: usize,
    ) -> Result<Vec<usize>> {
        // Algorithm implementation
    }
}
```

### Adding Tool Support

1. Create tool module in `src/index/`:
```rust
// src/index/mytool.rs
use crate::core::traits::IndexBuilder;

pub struct MyToolIndexBuilder {
    // Configuration
}

impl IndexBuilder for MyToolIndexBuilder {
    fn build_index(&self, sequences: &Path) -> Result<()> {
        // Build tool-specific index
    }
}
```

2. Register in tool manager:
```rust
// src/core/tool_manager.rs
pub fn get_index_builder(tool: &str) -> Result<Box<dyn IndexBuilder>> {
    match tool {
        "mytool" => Ok(Box::new(MyToolIndexBuilder::new())),
        // ...
    }
}
```

## Performance Optimization

### Thread Pool Configuration

```rust
// Automatic configuration based on system
let threads = num_cpus::get();

// Manual configuration
std::env::set_var("TALARIA_THREADS", "16");

// Per-operation override
rayon::ThreadPoolBuilder::new()
    .num_threads(8)
    .build()
    .install(|| {
        // Parallel operation
    });
```

### Memory Management

The CLI implements adaptive memory management:

1. **Memory Estimation**: Pre-calculate memory requirements
2. **Batch Processing**: Automatic batch sizing based on available RAM
3. **Streaming Processing**: Process large files without loading into memory
4. **Memory Limits**: Configurable limits with graceful degradation

```rust
// Memory-aware batch sizing
let available_memory = get_available_memory()?;
let sequence_size = estimate_sequence_size(&sequences)?;
let batch_size = calculate_optimal_batch_size(available_memory, sequence_size);
```

### I/O Optimization

1. **Buffered I/O**: Configurable buffer sizes
2. **Memory-mapped Files**: For large read-only data
3. **Parallel I/O**: Multiple reader/writer threads
4. **Compression**: On-the-fly compression/decompression

```rust
// Optimized file reading
let file = File::open(path)?;
let reader = BufReader::with_capacity(8 * 1024 * 1024, file);
```

### Cache Strategies

1. **Alignment Cache**: Reuse alignment results
2. **Index Cache**: Keep frequently used indices in memory
3. **Chunk Cache**: LRU cache for SEQUOIA chunks
4. **Metadata Cache**: Database metadata caching

## Advanced Features

### Bi-Temporal Queries

Query databases at specific points in both sequence and taxonomy time:

```bash
# Query as of specific date
talaria temporal uniprot/swissprot \
    --sequence-time 2024-01-01 \
    --taxonomy-time 2024-03-01 \
    --output temporal_result.fasta

# Show changes between times
talaria temporal diff \
    --from 2024-01-01 \
    --to 2024-04-01 \
    --show-reclassified
```

### Merkle Verification

Cryptographic verification of data integrity:

```bash
# Generate proof
talaria verify generate-proof \
    --database uniprot/swissprot \
    --sequence NP_001234 \
    --output proof.json

# Verify proof
talaria verify check-proof \
    --proof proof.json \
    --root-hash abc123...
```

### Cloud Integration

S3/GCS/Azure backend support:

```bash
# Configure S3 backend
export TALARIA_SEQUOIA_BACKEND=s3
export TALARIA_S3_BUCKET=my-talaria-bucket
export TALARIA_S3_REGION=us-east-1

# Sync to cloud
talaria sequoia sync s3://my-bucket/talaria

# Use cloud-backed storage
talaria reduce uniprot/swissprot \
    --backend s3 \
    --cache-locally
```

### Database Mirroring

Create local mirrors of remote databases:

```bash
# Mirror UniProt
talaria database mirror \
    --source https://ftp.uniprot.org \
    --destination /data/mirrors/uniprot \
    --update-interval daily \
    --compression gzip
```

## Testing

### Test Coverage

The CLI module includes comprehensive testing with **81+ tests** covering critical functionality:

- **Unit Tests**: 34 tests for core command logic
- **Integration Tests**: 23 end-to-end workflow tests
- **Error Scenarios**: 21 edge case and error handling tests
- **Benchmarks**: 10 performance benchmark groups

### Testing Infrastructure

The test suite uses industry-standard Rust testing tools:

- **assert_cmd**: Command-line testing framework
- **predicates**: Flexible assertion library
- **insta**: Snapshot testing for output validation
- **criterion**: Statistical benchmarking
- **mockall**: Mock object framework
- **proptest**: Property-based testing

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test categories
cargo test --lib              # Unit tests only
cargo test --tests            # Integration tests only
cargo test --benches          # Benchmark tests

# Run specific test files
cargo test --test cli_integration
cargo test --test error_scenarios

# Run with output for debugging
cargo test -- --nocapture

# Run ignored tests (require setup)
cargo test -- --ignored

# Run tests in parallel
cargo test -- --test-threads=8
```

### Unit Tests

Unit tests are colocated with source code using `#[cfg(test)]` modules:

```bash
# Run unit tests for specific commands
cargo test reduce::
cargo test reconstruct::
cargo test database::

# Test coverage areas:
# - Command argument validation
# - Configuration parsing
# - Path handling
# - Error propagation
# - Output formatting
```

### Integration Tests

Integration tests in `tests/` directory:

```bash
# Main integration test suites
cargo test --test cli_integration    # 23 workflow tests
cargo test --test error_scenarios    # 21 error handling tests

# Test areas covered:
# - End-to-end reduction workflows
# - Database download and management
# - Tool integration (LAMBDA, DIAMOND, etc.)
# - Batch processing
# - Parallel execution
# - Unicode handling
# - Network timeouts
```

### Benchmark Tests

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark groups
cargo bench --bench cli_benchmarks

# Benchmark areas:
# - FASTA parsing performance
# - Sequence comparison algorithms
# - Compression (gzip levels 1, 6, 9)
# - Delta encoding efficiency
# - Batch processing throughput
# - Taxonomy parsing speed
# - SHA256 hash computation
# - Parallel vs sequential processing
# - File I/O operations
# - Reference selection algorithms

# Generate benchmark report
cargo bench -- --save-baseline main
cargo bench -- --baseline main
```

### Test Helpers

Common test utilities in `tests/common/mod.rs`:

```rust
// Test environment setup
TestEnvironment::new()  // Creates isolated test directories

// FASTA generation
create_simple_fasta(n)     // Generate n test sequences
create_taxonomic_fasta()   // Sequences with taxonomy
create_redundant_fasta()   // Highly similar sequences
create_corrupted_fasta()   // Invalid FASTA for error testing
create_large_fasta(s, l)   // s sequences of length l

// Command helpers
talaria_cmd()              // Pre-configured Command
run_reduce(input, output)  // Run reduction with defaults
count_sequences(file)       // Count FASTA sequences
```

### Property-Based Testing

```bash
# Run property tests
cargo test proptest

# Areas with property testing:
# - Compression level bounds (1-22)
# - Ratio validation (0.0-1.0)
# - Batch size calculations
# - Thread count limits
# - Similarity thresholds
```

### Snapshot Testing

```bash
# Run snapshot tests
cargo test insta

# Review snapshots
cargo insta review

# Update snapshots
cargo insta accept
```

### Coverage Analysis

```bash
# Install coverage tools
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html
open tarpaulin-report.html

# Coverage with specific features
cargo tarpaulin --features "cloud,interactive"
```

### Continuous Integration

Tests automatically run on:
- Every push to main branch
- All pull requests
- Nightly builds
- Release tags

### Writing New Tests

#### Unit Test Example
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_validation() {
        assert!(validate_compression_level(19).is_ok());
        assert!(validate_compression_level(23).is_err());
    }
}
```

#### Integration Test Example
```rust
#[test]
fn test_reduce_workflow() -> Result<()> {
    let env = TestEnvironment::new()?;
    let input = env.create_input_file("test.fasta", &create_simple_fasta(100))?;
    let output = env.output_path("reduced.fasta");

    let mut cmd = talaria_cmd();
    cmd.arg("reduce")
        .arg("-i").arg(&input)
        .arg("-o").arg(&output);

    cmd.assert().success();
    assert!(output.exists());
    Ok(())
}
```

#### Benchmark Example
```rust
fn bench_fasta_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("fasta_parsing");

    for size in [10, 100, 1000, 10000].iter() {
        let fasta_content = create_test_fasta(*size, 100);
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &fasta_content,
            |b, content| b.iter(|| parse_fasta(black_box(content)))
        );
    }
}
```

## Troubleshooting

### Common Issues

#### Out of Memory
```bash
# Reduce batch size
talaria reduce --batch-size 1000 ...

# Increase memory limit
export TALARIA_MEMORY_LIMIT=32

# Enable swap
sudo swapon /swapfile
```

#### Slow Performance
```bash
# Check thread utilization
talaria reduce --verbose ...

# Profile bottlenecks
TALARIA_LOG=trace talaria reduce ... 2>trace.log

# Optimize I/O
talaria reduce --io-threads 4 ...
```

#### Network Issues
```bash
# Use different mirror
talaria database download --mirror ebi ...

# Increase timeout
export TALARIA_TIMEOUT=7200

# Enable retry
export TALARIA_RETRY_COUNT=5
```

#### Corrupted Data
```bash
# Verify integrity
talaria database verify uniprot/swissprot

# Repair if possible
talaria database repair uniprot/swissprot

# Force redownload
talaria database download --force uniprot/swissprot
```

#### Test Failures

##### Unit Test Failures
```bash
# Run specific failing test with output
cargo test test_name -- --nocapture

# Check for race conditions
cargo test -- --test-threads=1

# Clean and rebuild
cargo clean && cargo test
```

##### Integration Test Failures
```bash
# Ensure test dependencies are installed
which lambda3 || talaria tools install lambda

# Check file permissions
chmod +x tests/common/*.sh

# Run with temp directory override
TMPDIR=/var/tmp cargo test
```

##### Benchmark Failures
```bash
# Ensure release build
cargo build --release

# Increase timeout for slow systems
cargo bench -- --measurement-time 60

# Skip statistical analysis
cargo bench -- --warm-up-time 0
```

### Debug Mode

```bash
# Maximum verbosity
TALARIA_LOG=trace talaria -vvv reduce ...

# Preserve all workspaces
export TALARIA_PRESERVE_ALWAYS=1

# Enable core dumps
ulimit -c unlimited

# Debug symbols
cargo build --features debug
```

## Best Practices

### Reduction Strategy Selection

1. **For Sensitivity**: Use lower reduction ratios (0.5-0.7)
2. **For Speed**: Use higher reduction ratios (0.2-0.3)
3. **For Taxonomy Studies**: Enable taxonomy-aware mode
4. **For Metagenomics**: Use community detection algorithms
5. **For Proteins**: Use phylogenetic clustering
6. **For Nucleotides**: Use k-mer based selection

### Database Management

1. **Regular Updates**: Enable auto-update checks
2. **Version Retention**: Keep 2-3 recent versions
3. **Compression**: Use zstd level 19 for storage
4. **Deduplication**: Enable cross-database deduplication
5. **Verification**: Regular integrity checks

### Performance Tuning

1. **Thread Count**: Set to CPU cores for CPU-bound tasks
2. **Memory Limits**: Set to 80% of available RAM
3. **Batch Size**: Adjust based on sequence length
4. **Cache Size**: 10-20% of frequently accessed data
5. **I/O Buffers**: 8-16 MB for sequential reads

## API Reference

### Core Traits

```rust
/// Reference selection interface
pub trait ReferenceSelector {
    fn select_references(&self, sequences: &[Sequence], target: usize) -> Result<Vec<usize>>;
}

/// Index builder interface
pub trait IndexBuilder {
    fn build_index(&self, sequences: &Path) -> Result<()>;
}

/// Database backend interface
pub trait DatabaseBackend {
    fn store_sequences(&mut self, sequences: &[Sequence]) -> Result<()>;
    fn query_sequences(&self, query: &Query) -> Result<Vec<Sequence>>;
}

/// Report generator interface
pub trait ReportGenerator {
    fn generate(&self, data: &ReportData) -> Result<String>;
}
```

### Extension Points

The CLI provides multiple extension points:

1. **Custom Commands**: Add new subcommands
2. **Selection Algorithms**: Implement `ReferenceSelector`
3. **Tool Support**: Implement `IndexBuilder`
4. **Output Formats**: Implement `ReportGenerator`
5. **Database Backends**: Implement `DatabaseBackend`
6. **Progress Indicators**: Custom progress styles

## Examples

### Complete Workflow Example

```bash
#!/bin/bash

# 1. Download and setup databases
talaria database download uniprot/swissprot
talaria database download ncbi/taxonomy

# 2. Reduce database for LAMBDA
talaria reduce uniprot/swissprot \
    --output swissprot_reduced.fasta \
    --target-aligner lambda \
    --reduction-ratio 0.3 \
    --html-report reduction_report.html

# 3. Build LAMBDA index
talaria tools install lambda
lambda3 mkindexp -d swissprot_reduced.fasta

# 4. Perform search
lambda3 searchp -q queries.fasta -i swissprot_reduced.lambda -o results.m8

# 5. Reconstruct full matches if needed
talaria reconstruct \
    --input swissprot_reduced.fasta \
    --metadata swissprot_reduced.meta \
    --output full_hits.fasta \
    --filter results.m8
```

### Custom Database Creation

```bash
# Create custom viral database
talaria database add viruses /data/viral_genomes.fasta \
    --description "Custom viral genome database" \
    --taxonomy-mapping viral_taxonomy.tsv \
    --chunk-by-genus \
    --compression-level 22

# Reduce for Kraken
talaria reduce custom/viruses \
    --output viruses_kraken.fasta \
    --target-aligner kraken \
    --min-length 1000

# Export specific families
talaria database export custom/viruses \
    --taxonomy-filter "family:Coronaviridae" \
    --output coronaviruses.fasta
```

### Temporal Analysis

```bash
# Compare database versions
talaria database diff \
    uniprot/swissprot@2024-01 \
    uniprot/swissprot@2024-04 \
    --output changes.json

# Track sequence evolution
talaria temporal evolution \
    --sequence NP_001234 \
    --from 2020-01 \
    --to 2024-04 \
    --show-mutations

# Retroactive analysis
talaria temporal retroactive \
    --database uniprot/swissprot \
    --apply-taxonomy 2024-04 \
    --to-sequences 2023-01
```

## Contributing

### Development Setup

```bash
# Fork and clone
git clone https://github.com/yourusername/talaria
cd talaria

# Install development tools
rustup component add rustfmt clippy
cargo install cargo-audit cargo-outdated

# Setup pre-commit hooks
cp scripts/pre-commit .git/hooks/
chmod +x .git/hooks/pre-commit
```

### Code Style

- Follow Rust standard formatting (`cargo fmt`)
- Pass clippy lints (`cargo clippy`)
- Add tests for new functionality
- Update documentation
- Include examples

### Submitting Changes

1. Create feature branch
2. Make changes with tests
3. Run full test suite
4. Submit pull request

## License

Licensed under MIT License. See LICENSE file for details.

## References

- [Talaria Paper](https://example.com/talaria-paper)
- [SEQUOIA Documentation](../talaria-sequoia/README.md)
- [Bioinformatics Tools](../talaria-tools/README.md)
- [API Documentation](https://docs.rs/talaria-cli)
