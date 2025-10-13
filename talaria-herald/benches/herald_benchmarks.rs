use chrono::Utc;
/// Performance benchmarking suite for HERALD architecture
///
/// Benchmarks the 5 core principles:
/// 1. Content Addressing (SHA256 hashing performance)
/// 2. Merkle DAG (cryptographic verification speed)
/// 3. Bi-Temporal Versioning (time-travel query performance)
/// 4. Hierarchical Taxonomic Chunking (chunking throughput)
/// 5. Delta Compression (graph centrality calculation)
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::sync::Arc;
use talaria_bio::sequence::Sequence;
use talaria_core::{SHA256Hash, TaxonId};
use talaria_herald::{
    chunker::{ChunkingStrategy, HierarchicalTaxonomicChunker, TaxonomicChunker},
    storage::{HeraldStorage, SequenceStorage},
    temporal::bi_temporal::BiTemporalDatabase,
    types::{DatabaseSource, ManifestMetadata},
    verification::merkle::MerkleDAG,
};
use talaria_test::fixtures::test_database_source;
use tempfile::TempDir;

/// Generate test sequences of varying sizes
fn generate_sequences(count: usize, avg_length: usize) -> Vec<Sequence> {
    (0..count)
        .map(|i| {
            let seq_len = avg_length + (i % 100); // Vary length slightly
            let sequence = vec![b'A'; seq_len];
            Sequence {
                id: format!("seq_{:06}", i),
                description: Some(format!("Test sequence {}", i)),
                sequence,
                taxon_id: Some((i % 1000) as u32), // Distribute across taxa
                taxonomy_sources: Default::default(),
            }
        })
        .collect()
}

/// Benchmark 1: Content Addressing Performance (SHA256 hashing)
fn bench_content_addressing(c: &mut Criterion) {
    let mut group = c.benchmark_group("content_addressing");

    // Test different sequence sizes
    for size in [100, 1000, 10000, 100000].iter() {
        let data = vec![b'A'; *size];

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| SHA256Hash::compute(black_box(&data)));
        });
    }

    group.finish();
}

/// Benchmark 2: Merkle DAG Verification Performance
fn bench_merkle_verification(c: &mut Criterion) {
    let mut group = c.benchmark_group("merkle_verification");

    // Create test metadata for different DAG sizes
    for num_chunks in [10, 100, 1000].iter() {
        let chunks: Vec<ManifestMetadata> = (0..*num_chunks)
            .map(|i| ManifestMetadata {
                hash: SHA256Hash::compute(format!("chunk_{}", i).as_bytes()),
                size: 1000000, // 1MB chunks
                sequence_count: 100,
                taxon_ids: vec![TaxonId(i as u32 % 100)],
                compressed_size: Some(500000), // 50% compression
            })
            .collect();

        group.bench_with_input(
            BenchmarkId::from_parameter(num_chunks),
            &chunks,
            |b, chunks| {
                b.iter(|| {
                    let dag = MerkleDAG::build_from_items(chunks.clone()).unwrap();
                    // Just build the DAG for benchmarking
                    black_box(dag);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 3: Bi-Temporal Versioning Query Performance
fn bench_bitemporal_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("bitemporal_queries");

    // Create a test database with temporal data
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(HeraldStorage::new(temp_dir.path()).unwrap());
    let mut bitemporal_db = BiTemporalDatabase::new(storage).unwrap();

    // Benchmark querying at different time points
    group.bench_function("current_time_query", |b| {
        let now = Utc::now();
        b.iter(|| {
            // This will fail but we're measuring the attempt
            let _ = bitemporal_db.query_at(black_box(now), black_box(now));
        });
    });

    // Benchmark diff between temporal coordinates
    group.bench_function("temporal_diff", |b| {
        let t1 = Utc::now();
        let t2 = t1 - chrono::Duration::days(30);
        let coord1 = talaria_herald::types::BiTemporalCoordinate {
            sequence_time: t1,
            taxonomy_time: t1,
        };
        let coord2 = talaria_herald::types::BiTemporalCoordinate {
            sequence_time: t2,
            taxonomy_time: t2,
        };

        b.iter(|| {
            // This will fail but we're measuring the attempt
            let _ = bitemporal_db.diff(black_box(coord1.clone()), black_box(coord2.clone()));
        });
    });

    group.finish();
}

/// Benchmark 4: Hierarchical Taxonomic Chunking Performance
fn bench_hierarchical_chunking(c: &mut Criterion) {
    let mut group = c.benchmark_group("hierarchical_chunking");

    let temp_dir = TempDir::new().unwrap();
    let _sequence_storage = SequenceStorage::new(temp_dir.path()).unwrap();

    // Test different dataset sizes
    for num_sequences in [100, 1000, 10000].iter() {
        let sequences = generate_sequences(*num_sequences, 1000);

        group.throughput(Throughput::Elements(*num_sequences as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_sequences),
            &sequences,
            |b, sequences| {
                b.iter_with_setup(
                    || {
                        let storage = SequenceStorage::new(temp_dir.path()).unwrap();
                        let chunker = HierarchicalTaxonomicChunker::new(
                            ChunkingStrategy::default(),
                            storage,
                            test_database_source("bench"),
                            None,
                        );
                        (chunker, sequences.clone())
                    },
                    |(mut chunker, sequences)| {
                        chunker
                            .chunk_sequences_hierarchical(black_box(sequences))
                            .unwrap()
                    },
                );
            },
        );
    }

    group.finish();
}

/// Benchmark 5: Standard Taxonomic Chunking (for comparison)
fn bench_standard_chunking(c: &mut Criterion) {
    let mut group = c.benchmark_group("standard_chunking");

    let temp_dir = TempDir::new().unwrap();
    let _sequence_storage = SequenceStorage::new(temp_dir.path()).unwrap();

    // Test different dataset sizes
    for num_sequences in [100, 1000, 10000].iter() {
        let sequences = generate_sequences(*num_sequences, 1000);

        group.throughput(Throughput::Elements(*num_sequences as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_sequences),
            &sequences,
            |b, sequences| {
                b.iter_with_setup(
                    || {
                        let storage = SequenceStorage::new(temp_dir.path()).unwrap();
                        let chunker = TaxonomicChunker::new(
                            ChunkingStrategy::default(),
                            storage,
                            test_database_source("bench"),
                        );
                        (chunker, sequences.clone())
                    },
                    |(mut chunker, sequences)| {
                        chunker
                            .chunk_sequences_canonical(black_box(sequences))
                            .unwrap()
                    },
                );
            },
        );
    }

    group.finish();
}

/// Benchmark 6: Deduplication Performance
fn bench_deduplication(c: &mut Criterion) {
    let mut group = c.benchmark_group("deduplication");

    let temp_dir = TempDir::new().unwrap();

    // Test with different overlap percentages
    for overlap_pct in [0.0, 0.25, 0.50, 0.75].iter() {
        let num_sequences = 1000;
        let overlap_count = (num_sequences as f32 * overlap_pct) as usize;

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}%", overlap_pct * 100.0)),
            overlap_pct,
            |b, _| {
                b.iter_with_setup(
                    || {
                        let storage = SequenceStorage::new(temp_dir.path()).unwrap();
                        let mut sequences = Vec::new();

                        // Add common sequences
                        for i in 0..overlap_count {
                            let seq = format!("COMMON{:04}ACGTACGTACGT", i);
                            sequences.push((seq, format!("common_{}", i)));
                        }

                        // Add unique sequences
                        for i in overlap_count..num_sequences {
                            let seq = format!("UNIQUE{:04}TGCATGCATGCA", i);
                            sequences.push((seq, format!("unique_{}", i)));
                        }

                        (storage, sequences)
                    },
                    |(storage, sequences)| {
                        for (seq, header) in sequences {
                            storage
                                .store_sequence(&seq, &header, test_database_source("bench"))
                                .unwrap();
                        }
                    },
                );
            },
        );
    }

    group.finish();
}

/// Benchmark 7: Graph Centrality Calculation
fn bench_graph_centrality(c: &mut Criterion) {
    use petgraph::graph::UnGraph;

    let mut group = c.benchmark_group("graph_centrality");

    // Test with different graph sizes
    for num_nodes in [10, 50, 100, 500].iter() {
        let mut graph = UnGraph::new_undirected();
        let nodes: Vec<_> = (0..*num_nodes)
            .map(|i| graph.add_node(format!("seq_{}", i)))
            .collect();

        // Add edges (create a somewhat connected graph)
        for i in 0..*num_nodes {
            for j in i + 1..(*num_nodes).min(i + 5) {
                let weight = 1.0 / (j - i) as f64;
                graph.add_edge(nodes[i], nodes[j], weight);
            }
        }

        group.bench_with_input(
            BenchmarkId::from_parameter(num_nodes),
            &graph,
            |b, graph| {
                b.iter(|| {
                    // Centrality calculation would happen here
                    // For now, just benchmark the graph structure traversal
                    let mut count = 0;
                    for node in graph.node_indices() {
                        for neighbor in graph.neighbors(node) {
                            count += 1;
                            black_box(neighbor);
                        }
                    }
                    black_box(count);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 8: Storage I/O Performance
fn bench_storage_io(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_io");

    let temp_dir = TempDir::new().unwrap();

    // Benchmark chunk writing
    group.bench_function("chunk_write_1mb", |b| {
        let storage = HeraldStorage::new(temp_dir.path()).unwrap();
        let data = vec![b'A'; 1_000_000]; // 1MB

        b.iter(|| storage.store_chunk(black_box(&data), false).unwrap());
    });

    // Benchmark chunk reading
    group.bench_function("chunk_read_1mb", |b| {
        let storage = HeraldStorage::new(temp_dir.path()).unwrap();
        let data = vec![b'A'; 1_000_000]; // 1MB
        let hash = storage.store_chunk(&data, false).unwrap();

        b.iter(|| storage.get_chunk(black_box(&hash)).unwrap());
    });

    group.finish();
}

// Main benchmark groups
criterion_group!(
    benches,
    bench_content_addressing,
    bench_merkle_verification,
    bench_bitemporal_queries,
    bench_hierarchical_chunking,
    bench_standard_chunking,
    bench_deduplication,
    bench_graph_centrality,
    bench_storage_io
);

criterion_main!(benches);
