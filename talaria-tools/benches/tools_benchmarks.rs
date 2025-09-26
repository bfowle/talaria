use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use talaria_tools::optimizers::{
    blast::BlastOptimizer, generic::GenericOptimizer,
    kraken::KrakenOptimizer, lambda::LambdaOptimizer
};
use talaria_bio::sequence::Sequence;

// Note: Parser benchmarks removed as parser module is not publicly exposed

// ===== Optimizer Benchmarks =====

fn create_test_sequences(count: usize) -> Vec<Sequence> {
    (0..count)
        .map(|i| {
            let len = (i % 100) + 50; // Vary lengths between 50-150
            let seq = vec![b'A'; len];
            let taxon = if i % 3 == 0 { Some((i % 1000) as u32) } else { None };

            let mut sequence = Sequence::new(format!("seq_{}", i), seq);
            if let Some(t) = taxon {
                sequence = sequence.with_taxon(t);
            }
            sequence
        })
        .collect()
}

fn bench_sequence_optimization(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequence_optimization");

    // Different dataset sizes
    let sizes = vec![100, 1000, 10000];

    for size in sizes {
        let sequences = create_test_sequences(size);

        group.bench_with_input(
            BenchmarkId::new("blast_optimizer", size),
            &sequences,
            |b, seqs| {
                let optimizer = BlastOptimizer::new();
                b.iter(|| {
                    let mut s = seqs.clone();
                    optimizer.optimize_for_blast(black_box(&mut s))
                })
            }
        );

        group.bench_with_input(
            BenchmarkId::new("lambda_optimizer", size),
            &sequences,
            |b, seqs| {
                let optimizer = LambdaOptimizer::new();
                b.iter(|| {
                    let mut s = seqs.clone();
                    optimizer.optimize_for_lambda(black_box(&mut s))
                })
            }
        );

        group.bench_with_input(
            BenchmarkId::new("kraken_optimizer", size),
            &sequences,
            |b, seqs| {
                let optimizer = KrakenOptimizer::new();
                b.iter(|| {
                    let mut s = seqs.clone();
                    optimizer.optimize_for_kraken(black_box(&mut s))
                })
            }
        );

        group.bench_with_input(
            BenchmarkId::new("generic_optimizer", size),
            &sequences,
            |b, seqs| {
                let optimizer = GenericOptimizer::new();
                b.iter(|| {
                    let mut s = seqs.clone();
                    optimizer.optimize(black_box(&mut s))
                })
            }
        );
    }

    group.finish();
}

// ===== Taxonomy Mapping Benchmarks =====

fn bench_taxonomy_mapping(c: &mut Criterion) {
    let mut group = c.benchmark_group("taxonomy_mapping");

    let sizes = vec![100, 1000, 10000];
    let optimizer = LambdaOptimizer::new();

    for size in sizes {
        let sequences = create_test_sequences(size);

        group.bench_with_input(
            BenchmarkId::new("prepare_mapping", size),
            &sequences,
            |b, seqs| {
                b.iter(|| optimizer.prepare_taxonomy_mapping(black_box(seqs)))
            }
        );
    }

    group.finish();
}

// ===== Critical Path Benchmarks =====

fn bench_critical_workflows(c: &mut Criterion) {
    let mut group = c.benchmark_group("critical_workflows");

    // Benchmark: Optimize and prepare for alignment (reduction workflow)
    group.bench_function("optimize_for_alignment", |b| {
        let sequences = create_test_sequences(1000);
        let lambda_opt = LambdaOptimizer::new();

        b.iter(|| {
            let mut seqs = sequences.clone();
            lambda_opt.optimize_for_lambda(black_box(&mut seqs));
            let mapping = lambda_opt.prepare_taxonomy_mapping(&seqs);
            black_box(mapping);
        })
    });

    // Benchmark: Multi-stage optimization pipeline
    group.bench_function("multistage_optimization", |b| {
        let sequences = create_test_sequences(500);

        b.iter(|| {
            let mut seqs = sequences.clone();

            // Stage 1: Generic optimization
            GenericOptimizer::new().optimize(black_box(&mut seqs));

            // Stage 2: Tool-specific optimization
            LambdaOptimizer::new().optimize_for_lambda(black_box(&mut seqs));

            // Stage 3: Extract metadata
            let mapping = LambdaOptimizer::new().prepare_taxonomy_mapping(&seqs);

            black_box(mapping);
        })
    });

    group.finish();
}

// ===== String Processing Benchmarks =====

fn bench_string_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_operations");

    // Benchmark version comparison (used in tool management)
    group.bench_function("version_comparison", |b| {
        use talaria_tools::manager::ToolManager;
        let temp_dir = tempfile::TempDir::new().unwrap();
        let manager = ToolManager::with_directory(temp_dir.path());

        b.iter(|| {
            let _ = manager.compare_versions(black_box("1.2.3"), black_box("1.2.4"));
            let _ = manager.compare_versions(black_box("2.0.0"), black_box("1.9.9"));
            let _ = manager.compare_versions(black_box("1.0.0"), black_box("1.0.0"));
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_sequence_optimization,
    bench_taxonomy_mapping,
    bench_critical_workflows,
    bench_string_operations
);
criterion_main!(benches);