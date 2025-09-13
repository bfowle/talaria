use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::fs;
use std::hint::black_box;
use talaria::bio::fasta::parse_fasta;

fn generate_fasta_file(num_sequences: usize, seq_length: usize) -> String {
    let mut content = String::new();
    let bases = b"ACGT";
    
    for i in 0..num_sequences {
        content.push_str(&format!(">seq_{} description\n", i));
        for j in 0..seq_length {
            content.push(bases[(i + j) % 4] as char);
            if (j + 1) % 80 == 0 {
                content.push('\n');
            }
        }
        if seq_length % 80 != 0 {
            content.push('\n');
        }
    }
    
    content
}

fn bench_small_file_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("fasta_parsing/small");
    
    for num_seqs in [10, 50, 100, 500].iter() {
        let content = generate_fasta_file(*num_seqs, 1000);
        let temp_file = format!("/tmp/bench_fasta_{}.fa", num_seqs);
        fs::write(&temp_file, &content).unwrap();
        
        group.bench_with_input(
            BenchmarkId::from_parameter(num_seqs),
            num_seqs,
            |b, _| {
                b.iter(|| {
                    let sequences = parse_fasta(&temp_file).unwrap();
                    black_box(sequences);
                });
            },
        );
        
        fs::remove_file(&temp_file).ok();
    }
    
    group.finish();
}

fn bench_medium_file_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("fasta_parsing/medium");
    
    for num_seqs in [1000, 5000, 10000].iter() {
        let content = generate_fasta_file(*num_seqs, 500);
        let temp_file = format!("/tmp/bench_fasta_{}.fa", num_seqs);
        fs::write(&temp_file, &content).unwrap();
        
        group.bench_with_input(
            BenchmarkId::from_parameter(num_seqs),
            num_seqs,
            |b, _| {
                b.iter(|| {
                    let sequences = parse_fasta(&temp_file).unwrap();
                    black_box(sequences);
                });
            },
        );
        
        fs::remove_file(&temp_file).ok();
    }
    
    group.finish();
}

fn bench_parallel_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("fasta_parsing/parallel");
    
    for num_seqs in [10000, 50000].iter() {
        let content = generate_fasta_file(*num_seqs, 500);
        let temp_file = format!("/tmp/bench_fasta_parallel_{}.fa", num_seqs);
        fs::write(&temp_file, &content).unwrap();
        
        group.bench_with_input(
            BenchmarkId::from_parameter(num_seqs),
            num_seqs,
            |b, _| {
                b.iter(|| {
                    // For now, use the same parse_fasta function
                    // In production, this would use the parallel version
                    let sequences = parse_fasta(&temp_file).unwrap();
                    black_box(sequences);
                });
            },
        );
        
        fs::remove_file(&temp_file).ok();
    }
    
    group.finish();
}

fn bench_memory_mapped_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("fasta_parsing/mmap");
    
    for num_seqs in [1000, 10000, 50000].iter() {
        let content = generate_fasta_file(*num_seqs, 500);
        let temp_file = format!("/tmp/bench_fasta_mmap_{}.fa", num_seqs);
        fs::write(&temp_file, &content).unwrap();
        
        group.bench_with_input(
            BenchmarkId::from_parameter(num_seqs),
            num_seqs,
            |b, _| {
                b.iter(|| {
                    let sequences = parse_fasta(&temp_file).unwrap();
                    black_box(sequences);
                });
            },
        );
        
        fs::remove_file(&temp_file).ok();
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_small_file_parsing,
    bench_medium_file_parsing,
    bench_parallel_parsing,
    bench_memory_mapped_parsing
);
criterion_main!(benches);