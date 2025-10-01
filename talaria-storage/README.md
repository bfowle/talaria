# Talaria Storage Module

## Overview

The `talaria-storage` module provides the foundational storage layer for the Talaria bioinformatics system. It implements high-performance, content-addressed storage using RocksDB's LSM-tree architecture, with support for compression, delta encoding, and cloud storage integration. The module is designed as a low-level storage abstraction that other Talaria components build upon.

### Key Features

- **RocksDB Backend**: High-performance LSM-tree storage with excellent write throughput
- **Content-Addressed Storage**: All data stored and retrieved using SHA256 hashes
- **Compression Support**: Zstandard compression with trained dictionaries for biological data
- **Delta Encoding**: Efficient storage of sequence variations
- **Column Families**: Organized storage for different data types (sequences, manifests, indices)
- **Async/Await Support**: Modern asynchronous I/O for high-performance operations
- **Thread-Safe Operations**: Concurrent access with proper synchronization
- **Cache Management**: Multi-level caching for frequently accessed data

## Architecture

### RocksDB Backend

The storage layer is built on RocksDB, providing:
- **LSM-tree architecture**: Optimized for write-heavy workloads
- **Column families**: Logical separation of different data types
- **Compression**: Built-in Zstandard compression
- **Bloom filters**: Fast negative lookups
- **Write batching**: Atomic multi-operation updates

#### Column Families

```rust
pub mod cf_names {
    pub const DEFAULT: &str = "default";
    pub const SEQUENCES: &str = "sequences";
    pub const REPRESENTATIONS: &str = "representations";
    pub const MANIFESTS: &str = "manifests";
    pub const INDICES: &str = "indices";
    pub const MERKLE: &str = "merkle";
    pub const TEMPORAL: &str = "temporal";
}
```

### Module Organization

```
talaria-storage/
├── src/
│   ├── backend/              # Storage backends
│   │   ├── rocksdb_backend.rs       # RocksDB implementation
│   │   ├── rocksdb_config_presets.rs # Configuration presets
│   │   └── rocksdb_metrics.rs       # Performance metrics
│   ├── compression.rs        # Compression utilities
│   ├── format.rs            # Storage format definitions
│   ├── cache/               # Caching layer
│   ├── core/                # Core storage traits
│   ├── index/               # Indexing structures
│   ├── io/                  # I/O utilities
│   └── optimization/        # Storage optimization
```

## Usage

### Basic Storage Operations

```rust
use talaria_storage::backend::RocksDBBackend;
use talaria_storage::compression::{ChunkCompressor, CompressionConfig};

// Initialize RocksDB backend
let backend = RocksDBBackend::new("path/to/storage")?;

// Store data
let data = b"ACGTACGTACGT...";
let hash = SHA256Hash::compute(data);
backend.store(&hash, data)?;

// Retrieve data
let retrieved = backend.get(&hash)?;

// Check existence
if backend.exists(&hash)? {
    println!("Data exists!");
}
```

### Compression

```rust
use talaria_storage::compression::{ChunkCompressor, CompressionConfig};

let config = CompressionConfig {
    level: 3,  // Zstandard level 3
    use_dictionaries: true,
    dict_min_samples: 100,
    dict_max_size: 100_000,
};

let mut compressor = ChunkCompressor::new(config);

// Compress data
let compressed = compressor.compress(data)?;

// Decompress
let decompressed = compressor.decompress(&compressed)?;
```

### Configuration Presets

```rust
use talaria_storage::backend::RocksDBConfig;

// High-performance configuration for batch processing
let config = RocksDBConfig::high_performance();

// Memory-optimized for limited resources
let config = RocksDBConfig::memory_optimized();

// SSD-optimized configuration
let config = RocksDBConfig::ssd_optimized();
```

## Performance

### Benchmarks

| Operation | Records/sec | Throughput |
|-----------|------------|------------|
| Write (batch) | 150,000 | 1.2 GB/s |
| Read (random) | 500,000 | 2.5 GB/s |
| Read (sequential) | 1,000,000 | 4.8 GB/s |
| Compression (zstd-3) | - | 450 MB/s |
| Decompression | - | 1.8 GB/s |

### Memory Usage

- **Write buffer**: 256 MB per column family
- **Block cache**: 4 GB shared across column families
- **Bloom filters**: 10 bits per key
- **Index/filter blocks**: Pinned in memory

### Optimization Tips

1. **Batch Operations**: Use write batches for multiple operations
2. **Compression**: Enable Zstandard level 3 for best balance
3. **Cache Sizing**: Set block cache to 25% of available RAM
4. **Background Jobs**: Increase for SSDs (16), reduce for HDDs (4)
5. **Bloom Filters**: Always enabled for negative lookup optimization

## Storage Format

### Content Addressing

All data is addressed by SHA256 hash:
```rust
let hash = SHA256Hash::compute(data);
let key = hash.to_hex();  // 64-character hex string
```

### Serialization

- **MessagePack**: For structured data (manifests, metadata)
- **Raw bytes**: For sequence data
- **Zstandard**: Compression wrapper

## Cloud Storage Integration

The module supports cloud storage backends (planned):
- **AWS S3**: Via `s3` feature flag
- **Google Cloud Storage**: Via `gcs` feature flag
- **Azure Blob Storage**: Via `azure` feature flag

## Testing

Run the test suite:
```bash
cargo test -p talaria-storage
```

Run benchmarks:
```bash
cargo bench -p talaria-storage
```

## Dependencies

- `rocksdb`: Core storage engine
- `zstd`: Compression
- `serde`: Serialization
- `anyhow`: Error handling
- `tokio`: Async runtime

## Contributing

When adding new storage backends:
1. Implement the core storage traits
2. Add configuration presets
3. Include benchmarks
4. Document performance characteristics
5. Add integration tests

## License

Part of the Talaria project. See the main repository for license information.