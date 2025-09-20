use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use talaria::bio::sequence::Sequence;
use talaria::cli::TargetAligner;
use talaria::core::config::Config;
use talaria::core::reducer::Reducer;
use talaria::core::reference_selector::ReferenceSelector;

fn generate_sequences(count: usize, avg_length: usize, similarity: f64) -> Vec<Sequence> {
    let mut sequences = Vec::with_capacity(count);
    let bases = b"ACGT";

    // Generate a reference sequence
    let mut ref_seq = Vec::with_capacity(avg_length);
    for i in 0..avg_length {
        ref_seq.push(bases[i % 4]);
    }

    for i in 0..count {
        let mut seq = ref_seq.clone();

        // Introduce mutations based on similarity
        let num_mutations = ((1.0 - similarity) * avg_length as f64) as usize;
        for _ in 0..num_mutations {
            let pos = (i * 7 + 13) % avg_length; // Pseudo-random position
            seq[pos] = bases[(seq[pos] as usize + 1) % 4];
        }

        sequences.push(Sequence::new(format!("seq_{}", i), seq));
    }

    sequences
}

fn bench_reference_selection(c: &mut Criterion) {
    let mut group = c.benchmark_group("reduction/reference_selection");

    for num_seqs in [100, 500, 1000, 5000].iter() {
        let sequences = generate_sequences(*num_seqs, 500, 0.8);
        let selector = ReferenceSelector::new().with_similarity_threshold(0.8);

        group.bench_with_input(BenchmarkId::from_parameter(num_seqs), num_seqs, |b, _| {
            b.iter(|| {
                let refs = selector.select_references(black_box(sequences.clone()), 0.5);
                black_box(refs);
            });
        });
    }

    group.finish();
}

fn bench_full_reduction(c: &mut Criterion) {
    let mut group = c.benchmark_group("reduction/full_pipeline");
    group.sample_size(10); // Reduce sample size for longer benchmarks

    for num_seqs in [100, 500, 1000].iter() {
        let sequences = generate_sequences(*num_seqs, 500, 0.85);
        let config = Config::default();
        let reducer = Reducer::new(config).with_silent(true);

        group.bench_with_input(BenchmarkId::from_parameter(num_seqs), num_seqs, |b, _| {
            b.iter(|| {
                let (refs, deltas) = reducer
                    .reduce(black_box(sequences.clone()), 0.5, TargetAligner::Generic)
                    .unwrap();
                black_box((refs, deltas));
            });
        });
    }

    group.finish();
}

fn bench_reduction_by_similarity(c: &mut Criterion) {
    let mut group = c.benchmark_group("reduction/by_similarity");

    let num_seqs = 500;
    for similarity in [0.7, 0.8, 0.9, 0.95].iter() {
        let sequences = generate_sequences(num_seqs, 500, *similarity);
        let config = Config::default();
        let reducer = Reducer::new(config).with_silent(true);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{:.0}%", similarity * 100.0)),
            similarity,
            |b, _| {
                b.iter(|| {
                    let (refs, deltas) = reducer
                        .reduce(black_box(sequences.clone()), 0.5, TargetAligner::Generic)
                        .unwrap();
                    black_box((refs, deltas));
                });
            },
        );
    }

    group.finish();
}

fn bench_aligner_specific_reduction(c: &mut Criterion) {
    let mut group = c.benchmark_group("reduction/by_aligner");

    let sequences = generate_sequences(500, 500, 0.85);
    let config = Config::default();
    let reducer = Reducer::new(config).with_silent(true);

    for aligner in [
        TargetAligner::Lambda,
        TargetAligner::Blast,
        TargetAligner::Diamond,
        TargetAligner::MMseqs2,
        TargetAligner::Kraken,
        TargetAligner::Generic,
    ]
    .iter()
    {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{:?}", aligner)),
            aligner,
            |b, aligner| {
                b.iter(|| {
                    let (refs, deltas) = reducer
                        .reduce(black_box(sequences.clone()), 0.5, aligner.clone())
                        .unwrap();
                    black_box((refs, deltas));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_reference_selection,
    bench_full_reduction,
    bench_reduction_by_similarity,
    bench_aligner_specific_reduction
);
criterion_main!(benches);
