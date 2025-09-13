use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::hint::black_box;
use talaria::bio::alignment::Alignment;
use talaria::bio::sequence::Sequence;

fn generate_sequence(length: usize, seed: u8) -> Sequence {
    let mut seq = Vec::with_capacity(length);
    let bases = b"ACGT";
    for i in 0..length {
        seq.push(bases[(i + seed as usize) % 4]);
    }
    Sequence::new(format!("seq_{}", seed), seq)
}

fn generate_protein_sequence(length: usize, seed: u8) -> Sequence {
    let mut seq = Vec::with_capacity(length);
    let amino_acids = b"ACDEFGHIKLMNPQRSTVWY";
    for i in 0..length {
        seq.push(amino_acids[(i + seed as usize) % 20]);
    }
    Sequence::new(format!("protein_{}", seed), seq)
}

fn bench_short_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("alignment/short");
    
    for length in [10, 25, 50, 100].iter() {
        let seq1 = generate_sequence(*length, 1);
        let seq2 = generate_sequence(*length, 2);
        
        group.bench_with_input(
            BenchmarkId::from_parameter(length),
            length,
            |b, _| {
                b.iter(|| {
                    Alignment::global(black_box(&seq1), black_box(&seq2))
                });
            },
        );
    }
    
    group.finish();
}

fn bench_medium_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("alignment/medium");
    
    for length in [250, 500, 750, 1000].iter() {
        let seq1 = generate_sequence(*length, 1);
        let seq2 = generate_sequence(*length, 2);
        
        group.bench_with_input(
            BenchmarkId::from_parameter(length),
            length,
            |b, _| {
                b.iter(|| {
                    Alignment::global(black_box(&seq1), black_box(&seq2))
                });
            },
        );
    }
    
    group.finish();
}

fn bench_long_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("alignment/long");
    group.sample_size(10); // Reduce sample size for long sequences
    
    for length in [2500, 5000].iter() {
        let seq1 = generate_sequence(*length, 1);
        let seq2 = generate_sequence(*length, 2);
        
        group.bench_with_input(
            BenchmarkId::from_parameter(length),
            length,
            |b, _| {
                b.iter(|| {
                    Alignment::global(black_box(&seq1), black_box(&seq2))
                });
            },
        );
    }
    
    group.finish();
}

fn bench_protein_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("alignment/protein");
    
    for length in [50, 100, 250, 500].iter() {
        let seq1 = generate_protein_sequence(*length, 1);
        let seq2 = generate_protein_sequence(*length, 2);
        
        group.bench_with_input(
            BenchmarkId::from_parameter(length),
            length,
            |b, _| {
                b.iter(|| {
                    Alignment::global(black_box(&seq1), black_box(&seq2))
                });
            },
        );
    }
    
    group.finish();
}

fn bench_identical_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("alignment/identical");
    
    for length in [100, 500, 1000].iter() {
        let seq = generate_sequence(*length, 1);
        
        group.bench_with_input(
            BenchmarkId::from_parameter(length),
            length,
            |b, _| {
                b.iter(|| {
                    Alignment::global(black_box(&seq), black_box(&seq))
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_short_alignment,
    bench_medium_alignment,
    bench_long_alignment,
    bench_protein_alignment,
    bench_identical_alignment
);
criterion_main!(benches);