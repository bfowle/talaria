use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// Helper to create test FASTA content
fn create_test_fasta(num_sequences: usize, seq_length: usize) -> String {
    let mut content = String::new();
    let bases = ['A', 'T', 'G', 'C'];

    for i in 0..num_sequences {
        content.push_str(&format!(">seq_{} Benchmark sequence {}\n", i, i));
        for j in 0..seq_length {
            content.push(bases[(i + j) % 4]);
        }
        content.push('\n');
    }

    content
}

fn bench_fasta_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("fasta_parsing");

    for size in [10, 100, 1000, 10000].iter() {
        let fasta_content = create_test_fasta(*size, 100);

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &fasta_content,
            |b, content| {
                b.iter(|| {
                    // Benchmark FASTA parsing
                    let lines: Vec<&str> = content.lines().collect();
                    let seq_count = lines.iter()
                        .filter(|l| l.starts_with('>'))
                        .count();
                    black_box(seq_count);
                });
            },
        );
    }

    group.finish();
}

fn bench_sequence_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequence_comparison");

    let seq1 = "ATGATGATGATGATGATGATGATGATGATGATGATGATGATGATG";
    let seq2 = "ATGATGATGATGATGATGATGATGATGATGATGATGATGATGACG";

    group.bench_function("hamming_distance", |b| {
        b.iter(|| {
            let distance: usize = seq1.chars()
                .zip(seq2.chars())
                .filter(|(a, b)| a != b)
                .count();
            black_box(distance);
        });
    });

    group.bench_function("similarity_ratio", |b| {
        b.iter(|| {
            let matches: usize = seq1.chars()
                .zip(seq2.chars())
                .filter(|(a, b)| a == b)
                .count();
            let similarity = matches as f64 / seq1.len() as f64;
            black_box(similarity);
        });
    });

    group.finish();
}

fn bench_compression(c: &mut Criterion) {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    let mut group = c.benchmark_group("compression");

    let test_data = create_test_fasta(100, 1000);
    let data_bytes = test_data.as_bytes();

    for level in [1, 6, 9].iter() {
        group.bench_with_input(
            BenchmarkId::new("gzip", level),
            &data_bytes,
            |b, data| {
                b.iter(|| {
                    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(*level));
                    encoder.write_all(data).unwrap();
                    let compressed = encoder.finish().unwrap();
                    black_box(compressed.len());
                });
            },
        );
    }

    group.finish();
}

fn bench_delta_encoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("delta_encoding");

    let reference = "ATGATGATGATGATGATGATGATGATGATGATGATGATGATGATG";
    let similar = "ATGATGATGATGATGATGATGATGATGATGATGATGATGATGACG";
    let different = "CGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCG";

    group.bench_function("similar_sequences", |b| {
        b.iter(|| {
            // Simulate delta encoding
            let mut ops = Vec::new();
            for (i, (r, s)) in reference.chars().zip(similar.chars()).enumerate() {
                if r != s {
                    ops.push((i, r, s));
                }
            }
            black_box(ops.len());
        });
    });

    group.bench_function("different_sequences", |b| {
        b.iter(|| {
            // Simulate delta encoding
            let mut ops = Vec::new();
            for (i, (r, s)) in reference.chars().zip(different.chars()).enumerate() {
                if r != s {
                    ops.push((i, r, s));
                }
            }
            black_box(ops.len());
        });
    });

    group.finish();
}

fn bench_batch_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_processing");

    let sequences: Vec<String> = (0..1000)
        .map(|i| format!("SEQUENCE_{}", i))
        .collect();

    for batch_size in [10, 50, 100, 500].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            batch_size,
            |b, &size| {
                b.iter(|| {
                    let batches: Vec<Vec<&String>> = sequences
                        .chunks(size)
                        .map(|chunk| chunk.iter().collect())
                        .collect();
                    black_box(batches.len());
                });
            },
        );
    }

    group.finish();
}

fn bench_taxonomy_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("taxonomy_parsing");

    let headers = vec![
        ">seq1 Escherichia coli OX=562 GN=gene1",
        ">seq2 Homo sapiens OX=9606 GN=BRCA1",
        ">seq3 Mus musculus OX=10090 GN=Tp53",
    ];

    group.bench_function("extract_taxon_id", |b| {
        b.iter(|| {
            for header in &headers {
                if let Some(ox_pos) = header.find("OX=") {
                    let taxon_str = &header[ox_pos + 3..];
                    let taxon_id: u32 = taxon_str
                        .split_whitespace()
                        .next()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    black_box(taxon_id);
                }
            }
        });
    });

    group.finish();
}

fn bench_hash_computation(c: &mut Criterion) {
    use sha2::{Sha256, Digest};

    let mut group = c.benchmark_group("hash_computation");

    for size in [100, 1000, 10000, 100000].iter() {
        let data = vec![0u8; *size];

        group.bench_with_input(
            BenchmarkId::new("sha256", size),
            &data,
            |b, data| {
                b.iter(|| {
                    let mut hasher = Sha256::new();
                    hasher.update(data);
                    let hash = hasher.finalize();
                    black_box(hash);
                });
            },
        );
    }

    group.finish();
}

fn bench_parallel_processing(c: &mut Criterion) {
    use rayon::prelude::*;

    let mut group = c.benchmark_group("parallel_processing");

    let data: Vec<String> = (0..10000)
        .map(|i| format!("SEQUENCE_DATA_{}", i))
        .collect();

    group.bench_function("sequential", |b| {
        b.iter(|| {
            let results: Vec<usize> = data
                .iter()
                .map(|s| s.len())
                .collect();
            black_box(results.len());
        });
    });

    group.bench_function("parallel", |b| {
        b.iter(|| {
            let results: Vec<usize> = data
                .par_iter()
                .map(|s| s.len())
                .collect();
            black_box(results.len());
        });
    });

    group.finish();
}

fn bench_file_io(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_io");

    let temp_dir = TempDir::new().unwrap();

    for size in [100, 1000, 10000].iter() {
        let content = create_test_fasta(*size, 100);
        let file_path = temp_dir.path().join(format!("test_{}.fasta", size));

        group.bench_with_input(
            BenchmarkId::new("write", size),
            &content,
            |b, content| {
                b.iter(|| {
                    fs::write(&file_path, content).unwrap();
                    black_box(());
                });
            },
        );

        // Write file once for read benchmark
        fs::write(&file_path, &content).unwrap();

        group.bench_with_input(
            BenchmarkId::new("read", size),
            &file_path,
            |b, path| {
                b.iter(|| {
                    let content = fs::read_to_string(path).unwrap();
                    black_box(content.len());
                });
            },
        );
    }

    group.finish();
}

fn bench_reference_selection(c: &mut Criterion) {
    let mut group = c.benchmark_group("reference_selection");

    let sequences: Vec<String> = (0..100)
        .map(|i| {
            let mut seq = String::new();
            for j in 0..100 {
                seq.push(['A', 'T', 'G', 'C'][(i + j) % 4]);
            }
            seq
        })
        .collect();

    group.bench_function("greedy_selection", |b| {
        b.iter(|| {
            // Simulate greedy reference selection
            let mut selected = Vec::new();
            let target_count = sequences.len() / 10;

            for i in 0..target_count {
                selected.push(&sequences[i * 10]);
            }

            black_box(selected.len());
        });
    });

    group.bench_function("similarity_based_selection", |b| {
        b.iter(|| {
            // Simulate similarity-based selection
            let mut selected = Vec::new();
            let mut remaining: Vec<_> = sequences.iter().collect();

            while selected.len() < 10 && !remaining.is_empty() {
                // Pick most diverse sequence (simplified)
                selected.push(remaining.remove(0));
            }

            black_box(selected.len());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_fasta_parsing,
    bench_sequence_comparison,
    bench_compression,
    bench_delta_encoding,
    bench_batch_processing,
    bench_taxonomy_parsing,
    bench_hash_computation,
    bench_parallel_processing,
    bench_file_io,
    bench_reference_selection,
);

criterion_main!(benches);