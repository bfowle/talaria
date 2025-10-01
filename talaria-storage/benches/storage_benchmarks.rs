/// Benchmarks for critical storage paths
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;
use tempfile::TempDir;

// Mock types for benchmarking
mod types {
    use sha2::{Digest, Sha256};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct SHA256Hash([u8; 32]);

    impl SHA256Hash {
        pub fn compute(data: &[u8]) -> Self {
            let mut hasher = Sha256::new();
            hasher.update(data);
            let result = hasher.finalize();
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&result);
            SHA256Hash(hash)
        }

        pub fn to_hex(&self) -> String {
            hex::encode(&self.0)
        }
    }
}

use types::*;

/// Benchmark hash computation performance
fn bench_hash_computation(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_computation");

    for size in [100, 1_000, 10_000, 100_000, 1_000_000] {
        let data = vec![0u8; size];

        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &data, |b, data| {
            b.iter(|| SHA256Hash::compute(black_box(data)));
        });
    }

    group.finish();
}

/// Benchmark chunk storage operations
fn bench_chunk_storage(c: &mut Criterion) {
    let mut group = c.benchmark_group("chunk_storage");
    let temp_dir = TempDir::new().unwrap();
    let chunks_dir = temp_dir.path().join("chunks");
    std::fs::create_dir_all(&chunks_dir).unwrap();

    for size in [1_000, 10_000, 100_000] {
        let data = vec![42u8; size];
        let hash = SHA256Hash::compute(&data);

        group.throughput(Throughput::Bytes(size as u64));

        // Benchmark write
        group.bench_with_input(BenchmarkId::new("write", size), &data, |b, data| {
            b.iter(|| {
                let path = chunks_dir.join(format!("{}_{}", hash.to_hex(), rand::random::<u32>()));
                std::fs::write(&path, black_box(data)).unwrap();
                std::fs::remove_file(path).unwrap(); // Clean up
            });
        });

        // Setup for read benchmark
        let read_path = chunks_dir.join(hash.to_hex());
        std::fs::write(&read_path, &data).unwrap();

        // Benchmark read
        group.bench_with_input(BenchmarkId::new("read", size), &read_path, |b, path| {
            b.iter(|| std::fs::read(black_box(path)).unwrap());
        });
    }

    group.finish();
}

/// Benchmark compression operations
fn bench_compression(c: &mut Criterion) {
    use flate2::read::GzDecoder;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::{Read, Write};

    let mut group = c.benchmark_group("compression");

    for size in [1_000, 10_000, 100_000] {
        // Create compressible data (repeated pattern)
        let data: Vec<u8> = (0..size).map(|i| (i % 10) as u8).collect();

        group.throughput(Throughput::Bytes(size as u64));

        // Benchmark compression
        group.bench_with_input(BenchmarkId::new("gzip_compress", size), &data, |b, data| {
            b.iter(|| {
                let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(black_box(data)).unwrap();
                encoder.finish().unwrap()
            });
        });

        // Create compressed data for decompression benchmark
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&data).unwrap();
        let compressed = encoder.finish().unwrap();

        // Benchmark decompression
        group.bench_with_input(
            BenchmarkId::new("gzip_decompress", size),
            &compressed,
            |b, compressed| {
                b.iter(|| {
                    let mut decoder = GzDecoder::new(&compressed[..]);
                    let mut decompressed = Vec::new();
                    decoder.read_to_end(&mut decompressed).unwrap();
                    decompressed
                });
            },
        );
    }

    group.finish();
}

/// Benchmark index operations
fn bench_index_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("index_operations");

    for num_entries in [100, 1_000, 10_000] {
        let mut index = HashMap::new();

        // Create index entries
        for i in 0..num_entries {
            let data = format!("data_{}", i);
            let hash = SHA256Hash::compute(data.as_bytes());
            index.insert(hash, vec![format!("seq_{}", i)]);
        }

        // Create search key
        let search_data = format!("data_{}", num_entries / 2);
        let search_hash = SHA256Hash::compute(search_data.as_bytes());

        group.bench_with_input(
            BenchmarkId::new("lookup", num_entries),
            &search_hash,
            |b, hash| {
                b.iter(|| index.get(black_box(hash)));
            },
        );

        // Benchmark insertion
        group.bench_with_input(
            BenchmarkId::new("insert", num_entries),
            &num_entries,
            |b, &n| {
                b.iter(|| {
                    let data = format!("new_data_{}", n);
                    let hash = SHA256Hash::compute(data.as_bytes());
                    index.insert(hash, vec![format!("new_seq_{}", n)]);
                    index.remove(&hash); // Clean up for next iteration
                });
            },
        );
    }

    group.finish();
}

/// Benchmark cache operations using DashMap
fn bench_cache_operations(c: &mut Criterion) {
    use dashmap::DashMap;
    use std::sync::Arc;

    let mut group = c.benchmark_group("cache_operations");

    for size in [100, 1_000, 10_000] {
        let cache = Arc::new(DashMap::new());

        // Populate cache
        for i in 0..size {
            let key = SHA256Hash::compute(format!("key_{}", i).as_bytes());
            let value = vec![i as u8; 100];
            cache.insert(key, value);
        }

        // Create keys for benchmarking
        let hit_key = SHA256Hash::compute(format!("key_{}", size / 2).as_bytes());
        let miss_key = SHA256Hash::compute(b"nonexistent");

        // Benchmark cache hit
        group.bench_with_input(BenchmarkId::new("cache_hit", size), &hit_key, |b, key| {
            b.iter(|| cache.get(black_box(key)));
        });

        // Benchmark cache miss
        group.bench_with_input(BenchmarkId::new("cache_miss", size), &miss_key, |b, key| {
            b.iter(|| cache.get(black_box(key)));
        });

        // Benchmark concurrent reads
        let cache_clone = Arc::clone(&cache);
        group.bench_with_input(
            BenchmarkId::new("concurrent_read", size),
            &hit_key,
            |b, key| {
                b.iter(|| {
                    let cache = Arc::clone(&cache_clone);
                    let key = *key;
                    std::thread::spawn(move || cache.get(&key)).join().unwrap()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark deduplication detection
fn bench_deduplication(c: &mut Criterion) {
    let mut group = c.benchmark_group("deduplication");

    for num_chunks in [100, 1_000, 10_000] {
        let mut chunks = Vec::new();
        let mut hashes = HashMap::new();

        // Create chunks with duplicates
        for i in 0..num_chunks {
            // Create some duplicates (every 10th chunk is a duplicate)
            let data = if i % 10 == 0 {
                b"duplicate_data".to_vec()
            } else {
                format!("unique_data_{}", i).into_bytes()
            };

            let hash = SHA256Hash::compute(&data);
            chunks.push((hash, data));

            *hashes.entry(hash).or_insert(0) += 1;
        }

        group.bench_with_input(
            BenchmarkId::new("find_duplicates", num_chunks),
            &hashes,
            |b, hashes| {
                b.iter(|| {
                    let duplicates: Vec<_> = hashes
                        .iter()
                        .filter(|(_, &count)| count > 1)
                        .map(|(hash, count)| (*hash, *count))
                        .collect();
                    black_box(duplicates)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark manifest serialization
fn bench_manifest_serialization(c: &mut Criterion) {
    use serde_json::json;

    let mut group = c.benchmark_group("manifest_serialization");

    for num_chunks in [10, 100, 1_000] {
        let mut chunks = Vec::new();
        for i in 0..num_chunks {
            let hash = SHA256Hash::compute(format!("chunk_{}", i).as_bytes());
            chunks.push(hash.to_hex());
        }

        let manifest = json!({
            "version": "1.0.0",
            "chunks": chunks,
            "metadata": {
                "created": "2024-01-01",
                "profile": "default"
            }
        });

        group.bench_with_input(
            BenchmarkId::new("serialize", num_chunks),
            &manifest,
            |b, manifest| {
                b.iter(|| serde_json::to_string(black_box(manifest)).unwrap());
            },
        );

        let serialized = serde_json::to_string(&manifest).unwrap();

        group.bench_with_input(
            BenchmarkId::new("deserialize", num_chunks),
            &serialized,
            |b, serialized| {
                b.iter(|| {
                    let _: serde_json::Value = serde_json::from_str(black_box(serialized)).unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark optimization analysis
fn bench_optimization_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("optimization_analysis");

    for num_chunks in [100, 1_000, 10_000] {
        // Create chunk metadata
        let mut chunks = Vec::new();
        for i in 0..num_chunks {
            let size = if i % 3 == 0 { 100 } else { 10_000 };
            let compressed = i % 5 == 0;
            chunks.push((size, compressed));
        }

        group.bench_with_input(
            BenchmarkId::new("analyze_compressible", num_chunks),
            &chunks,
            |b, chunks| {
                b.iter(|| {
                    let compressible: Vec<_> = chunks
                        .iter()
                        .filter(|(size, compressed)| !compressed && *size > 1024)
                        .collect();
                    black_box(compressible)
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("calculate_savings", num_chunks),
            &chunks,
            |b, chunks| {
                b.iter(|| {
                    let savings: usize = chunks
                        .iter()
                        .filter(|(size, compressed)| !compressed && *size > 1024)
                        .map(|(size, _)| size - (size / 3)) // Estimate compression
                        .sum();
                    black_box(savings)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark concurrent writes
fn bench_concurrent_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_writes");

    for num_threads in [1, 2, 4, 8] {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();

        group.bench_with_input(
            BenchmarkId::new("parallel_writes", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    use std::sync::Arc;
                    use std::thread;

                    let base = Arc::new(base_path.clone());
                    let mut handles = vec![];

                    for t in 0..num_threads {
                        let base = Arc::clone(&base);
                        let handle = thread::spawn(move || {
                            for i in 0..10 {
                                let data = vec![t as u8; 1000];
                                let path = base.join(format!("chunk_{}_{}", t, i));
                                std::fs::write(&path, &data).unwrap();
                                std::fs::remove_file(&path).unwrap();
                            }
                        });
                        handles.push(handle);
                    }

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark metadata I/O operations
fn bench_metadata_io(c: &mut Criterion) {
    let mut group = c.benchmark_group("metadata_io");
    let temp_dir = TempDir::new().unwrap();

    for num_entries in [10, 100, 1_000] {
        // Create metadata entries
        let mut metadata = Vec::new();
        for i in 0..num_entries {
            let line = format!("child_{}\tref_{}\t1,2,3\t3", i, i % 10);
            metadata.push(line);
        }

        let content = metadata.join("\n");
        let file_path = temp_dir
            .path()
            .join(format!("metadata_{}.dat", num_entries));

        group.throughput(Throughput::Bytes(content.len() as u64));

        // Benchmark write
        group.bench_with_input(
            BenchmarkId::new("write", num_entries),
            &content,
            |b, content| {
                b.iter(|| {
                    std::fs::write(&file_path, black_box(content)).unwrap();
                });
            },
        );

        // Setup for read
        std::fs::write(&file_path, &content).unwrap();

        // Benchmark read
        group.bench_with_input(
            BenchmarkId::new("read", num_entries),
            &file_path,
            |b, path| {
                b.iter(|| std::fs::read_to_string(black_box(path)).unwrap());
            },
        );

        // Benchmark parse
        group.bench_with_input(
            BenchmarkId::new("parse", num_entries),
            &content,
            |b, content| {
                b.iter(|| {
                    for line in content.lines() {
                        let parts: Vec<&str> = line.split('\t').collect();
                        black_box(parts);
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_hash_computation,
    bench_chunk_storage,
    bench_compression,
    bench_index_operations,
    bench_cache_operations,
    bench_deduplication,
    bench_manifest_serialization,
    bench_optimization_analysis,
    bench_concurrent_writes,
    bench_metadata_io
);

criterion_main!(benches);
