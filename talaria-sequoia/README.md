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
│   │   ├── core.rs                     # SEQUOIAStorage implementation
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
│   │   └── canonical.rs                # Canonical delta compression
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
│   ├── download/                       # Download handlers
│   │   ├── ncbi.rs                     # NCBI database downloads
│   │   ├── uniprot.rs                  # UniProt database downloads
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

### Content-Addressed Storage

All sequence data is stored using content addressing with SHA256 hashes:

```
.talaria/sequoia/
├── chunks/            # Content-addressed chunks
│   ├── ab/            # First 2 chars of hash
│   │   └── abcd...    # Full hash as filename
├── manifests/         # Temporal manifests
│   ├── v1.0.0.json
│   └── v1.1.0.msgpack
├── indices/           # Fast lookup indices
│   ├── accession.idx
│   ├── taxon.idx
│   └── bloom.filter
└── packs/             # Packed sequence storage
    ├── pack-001.dat   # Pack file with multiple sequences
    └── pack-001.idx   # Pack index
```

### Chunking Strategy

Sequences are grouped into chunks based on taxonomy:

1. **Taxonomic Grouping**: Sequences with same `TaxonId` grouped together
2. **Size Limits**: Chunks between 1MB-10MB (configurable)
3. **Compression**: Per-taxon Zstd dictionaries for better compression
4. **Delta Encoding**: Similar sequences stored as deltas from references

### Pack File Format

To avoid filesystem overhead, multiple small sequences are packed:

```
Pack File Structure:
[Header]
[Index]
[Sequence 1]
[Sequence 2]
...
[Footer]
```

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
let verifier = SEQUOIAVerifier::new(&storage);

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
let storage = SEQUOIAStorage::with_backend(Box::new(backend))?;
```

### Hybrid Storage

Local cache with cloud backing:

```rust
let storage = SEQUOIAStorage::hybrid(
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
use talaria_sequoia::{SEQUOIARepository, ReductionParameters};

let repo = SEQUOIARepository::init(path)?;

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
    SEQUOIARepository,
    ChunkingStrategy,
    ReductionParameters,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize repository
    let repo = SEQUOIARepository::init("./sequoia-db")?;

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
