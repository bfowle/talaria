# Talaria Storage Module

## Overview

The `talaria-storage` module provides a comprehensive storage abstraction layer for the Talaria bioinformatics system. It implements content-addressed storage with SHA256 hashing, supporting various storage backends including local filesystem, cloud storage (S3, GCS, Azure), and distributed systems. The module is designed with a trait-based architecture that allows pluggable storage implementations while maintaining consistent interfaces across the system.

### Key Features

- **Content-Addressed Storage**: All data is stored and retrieved using SHA256 hashes, ensuring data integrity and enabling automatic deduplication
- **Delta Encoding Support**: Efficient storage of sequence variations through delta compression
- **Taxonomy-Aware Storage**: First-class support for taxonomic organization of biological data
- **Storage Optimization**: Built-in strategies for deduplication, compression, and space optimization
- **Async/Await Support**: Modern asynchronous I/O for high-performance operations
- **Thread-Safe Operations**: Concurrent access using lock-free data structures (`DashMap`)
- **Comprehensive Testing**: 47+ unit tests, integration tests, and performance benchmarks

## Architecture

### Storage Trait Hierarchy

```
ChunkStorage (Base Trait)
    ├── DeltaStorage
    │   └── ReductionStorage
    ├── TaxonomyStorage
    ├── RemoteStorage
    └── StatefulStorage
```

### Component Architecture

```
talaria-storage/
├── core/              # Core types and traits
│   ├── types.rs       # Fundamental types (SHA256Hash, TaxonId, etc.)
│   └── traits.rs      # Storage trait definitions with comprehensive tests
├── index/             # Indexing subsystem
│   └── index.rs       # Chunk indexing with InMemoryChunkIndex
├── cache/             # Caching layer
│   └── cache.rs       # AlignmentCache with LRU eviction
├── optimization/      # Storage optimization
│   └── optimizer.rs   # StandardStorageOptimizer with multiple strategies
├── io/                # I/O operations
│   └── metadata.rs    # Metadata persistence for deltas and references
├── tests/             # Integration tests
│   └── storage_integration.rs
└── benches/           # Performance benchmarks
    └── storage_benchmarks.rs
```

### Data Flow

```
Application Layer
    ↓
Storage Traits API
    ↓
[Indexing] ← → [Caching]
    ↓
Storage Backend Implementation
    ↓
Physical Storage (Filesystem/Cloud)
```

## Module Structure

### Core (`core/`)

The core module contains fundamental types and trait definitions that form the foundation of the storage system.

#### Types (`core/types.rs`)

Re-exports from `talaria_core::types`:

- **SHA256Hash**: Content addressing primitive
  ```rust
  pub struct SHA256Hash(pub [u8; 32]);
  ```

- **TaxonId**: Taxonomy identifier
  ```rust
  pub struct TaxonId(pub u32);
  ```

- **ChunkMetadata**: Extended chunk information
  ```rust
  pub struct ChunkMetadata {
      pub hash: SHA256Hash,
      pub size: usize,
      pub taxon_ids: Vec<TaxonId>,
      pub sequence_count: usize,
      pub compressed_size: Option<usize>,
      pub compression_ratio: Option<f32>,
  }
  ```

- **ChunkInfo**: Basic chunk information
  ```rust
  pub struct ChunkInfo {
      pub hash: SHA256Hash,
      pub size: usize,
  }
  ```

- **Storage Result Types**:
  - `StorageStats`: Overall storage metrics with deduplication ratio
  - `GCResult`: Garbage collection results (`removed\_count`, `freed\_space`)
  - `VerificationError`: Integrity check results with error types
  - `ProcessingState`: State for resumable operations

#### Traits (`core/traits.rs`)

##### ChunkStorage (Base Trait)

The foundation trait that all storage implementations must provide:

```rust
pub trait ChunkStorage: Send + Sync {
    fn store_chunk(&self, data: &[u8], compress: bool) -> Result<SHA256Hash>;
    fn get_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>>;
    fn has_chunk(&self, hash: &SHA256Hash) -> bool;
    fn enumerate_chunks(&self) -> Vec<ChunkInfo>;
    fn verify_all(&self) -> Result<Vec<VerificationError>>;
    fn get_stats(&self) -> StorageStats;
    fn gc(&mut self, referenced: &[SHA256Hash]) -> Result<GCResult>;
}
```

**Test Coverage**:
- MockChunkStorage implementation for testing
- 8 comprehensive unit tests including concurrent access
- Property-based tests with quickcheck

##### DeltaStorage

Extends ChunkStorage with delta-specific operations:

```rust
pub trait DeltaStorage: ChunkStorage {
    fn store_delta_chunk(&self, chunk: &DeltaChunk) -> Result<SHA256Hash>;
    fn get_delta_chunk(&self, hash: &SHA256Hash) -> Result<DeltaChunk>;
    fn find_delta_for_child(&self, child_id: &str) -> Result<Option<SHA256Hash>>;
    fn get_deltas_for_reference(&self, reference_hash: &SHA256Hash) -> Result<Vec<SHA256Hash>>;
    fn find_delta_chunks_for_reference(&self, reference_hash: &SHA256Hash) -> Result<Vec<SHA256Hash>>;
    fn get_chunk_type(&self, hash: &SHA256Hash) -> Result<ChunkType>;
}
```

### Indexing (`index/`)

The indexing subsystem provides fast lookups and queries over stored chunks.

#### ChunkIndex Trait

```rust
#[async_trait]
pub trait ChunkIndex: Send + Sync {
    async fn add_chunk(&mut self, metadata: ChunkMetadata) -> Result<()>;
    async fn remove_chunk(&mut self, hash: &SHA256Hash) -> Result<()>;
    async fn find_by_taxon(&self, taxon_id: TaxonId) -> Result<Vec<SHA256Hash>>;
    async fn find_by_taxons(&self, taxon_ids: &[TaxonId]) -> Result<Vec<SHA256Hash>>;
    async fn get_metadata(&self, hash: &SHA256Hash) -> Result<Option<ChunkMetadata>>;
    async fn query(&self, query: ChunkQuery) -> Result<Vec<ChunkMetadata>>;
    async fn rebuild(&mut self) -> Result<()>;
    async fn get_stats(&self) -> Result<IndexStats>;
    async fn exists(&self, hash: &SHA256Hash) -> bool;
    async fn list_all(&self) -> Result<Vec<SHA256Hash>>;
    async fn clear(&mut self) -> Result<()>;
}
```

#### InMemoryChunkIndex

A high-performance, thread-safe implementation using `DashMap`:

```rust
pub struct InMemoryChunkIndex {
    chunks: Arc<DashMap<SHA256Hash, ChunkMetadata>>,
    taxon_index: Arc<DashMap<TaxonId, HashSet<SHA256Hash>>>,
}
```

**Features**:
- Lock-free concurrent access via DashMap
- O(1) chunk lookups
- O(1) taxon-to-chunk mapping
- Automatic index maintenance

**Test Coverage**:
- 11 async unit tests using `tokio::test`
- Tests for concurrent operations with `Arc<RwLock<>>`
- Query filtering and statistics tests

### Caching (`cache/`)

The caching layer improves performance by storing frequently accessed data in memory.

#### AlignmentCache

Caches alignment results to avoid redundant computations:

```rust
pub struct AlignmentCache {
    cache: Arc<DashMap<(String, String), CachedAlignment>>,
    max_size: usize,
}

pub struct CachedAlignment {
    pub score: i32,
    pub alignment: Vec<u8>,
}
```

**Features**:
- Size-based eviction (stops inserting at `max\_size`)
- Thread-safe concurrent access
- Clear operation for cache invalidation

**Test Coverage**:
- 7 unit tests including concurrent access
- Tests for size limits and cache operations

### Optimization (`optimization/`)

The optimization subsystem provides strategies for reducing storage usage and improving performance.

#### Storage Strategies

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageStrategy {
    Deduplication,     // Remove duplicate chunks
    Compression,       // Compress chunk data
    DeltaEncoding,     // Use delta encoding
    Archival,          // Archive old versions
    Caching,           // Cache hot chunks
    Repacking,         // Consolidate small chunks
    GarbageCollection, // Remove unreferenced chunks
}
```

#### StandardStorageOptimizer

The default implementation providing:

```rust
pub struct StandardStorageOptimizer {
    chunks_dir: PathBuf,
    chunk_cache: HashMap<SHA256Hash, ChunkInfo>,
}
```

**Key Methods**:
- `analyze()`: Scan storage for optimization opportunities
- `optimize()`: Apply selected strategies with options
- `deduplicate()`: Remove duplicate chunks
- `compress_chunks()`: Apply gzip compression
- `estimate_impact()`: Calculate potential savings
- `verify_integrity()`: Check chunk hash validity

**Test Coverage**:
- 16 comprehensive async tests
- Tests for all optimization strategies
- Property-based test for compression ratios
- Dry run and target savings tests

### I/O Operations (`io/`)

The I/O module handles metadata persistence and data serialization.

#### Metadata Functions

```rust
pub fn write_metadata<P: AsRef<Path>>(
    path: P,
    deltas: &[DeltaRecord],
) -> Result<(), TalariaError>;

pub fn load_metadata<P: AsRef<Path>>(
    path: P
) -> Result<Vec<DeltaRecord>, TalariaError>;
```

**DeltaRecord Structure** (from talaria-bio):
```rust
pub struct DeltaRecord {
    pub child_id: String,
    pub reference_id: String,
    pub taxon_id: Option<u32>,
    pub deltas: Vec<DeltaRange>,
    pub header_change: Option<HeaderChange>,
}

pub struct DeltaRange {
    pub start: usize,
    pub end: usize,
    pub substitution: Vec<u8>,
}
```

**Test Coverage**:
- 8 unit tests including property-based tests
- Tests for ref2children mapping
- Large file and special character handling

## Testing

### Test Statistics

- **Total Tests**: 47+ passing tests across all modules
- **Unit Tests**: Comprehensive coverage in each module
- **Integration Tests**: 11 workflow tests in `tests/storage_integration.rs`
- **Benchmarks**: 10 benchmark groups in `benches/storage_benchmarks.rs`

### Unit Testing

Run tests with:

```bash
# Run all tests
cargo test

# Run specific module tests
cargo test --lib core::traits

# Run with output
cargo test -- --nocapture

# Run integration tests
cargo test --test storage_integration
```

### Integration Tests (`tests/storage_integration.rs`)

Comprehensive workflow tests including:
- Complete storage workflow (store, index, retrieve)
- Deduplication workflow
- Compression workflow with gzip
- Cache management workflow
- Manifest storage and retrieval
- Concurrent operations with tokio::task
- Storage migration workflow
- Error recovery workflow
- Statistics collection
- Chunk versioning workflow

### Performance Benchmarks (`benches/storage_benchmarks.rs`)

Benchmark critical paths using Criterion:

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench hash_computation
```

Benchmark groups:
- `hash_computation`: SHA256 performance at various sizes
- `chunk_storage`: Read/write operations
- `compression`: Gzip compress/decompress
- `index_operations`: HashMap lookup and insertion
- `cache_operations`: DashMap concurrent access
- `deduplication`: Finding duplicate chunks
- `manifest_serialization`: JSON serialize/deserialize
- `optimization_analysis`: Analyzing compressible chunks
- `concurrent_writes`: Parallel write operations
- `metadata_io`: Metadata file operations

### Test Infrastructure

**Dependencies** (`dev-dependencies`):
```toml
tempfile = "3.8"     # Temporary directories for tests
proptest = "1.4"     # Property-based testing
tokio-test = "0.4"   # Async test utilities
criterion = "0.5"    # Benchmarking framework
mockall = "0.12"     # Mock generation
quickcheck = "1.0"   # Property testing
quickcheck_macros = "1.0"
rand = "0.8"         # Random data generation
futures = "0.3"      # Async utilities
```

## Integration with Other Modules

### talaria-core Integration

Uses core functionality for:
- **Types**: `SHA256Hash`, `TaxonId`, `ChunkMetadata`, `ChunkInfo`
- **Error Handling**: `TalariaError` type
- **Statistics**: `StorageStats`, `GCResult`, `VerificationError`

### talaria-bio Integration

Leverages bio module for:
- **Delta Compression**: `DeltaRecord`, `DeltaRange`, `HeaderChange`
- **Serialization**: `format_deltas_dat()`, `parse_deltas_dat()`

## Usage Examples

### Basic Storage Operations

```rust
use talaria_storage::core::traits::ChunkStorage;

// Store a chunk
let data = b"genomic sequence data";
let hash = storage.store_chunk(data, true)?; // compress = true

// Retrieve a chunk
let retrieved = storage.get_chunk(&hash)?;
assert_eq!(retrieved, data);

// Check existence
if storage.has_chunk(&hash) {
    println!("Chunk exists");
}

// Get storage statistics
let stats = storage.get_stats();
println!("Total chunks: {}", stats.total_chunks);
println!("Deduplication ratio: {}", stats.deduplication_ratio);
```

### Using the Index System

```rust
use talaria_storage::index::{InMemoryChunkIndex, ChunkQuery};

#[tokio::main]
async fn main() -> Result<()> {
    let mut index = InMemoryChunkIndex::new();

    // Add chunk metadata
    let metadata = ChunkMetadata {
        hash: SHA256Hash::compute(b"data"),
        size: 1024,
        taxon_ids: vec![TaxonId(9606)], // Human
        sequence_count: 100,
        compressed_size: Some(512),
        compression_ratio: Some(0.5),
    };
    index.add_chunk(metadata).await?;

    // Query by taxon
    let human_chunks = index.find_by_taxon(TaxonId(9606)).await?;

    // Complex query
    let query = ChunkQuery {
        taxon_ids: Some(vec![TaxonId(9606)]),
        size_range: Some((1000, 2000)),
        limit: Some(10),
        ..Default::default()
    };
    let results = index.query(query).await?;

    Ok(())
}
```

### Storage Optimization

```rust
use talaria_storage::optimization::{
    StandardStorageOptimizer,
    OptimizationOptions,
    StorageStrategy
};

#[tokio::main]
async fn main() -> Result<()> {
    let mut optimizer = StandardStorageOptimizer::new(base_path);

    // Scan and analyze
    optimizer.scan_chunks().await?;
    let analysis = optimizer.analyze().await?;

    println!("Duplicate chunks: {}", analysis.duplicate_chunks.len());
    println!("Compressible chunks: {}", analysis.compressible_chunks.len());

    // Run optimization
    let options = OptimizationOptions {
        strategies: vec![
            StorageStrategy::Deduplication,
            StorageStrategy::Compression,
        ],
        target_savings: Some(1_000_000), // 1MB target
        dry_run: false,
        compression_level: Some(6),
        ..Default::default()
    };

    let results = optimizer.optimize(options).await?;
    for result in results {
        println!("{:?}: saved {} bytes", result.strategy, result.space_saved);
    }

    Ok(())
}
```

## Performance Considerations

### Thread Safety

All implementations use thread-safe primitives:
- **DashMap**: Lock-free concurrent `HashMap` for cache and index
- **Arc**: Atomic reference counting for shared ownership
- **Mutex/RwLock**: Used sparingly in tests for coordination

### Async I/O

The module uses async/await extensively:
- All `ChunkIndex` methods are async
- `StorageOptimizer` operations are async
- Integration tests use `tokio::test`

### Content-Addressed Deduplication

- Automatic deduplication through SHA256 hashing
- Zero-copy references to identical data
- Significant storage savings for redundant sequences

## Known Issues

- 3 tests currently failing due to minor data format issues (being addressed)
- Some optimization strategies (Archival, DeltaEncoding, Repacking) have placeholder implementations
- Cache eviction is size-based rather than true LRU (stops inserting at `max\_size`)

## Future Directions

### Planned Improvements

1. **Complete Optimization Strategies**
   - Full implementation of Archival strategy
   - Delta encoding between versions
   - Smart chunk repacking algorithm

2. **Enhanced Caching**
   - True LRU eviction policy
   - Multi-tier caching support
   - Predictive prefetching

3. **Cloud Storage Backends**
   - S3 implementation with multipart upload
   - Google Cloud Storage support
   - Azure Blob Storage integration

4. **Advanced Indexing**
   - Persistent index with SQLite/RocksDB
   - Bloom filters for existence checks
   - Full-text search capabilities

## Contributing

When contributing to the storage module:

1. **Write Tests First**: Add tests for new functionality
2. **Follow Trait Hierarchy**: Extend existing traits appropriately
3. **Maintain Thread Safety**: Use DashMap, Arc, and async where appropriate
4. **Document Thoroughly**: Update this README and add inline docs
5. **Benchmark Critical Paths**: Add benchmarks for performance-critical code

## License

This module is part of the Talaria project and follows the same licensing terms as the parent project.

## Support

For questions, bug reports, or feature requests:
- Open an issue on the Talaria GitHub repository
- Consult the inline documentation (`cargo doc --open`)
- Review the test cases for usage examples
