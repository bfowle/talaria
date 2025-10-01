use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use talaria_bio::alignment::{Alignment, NeedlemanWunsch, NucleotideMatrix, BLOSUM62};
use talaria_bio::sequence::Sequence;

fn create_dna_sequence(length: usize) -> Vec<u8> {
    let bases = b"ATGC";
    (0..length).map(|i| bases[i % 4]).collect()
}

fn create_protein_sequence(length: usize) -> Vec<u8> {
    let amino_acids = b"ACDEFGHIKLMNPQRSTVWY";
    (0..length).map(|i| amino_acids[i % 20]).collect()
}

fn create_sequences_with_mutations(base: &[u8], mutation_rate: f64) -> Vec<u8> {
    base.iter()
        .map(|&b| {
            if rand::random::<f64>() < mutation_rate {
                // Randomly mutate to a different base
                match b {
                    b'A' => b'T',
                    b'T' => b'G',
                    b'G' => b'C',
                    b'C' => b'A',
                    _ => b,
                }
            } else {
                b
            }
        })
        .collect()
}

fn bench_dna_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("dna_alignment");

    for length in &[50, 100, 500, 1000] {
        let ref_seq = create_dna_sequence(*length);
        let query_seq = create_sequences_with_mutations(&ref_seq, 0.05); // 5% mutation rate

        group.throughput(Throughput::Elements(*length as u64));

        group.bench_with_input(
            BenchmarkId::new("needleman_wunsch", length),
            &(ref_seq.clone(), query_seq.clone()),
            |b, (ref_seq, query_seq)| {
                let aligner = NeedlemanWunsch::new(NucleotideMatrix::new());
                b.iter(|| aligner.align(black_box(ref_seq), black_box(query_seq)));
            },
        );

        // Test with Sequence wrapper
        let ref_sequence = Sequence::new("ref".to_string(), ref_seq.clone());
        let query_sequence = Sequence::new("query".to_string(), query_seq.clone());

        group.bench_with_input(
            BenchmarkId::new("global_alignment", length),
            &(ref_sequence.clone(), query_sequence.clone()),
            |b, (ref_seq, query_seq)| {
                b.iter(|| Alignment::global(black_box(ref_seq), black_box(query_seq)));
            },
        );
    }

    group.finish();
}

fn bench_protein_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("protein_alignment");

    for length in &[20, 50, 100, 200] {
        let ref_seq = create_protein_sequence(*length);
        let query_seq = create_sequences_with_mutations(&ref_seq, 0.1); // 10% mutation rate

        group.throughput(Throughput::Elements(*length as u64));

        group.bench_with_input(
            BenchmarkId::new("blosum62", length),
            &(ref_seq.clone(), query_seq.clone()),
            |b, (ref_seq, query_seq)| {
                let aligner = NeedlemanWunsch::new(BLOSUM62::new());
                b.iter(|| aligner.align(black_box(ref_seq), black_box(query_seq)));
            },
        );
    }

    group.finish();
}

fn bench_identical_sequences(c: &mut Criterion) {
    let mut group = c.benchmark_group("identical_sequences");

    for length in &[100, 500, 1000] {
        let sequence = create_dna_sequence(*length);

        group.throughput(Throughput::Elements(*length as u64));

        group.bench_with_input(
            BenchmarkId::new("perfect_match", length),
            &sequence,
            |b, seq| {
                let aligner = NeedlemanWunsch::new(NucleotideMatrix::new());
                b.iter(|| aligner.align(black_box(seq), black_box(seq)));
            },
        );
    }

    group.finish();
}

fn bench_gap_heavy_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("gap_alignment");

    for length in &[50, 100, 200] {
        let ref_seq = create_dna_sequence(*length);

        // Create query with deletions
        let mut query_seq = ref_seq.clone();
        // Remove every 10th base
        query_seq = query_seq
            .into_iter()
            .enumerate()
            .filter_map(|(i, b)| if i % 10 != 0 { Some(b) } else { None })
            .collect();

        group.throughput(Throughput::Elements(*length as u64));

        group.bench_with_input(
            BenchmarkId::new("with_gaps", length),
            &(ref_seq.clone(), query_seq.clone()),
            |b, (ref_seq, query_seq)| {
                let aligner = NeedlemanWunsch::new(NucleotideMatrix::new());
                b.iter(|| aligner.align(black_box(ref_seq), black_box(query_seq)));
            },
        );
    }

    group.finish();
}

fn bench_varying_similarity(c: &mut Criterion) {
    let mut group = c.benchmark_group("similarity_levels");
    let length = 100;
    let ref_seq = create_dna_sequence(length);

    for mutation_rate in &[0.0, 0.1, 0.3, 0.5] {
        let query_seq = create_sequences_with_mutations(&ref_seq, *mutation_rate);

        group.throughput(Throughput::Elements(length as u64));

        group.bench_with_input(
            BenchmarkId::new("mutation_rate", format!("{:.0}%", mutation_rate * 100.0)),
            &(ref_seq.clone(), query_seq.clone()),
            |b, (ref_seq, query_seq)| {
                let aligner = NeedlemanWunsch::new(NucleotideMatrix::new());
                b.iter(|| aligner.align(black_box(ref_seq), black_box(query_seq)));
            },
        );
    }

    group.finish();
}

fn bench_real_world_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("real_world");

    // Common sequence sizes in bioinformatics
    let sizes = vec![
        ("illumina_read", 150), // Typical Illumina read
        ("sanger_read", 800),   // Typical Sanger sequencing
        ("gene", 3000),         // Average gene size
        ("small_protein", 300), // Small protein
    ];

    for (name, size) in sizes {
        let ref_seq = create_dna_sequence(size);
        let query_seq = create_sequences_with_mutations(&ref_seq, 0.02); // 2% error rate

        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(
            BenchmarkId::new("align", name),
            &(ref_seq.clone(), query_seq.clone()),
            |b, (ref_seq, query_seq)| {
                let aligner = NeedlemanWunsch::new(NucleotideMatrix::new());
                b.iter(|| aligner.align(black_box(ref_seq), black_box(query_seq)));
            },
        );
    }

    group.finish();
}

fn bench_worst_case(c: &mut Criterion) {
    let mut group = c.benchmark_group("worst_case");

    // Test with completely different sequences (worst case for dynamic programming)
    for length in &[50, 100, 150] {
        let ref_seq = vec![b'A'; *length];
        let query_seq = vec![b'T'; *length];

        group.throughput(Throughput::Elements(*length as u64));

        group.bench_with_input(
            BenchmarkId::new("no_matches", length),
            &(ref_seq.clone(), query_seq.clone()),
            |b, (ref_seq, query_seq)| {
                let aligner = NeedlemanWunsch::new(NucleotideMatrix::new());
                b.iter(|| aligner.align(black_box(ref_seq), black_box(query_seq)));
            },
        );
    }

    group.finish();
}

fn bench_delta_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("delta_extraction");

    for length in &[100, 500, 1000] {
        let ref_seq = Sequence::new("ref".to_string(), create_dna_sequence(*length));
        let query_bytes = create_sequences_with_mutations(&ref_seq.sequence, 0.1);
        let query_seq = Sequence::new("query".to_string(), query_bytes);

        group.throughput(Throughput::Elements(*length as u64));

        group.bench_with_input(
            BenchmarkId::new("extract_deltas", length),
            &(ref_seq.clone(), query_seq.clone()),
            |b, (ref_seq, query_seq)| {
                b.iter(|| {
                    let alignment = Alignment::global(black_box(ref_seq), black_box(query_seq));
                    black_box(alignment.deltas);
                });
            },
        );
    }

    group.finish();
}

// Add rand dependency for mutations
use rand;

criterion_group!(
    benches,
    bench_dna_alignment,
    bench_protein_alignment,
    bench_identical_sequences,
    bench_gap_heavy_alignment,
    bench_varying_similarity,
    bench_real_world_sizes,
    bench_worst_case,
    bench_delta_extraction
);
criterion_main!(benches);
