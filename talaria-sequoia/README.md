# SEQUOIA - Sequence Query Optimization with Indexed Architecture

## Overview

SEQUOIA (Sequence Query Optimization with Indexed Architecture) is the core storage and indexing engine for Talaria. It provides a content-addressed storage system with cryptographic verification, bi-temporal versioning, and taxonomy-aware chunking for efficient management of biological sequence databases.

### Key Features

- **Content-Addressed Storage**: Deduplication through SHA256 hashing
- **Merkle DAG Verification**: Cryptographic proof of data integrity
- **Bi-Temporal Database**: Independent versioning of sequence data and taxonomy
- **Taxonomy-Aware Chunking**: Intelligent grouping by taxonomic classification
- **Delta Encoding**: Efficient storage through reference-based compression
- **Cloud-Native**: S3/GCS/Azure blob storage support with local caching
- **Incremental Updates**: Only download changed chunks during updates
- **Cross-Database Deduplication**: Canonical sequences shared across databases
- **Retroactive Analysis**: Query historical states and track evolution

## Download Management

### Resume Capability

SEQUOIA includes robust download management with automatic resume capability:

- **Automatic Discovery**: Finds existing downloads for the same database source
- **State Persistence**: Preserves download progress across interruptions
- **Workspace Isolation**: Each download gets a unique workspace preventing collisions
- **Multi-Stage Resume**: Can resume at any stage (download, decompress, process)
- **Smart Selection**: Automatically chooses the most recent workspace when multiple exist

### Workspace Structure

Downloads are managed in isolated workspaces:

```
${TALARIA_DATA_DIR}/downloads/{database}_{version}_{session}/
├── state.json          # Persistent state for resume
├── *.fasta.gz         # Compressed download
├── *.fasta            # Decompressed file
├── chunks/            # Processing artifacts
└── .lock             # Process lock file
```

### Resume Example

```bash
# Start a large download
talaria database download uniprot/uniref50

# If interrupted, resume from where it left off
talaria database download uniprot/uniref50 --resume

# Messages show resume status:
# ✓ Found existing download at: ~/.talaria/downloads/uniprot_uniref50_...
# ├─ Found download from 2 hours ago
# └─ Download was 42% complete (5.77 GB of 13.72 GB)
# ▶ Resuming download from byte position 5.77 GB...
```

### Environment Variables for Downloads

- `TALARIA_PRESERVE_DOWNLOADS`: Keep download workspace after successful processing
- `TALARIA_PRESERVE_ON_FAILURE`: Keep workspace on errors for debugging
- `TALARIA_PRESERVE_ALWAYS`: Never clean up workspaces

## Architecture

### Core Components

```
talaria-sequoia/
├── src/
│   ├── types.rs                        # Core type definitions
│   ├── traits/                         # Capability traits
│   │   ├── temporal.rs                 # Temporal query traits
│   │   └── renderable.rs               # Rendering traits
│   ├── storage/                        # Storage layer
│   │   ├── core.rs                     # SequoiaStorage implementation
│   │   ├── sequence.rs                 # Canonical sequence storage
│   │   ├── packed.rs                   # Pack file backend
│   │   ├── indices.rs                  # Fast lookup indices
│   │   ├── chunk_index.rs              # Chunk indexing
│   │   ├── compression.rs              # Zstd compression with dictionaries
│   │   └── format.rs                   # Serialization formats
│   ├── manifest/                       # Manifest management
│   │   ├── core.rs                     # Main manifest structure
│   │   └── taxonomy.rs                 # Taxonomy-specific manifests
│   ├── chunker/                        # Chunking strategies
│   │   ├── canonical_taxonomic.rs      # Taxonomy-based chunking
│   │   └── hierarchical_taxonomic.rs   # Hierarchical taxonomy chunking
│   ├── delta/                          # Delta encoding
│   │   ├── traits.rs                   # Delta generation/reconstruction
│   │   ├── generator.rs                # Delta generation implementation
│   │   ├── reconstructor.rs            # Delta reconstruction
│   │   └── canonical.rs                # Canonical delta compression with banded Myers algorithm
│   ├── temporal/                       # Temporal features
│   │   ├── core.rs                     # Temporal index
│   │   ├── bi_temporal.rs              # Bi-temporal database
│   │   ├── retroactive.rs              # Retroactive analysis
│   │   ├── renderable.rs               # Temporal rendering
│   │   └── version_store.rs            # Version management
│   ├── verification/                   # Verification and validation
│   │   ├── merkle.rs                   # Merkle DAG implementation
│   │   ├── verifier.rs                 # Cryptographic verification
│   │   └── validator.rs                # Manifest validation
│   ├── operations/                     # Database operations
│   │   ├── assembler.rs                # FASTA reconstruction
│   │   ├── differ.rs                   # Manifest comparison
│   │   ├── reduction.rs                # Reduction operations
│   │   └── state.rs                    # Processing state management
│   ├── taxonomy/                       # Taxonomy management
│   │   ├── mod.rs                      # TaxonomyManager
│   │   ├── evolution.rs                # Taxonomy evolution tracking
│   │   ├── filter.rs                   # Boolean taxonomy filtering
│   │   ├── extractor.rs                # Taxonomy extraction
│   │   ├── discrepancy.rs              # Discrepancy detection
│   │   ├── manifest.rs                 # Taxonomy manifests
│   │   └── version_store.rs            # Taxonomy versioning
│   ├── database/                       # Database management
│   │   ├── manager.rs                  # Database manager
│   │   └── diff.rs                     # Database diffing
│   ├── download/                       # Download handlers with resume support
│   │   ├── manager.rs                  # Download state machine manager
│   │   ├── workspace.rs                # Workspace isolation & locking
│   │   ├── ncbi.rs                     # NCBI database downloads
│   │   ├── uniprot.rs                  # UniProt database downloads
│   │   ├── resumable_downloader.rs     # Resumable download implementation
│   │   └── progress.rs                 # Download progress tracking
│   ├── processing/                     # Processing pipeline
│   │   ├── pipeline.rs                 # Processing pipeline implementation
│   │   └── traits.rs                   # Processing traits
│   ├── backup/                         # Backup functionality
│   │   └── mod.rs                      # Backup operations
│   └── cloud/                          # Cloud storage integration
│       ├── mod.rs                      # Cloud abstraction
│       └── s3.rs                       # S3 implementation
```

### Design Principles

1. **Content Addressing**: All data is identified by cryptographic hashes
2. **Immutability**: Once written, chunks are never modified
3. **Deduplication**: Identical sequences stored only once
4. **Verification**: Every chunk can be cryptographically verified
5. **Temporal Independence**: Sequence and taxonomy versions evolve independently
6. **Lazy Loading**: Data fetched only when needed
7. **Format Agnostic**: Support for JSON, MessagePack, and Talaria binary formats

## Data Model

### Core Types

#### SHA256Hash
Content identifier for all data in the system.

```rust
pub struct SHA256Hash([u8; 32]);
```

#### BiTemporalCoordinate
Represents a point in bi-temporal space.

```rust
pub struct BiTemporalCoordinate {
    pub sequence_time: DateTime<Utc>,
    pub taxonomy_time: DateTime<Utc>,
}
```

#### ChunkManifest
Metadata for a content-addressed chunk.

```rust
pub struct ChunkManifest {
    pub chunk_hash: SHA256Hash,
    pub classification: ChunkClassification,
    pub taxon_ids: Vec<TaxonId>,
    pub sequence_refs: Vec<SequenceRef>,
    pub delta_refs: Vec<DeltaRef>,
}
```

#### TemporalManifest
Complete manifest with bi-temporal versioning.

```rust
pub struct TemporalManifest {
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub sequence_version: String,
    pub taxonomy_version: String,
    pub temporal_coordinate: Option<BiTemporalCoordinate>,
    pub taxonomy_root: MerkleHash,
    pub sequence_root: MerkleHash,
    pub chunk_index: Vec<ManifestMetadata>,
}
```

## Storage Architecture

### Content-Addressed Storage with Unified Packed Backend

All data in SEQUOIA is content-addressed using SHA256 hashes and stored in efficient pack files:

```
.talaria/
├── databases/
│   ├── sequences/         # Global canonical sequences
│   │   ├── packs/        # Pack files (NOT individual files!)
│   │   └── indices/      # Fast hash lookups
│   └── data/
│       └── [database]/
│           ├── chunk_packs/  # Chunk manifests in packs
│           ├── manifests/    # Temporal manifests
│           └── indices/      # Database-specific indices
```

**Key Innovation**: No more millions of individual files! Everything uses packed storage.

### Why Unified Packed Storage Matters

#### For End Users
- **Instant Startup**: No more waiting 10+ seconds for the system to scan directories
- **Fast Downloads**: Resume capability with isolated workspaces
- **Reliable Backups**: Backup 400 files instead of millions
- **Cloud Compatible**: Pack files work perfectly with S3/GCS/Azure

#### For System Administrators
- **No Inode Exhaustion**: 400 files instead of 2.2M means no filesystem limits
- **Fast Transfers**: `rsync` or `scp` 400 files in seconds, not hours
- **Easy Monitoring**: Simple to track pack file growth and usage
- **Disaster Recovery**: Quick restore from backup or cloud storage

#### For Developers
- **Single Code Path**: One trait (`PackedStorageBackend`) handles everything
- **No Migration Complexity**: Clean architecture, no backwards compatibility baggage
- **Predictable Performance**: O(1) lookups with in-memory index
- **Extensible Design**: Easy to add new data types using same backend

### Chunking Strategy

Sequences are grouped into chunks based on taxonomy:

1. **Taxonomic Grouping**: Sequences with same `TaxonId` grouped together
2. **Size Limits**: Chunks between 1MB-10MB (configurable)
3. **Compression**: Per-taxon Zstd dictionaries for better compression
4. **Delta Encoding**: Similar sequences stored as deltas from references using banded Myers algorithm

#### Banded Myers Delta Algorithm

SEQUOIA uses an optimized banded Myers diff algorithm for delta encoding of similar sequences:

**Algorithm Features:**
- **Time Complexity**: O(k*min(n,m)) where k = max_distance, instead of O(n*m)
- **Space Complexity**: O(max_distance) instead of O(n*m)
- **Early Termination**: Rejects dissimilar sequences when edit distance exceeds threshold
- **Configurable Banding**: Can disable banding for testing/debugging

**Benefits for Biological Sequences:**
- Related sequences typically have small edit distances (SNPs, indels)
- Fast rejection of unrelated sequences saves computation
- Memory efficient for large genomes
- Achieves 10-100x compression for similar sequences

**Configuration:**
```rust
// Create compressor with max_distance=1000, banded mode enabled
let compressor = MyersDeltaCompressor::new(1000, true);

// Disable banding for unbounded search
let unbanded = MyersDeltaCompressor::new(1000, false);
```

**Performance:**
- Sequences with <10 edits: 5-10ms per comparison
- Dissimilar sequences (>max_distance): <1ms rejection
- Compression ratio: 0.05-0.3 for biological variants

### Unified Packed Storage Architecture

**Revolutionary Change**: SEQUOIA now uses unified packed storage for BOTH sequences AND chunk manifests, achieving unprecedented performance and simplification.

#### The Problem We Solved
Previously, SEQUOIA created individual files for each chunk and sequence:
- 2M sequences = 2M individual files
- 224K chunk manifests = 224K individual files
- **Total: 2.2 MILLION files** causing filesystem exhaustion

#### The Solution: Unified Pack Files
Now everything uses the same `PackedStorageBackend`:
- 2M sequences → ~200 pack files (64MB each)
- 224K chunks → ~200 pack files
- **Total: ~400 pack files** (5,500× reduction!)

#### Storage Hierarchy
```
.talaria/
├── databases/
│   ├── sequences/           # Global canonical sequences
│   │   ├── packs/          # ~200 pack files for 2M sequences
│   │   │   ├── pack_0001.tal
│   │   │   ├── pack_0002.tal
│   │   │   └── ...
│   │   └── indices/        # Hash → pack location index
│   │       └── sequence_index.tal
│   └── data/
│       └── [database]/
│           └── chunk_packs/ # Database-specific chunk manifests
│               ├── packs/   # ~200 pack files for 200K chunks
│               │   ├── pack_0001.tal
│               │   └── ...
│               └── indices/
│                   └── chunk_index.tal
```

#### Pack File Format

Both sequences and chunks use identical format:

```
Pack File Structure (64MB):
[Header: Magic + Version + ID]     # 9 bytes
[Entry 1: Length + Data]           # Variable
[Entry 2: Length + Data]           # Variable
...
[Entry N: Length + Data]           # Variable
[Footer: Entry Count]              # 4 bytes
[Zstandard Compression]            # Entire file compressed
```

#### Performance Impact

| Metric | Before (Individual Files) | After (Packed Storage) | Improvement |
|--------|--------------------------|------------------------|-------------|
| File Count | 2.2M files | 400 files | **5,500×** fewer |
| Startup Time | 10-30 seconds | <1 second | **30×** faster |
| Import Speed | 5K seq/sec | 50K+ seq/sec | **10×** faster |
| Backup Time | Hours | Seconds | **1000×** faster |
| Directory Listing | Minutes | Instant | **∞** faster |
| Inode Usage | 2.2M inodes | 400 inodes | **5,500×** fewer |

#### The PackedStorageBackend Trait

Single trait powers everything:

```rust
pub trait PackedStorageBackend: Send + Sync {
    fn exists(&self, hash: &SHA256Hash) -> Result<bool>;
    fn store(&self, hash: &SHA256Hash, data: &[u8]) -> Result<()>;
    fn load(&self, hash: &SHA256Hash) -> Result<Vec<u8>>;
    fn flush(&self) -> Result<()>;
    fn get_stats(&self) -> Result<StorageStats>;
}
```

This unified approach means:
- **Same code** handles sequences and chunks
- **Same optimizations** apply everywhere
- **Same reliability** across all data types
- **No special cases** or complex branching

## Temporal Features

### Bi-Temporal Database

SEQUOIA supports two independent time dimensions:

1. **Sequence Time**: When sequence data changes
2. **Taxonomy Time**: When taxonomy understanding changes

This enables queries like:
- "Give me the database as of January with March taxonomy"
- "Show how taxonomy reclassification affects sequence organization"
- "Track sequence additions over time with consistent taxonomy"

### Version Store

```rust
let version_store = FilesystemVersionStore::new(path)?;

// List versions
let versions = version_store.list_versions(ListOptions {
    after: Some(DateTime::from(2024, 1, 1)),
    before: None,
    limit: 10,
})?;

// Get specific version
let v1_0_0 = version_store.get_version("v1.0.0")?;
```

### Retroactive Analysis

Analyze how past data would be classified with current taxonomy:

```rust
let analyzer = RetroactiveAnalyzer::new(repository)?;

// Analyze impact of taxonomy update
let impact = analyzer.analyze_taxonomy_impact(
    old_taxonomy_version,
    new_taxonomy_version
)?;

// Find reclassified sequences
let reclassified = analyzer.find_reclassified_sequences(
    taxon_id,
    time_range
)?;
```

## Verification System

### Merkle DAG

All data is organized into a Merkle Directed Acyclic Graph:

```
         Root
        /    \
   Taxonomy  Sequences
      |         |
   [Nodes]  [Chunks]
      |         |
   [Taxa]   [Sequences]
```

### Verification Process

```rust
let verifier = SequoiaVerifier::new(&storage);

// Verify entire manifest
let result = verifier.verify_manifest(&manifest)?;

// Verify specific chunk
let chunk_valid = verifier.verify_chunk(&chunk_hash)?;

// Generate proof
let proof = verifier.generate_proof(&sequence_hash)?;
```

### Validation

```rust
let validator = StandardTemporalManifestValidator::new();

let result = validator.validate(
    &manifest,
    ValidationOptions {
        check_chunks: true,
        check_taxonomy: true,
        check_temporal: true,
    }
)?;
```

## Download Management

### Resumable Downloads with Workspace Isolation

SEQUOIA implements a robust download system with automatic resume capability and workspace isolation for concurrent operations:

#### Features

- **State Machine Architecture**: Downloads progress through well-defined stages with checkpoint recovery
- **Workspace Isolation**: Each download gets a unique workspace preventing conflicts
- **Automatic Resume**: Interrupted downloads resume from the last successful stage
- **Lock-Based Concurrency**: File-based locks prevent concurrent access to same download
- **Selective Cleanup**: Failed downloads preserve critical files for retry

#### Download State Machine

Downloads progress through these stages:

1. **Initializing** → Setting up workspace
2. **Downloading** → Fetching database with byte-level resume
3. **Verifying** → Checksum verification (optional)
4. **Decompressing** → Extracting compressed files
5. **Processing** → Converting to SEQUOIA chunks
6. **Finalizing** → Moving to final location
7. **Complete** → Successfully finished

Each stage transition creates a checkpoint for recovery.

#### Workspace Structure

```
.talaria/downloads/
├── uniprot_swissprot_20240326_a1b2c3d4/
│   ├── state.json           # Download state machine
│   ├── .lock                # Process lock file
│   ├── uniprot_sprot.fasta.gz.tmp  # Partial download
│   ├── uniprot_sprot.fasta  # Decompressed file
│   └── chunks/              # Processing directory
```

Each workspace is named: `{database}_{version}_{session_id}`

#### Resume Capabilities

```rust
use talaria_sequoia::download::{DownloadManager, DownloadOptions};

let mut manager = DownloadManager::new()?;

let options = DownloadOptions {
    resume: true,              // Enable resume (default)
    preserve_on_failure: true, // Keep files on failure
    skip_verify: false,        // Verify checksums
    force: false,              // Don't force re-download
    preserve_always: false,    // Clean on success
};

// Download with automatic resume on failure
let path = manager.download_with_state(
    source,
    options,
    &mut progress
).await?;
```

#### Recovery Commands

List resumable downloads:
```bash
talaria database list-resumable
```

Resume specific download:
```bash
talaria database resume <download_id>
```

Clean old download workspaces:
```bash
talaria database clean-downloads --max-age-hours 168
```

#### Environment Variables

Control download behavior with environment variables:

```bash
# Preserve files on failure for debugging
TALARIA_PRESERVE_ON_FAILURE=1

# Always preserve workspace (debugging)
TALARIA_PRESERVE_ALWAYS=1

# Keep downloaded files after processing
TALARIA_PRESERVE_DOWNLOADS=1
```

#### Error Recovery

The download manager handles various failure scenarios:

- **Network Interruption**: Resume from last downloaded byte
- **Disk Full**: Preserve compressed file for retry after space is freed
- **Processing Failure**: Keep decompressed file to avoid re-download
- **Corrupted State**: Start fresh if state file is unreadable
- **Stale Locks**: Automatic cleanup of locks from dead processes

## Operations

### Database Reduction

Convert FASTA files to SEQUOIA format:

```rust
let reduction = ReductionManager::new(storage)?;

let manifest = reduction.reduce(
    input_fasta,
    ReductionParameters {
        chunking_strategy: ChunkingStrategy::Taxonomic,
        compression_level: 19,
        enable_delta: true,
        reference_selection: ReferenceSelection::Automatic,
    }
)?;
```

### FASTA Assembly

Reconstruct FASTA from SEQUOIA:

```rust
let assembler = FastaAssembler::new(&storage);

// Assemble all sequences
assembler.assemble_all(&manifest, output_path)?;

// Assemble specific taxon
assembler.assemble_taxon(&manifest, taxon_id, output_path)?;

// Assemble with filters
assembler.assemble_filtered(&manifest, filter, output_path)?;
```

### Manifest Diffing

Compare manifests to find changes:

```rust
let differ = StandardTemporalManifestDiffer;

let diff = differ.diff(
    &old_manifest,
    &new_manifest,
    DiffOptions {
        include_sequences: true,
        include_taxonomy: true,
    }
)?;

// Process changes
for change in diff.changes {
    match change.change_type {
        ChangeType::Added =>        // New sequences
        ChangeType::Removed =>      // Deleted sequences
        ChangeType::Modified =>     // Updated sequences
        ChangeType::Reclassified => // Taxonomy changes
    }
}
```

### Database Comparison

Comprehensive comparison between databases:

```rust
use talaria_sequoia::operations::DatabaseDiffer;

// Compare two databases
let differ = DatabaseDiffer::new(path_a, path_b)?;
let comparison = differ.compare()?;

// Chunk-level analysis
println!("Total chunks in A: {}", comparison.chunk_analysis.total_chunks_a);
println!("Total chunks in B: {}", comparison.chunk_analysis.total_chunks_b);
println!("Shared chunks: {} ({:.1}% / {:.1}%)",
    comparison.chunk_analysis.shared_chunks.len(),
    comparison.chunk_analysis.shared_percentage_a,
    comparison.chunk_analysis.shared_percentage_b);

// Sequence-level analysis
println!("Total sequences in A: {}", comparison.sequence_analysis.total_sequences_a);
println!("Total sequences in B: {}", comparison.sequence_analysis.total_sequences_b);
println!("Shared sequences: {}", comparison.sequence_analysis.shared_sequences);

// Taxonomy distribution
println!("Taxa in A: {}", comparison.taxonomy_analysis.total_taxa_a);
println!("Taxa in B: {}", comparison.taxonomy_analysis.total_taxa_b);
println!("Shared taxa: {}", comparison.taxonomy_analysis.shared_taxa.len());

// Top shared taxa
for taxon in &comparison.taxonomy_analysis.top_shared_taxa {
    println!("{} ({}): {} / {} sequences",
        taxon.taxon_name,
        taxon.taxon_id.0,
        taxon.count_in_a,
        taxon.count_in_b);
}

// Storage metrics
println!("Size A: {}", format_bytes(comparison.storage_metrics.size_a_bytes));
println!("Size B: {}", format_bytes(comparison.storage_metrics.size_b_bytes));
println!("Deduplication savings: {}",
    format_bytes(comparison.storage_metrics.dedup_savings_bytes));
```

## Taxonomy Management

### TaxonomyManager

Central taxonomy management:

```rust
let mut taxonomy = TaxonomyManager::new(path)?;

// Load NCBI taxonomy
taxonomy.load_ncbi_taxonomy(taxdump_dir)?;

// Map accessions to taxa
taxonomy.load_uniprot_mapping(idmapping_file)?;

// Query taxonomy
let lineage = taxonomy.get_lineage(&taxon_id)?;
let parent = taxonomy.get_parent(taxon_id)?;
let descendants = taxonomy.get_descendant_taxa(&taxon_id)?;
```

### Evolution Tracking

Track taxonomy changes over time:

```rust
let tracker = TaxonomyEvolutionTracker::new(path)?;

// Track mass reclassification
let reclassification = tracker.track_reclassification(
    old_taxon_id,
    new_taxon_id,
    affected_sequences
)?;

// Generate evolution report
let report = tracker.generate_report(
    start_version,
    end_version
)?;
```

### Discrepancy Detection

Find inconsistencies in taxonomy annotations:

```rust
let discrepancies = taxonomy.detect_discrepancies(&storage)?;

for discrepancy in discrepancies {
    match discrepancy.discrepancy_type {
        DiscrepancyType::Missing =>      // No taxonomy info
        DiscrepancyType::Conflict =>     // Sources disagree
        DiscrepancyType::Outdated =>     // Using old taxonomy
        DiscrepancyType::Reclassified => // Needs update
    }
}
```

## Cloud Integration

### S3 Storage Backend

```rust
use talaria_sequoia::cloud::S3Backend;

let backend = S3Backend::new(S3Config {
    bucket: "my-sequoia-bucket",
    prefix: "databases/",
    region: Region::UsEast1,
})?;

// Transparent cloud storage
let storage = SequoiaStorage::with_backend(Box::new(backend))?;
```

### Hybrid Storage

Local cache with cloud backing:

```rust
let storage = SequoiaStorage::hybrid(
    local_path,
    cloud_backend,
    CachePolicy {
        max_size: 10_000_000_000, // 10GB
        eviction: EvictionPolicy::LRU,
    }
)?;
```

## Performance

### Indexing

Multiple index types for fast lookups:

1. **Bloom Filters**: O(1) existence checks with low false positive rate
2. **B-Tree Indices**: Sorted access by accession/taxon
3. **Hash Maps**: Direct lookups by hash
4. **Inverted Indices**: Find chunks containing specific sequences

### Compression

- **Zstd Level 19**: High compression for long-term storage
- **Taxonomy Dictionaries**: Trained per-taxon for better ratios
- **Delta Encoding**: Store similar sequences as differences
- **Pack Files**: Reduce filesystem overhead

### Parallelism

- **Rayon**: Parallel chunk processing
- **DashMap**: Concurrent hash maps
- **Arc/Mutex**: Thread-safe shared state
- **Async I/O**: Non-blocking cloud operations

## Integration with Talaria

### Dependencies

SEQUOIA integrates with other Talaria modules:

- **talaria-core**: Core types (`SHA256Hash`, `TaxonId`, `DatabaseReference`, `ChunkMetadata`, `StorageStats`)
- **talaria-bio**: Biological sequence handling (using talaria-core `SequenceType`)
- **talaria-storage**: Storage abstractions (using `StorageChunkInfo`)
- **talaria-utils**: Display and workspace utilities (`WorkspaceConfig`, `WorkspaceStats`)
- **talaria-tools**: External tool integration

### Dev Dependencies

For testing and development:

- **mockall = "0.12"**: Mock implementations for testing external dependencies
- **serial_test = "3.0"**: Serial test execution for tests modifying global state
- **tempfile**: Temporary directory management with RAII cleanup
- **criterion = "0.5"**: Benchmarking framework
- **proptest = "1.3"**: Property-based testing
- **pretty_assertions = "1.4"**: Enhanced assertion output

### Usage in Talaria CLI

```rust
// In talaria-cli
use talaria_sequoia::{SequoiaRepository, ReductionParameters};

let repo = SequoiaRepository::init(path)?;

// Reduce FASTA to SEQUOIA
let manifest = repo.reduce(
    input_fasta,
    parameters
)?;

// Query by taxonomy
let sequences = repo.query_taxon(taxon_id)?;
```

## Configuration

### Environment Variables

```bash
# Storage paths
TALARIA_SEQUOIA_DIR=$TALARIA_HOME/sequoia
TALARIA_SEQUOIA_CACHE_DIR=$TALARIA_CACHE_DIR/sequoia

# Performance
TALARIA_SEQUOIA_THREADS=8
TALARIA_SEQUOIA_COMPRESSION_LEVEL=19

# Cloud
TALARIA_SEQUOIA_S3_BUCKET=my-bucket
TALARIA_SEQUOIA_S3_REGION=us-east-1
```

### Configuration File

```toml
[sequoia]
chunk_size_min = 1_000_000  # 1MB
chunk_size_max = 10_000_000 # 10MB
compression_level = 19
enable_delta_encoding = true

[sequoia.indices]
use_bloom_filter = true
bloom_false_positive_rate = 0.001

[sequoia.cache]
max_size = 10_000_000_000  # 10GB
eviction = "lru"
```

## Test Coverage

SEQUOIA has comprehensive test coverage with **145+ tests** ensuring reliability and correctness:

### Test Statistics
- **100 unit tests** across 21 test modules in `src/`
- **34 integration tests** in `tests/sequoia_integration/`
- **1 benchmark suite** for performance testing

### Test Modules
- **Unit Tests**: Core functionality testing within each module
  - Storage operations (chunking, compression, deduplication)
  - Manifest management (creation, serialization, validation)
  - Verification system (Merkle proofs, chunk verification)
  - Temporal operations (versioning, bi-temporal queries)

- **Integration Tests**:
  - `basic_operations` - Core SEQUOIA operations
  - `bi_temporal_test` - Bi-temporal database functionality
  - `cloud_sync_tests` - Cloud synchronization with mocks
  - `end_to_end_tests` - Complete reduction workflows
  - `error_handling` - Error recovery and edge cases
  - `manifest_operations` - Manifest manipulation
  - `temporal_operations` - Temporal versioning

### Testing Best Practices
- Mock implementations using `mockall` for external dependencies
- RAII pattern with `TempDir` for automatic test cleanup
- Serial test execution for global state modifications
- Property-based testing for algorithmic correctness
- Comprehensive error path coverage

## Development

### Running Tests

```bash
# All tests for SEQUOIA
cargo test -p talaria-sequoia

# Unit tests only
cargo test -p talaria-sequoia --lib

# Specific integration test module
cargo test -p talaria-sequoia --test basic_operations
cargo test -p talaria-sequoia --test bi_temporal_test
cargo test -p talaria-sequoia --test end_to_end_tests

# With cloud features
cargo test -p talaria-sequoia --features cloud

# Benchmarks
cargo bench -p talaria-sequoia

# With debug logging
RUST_LOG=debug cargo test -p talaria-sequoia

# With trace-level logging for specific module
RUST_LOG=talaria_sequoia::storage=trace cargo test -p talaria-sequoia
```

### Example Usage

```rust
use talaria_sequoia::{
    SequoiaRepository,
    ChunkingStrategy,
    ReductionParameters,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize repository
    let repo = SequoiaRepository::init("./sequoia-db")?;

    // Configure reduction
    let params = ReductionParameters {
        chunking_strategy: ChunkingStrategy::Taxonomic,
        compression_level: 19,
        enable_delta: true,
    };

    // Reduce FASTA file
    let manifest = repo.storage.reduce_fasta(
        "input.fasta",
        params
    )?;

    // Query sequences
    let human_sequences = repo.taxonomy
        .get_chunks_for_taxon("Homo sapiens")?;

    // Assemble output
    let assembler = FastaAssembler::new(&repo.storage);
    assembler.assemble_chunks(
        &human_sequences,
        "human_output.fasta"
    )?;

    Ok(())
}
```

### Contributing

1. Follow Rust idioms and conventions
2. Add tests for new functionality
3. Update documentation
4. Run `cargo fmt` and `cargo clippy`
5. Ensure all tests pass

## Performance Metrics

Typical performance on modern hardware:

- **Reduction**: ~100MB/s (FASTA to SEQUOIA)
- **Assembly**: ~200MB/s (SEQUOIA to FASTA)
- **Compression Ratio**: 5-10x (depending on data)
- **Deduplication**: 20-40% storage savings
- **Query Latency**: <10ms for indexed lookups
- **Verification**: ~500MB/s chunk verification

## Troubleshooting

### Common Issues

1. **Out of Memory**: Reduce chunk size or increase system RAM
2. **Slow Reduction**: Enable parallel processing with `TALARIA_THREADS`
3. **Storage Space**: Enable compression and deduplication
4. **Network Latency**: Use local cache for cloud backends
5. **Corruption**: Run verification to detect and repair

### Download Issues

6. **Download Interrupted**:
   - Run `talaria database list-resumable` to see interrupted downloads
   - Use `talaria database resume <id>` to continue from last checkpoint
   - Set `TALARIA_PRESERVE_ON_FAILURE=1` to keep files for debugging

7. **Download Already in Progress**:
   - Check for stale locks: `ls ~/.talaria/downloads/*/. lock`
   - If process is dead, remove lock file and retry
   - Use `talaria database clean-downloads` to clean stale workspaces

8. **Disk Full During Download**:
   - Free disk space
   - Resume download with `talaria database resume <id>`
   - Compressed file is preserved, won't re-download

9. **Checksum Verification Failed**:
   - Download will be marked for retry
   - Check network stability
   - Use `--skip-verify` flag if checksums unavailable

10. **Processing Failed After Download**:
    - Downloaded file is preserved in workspace
    - Fix the issue (e.g., memory, disk space)
    - Resume with `talaria database resume <id>`
    - No re-download needed

### Debug Output

```bash
# Enable debug logging
RUST_LOG=talaria_sequoia=debug cargo run

# Trace-level logging
RUST_LOG=talaria_sequoia=trace cargo run

# Specific module
RUST_LOG=talaria_sequoia::storage=debug cargo run
```

## Future Enhancements

- **Graph-based Storage**: Sequence relationship graphs
- **Machine Learning**: Smart reference selection
- **Distributed Processing**: Multi-node reduction
- **Real-time Sync**: Live replication to cloud
- **Custom Compression**: Sequence-specific algorithms
- **Query Language**: SQL-like queries for sequences
- **Visualization**: Interactive exploration tools

## License

Licensed under the same terms as the Talaria project.

## References

- [Content-Addressed Storage](https://en.wikipedia.org/wiki/Content-addressable_storage)
- [Merkle Trees](https://en.wikipedia.org/wiki/Merkle_tree)
- [Bi-temporal Databases](https://en.wikipedia.org/wiki/Temporal_database)
- [Delta Encoding](https://en.wikipedia.org/wiki/Delta_encoding)
- [Zstandard Compression](https://facebook.github.io/zstd/)
