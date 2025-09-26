# talaria-core

## Overview

`talaria-core` is the foundational module of the Talaria sequence database reduction system. It provides essential shared utilities, configuration management, error handling, path management, and version control that all other Talaria modules depend upon. This module ensures consistency across the entire Talaria ecosystem by centralizing critical infrastructure components.

### Purpose

- **Centralized Configuration**: Unified configuration management for all Talaria components
- **Error Handling**: Consistent error types and propagation across modules
- **Path Management**: Environment-aware path resolution with caching
- **Version Control**: Semantic versioning and compatibility checking
- **Cross-Platform Support**: Handles platform-specific differences transparently

## Architecture

### Module Organization

```
talaria-core/
├── src/
│   ├── lib.rs                # Module declarations and re-exports
│   ├── types/                # Shared type definitions
│   │   ├── mod.rs            # Type module exports
│   │   ├── aligner.rs        # TargetAligner enum
│   │   ├── chunk.rs          # ChunkInfo, ChunkMetadata, DeltaChunk, ChunkType
│   │   ├── database.rs       # DatabaseReference, DatabaseSource, DatabaseSourceInfo
│   │   ├── format.rs         # OutputFormat enum
│   │   ├── hash.rs           # SHA256Hash type
│   │   ├── sequence.rs       # SequenceType enum
│   │   ├── stats.rs          # StorageStats, GCResult, DetailedStorageStats, etc.
│   │   ├── taxonomy.rs       # TaxonId, TaxonomyDataSource
│   │   └── version.rs        # Version info types
│   ├── config/               # Configuration management
│   │   └── mod.rs            # Config structures and serialization
│   ├── error/                # Error handling
│   │   ├── mod.rs            # TalariaError and Result types
│   │   └── verification.rs   # VerificationError and VerificationErrorType
│   └── system/               # System utilities
│       ├── mod.rs            # Re-exports for paths and version
│       ├── paths.rs          # Path management with caching
│       └── version.rs        # Version compatibility checking
└── Cargo.toml                # Dependencies and metadata
```

### Design Principles

1. **Zero-Cost Abstractions**: Use compile-time optimizations where possible
2. **Thread Safety**: All components are thread-safe by default
3. **Lazy Initialization**: Paths are computed once and cached using `OnceLock`
4. **Environment-First**: Configuration via environment variables takes precedence
5. **Fail-Fast**: Clear error messages for configuration issues
6. **Cross-Platform**: Works on Linux, macOS, and Windows

## Core Components

### 1. Type Definitions (`types/`)

The types module provides shared data types used across all Talaria modules:

#### Sequence Types

```rust
// Unified sequence type enumeration
pub enum SequenceType {
    Protein,
    DNA,
    RNA,
    Nucleotide, // Generic nucleotide type
    Unknown,
}
```

#### Hash Type

```rust
// SHA256 hash type used throughout the system
pub type SHA256Hash = [u8; 32];

impl SHA256Hash {
    pub fn compute(data: &[u8]) -> Self;
    pub fn from_bytes(bytes: &[u8]) -> Result<Self>;
    // Display implementation shows hex string
}
```

#### Database Types

```rust
// Database source identifier
pub enum DatabaseSource {
    UniProt(UniProtDatabase),
    NCBI(NCBIDatabase),
    Custom(String),
    Test,
}

// UniProt database variants
pub enum UniProtDatabase {
    SwissProt,
    TrEMBL,
    UniRef50,
    UniRef90,
    UniRef100,
    IdMapping,
}

// NCBI database variants
pub enum NCBIDatabase {
    Taxonomy,
    ProtAccession2TaxId,
    NuclAccession2TaxId,
    RefSeq,
    RefSeqProtein,
    RefSeqGenomic,
    NR,
    NT,
    GenBank,
}

// Database reference with version and profile support
pub struct DatabaseReference {
    pub source: String,          // e.g., "uniprot", "ncbi", "custom"
    pub dataset: String,         // e.g., "swissprot", "nr"
    pub version: Option<String>, // e.g., "2024_04", "current"
    pub profile: Option<String>, // e.g., "50-percent", "minimal"
}

impl DatabaseReference {
    pub fn new(source: String, dataset: String) -> Self;
    pub fn with_all(source: String, dataset: String, version: Option<String>, profile: Option<String>) -> Self;
    pub fn parse(reference: &str) -> Result<Self>; // Parse "source/dataset:version#profile"
    pub fn version_or_default(&self) -> &str;      // Returns version or "current"
    pub fn profile_or_default(&self) -> &str;      // Returns profile or "auto-detect"
    pub fn matches(&self, other: &DatabaseReference) -> bool;
}
```

#### Taxonomy Types

```rust
// Taxon identifier
pub struct TaxonId(pub u32);

// Data source for taxonomy information
pub enum TaxonomyDataSource {
    Api,             // From NCBI/UniProt API
    User,            // User-provided
    Accession2Taxid, // From accession2taxid file
    Header,          // Parsed from sequence header
    Inherited,       // Inherited from parent
    Unknown,
}
```

#### Chunk Types

```rust
// Chunk type enumeration
pub enum ChunkType {
    Reference,
    Delta,
    Metadata,
    Index,
}

// Chunk metadata for content-addressed storage
pub struct ChunkMetadata {
    pub hash: SHA256Hash,
    pub size: usize,
    pub sequence_count: usize,
    pub taxon_ids: Vec<TaxonId>,
    pub compressed_size: Option<usize>,
}

// Chunk information
pub struct ChunkInfo {
    pub hash: SHA256Hash,
    pub size: usize,
    pub chunk_type: ChunkType,
    pub timestamp: i64,
}

// Delta-encoded chunk
pub struct DeltaChunk {
    pub reference_hash: SHA256Hash,
    pub delta_data: Vec<u8>,
    pub sequence_ids: Vec<String>,
}
```

#### Target Aligner

```rust
// Supported alignment tools
pub enum TargetAligner {
    Lambda,
    Diamond,
    MMseqs2,
    Blast,
}
```

#### Storage Types

```rust
// Storage statistics
pub struct StorageStats {
    pub total_chunks: usize,
    pub total_size: usize,
    pub compressed_chunks: usize,
    pub deduplication_ratio: f32,
    pub total_sequences: Option<usize>,
    pub total_representations: Option<usize>,
}

// Garbage collection result
pub struct GCResult {
    pub chunks_removed: usize,
    pub bytes_freed: usize,
    pub duration_ms: u64,
}

// Detailed storage statistics
pub struct DetailedStorageStats {
    pub storage_stats: StorageStats,
    pub chunk_distribution: HashMap<ChunkType, usize>,
    pub compression_stats: CompressionStats,
}
```

#### Version Types

```rust
// Database version information
pub struct DatabaseVersionInfo {
    pub timestamp: String,
    pub upstream_version: Option<String>, // e.g., UniProt release 2024_04
    pub aliases: Vec<String>,             // e.g., ["current", "stable"]
    pub size_bytes: usize,
    pub entry_count: usize,
    pub chunk_count: usize,
}

// Temporal version information for SEQUOIA
pub struct TemporalVersionInfo {
    pub version: String,
    pub timestamp: DateTime<Utc>,
    pub manifest_hash: SHA256Hash,
    pub chunk_count: usize,
    pub total_size: usize,
    pub metadata: HashMap<String, String>,
}

// Update status for databases
pub struct UpdateStatus {
    pub updates_available: bool,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub changes_summary: Option<String>,
}
```

#### Output Format

```rust
// Output format options
pub enum OutputFormat {
    Fasta,
    FastaGz,
    Json,
    JsonGz,
    Tsv,
    TsvGz,
}
```

### 2. Configuration System (`config/`)

The configuration system provides strongly-typed configuration for all Talaria operations:

```rust
pub struct Config {
    pub reduction: ReductionConfig,
    pub alignment: AlignmentConfig,
    pub output: OutputConfig,
    pub performance: PerformanceConfig,
    pub database: DatabaseConfig,
}
```

#### ReductionConfig
Controls sequence reduction parameters:

```rust
pub struct ReductionConfig {
    pub target_ratio: f64,          // Target compression ratio (default: 0.3)
    pub min_sequence_length: usize, // Minimum sequence length (default: 50)
    pub max_delta_distance: usize,  // Max edit distance for delta encoding (default: 100)
    pub similarity_threshold: f64,  // Similarity threshold (default: 0.0, disabled)
    pub taxonomy_aware: bool,       // Use taxonomy for grouping (default: false)
}
```

#### AlignmentConfig
Configures sequence alignment algorithms:

```rust
pub struct AlignmentConfig {
    pub gap_penalty: i32,   // Gap opening penalty (default: 20)
    pub gap_extension: i32, // Gap extension penalty (default: 10)
    pub algorithm: String,  // Algorithm name (default: "needleman-wunsch")
}
```

#### OutputConfig
Controls output formatting:

```rust
pub struct OutputConfig {
    pub format: String,         // Output format (default: "fasta")
    pub include_metadata: bool, // Include metadata in output (default: true)
    pub compress_output: bool,  // Compress output files (default: false)
}
```

#### PerformanceConfig
Performance tuning parameters:

```rust
pub struct PerformanceConfig {
    pub chunk_size: usize,      // Processing chunk size (default: 10000)
    pub batch_size: usize,      // Batch processing size (default: 1000)
    pub cache_alignments: bool, // Cache alignment results (default: true)
}
```

#### DatabaseConfig
Database management configuration:

```rust
pub struct DatabaseConfig {
    pub database_dir: Option<String>,     // Custom database directory
    pub retention_count: usize,           // Old versions to keep (default: 3)
    pub auto_update_check: bool,          // Auto-check for updates (default: false)
    pub preferred_mirror: Option<String>, // Download mirror (default: Some("ebi"))
}
```

#### Configuration Functions

```rust
// Load configuration from TOML file
pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config, TalariaError>

// Save configuration to TOML file
pub fn save_config<P: AsRef<Path>>(path: P, config: &Config) -> Result<(), TalariaError>

// Get default configuration
pub fn default_config() -> Config
```

### 3. Error Handling (`error/`)

Comprehensive error type with automatic conversions:

```rust
#[derive(Error, Debug)]
pub enum TalariaError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Path error: {0}")]
    Path(String),

    #[error("Version error: {0}")]
    Version(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Parsing error: {0}")]
    Parse(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Other error: {0}")]
    Other(String),
}

// Convenient Result type alias
pub type TalariaResult<T> = Result<T, TalariaError>;
```

#### Verification Errors

Specialized error types for data verification:

```rust
// Verification error with context
pub struct VerificationError {
    pub chunk_hash: SHA256Hash,
    pub error_type: VerificationErrorType,
    pub context: Option<String>,
}

// Specific verification error types
pub enum VerificationErrorType {
    HashMismatch {
        expected: SHA256Hash,
        actual: SHA256Hash,
    },
    ReadError(String),
    CorruptedData(String),
    MissingData(String),
    InvalidFormat(String),
    SizeMismatch {
        expected: usize,
        actual: usize,
    },
}

impl VerificationError {
    pub fn new(chunk_hash: SHA256Hash, error_type: VerificationErrorType) -> Self;
    pub fn with_context(
        chunk_hash: SHA256Hash,
        error_type: VerificationErrorType,
        context: impl Into<String>,
    ) -> Self;
}
```

#### Error Conversion

Automatic conversion from common error types:

```rust
// From std::io::Error (automatic via #[from])
let file = std::fs::read("path")?; // Automatically converts to TalariaError::Io

// From serde_json::Error
impl From<serde_json::Error> for TalariaError {
    fn from(err: serde_json::Error) -> Self {
        TalariaError::Serialization(err.to_string())
    }
}

// From anyhow::Error
impl From<anyhow::Error> for TalariaError {
    fn from(err: anyhow::Error) -> Self {
        TalariaError::Other(err.to_string())
    }
}
```

### 4. Path Management (`system/paths.rs`)

Sophisticated path management with environment variable support and caching:

#### Core Path Functions

```rust
// Main Talaria directories
pub fn talaria_home() -> PathBuf          // Base directory ($TALARIA_HOME or ~/.talaria)
pub fn talaria_data_dir() -> PathBuf      // Data directory ($TALARIA_DATA_DIR or $TALARIA_HOME)
pub fn talaria_databases_dir() -> PathBuf // Databases ($TALARIA_DATABASES_DIR or $TALARIA_DATA_DIR/databases)
pub fn talaria_tools_dir() -> PathBuf     // External tools ($TALARIA_TOOLS_DIR or $TALARIA_DATA_DIR/tools)
pub fn talaria_cache_dir() -> PathBuf     // Cache ($TALARIA_CACHE_DIR or $TALARIA_DATA_DIR/cache)
pub fn talaria_workspace_dir() -> PathBuf // Temp workspace ($TALARIA_WORKSPACE_DIR or /tmp/talaria)

// Taxonomy-specific paths
pub fn talaria_taxonomy_versions_dir() -> PathBuf             // All taxonomy versions
pub fn talaria_taxonomy_current_dir() -> PathBuf              // Current taxonomy (symlink)
pub fn talaria_taxonomy_version_dir(version: &str) -> PathBuf // Specific version

// Canonical sequence paths
pub fn canonical_sequence_storage_dir() -> PathBuf // Base sequence storage
pub fn canonical_sequence_packs_dir() -> PathBuf   // Packed sequences
pub fn canonical_sequence_indices_dir() -> PathBuf // Sequence indices
pub fn canonical_sequence_index_path() -> PathBuf  // Main index file

// Database paths
pub fn database_path(source: &str, dataset: &str) -> PathBuf
pub fn storage_path() -> PathBuf
pub fn manifest_path(source: &str, dataset: &str) -> PathBuf

// Utilities
pub fn is_custom_data_dir() -> bool       // Check if using custom paths
pub fn describe_paths() -> String         // Human-readable path description
pub fn generate_utc_timestamp() -> String // UTC timestamp for versioning
```

#### Path Caching

Paths are cached using `OnceLock` for performance:

```rust
static TALARIA_HOME: OnceLock<PathBuf> = OnceLock::new();

pub fn talaria_home() -> PathBuf {
    TALARIA_HOME
        .get_or_init(|| {
            if let Ok(path) = std::env::var("TALARIA_HOME") {
                PathBuf::from(path)
            } else {
                // Fallback to ~/.talaria
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".talaria")
            }
        })
        .clone()
}
```

### 5. Version Management (`system/version.rs`)

Semantic versioning with compatibility checking:

```rust
// Parse version string
pub fn parse_version(version_str: &str) -> Result<Version, TalariaError>

// Check compatibility (same major version)
pub fn is_compatible(v1: &Version, v2: &Version) -> bool

// Get current Talaria version from Cargo.toml
pub fn current_version() -> Version
```

## Environment Variables

talaria-core respects the following environment variables:

| Variable                | Description                                 | Default                             |
| ----------------------- | ------------------------------------------- | ----------------------------------- |
| `TALARIA_HOME`          | Base directory for all Talaria data         | `~/.talaria`                        |
| `TALARIA_DATA_DIR`      | Data directory (overrides `TALARIA\_HOME`)  | `$TALARIA_HOME`                     |
| `TALARIA_DATABASES_DIR` | Database storage directory                  | `$TALARIA_DATA_DIR/databases`       |
| `TALARIA_TOOLS_DIR`     | External tools directory                    | `$TALARIA_DATA_DIR/tools`           |
| `TALARIA_CACHE_DIR`     | Cache directory                             | `$TALARIA_DATA_DIR/cache`           |
| `TALARIA_WORKSPACE_DIR` | Temporary workspace directory               | `/tmp/talaria` or `$TMPDIR/talaria` |
| `HOME`                  | User home directory (fallback)              | System-dependent                    |
| `USERPROFILE`           | Windows user profile (fallback)             | Windows-specific                    |
| `TMPDIR`                | Temporary directory (fallback)              | System-dependent                    |

## Integration with Other Modules

### Used By All Modules

Every Talaria module depends on talaria-core for:

1. **Error Handling**: Consistent error types across the system
2. **Configuration**: Unified configuration management
3. **Path Resolution**: Standard paths for data and cache
4. **Version Checking**: Compatibility verification

### Specific Integrations

#### talaria-cli
```rust
use talaria_core::{Config, TalariaError, talaria_databases_dir};

// Load user configuration
let config = load_config(&config_path)?;

// Get database directory
let db_dir = talaria_databases_dir();
```

#### talaria-bio
```rust
use talaria_core::{TalariaError, TalariaResult};

// Use consistent error types
pub fn parse_fasta(path: &Path) -> TalariaResult<Vec<Sequence>> {
    // Implementation
}
```

#### talaria-sequoia
```rust
use talaria_core::{storage_path, SHA256Hash};

// Store chunks in standard location
let storage = storage_path();

// Use common hash type
let hash = SHA256Hash::compute(&data);
```

#### talaria-tools
```rust
use talaria_core::talaria_tools_dir;

// Find external tools
let lambda_path = talaria_tools_dir().join("lambda");
```

#### talaria-storage
```rust
use talaria_core::{TalariaError, ChunkMetadata};

// Consistent error handling and types
pub fn write_metadata(path: &Path, metadata: &ChunkMetadata) -> Result<(), TalariaError>
```

## Usage Examples

### 1. Configuration Management

```rust
use talaria_core::{Config, load_config, save_config};

// Load configuration from file
let config = load_config("talaria.toml")?;

// Use default configuration
let mut config = Config::default();
config.reduction.target_ratio = 0.5;
config.performance.chunk_size = 20000;

// Save configuration
save_config("custom.toml", &config)?;
```

### 2. Error Handling

```rust
use talaria_core::{TalariaError, TalariaResult};

fn process_database(name: &str) -> TalariaResult<()> {
    // Automatic conversion from io::Error
    let data = std::fs::read(name)?;

    // Custom error
    if data.is_empty() {
        return Err(TalariaError::InvalidInput(
            "Database file is empty".to_string()
        ));
    }

    // Chain operations with ?
    let parsed = parse_data(&data)?;
    let processed = process(&parsed)?;
    save_results(&processed)?;

    Ok(())
}
```

### 3. Verification Error Handling

```rust
use talaria_core::{VerificationError, VerificationErrorType, SHA256Hash};

fn verify_chunk(hash: SHA256Hash, data: &[u8]) -> Result<(), VerificationError> {
    let actual = SHA256Hash::compute(data);

    if actual != hash {
        return Err(VerificationError::new(
            hash,
            VerificationErrorType::HashMismatch {
                expected: hash,
                actual,
            }
        ));
    }

    Ok(())
}

// With context
fn verify_with_context(hash: SHA256Hash, data: &[u8], path: &Path) -> Result<(), VerificationError> {
    verify_chunk(hash, data).map_err(|e| {
        VerificationError::with_context(
            hash,
            e.error_type,
            format!("Failed to verify chunk at {}", path.display())
        )
    })
}
```

### 4. Path Management

```rust
use talaria_core::{talaria_home, talaria_databases_dir, database_path};
use talaria_core::{is_custom_data_dir, describe_paths, generate_utc_timestamp};

// Get standard paths
let home = talaria_home();
let databases = talaria_databases_dir();

// Create database path
let uniprot_path = database_path("uniprot", "swissprot");

// Check if using custom configuration
if is_custom_data_dir() {
    println!("Using custom data directory");
}

// Generate versioned path
let timestamp = generate_utc_timestamp();
let version_dir = talaria_taxonomy_version_dir(&timestamp);

// Debug path configuration
println!("{}", describe_paths());
```

### 5. Version Compatibility

```rust
use talaria_core::{parse_version, is_compatible, current_version};
use talaria_core::TalariaError;

// Check if database version is compatible
let db_version = parse_version("1.2.3")?;
let current = current_version();

if !is_compatible(&db_version, &current) {
    return Err(TalariaError::Version(format!(
        "Incompatible database version: {} (current: {})",
        db_version, current
    )));
}
```

### 6. Working with Database Types

```rust
use talaria_core::{DatabaseSource, DatabaseReference, UniProtDatabase, NCBIDatabase};

// Create database sources
let uniprot = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
let ncbi = DatabaseSource::NCBI(NCBIDatabase::NR);
let custom = DatabaseSource::Custom("mydb".to_string());

// Parse database reference
let db_ref = DatabaseReference::parse("uniprot/swissprot:2024_04#blast-30")?;
assert_eq!(db_ref.source, "uniprot");
assert_eq!(db_ref.dataset, "swissprot");
assert_eq!(db_ref.version_or_default(), "2024_04");
assert_eq!(db_ref.profile_or_default(), "blast-30");

// Check if references match
let ref1 = DatabaseReference::new("uniprot".to_string(), "swissprot".to_string());
let ref2 = DatabaseReference::new("uniprot".to_string(), "swissprot".to_string());
assert!(ref1.matches(&ref2));
```

### 7. Complete Example: Database Manager

```rust
use talaria_core::{Config, TalariaResult, talaria_databases_dir};
use std::path::PathBuf;

pub struct DatabaseManager {
    config: Config,
    base_path: PathBuf,
}

impl DatabaseManager {
    pub fn new(config: Config) -> Self {
        let base_path = if let Some(ref dir) = config.database.database_dir {
            PathBuf::from(dir)
        } else {
            talaria_databases_dir()
        };

        Self { config, base_path }
    }

    pub fn download_database(&self, source: &str, name: &str) -> TalariaResult<()> {
        let db_path = self.base_path.join(source).join(name);

        // Create directory if needed
        std::fs::create_dir_all(&db_path)?;

        // Download using preferred mirror
        let mirror = self.config.database.preferred_mirror
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("ebi");

        // ... download logic ...

        Ok(())
    }
}
```

## API Reference

### Type API

```rust
// Core types
pub type SHA256Hash = [u8; 32];
pub struct TaxonId(pub u32);

// Sequence types
pub enum SequenceType { Protein, DNA, RNA, Nucleotide, Unknown }

// Database types
pub struct DatabaseReference { ... }
pub struct DatabaseSourceInfo { ... }
pub enum DatabaseSource { UniProt(..), NCBI(..), Custom(..), Test }
pub enum UniProtDatabase { SwissProt, TrEMBL, UniRef50, UniRef90, UniRef100, IdMapping }
pub enum NCBIDatabase { Taxonomy, ProtAccession2TaxId, NuclAccession2TaxId, RefSeq, RefSeqProtein, RefSeqGenomic, NR, NT, GenBank }

// Taxonomy types
pub enum TaxonomyDataSource { Api, User, Accession2Taxid, Header, Inherited, Unknown }

// Chunk types
pub enum ChunkType { Reference, Delta, Metadata, Index }
pub struct ChunkInfo { ... }
pub struct ChunkMetadata { ... }
pub struct DeltaChunk { ... }

// Target aligners
pub enum TargetAligner { Lambda, Diamond, MMseqs2, Blast }

// Version types
pub struct DatabaseVersionInfo { ... }
pub struct TemporalVersionInfo { ... }
pub struct UpdateStatus { ... }

// Storage types
pub struct StorageStats { ... }
pub struct GCResult { ... }
pub struct DetailedStorageStats { ... }

// Output format
pub enum OutputFormat { Fasta, FastaGz, Json, JsonGz, Tsv, TsvGz }
```

### Configuration API

```rust
// Types
pub struct Config { ... }
pub struct ReductionConfig { ... }
pub struct AlignmentConfig { ... }
pub struct OutputConfig { ... }
pub struct PerformanceConfig { ... }
pub struct DatabaseConfig { ... }

// Functions
impl Default for Config { ... }
pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config, TalariaError>
pub fn save_config<P: AsRef<Path>>(path: P, config: &Config) -> Result<(), TalariaError>
```

### Error API

```rust
// Types
pub enum TalariaError { ... }
pub type TalariaResult<T> = Result<T, TalariaError>;
pub struct VerificationError { ... }
pub enum VerificationErrorType { ... }

// Trait implementations
impl std::error::Error for TalariaError
impl std::fmt::Display for TalariaError
impl From<std::io::Error> for TalariaError
impl From<serde_json::Error> for TalariaError
impl From<anyhow::Error> for TalariaError
impl std::error::Error for VerificationError
impl std::fmt::Display for VerificationError
```

### Path API

```rust
// Directory functions
pub fn talaria_home() -> PathBuf
pub fn talaria_data_dir() -> PathBuf
pub fn talaria_databases_dir() -> PathBuf
pub fn talaria_tools_dir() -> PathBuf
pub fn talaria_cache_dir() -> PathBuf
pub fn talaria_workspace_dir() -> PathBuf

// Taxonomy paths
pub fn talaria_taxonomy_versions_dir() -> PathBuf
pub fn talaria_taxonomy_current_dir() -> PathBuf
pub fn talaria_taxonomy_version_dir(version: &str) -> PathBuf

// Canonical sequence paths
pub fn canonical_sequence_storage_dir() -> PathBuf
pub fn canonical_sequence_packs_dir() -> PathBuf
pub fn canonical_sequence_indices_dir() -> PathBuf
pub fn canonical_sequence_index_path() -> PathBuf

// Database paths
pub fn database_path(source: &str, dataset: &str) -> PathBuf
pub fn storage_path() -> PathBuf
pub fn manifest_path(source: &str, dataset: &str) -> PathBuf

// Utilities
pub fn is_custom_data_dir() -> bool
pub fn describe_paths() -> String
pub fn generate_utc_timestamp() -> String
```

### Version API

```rust
// Functions
pub fn parse_version(version_str: &str) -> Result<Version, TalariaError>
pub fn is_compatible(v1: &Version, v2: &Version) -> bool
pub fn current_version() -> Version

// Constants
pub const VERSION: &str     // From Cargo.toml
pub const AUTHORS: &str     // From Cargo.toml
pub const DESCRIPTION: &str // From Cargo.toml
```

## Testing

### Unit Tests

The module includes comprehensive unit tests for all components:

#### Error Tests (`error/mod.rs`)
- Error display formatting
- Error conversions (IO, JSON, Anyhow)
- Error result type usage
- Error chaining and source preservation
- Error type checking with pattern matching

#### Configuration Tests (`tests/config_integration.rs`)
- Loading from multiple sources
- Environment variable handling
- Migration compatibility
- Validation of invalid configs
- Serialization/deserialization preservation
- Concurrent access safety
- Format preservation

#### Path Tests (`tests/path_integration.rs`)
- Path construction consistency
- Path descriptions
- UTC timestamp generation
- Workspace directory structure
- Path hierarchy verification
- Custom data directory detection
- Manifest and database path formatting
- Path separator consistency
- Path length limits

### Integration Tests

```rust
use talaria_core::{Config, load_config, save_config};
use tempfile::TempDir;

#[test]
fn test_config_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("test.toml");

    // Save config
    let config = Config::default();
    save_config(&config_path, &config).unwrap();

    // Load and verify
    let loaded = load_config(&config_path).unwrap();
    assert_eq!(loaded.reduction.target_ratio, config.reduction.target_ratio);
}
```

## Performance Considerations

### Path Caching

All path functions use `OnceLock` for caching:
- First call: ~100μs (environment lookup + path construction)
- Subsequent calls: ~10ns (cached clone)

### Thread Safety

All components are thread-safe:
- `Config`: `Clone + Send + Sync`
- `TalariaError`: `Send + Sync`
- Path functions: Thread-safe via `OnceLock`
- Version functions: Pure functions, inherently thread-safe

### Memory Usage

- `Config`: ~500 bytes
- `TalariaError`: 24-48 bytes (depending on variant)
- Path cache: ~1KB total for all cached paths
- Version: ~32 bytes

## Best Practices

### 1. Configuration

```rust
// DO: Load configuration once at startup
let config = load_config(&config_path).unwrap_or_else(|_| Config::default());

// DON'T: Load configuration repeatedly
for item in items {
    let config = load_config(&config_path)?; // Bad: repeated I/O
}
```

### 2. Error Handling

```rust
// DO: Use specific error variants
return Err(TalariaError::NotFound(format!("Database {} not found", name)));

// DON'T: Use generic Other variant unnecessarily
return Err(TalariaError::Other("Database not found".to_string()));

// DO: Use verification errors for data integrity
return Err(VerificationError::new(
    hash,
    VerificationErrorType::HashMismatch { expected, actual }
));
```

### 3. Path Management

```rust
// DO: Use provided path functions
let db_path = database_path("uniprot", "swissprot");

// DON'T: Construct paths manually
let db_path = format!("{}/.talaria/databases/uniprot/swissprot", home);
```

### 4. Environment Variables

```rust
// DO: Set environment variables before first use
std::env::set_var("TALARIA_HOME", "/data/talaria");
let home = talaria_home(); // Will use /data/talaria

// DON'T: Change environment variables after initialization
let home1 = talaria_home();
std::env::set_var("TALARIA_HOME", "/other/path");
let home2 = talaria_home(); // Still returns original value (cached)
```

## Dependencies

### Core Dependencies
- `thiserror` - Error derive macros
- `serde` / `serde_json` - Serialization
- `toml` - Configuration file format
- `semver` - Semantic versioning
- `chrono` - UTC timestamp generation
- `dirs` - Platform-specific directory resolution

### Utility Dependencies
- `once_cell` - Lazy static alternatives (being replaced by std::sync::OnceLock)
- `tracing` - Structured logging
- `anyhow` - Error handling utilities
- `indicatif` - Progress indicators
- `rayon` - Parallel processing
- `sha2` - Hashing
- `dashmap` - Concurrent HashMap

## Future Improvements

### Planned Features

1. **Configuration Validation**
   - Schema validation for configuration files
   - Runtime validation of configuration values
   - Configuration migration between versions

2. **Enhanced Error Context**
   - Error chains with full context
   - Structured error metadata
   - Error recovery suggestions

3. **Path Management**
   - Automatic directory creation
   - Path validation and sanitization
   - Symlink management for versioning

4. **Observability**
   - Metrics collection
   - Structured logging integration
   - Telemetry support

### API Stability

The following APIs are considered stable:
- All path functions
- Error types and conversions
- Configuration structures
- Version functions

Experimental APIs (may change):
- Internal caching mechanisms
- Configuration schema

## Contributing

### Adding New Configuration Options

1. Add field to appropriate config struct
2. Update `Default` implementation
3. Document the new field
4. Add tests for serialization

### Adding New Error Variants

1. Add variant to `TalariaError` enum
2. Provide descriptive error message
3. Add conversion if needed
4. Update documentation

### Adding New Path Functions

1. Add function to `paths.rs`
2. Consider caching if frequently called
3. Support environment variable override
4. Add tests

## License

This module is part of the Talaria project and follows the same license terms as the parent project.

## Support

For issues or questions related to talaria-core:
- Open an issue on the Talaria GitHub repository
- Check the main Talaria documentation
- Review test cases for usage examples
