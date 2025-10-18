use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use std::time::Duration;
use talaria_core::DatabaseSource;
use talaria_herald::storage::sequence::SequenceStorage;
use talaria_herald::types::SHA256Hash;
use tempfile::TempDir;

fn generate_test_sequences(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|i| {
            let seq = format!("ACGTACGTACGT{}", "A".repeat(i % 100));
            let header = format!(">seq{} test sequence {}", i, i);
            (seq, header)
        })
        .collect()
}

fn benchmark_batch_existence_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("existence_checks");
    group.measurement_time(Duration::from_secs(10));

    for size in &[1000, 10000, 50000] {
        let temp_dir = TempDir::new().unwrap();
        let storage = SequenceStorage::new(temp_dir.path()).unwrap();
        let sequences = generate_test_sequences(*size);

        // Pre-populate storage
        for (seq, header) in &sequences {
            let _ = storage.store_sequence(seq, header, DatabaseSource::Custom("test".to_string()));
        }

        // Collect hashes for existence check
        let hashes: Vec<SHA256Hash> = sequences
            .iter()
            .map(|(seq, _)| SHA256Hash::compute(seq.as_bytes()))
            .collect();

        group.bench_function(format!("rocksdb_batch_{}", size), |b| {
            b.iter(|| {
                let exists = storage.canonical_exists_batch(&hashes).unwrap();
                black_box(exists);
            });
        });
    }

    group.finish();
}

fn benchmark_parallel_insertion(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_insertion");
    group.sample_size(10);

    for size in &[1000, 10000, 50000] {
        group.bench_function(format!("rocksdb_insert_{}", size), |b| {
            b.iter_batched(
                || {
                    let temp_dir = TempDir::new().unwrap();
                    let storage = SequenceStorage::new(temp_dir.path()).unwrap();
                    let sequences = generate_test_sequences(*size);
                    (storage, sequences)
                },
                |(storage, sequences)| {
                    let batch: Vec<_> = sequences
                        .iter()
                        .map(|(seq, header)| {
                            (
                                seq.as_str(),
                                header.as_str(),
                                DatabaseSource::Custom("test".to_string()),
                            )
                        })
                        .collect();

                    let results = storage.store_sequences_batch(batch).unwrap();
                    black_box(results);
                },
                BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

fn benchmark_retrieval(c: &mut Criterion) {
    let mut group = c.benchmark_group("retrieval");

    let temp_dir = TempDir::new().unwrap();
    let storage = SequenceStorage::new(temp_dir.path()).unwrap();
    let sequences = generate_test_sequences(10000);

    // Pre-populate
    let hashes: Vec<SHA256Hash> = sequences
        .iter()
        .map(|(seq, header)| {
            storage
                .store_sequence(seq, header, DatabaseSource::Custom("test".to_string()))
                .unwrap()
        })
        .collect();

    group.bench_function("rocksdb_load_canonical", |b| {
        let hash = &hashes[5000];
        b.iter(|| {
            let seq = storage.load_canonical(hash).unwrap();
            black_box(seq);
        });
    });

    group.bench_function("rocksdb_load_representations", |b| {
        let hash = &hashes[5000];
        b.iter(|| {
            let reps = storage.load_representations(hash).unwrap();
            black_box(reps);
        });
    });

    group.finish();
}

fn benchmark_deduplication(c: &mut Criterion) {
    let mut group = c.benchmark_group("deduplication");
    group.sample_size(10);

    group.bench_function("rocksdb_dedup_50k", |b| {
        b.iter_batched(
            || {
                let temp_dir = TempDir::new().unwrap();
                let storage = SequenceStorage::new(temp_dir.path()).unwrap();
                // Create sequences with 50% duplicates
                let mut sequences = generate_test_sequences(25000);
                let duplicates = sequences.clone();
                sequences.extend(duplicates);
                (storage, sequences)
            },
            |(storage, sequences)| {
                for (seq, header) in &sequences {
                    let hash = storage
                        .store_sequence(seq, header, DatabaseSource::Custom("test".to_string()))
                        .unwrap();
                    black_box(hash);
                }

                // Should have only 25000 unique sequences
                let all_hashes = storage.list_all_hashes().unwrap();
                assert!(all_hashes.len() <= 25001); // Allow for slight variation
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

fn benchmark_index_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("indices");

    let temp_dir = TempDir::new().unwrap();
    let storage = SequenceStorage::new(temp_dir.path()).unwrap();
    let sequences = generate_test_sequences(10000);

    // Pre-populate with accessions
    for (i, (seq, _)) in sequences.iter().enumerate() {
        let header = format!(">acc{} test sequence", i);
        storage
            .store_sequence(seq, &header, DatabaseSource::Custom("test".to_string()))
            .unwrap();
    }

    storage.save_indices().unwrap();

    group.bench_function("rocksdb_find_by_accession", |b| {
        b.iter(|| {
            let result = storage.find_by_accession("acc5000").unwrap();
            black_box(result);
        });
    });

    group.bench_function("rocksdb_rebuild_index", |b| {
        b.iter(|| {
            storage.rebuild_index().unwrap();
        });
    });

    group.finish();
}

fn benchmark_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression");

    let temp_dir = TempDir::new().unwrap();
    let storage = SequenceStorage::new(temp_dir.path()).unwrap();

    // Test different sequence sizes
    for size in &[100, 1000, 10000] {
        let sequence = "ACGT".repeat(*size);
        let header = format!(">test_seq_{}", size);

        group.bench_function(format!("rocksdb_store_{}bp", size * 4), |b| {
            b.iter(|| {
                let hash = storage
                    .store_sequence(
                        &sequence,
                        &header,
                        DatabaseSource::Custom("bench".to_string()),
                    )
                    .unwrap();
                black_box(hash);
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_batch_existence_check,
    benchmark_parallel_insertion,
    benchmark_retrieval,
    benchmark_deduplication,
    benchmark_index_operations,
    benchmark_compression
);
criterion_main!(benches);
