use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::io::Write;
use talaria_bio::formats::fasta::{
    parse_fasta, parse_fasta_from_bytes, parse_fasta_parallel, write_fasta,
};
use talaria_bio::sequence::Sequence;
use tempfile::NamedTempFile;

fn create_test_sequences(count: usize) -> Vec<Sequence> {
    (0..count)
        .map(|i| {
            let seq_data = match i % 3 {
                0 => b"ATGCATGCATGCATGCATGCATGCATGCATGC".to_vec(), // DNA
                1 => b"ACDEFGHIKLMNPQRSTVWYACDEFGHIKLMN".to_vec(), // Protein
                _ => b"ATGCNNNATGCATGCATGCATGCATGCATGC".to_vec(),  // DNA with ambiguous
            };
            Sequence::new(format!("seq_{}", i), seq_data)
                .with_description(format!("Test sequence {}", i))
        })
        .collect()
}

fn write_test_file(sequences: &[Sequence]) -> NamedTempFile {
    let mut temp_file = NamedTempFile::new().unwrap();

    for seq in sequences {
        writeln!(
            temp_file,
            ">{} {}",
            seq.id,
            seq.description.as_ref().unwrap_or(&String::new())
        )
        .unwrap();

        // Write sequence in 80-char lines
        for chunk in seq.sequence.chunks(80) {
            writeln!(temp_file, "{}", std::str::from_utf8(chunk).unwrap()).unwrap();
        }
    }

    temp_file.flush().unwrap();
    temp_file
}

fn bench_parse_fasta(c: &mut Criterion) {
    let mut group = c.benchmark_group("fasta_parsing");

    for size in &[100, 1000, 10000] {
        let sequences = create_test_sequences(*size);
        let temp_file = write_test_file(&sequences);
        let file_size = std::fs::metadata(temp_file.path()).unwrap().len();

        group.throughput(Throughput::Bytes(file_size));

        group.bench_with_input(BenchmarkId::new("serial", size), &temp_file, |b, file| {
            b.iter(|| parse_fasta(black_box(file.path())));
        });

        group.bench_with_input(BenchmarkId::new("parallel", size), &temp_file, |b, file| {
            b.iter(|| parse_fasta_parallel(black_box(file.path()), 1024 * 1024));
        });
    }

    group.finish();
}

fn bench_parse_from_bytes(c: &mut Criterion) {
    let mut group = c.benchmark_group("fasta_from_bytes");

    for size in &[100, 1000, 10000] {
        let sequences = create_test_sequences(*size);
        let temp_file = write_test_file(&sequences);
        let bytes = std::fs::read(temp_file.path()).unwrap();

        group.throughput(Throughput::Bytes(bytes.len() as u64));

        group.bench_with_input(BenchmarkId::new("from_bytes", size), &bytes, |b, data| {
            b.iter(|| parse_fasta_from_bytes(black_box(data)));
        });
    }

    group.finish();
}

fn bench_write_fasta(c: &mut Criterion) {
    let mut group = c.benchmark_group("fasta_writing");

    for size in &[100, 1000, 10000] {
        let sequences = create_test_sequences(*size);

        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("write", size), &sequences, |b, seqs| {
            b.iter(|| {
                let temp_file = NamedTempFile::new().unwrap();
                write_fasta(black_box(temp_file.path()), black_box(seqs))
            });
        });
    }

    group.finish();
}

fn bench_compressed_fasta(c: &mut Criterion) {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let mut group = c.benchmark_group("compressed_fasta");

    for size in &[100, 1000] {
        let sequences = create_test_sequences(*size);
        let temp_file = NamedTempFile::new().unwrap();
        let gz_path = format!("{}.gz", temp_file.path().display());

        // Create compressed file
        {
            let file = std::fs::File::create(&gz_path).unwrap();
            let mut gz = GzEncoder::new(file, Compression::default());

            for seq in &sequences {
                writeln!(
                    gz,
                    ">{} {}",
                    seq.id,
                    seq.description.as_ref().unwrap_or(&String::new())
                )
                .unwrap();
                gz.write_all(&seq.sequence).unwrap();
                gz.write_all(b"\n").unwrap();
            }
            gz.finish().unwrap();
        }

        let file_size = std::fs::metadata(&gz_path).unwrap().len();
        group.throughput(Throughput::Bytes(file_size));

        group.bench_with_input(
            BenchmarkId::new("parse_compressed", size),
            &gz_path,
            |b, path| {
                b.iter(|| parse_fasta(black_box(path)));
            },
        );

        // Clean up
        std::fs::remove_file(gz_path).ok();
    }

    group.finish();
}

fn bench_large_sequence(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_sequences");

    // Test with different sequence lengths
    for seq_len in &[1000, 10000, 100000] {
        let sequences = vec![Sequence::new(
            "large_seq".to_string(),
            b"ATGC".repeat(seq_len / 4).to_vec(),
        )];

        let temp_file = write_test_file(&sequences);
        let file_size = std::fs::metadata(temp_file.path()).unwrap().len();

        group.throughput(Throughput::Bytes(file_size));

        group.bench_with_input(
            BenchmarkId::new("parse_large", seq_len),
            &temp_file,
            |b, file| {
                b.iter(|| parse_fasta(black_box(file.path())));
            },
        );
    }

    group.finish();
}

fn bench_memory_mapped(c: &mut Criterion) {
    use memmap2::MmapOptions;
    use std::fs::File;

    let mut group = c.benchmark_group("memory_mapped");

    for size in &[100, 1000, 10000] {
        let sequences = create_test_sequences(*size);
        let temp_file = write_test_file(&sequences);

        // Memory map the file
        let file = File::open(temp_file.path()).unwrap();
        let mmap = unsafe { MmapOptions::new().map(&file).unwrap() };

        group.throughput(Throughput::Bytes(mmap.len() as u64));

        group.bench_with_input(BenchmarkId::new("mmap_parse", size), &mmap, |b, data| {
            b.iter(|| parse_fasta_from_bytes(black_box(data)));
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_parse_fasta,
    bench_parse_from_bytes,
    bench_write_fasta,
    bench_compressed_fasta,
    bench_large_sequence,
    bench_memory_mapped
);
criterion_main!(benches);
