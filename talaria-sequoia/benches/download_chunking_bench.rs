/// Performance benchmarks for the core download and chunking pipeline
///
/// This benchmark suite measures real-world performance of:
/// - FASTA file reading and parsing
/// - Sequence chunking and processing
/// - Deduplication performance
/// - Parallel vs sequential processing
/// - Memory usage patterns
///
/// Target performance goals:
/// - Process >100,000 sequences/second for typical sequences (500bp)
/// - Sustain >50 MB/second throughput for large files
/// - Linear scaling with CPU cores for parallel operations
/// - Memory usage <2GB for 1M sequences
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use talaria_bio::sequence::Sequence;
use talaria_test::fixtures::test_database_source;
use talaria_sequoia::{
    chunker::{ChunkingStrategy, TaxonomicChunker},
    database::DatabaseManager,
    download::DatabaseSource,
    storage::SequenceStorage,
};
use tempfile::{NamedTempFile, TempDir};

/// Generate a test FASTA file with specified number of sequences
fn generate_fasta_file(num_sequences: usize, avg_seq_length: usize) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();

    for i in 0..num_sequences {
        // Write header
        writeln!(file, ">seq_{:06} Test sequence {}", i, i).unwrap();

        // Generate sequence (mix of ACGT)
        let seq_len = avg_seq_length + (i % 100); // Vary length slightly
        let bases = ['A', 'C', 'G', 'T'];
        let sequence: String = (0..seq_len).map(|j| bases[(i + j) % 4]).collect();

        // Write sequence in 80-character lines (FASTA standard)
        for chunk in sequence.as_bytes().chunks(80) {
            writeln!(file, "{}", std::str::from_utf8(chunk).unwrap()).unwrap();
        }
    }

    file.flush().unwrap();
    file
}

/// Benchmark 1: FASTA Reading Performance
fn bench_fasta_reading(c: &mut Criterion) {
    let mut group = c.benchmark_group("fasta_reading");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    // Test different file sizes
    for num_sequences in [1000, 10000, 100000].iter() {
        let file = generate_fasta_file(*num_sequences, 500);
        let file_size = std::fs::metadata(file.path()).unwrap().len();

        group.throughput(Throughput::Bytes(file_size));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}seqs", num_sequences)),
            num_sequences,
            |b, _| {
                b.iter(|| {
                    use std::fs::File;
                    use std::io::{BufRead, BufReader};

                    let reader = BufReader::new(File::open(file.path()).unwrap());
                    let mut count = 0;
                    let mut in_sequence = false;

                    for line in reader.lines() {
                        let line = line.unwrap();
                        if line.starts_with('>') {
                            in_sequence = true;
                            count += 1;
                        } else if in_sequence {
                            black_box(&line);
                        }
                    }

                    assert_eq!(count, *num_sequences);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 2: Sequence Parsing Performance
fn bench_sequence_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequence_parsing");
    group.sample_size(10);

    for num_sequences in [1000, 10000, 50000].iter() {
        let file = generate_fasta_file(*num_sequences, 500);

        group.throughput(Throughput::Elements(*num_sequences as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}seqs", num_sequences)),
            &file,
            |b, file| {
                b.iter(|| {
                    use talaria_bio::formats::fasta::parse_fasta;

                    let sequences = parse_fasta(file.path()).unwrap();

                    assert_eq!(sequences.len(), *num_sequences);
                    black_box(sequences);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 3: Chunking Throughput
fn bench_chunking_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("chunking_throughput");
    group.sample_size(10);

    let temp_dir = TempDir::new().unwrap();

    // Generate test sequences
    for num_sequences in [1000, 10000, 50000].iter() {
        let sequences: Vec<Sequence> = (0..*num_sequences)
            .map(|i| {
                let seq_len = 500 + (i % 100);
                Sequence {
                    id: format!("seq_{:06}", i),
                    description: Some(format!("Test sequence {}", i)),
                    sequence: vec![b'A'; seq_len],
                    taxon_id: Some((i % 1000) as u32),
                    taxonomy_sources: Default::default(),
                }
            })
            .collect();

        let total_bytes: usize = sequences.iter().map(|s| s.sequence.len()).sum();

        group.throughput(Throughput::Bytes(total_bytes as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}seqs", num_sequences)),
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

/// Benchmark 4: Deduplication Performance
fn bench_deduplication_perf(c: &mut Criterion) {
    let mut group = c.benchmark_group("deduplication_perf");
    group.sample_size(10);

    let temp_dir = TempDir::new().unwrap();

    // Test with different duplicate ratios
    for dup_ratio in [0.0, 0.25, 0.50, 0.75].iter() {
        let num_unique = 10000;
        let num_duplicates = (num_unique as f64 * dup_ratio) as usize;
        let total_sequences = num_unique + num_duplicates;

        let sequences: Vec<_> = (0..num_unique)
            .map(|i| {
                (
                    format!("UNIQUE_SEQ_{:06}_ACGTACGT", i),
                    format!("seq_{}", i),
                )
            })
            .chain((0..num_duplicates).map(|i| {
                (
                    format!("UNIQUE_SEQ_{:06}_ACGTACGT", i % 100),
                    format!("dup_{}", i),
                )
            }))
            .collect();

        group.throughput(Throughput::Elements(total_sequences as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}dup", (dup_ratio * 100.0) as u32)),
            &sequences,
            |b, sequences| {
                b.iter_with_setup(
                    || {
                        let storage = SequenceStorage::new(temp_dir.path()).unwrap();
                        (storage, sequences.clone())
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

/// Benchmark 5: Parallel vs Sequential Processing
fn bench_parallel_vs_sequential(c: &mut Criterion) {
    use rayon::prelude::*;

    let mut group = c.benchmark_group("parallel_vs_sequential");
    group.sample_size(10);

    let sequences: Vec<Vec<u8>> = (0..100000)
        .map(|i| {
            let len = 500 + (i % 100);
            vec![b'A'; len]
        })
        .collect();

    // Sequential hashing
    group.bench_function("sequential_hash", |b| {
        b.iter(|| {
            let hashes: Vec<_> = sequences
                .iter()
                .map(|seq| {
                    use talaria_core::SHA256Hash;
                    SHA256Hash::compute(black_box(seq))
                })
                .collect();
            black_box(hashes);
        });
    });

    // Parallel hashing
    group.bench_function("parallel_hash", |b| {
        b.iter(|| {
            let hashes: Vec<_> = sequences
                .par_iter()
                .map(|seq| {
                    use talaria_core::SHA256Hash;
                    SHA256Hash::compute(black_box(seq))
                })
                .collect();
            black_box(hashes);
        });
    });

    group.finish();
}

/// Benchmark 6: Memory-Aware Batch Processing
fn bench_memory_aware_batching(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_aware_batching");
    group.sample_size(10);

    use talaria_sequoia::performance::{AdaptiveConfigBuilder, AdaptiveManager};

    // Test different memory limits
    for memory_limit_mb in [100, 500, 1000, 2000].iter() {
        let config = AdaptiveConfigBuilder::new()
            .memory_limit_mb(*memory_limit_mb)
            .min_batch_size(100)
            .max_batch_size(100000)
            .build();

        let adaptive = AdaptiveManager::with_config(config).unwrap();

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}mb", memory_limit_mb)),
            memory_limit_mb,
            |b, _| {
                b.iter(|| {
                    let batch_size = adaptive.get_memory_aware_batch_size();

                    // Simulate processing a batch
                    let sequences: Vec<Vec<u8>> = (0..batch_size)
                        .map(|i| vec![b'A'; 500 + (i % 100)])
                        .collect();

                    black_box(sequences);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 7: End-to-End Pipeline Performance
fn bench_end_to_end_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end_pipeline");
    group.sample_size(5);
    group.measurement_time(Duration::from_secs(30));

    let temp_dir = TempDir::new().unwrap();

    // Test with realistic file sizes
    for file_size_mb in [10, 100, 500].iter() {
        let num_sequences = (*file_size_mb * 1_000_000) / 500; // Assume ~500 bytes per sequence
        let file = generate_fasta_file(num_sequences, 500);

        group.throughput(Throughput::Bytes((*file_size_mb * 1_000_000) as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}mb", file_size_mb)),
            &file,
            |b, file| {
                b.iter_with_setup(
                    || {
                        std::env::set_var("TALARIA_HOME", temp_dir.path());
                        let manager = DatabaseManager::new(None).unwrap();
                        (manager, file.path().to_path_buf())
                    },
                    |(mut manager, path)| {
                        // This simulates the full pipeline
                        let source = test_database_source("bench");
                        manager.chunk_database(&path, &source, None).unwrap();
                    },
                );
            },
        );
    }

    group.finish();
}

/// Benchmark 8: Checkpoint/Resume Overhead
fn bench_checkpoint_overhead(c: &mut Criterion) {
    use talaria_sequoia::checkpoint::ChunkingCheckpoint;

    let mut group = c.benchmark_group("checkpoint_overhead");

    let temp_dir = TempDir::new().unwrap();
    let checkpoint_path = temp_dir.path().join("checkpoint.json");

    // Benchmark checkpoint saving
    group.bench_function("checkpoint_save", |b| {
        let checkpoint =
            ChunkingCheckpoint::new(checkpoint_path.clone(), "test_version".to_string());

        b.iter(|| {
            let mut cp = checkpoint.clone();
            cp.update(10000, 5_000_000, Some("seq_10000".to_string()));
            cp.save().unwrap();
        });
    });

    // Benchmark checkpoint loading
    group.bench_function("checkpoint_load", |b| {
        let checkpoint =
            ChunkingCheckpoint::new(checkpoint_path.clone(), "test_version".to_string());
        checkpoint.save().unwrap();

        b.iter(|| ChunkingCheckpoint::load(&checkpoint_path).unwrap());
    });

    group.finish();
}

// Main benchmark groups
criterion_group!(
    benches,
    bench_fasta_reading,
    bench_sequence_parsing,
    bench_chunking_throughput,
    bench_deduplication_perf,
    bench_parallel_vs_sequential,
    bench_memory_aware_batching,
    bench_end_to_end_pipeline,
    bench_checkpoint_overhead
);

criterion_main!(benches);
