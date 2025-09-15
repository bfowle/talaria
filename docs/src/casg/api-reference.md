# CASG API Reference

API reference for the Content-Addressed Sequence Graph (CASG) system as currently implemented in Talaria.

**Note**: This document describes the actual implemented API. Some advanced features mentioned in conceptual documentation are planned for future releases.

## Core Types

### SHA256Hash

Content address for all data in CASG.

```rust
pub struct SHA256Hash([u8; 32]);

impl SHA256Hash {
    pub fn compute(data: &[u8]) -> Self;
    pub fn from_hex(hex: &str) -> Result<Self>;
    pub fn to_hex(&self) -> String;
    pub fn verify(data: &[u8], expected: &Self) -> bool;
}
```

### TaxonId

NCBI taxonomy identifier.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaxonId(pub u32);

impl TaxonId {
    pub const UNCLASSIFIED: Self = Self(0);
    pub const E_COLI: Self = Self(562);
    pub const HUMAN: Self = Self(9606);
}
```

### TemporalManifest

Main manifest tracking database state with bi-temporal versioning.

```rust
pub struct TemporalManifest {
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub sequence_version: String,    // When sequences changed
    pub taxonomy_version: String,    // When taxonomy changed
    pub sequence_root: SHA256Hash,   // Merkle root of sequences
    pub taxonomy_root: SHA256Hash,   // Merkle root of taxonomy
    pub chunk_index: Vec<ChunkMetadata>,
    pub discrepancies: Vec<TaxonomicDiscrepancy>,
    pub etag: Option<String>,
    pub previous_version: Option<String>,
}

impl TemporalManifest {
    pub fn new(seq_ver: &str, tax_ver: &str) -> Self;
    pub fn compute_diff(&self, other: &Self) -> ManifestDiff;
    pub fn verify_integrity(&self) -> Result<()>;
    pub fn to_json(&self) -> Result<String>;
    pub fn from_json(json: &str) -> Result<Self>;
}
```

### ChunkMetadata

Metadata for individual chunks.

```rust
pub struct ChunkMetadata {
    pub hash: SHA256Hash,
    pub taxon_ids: Vec<TaxonId>,
    pub sequence_count: usize,
    pub byte_size: usize,
    pub compressed_size: Option<usize>,
    pub created_at: DateTime<Utc>,
}
```

### TaxonomicDiscrepancy

Tracks mismatches between different taxonomy sources.

```rust
pub struct TaxonomicDiscrepancy {
    pub accession: String,
    pub header_taxon: Option<TaxonId>,
    pub mapping_taxon: Option<TaxonId>,
    pub taxonomy_taxon: Option<TaxonId>,
    pub resolution: DiscrepancyResolution,
}

pub enum DiscrepancyResolution {
    UseHeader,
    UseMapping,
    UseTaxonomy,
    Manual(TaxonId),
}
```

## Storage Layer

### CASGStorage

Main storage interface for chunks.

```rust
pub struct CASGStorage {
    base_path: PathBuf,
    compression: CompressionType,
}

impl CASGStorage {
    pub fn new(path: PathBuf) -> Result<Self>;
    pub fn store_chunk(&mut self, data: &[u8]) -> Result<SHA256Hash>;
    pub fn get_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>>;
    pub fn has_chunk(&self, hash: &SHA256Hash) -> bool;
    pub fn delete_chunk(&mut self, hash: &SHA256Hash) -> Result<()>;
    pub fn get_stats(&self) -> StorageStats;

    // Streaming API
    pub fn get_chunk_stream(&self, hash: &SHA256Hash) -> Result<impl Read>;
    pub fn store_chunk_stream(&mut self, reader: impl Read) -> Result<SHA256Hash>;
}
```

### StorageStats

Statistics about storage usage.

```rust
pub struct StorageStats {
    pub total_chunks: usize,
    pub total_bytes: u64,
    pub compressed_bytes: u64,
    pub deduplication_ratio: f64,
    pub chunk_size_distribution: HashMap<String, usize>,
}
```

## Repository Management

### CASGRepository

Main repository interface for initializing CASG storage.

```rust
pub struct CASGRepository {
    storage: CASGStorage,
    manifests: HashMap<String, TemporalManifest>,
}

impl CASGRepository {
    pub fn init(path: &Path) -> Result<Self>;
    pub fn open(path: &Path) -> Result<Self>;
}
```

### CASGDatabaseManager

Manages database operations with CASG storage.

```rust
pub struct CASGDatabaseManager {
    base_path: PathBuf,
    storage: CASGStorage,
}

impl CASGDatabaseManager {
    pub fn new(base_path: Option<String>) -> Result<Self>;

    // Download operations (handles both initial and updates)
    pub async fn download(
        &mut self,
        source: &DatabaseSource,
        progress: impl Fn(&str)
    ) -> Result<DownloadResult>;

    // Get statistics
    pub fn get_stats(&self) -> Result<CASGStats>;

    // Check for existing manifest
    pub fn get_manifest(&self, source: &str) -> Result<Option<TemporalManifest>>;
}
```

### UpdateInfo

Information about available updates.

```rust
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub new_chunks: Vec<SHA256Hash>,
    pub modified_chunks: Vec<SHA256Hash>,
    pub deleted_chunks: Vec<SHA256Hash>,
    pub download_size: u64,
    pub changes_summary: String,
}
```

## Merkle DAG

### MerkleDAG

Cryptographic proof structure.

```rust
pub struct MerkleDAG {
    root: SHA256Hash,
    nodes: HashMap<SHA256Hash, MerkleNode>,
}

impl MerkleDAG {
    pub fn build_from_chunks(chunks: Vec<Vec<u8>>) -> Result<Self>;
    pub fn compute_root(&self) -> SHA256Hash;
    pub fn generate_proof(&self, data: &[u8]) -> Result<MerkleProof>;
    pub fn verify_proof(proof: &MerkleProof) -> bool;
    pub fn get_depth(&self) -> usize;
}
```

### MerkleProof

Proof of membership in Merkle tree.

```rust
pub struct MerkleProof {
    pub leaf_hash: SHA256Hash,
    pub root_hash: SHA256Hash,
    pub siblings: Vec<SHA256Hash>,
    pub path: Vec<bool>, // Left = false, Right = true
}

impl MerkleProof {
    pub fn verify(&self) -> bool;
    pub fn to_json(&self) -> Result<String>;
    pub fn from_json(json: &str) -> Result<Self>;
}
```

## Assembly

### FastaAssembler

Assembles FASTA files from chunks.

```rust
pub struct FastaAssembler<'a> {
    storage: &'a CASGStorage,
    verify: bool,
}

impl<'a> FastaAssembler<'a> {
    pub fn new(storage: &'a CASGStorage) -> Self;
    pub fn with_verification(mut self, verify: bool) -> Self;

    pub fn assemble_from_manifest(
        &self,
        manifest: &TemporalManifest,
        output: &Path
    ) -> Result<()>;

    pub fn assemble_from_chunks(
        &self,
        chunks: &[SHA256Hash]
    ) -> Result<Vec<u8>>;

    pub fn assemble_taxon(
        &self,
        manifest: &TemporalManifest,
        taxon_id: TaxonId,
        output: &Path
    ) -> Result<()>;

    pub fn stream_assembly(
        &self,
        chunks: &[SHA256Hash],
        writer: impl Write
    ) -> Result<()>;
}
```

## Chunking

### TaxonomyAwareChunker

Smart chunking based on taxonomic relationships.

```rust
pub struct TaxonomyAwareChunker {
    target_size: usize,
    min_size: usize,
    max_size: usize,
}

impl TaxonomyAwareChunker {
    pub fn new() -> Self;
    pub fn with_target_size(mut self, size: usize) -> Self;

    pub fn chunk_sequences(
        &self,
        sequences: Vec<Sequence>
    ) -> Result<Vec<TaxonomyAwareChunk>>;

    pub fn rechunk(
        &self,
        chunks: Vec<TaxonomyAwareChunk>
    ) -> Result<Vec<TaxonomyAwareChunk>>;
}
```

### TaxonomyAwareChunk

Chunk containing related sequences.

```rust
pub struct TaxonomyAwareChunk {
    pub hash: SHA256Hash,
    pub taxon_ids: Vec<TaxonId>,
    pub sequences: Vec<Sequence>,
    pub size: usize,
    pub compressed_size: Option<usize>,
}

impl TaxonomyAwareChunk {
    pub fn from_sequences(sequences: Vec<Sequence>) -> Self;
    pub fn compute_hash(&self) -> SHA256Hash;
    pub fn serialize(&self) -> Result<Vec<u8>>;
    pub fn deserialize(data: &[u8]) -> Result<Self>;
}
```

## Version Identification

### VersionIdentifier

Identifies which version a FASTA file corresponds to.

```rust
pub struct VersionIdentifier {
    repository: CASGRepository,
}

impl VersionIdentifier {
    pub fn new(repo: CASGRepository) -> Self;

    pub fn identify_file(
        &self,
        path: &Path,
        database: Option<&str>
    ) -> Result<VersionInfo>;

    pub fn identify_sequences(
        &self,
        sequences: &[Sequence],
        database: Option<&str>
    ) -> Result<VersionInfo>;
}
```

### VersionInfo

Version identification result.

```rust
pub enum VersionInfo {
    Known {
        database: String,
        version: String,
        sequence_version: String,
        taxonomy_version: String,
        merkle_root: SHA256Hash,
    },
    Modified {
        closest_database: String,
        closest_version: String,
        similarity: f64,
        added_sequences: usize,
        removed_sequences: usize,
        modified_sequences: usize,
    },
    Unknown,
}
```

## Download Results

### DownloadResult

Result of database download operation.

```rust
pub enum DownloadResult {
    /// Database is already up to date
    UpToDate,

    /// Database was updated with incremental changes
    Updated {
        chunks_added: usize,
        chunks_removed: usize,
    },

    /// Initial download completed
    InitialDownload,
}
```

### ManifestStatus

Status of remote manifest.

```rust
pub enum ManifestStatus {
    NotModified,
    Updated {
        etag: String,
        last_modified: DateTime<Utc>,
    },
    Error(String),
}
```

## Statistics

### CASGStats

Repository statistics returned by `get_stats()`.

```rust
pub struct CASGStats {
    pub total_chunks: usize,
    pub total_size: u64,
    pub compressed_chunks: usize,
    pub deduplication_ratio: f64,
    pub database_count: usize,
    pub databases: Vec<DatabaseStats>,
}

pub struct DatabaseStats {
    pub name: String,
    pub version: String,
    pub chunk_count: usize,
    pub total_size: u64,
}
```

## Error Types

### CASGError

Main error type for CASG operations.

```rust
#[derive(Debug, thiserror::Error)]
pub enum CASGError {
    #[error("Storage error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("Verification failed: expected {expected}, got {actual}")]
    VerificationFailed {
        expected: SHA256Hash,
        actual: SHA256Hash,
    },

    #[error("Chunk not found: {0}")]
    ChunkNotFound(SHA256Hash),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Manifest error: {0}")]
    Manifest(String),

    #[error("Version conflict: {0}")]
    VersionConflict(String),
}
```

## Usage Examples

### Command Line Usage

```bash
# Initialize CASG repository
talaria casg init

# Download database (handles both initial download and updates)
talaria database download uniprot -d swissprot
# Running again will check for updates and only download changes

# Add custom database
talaria database add -i sequences.fasta --source mylab --dataset proteins

# Show repository statistics
talaria casg stats

# List databases
talaria database list

# Get database info
talaria database info uniprot/swissprot

# List sequences in database
talaria database list-sequences uniprot/swissprot --limit 100

# Reduce database
talaria reduce uniprot/swissprot -r 0.3 -o reduced.fasta

# Validate reduction
talaria validate uniprot/swissprot:30-percent

# Reconstruct sequences
talaria reconstruct uniprot/swissprot:30-percent -o reconstructed.fasta
```

### Rust API Usage

```rust
use talaria::casg::CASGRepository;
use talaria::core::casg_database_manager::CASGDatabaseManager;

// Initialize repository
let repo = CASGRepository::init("/data/casg")?;

// Create database manager
let mut manager = CASGDatabaseManager::new(Some("/data/casg".to_string()))?;

// Download database (automatically handles updates)
let result = manager.download(&DatabaseSource::UniProt(UniProtDatabase::SwissProt),
                              |msg| println!("{}", msg)).await?;

match result {
    DownloadResult::UpToDate => println!("Already up to date"),
    DownloadResult::Updated { chunks_added, .. } =>
        println!("Updated: {} new chunks", chunks_added),
    DownloadResult::InitialDownload => println!("Initial download complete"),
}

// Get statistics
let stats = manager.get_stats()?;
println!("Total chunks: {}", stats.total_chunks);
```

## See Also

- [CASG Overview](overview.md) - High-level introduction
- [Architecture](architecture.md) - System design details
- [Troubleshooting](troubleshooting.md) - Common issues and solutions
- [CLI Reference](../api/cli-reference.md#casg) - Command-line interface