use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::time::Duration;
use talaria_utils::*;

// ===== Parallel Processing Benchmarks =====

fn bench_chunk_size_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("chunk_size_calculation");

    let sizes = vec![100, 1000, 10000, 100000, 1000000];
    let thread_counts = vec![1, 4, 8, 16, 32];

    for size in sizes {
        for threads in &thread_counts {
            group.bench_with_input(
                BenchmarkId::new("chunk_size", format!("{}items_{}threads", size, threads)),
                &(size, *threads),
                |b, &(size, threads)| {
                    b.iter(|| chunk_size_for_parallelism(black_box(size), black_box(threads)))
                },
            );
        }
    }

    group.finish();
}

fn bench_parallelization_decision(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallelization_decision");

    let item_counts = vec![10, 100, 1000, 10000];
    let thresholds = vec![50, 100, 500, 1000];

    for count in item_counts {
        for threshold in &thresholds {
            group.bench_with_input(
                BenchmarkId::new(
                    "should_parallelize",
                    format!("{}items_{}threshold", count, threshold),
                ),
                &(count, *threshold),
                |b, &(count, threshold)| {
                    b.iter(|| should_parallelize(black_box(count), black_box(threshold)))
                },
            );
        }
    }

    group.finish();
}

// ===== Tree Rendering Benchmarks =====

fn create_deep_tree(depth: usize, breadth: usize) -> TreeNode {
    fn add_children(
        node: TreeNode,
        current_depth: usize,
        max_depth: usize,
        breadth: usize,
    ) -> TreeNode {
        if current_depth >= max_depth {
            return node;
        }

        let mut node = node;
        for i in 0..breadth {
            let child = TreeNode::new(format!("Node_{}_{}", current_depth, i));
            let child = add_children(child, current_depth + 1, max_depth, breadth);
            node = node.add_child(child);
        }
        node
    }

    let root = TreeNode::new("Root");
    add_children(root, 0, depth, breadth)
}

fn bench_tree_rendering(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_rendering");

    // Different tree structures
    let trees = vec![
        ("small", create_deep_tree(3, 3)),  // 3 levels, 3 children per node
        ("medium", create_deep_tree(4, 4)), // 4 levels, 4 children per node
        ("deep", create_deep_tree(10, 2)),  // 10 levels, 2 children per node
        ("wide", create_deep_tree(3, 10)),  // 3 levels, 10 children per node
    ];

    for (name, tree) in trees {
        group.bench_with_input(BenchmarkId::new("render", name), &tree, |b, tree| {
            b.iter(|| tree.render())
        });
    }

    group.finish();
}

// ===== Number Formatting Benchmarks =====

fn bench_number_formatting(c: &mut Criterion) {
    let mut group = c.benchmark_group("number_formatting");

    let numbers: Vec<i64> = vec![
        0,
        999,
        1000,
        999999,
        1000000,
        999999999,
        1234567890,
        9223372036854775807, // max i64
    ];

    for num in numbers {
        group.bench_with_input(BenchmarkId::new("format_number", num), &num, |b, &num| {
            b.iter(|| format_number(black_box(num)))
        });
    }

    group.finish();
}

// ===== Progress Bar Benchmarks =====

fn bench_progress_bar_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("progress_bar_operations");

    group.bench_function("create_progress_bar", |b| {
        b.iter(|| {
            let pb = create_progress_bar(black_box(1000), black_box("Benchmark"));
            pb.finish();
        })
    });

    group.bench_function("create_spinner", |b| {
        b.iter(|| {
            let spinner = create_spinner(black_box("Benchmark"));
            spinner.finish();
        })
    });

    group.bench_function("progress_bar_updates", |b| {
        let pb = create_progress_bar(1000, "Benchmark");
        b.iter(|| {
            pb.inc(black_box(1));
        });
        pb.finish();
    });

    group.bench_function("progress_bar_manager", |b| {
        b.iter(|| {
            let manager = ProgressBarManager::new();
            let pb1 = manager.create_progress_bar(100, "Task 1");
            let pb2 = manager.create_spinner("Task 2");
            pb1.inc(50);
            pb2.tick();
            pb1.finish();
            pb2.finish();
        })
    });

    group.finish();
}

// ===== Format Utilities Benchmarks =====

fn bench_format_utilities(c: &mut Criterion) {
    let mut group = c.benchmark_group("format_utilities");

    // Benchmark byte formatting
    let byte_sizes: Vec<u64> = vec![
        0,
        1024,          // 1 KB
        1048576,       // 1 MB
        1073741824,    // 1 GB
        1099511627776, // 1 TB
    ];

    for size in byte_sizes {
        group.bench_with_input(BenchmarkId::new("format_bytes", size), &size, |b, &size| {
            b.iter(|| format_bytes(black_box(size)))
        });
    }

    // Benchmark duration formatting
    let durations = vec![
        Duration::from_secs(0),
        Duration::from_secs(59),
        Duration::from_secs(3600),
        Duration::from_secs(86400),
    ];

    for duration in durations {
        group.bench_with_input(
            BenchmarkId::new("format_duration", duration.as_secs()),
            &duration,
            |b, &duration| b.iter(|| format_duration(black_box(duration))),
        );
    }

    group.finish();
}

// ===== Memory Estimation Benchmarks =====

fn bench_memory_estimation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_estimation");

    let estimator = MemoryEstimator::new();

    let configs = vec![
        (100, 100),     // Small sequences
        (1000, 300),    // Medium sequences
        (10000, 500),   // Large sequences
        (100000, 1000), // Very large sequences
    ];

    for (count, avg_len) in configs {
        group.bench_with_input(
            BenchmarkId::new("estimate_sequence_memory", format!("{}x{}", count, avg_len)),
            &(count, avg_len),
            |b, &(count, avg_len)| {
                b.iter(|| estimator.estimate_sequence_memory(black_box(count), black_box(avg_len)))
            },
        );
    }

    group.bench_function("format_memory", |b| {
        b.iter(|| estimator.format_memory(black_box(1234567890)))
    });

    group.finish();
}

// ===== Database Reference Benchmarks =====

fn bench_database_references(c: &mut Criterion) {
    let mut group = c.benchmark_group("database_references");

    let refs = vec![
        "ncbi/nr",
        "uniprot/swissprot:2023.05",
        "custom/mydb:v1.2.3",
        "pdb/pdb_seqres:latest",
        "refseq/bacterial:2024.01.15",
    ];

    for ref_str in refs {
        group.bench_with_input(BenchmarkId::new("parse", ref_str), ref_str, |b, ref_str| {
            b.iter(|| DatabaseReference::parse(black_box(ref_str)))
        });
    }

    group.bench_function("version_comparison", |b| {
        let v1 = DatabaseVersion::new("2023.01.15");
        let v2 = DatabaseVersion::new("2023.02.01");
        b.iter(|| {
            let _ = black_box(&v1) < black_box(&v2);
        })
    });

    group.finish();
}

// ===== Critical Path Benchmarks =====

fn bench_critical_workflows(c: &mut Criterion) {
    let mut group = c.benchmark_group("critical_workflows");

    // Benchmark workspace creation and cleanup
    group.bench_function("workspace_lifecycle", |b| {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_WORKSPACE_DIR", temp_dir.path());

        b.iter(|| {
            let workspace = TempWorkspace::new(black_box("bench"));
            let _input = workspace.input_dir();
            let _output = workspace.output_dir();
            // Workspace will be cleaned up on drop
        });

        std::env::remove_var("TALARIA_WORKSPACE_DIR");
    });

    // Benchmark parallel chunk processing simulation
    group.bench_function("parallel_chunk_processing", |b| {
        use rayon::prelude::*;

        let data: Vec<i32> = (0..10000).collect();
        let chunk_size = chunk_size_for_parallelism(data.len(), 0);

        b.iter(|| {
            let sum: i32 = data
                .par_chunks(chunk_size)
                .map(|chunk| chunk.iter().sum::<i32>())
                .sum();
            black_box(sum);
        })
    });

    // Benchmark progress tracking with actual work
    group.bench_function("progress_with_work", |b| {
        b.iter(|| {
            let pb = create_progress_bar(100, "Processing");
            for i in 0..100 {
                // Simulate work
                let _result = i * 2;
                pb.inc(1);
            }
            pb.finish();
        })
    });

    group.finish();
}

// ===== String Processing Benchmarks =====

fn bench_string_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_operations");

    // Test tree rendering with Unicode
    group.bench_function("unicode_tree_rendering", |b| {
        let tree = TreeNode::new("Root 根节点")
            .add_child(TreeNode::new("Child 子节点 🌳"))
            .add_child(TreeNode::new("Another Ωμέγα"));

        b.iter(|| tree.render())
    });

    // Test output formatting with colors
    group.bench_function("colored_output", |b| {
        b.iter(|| {
            warning("Test warning");
            info("Test info");
            success("Test success");
            error("Test error");
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_chunk_size_calculation,
    bench_parallelization_decision,
    bench_tree_rendering,
    bench_number_formatting,
    bench_progress_bar_operations,
    bench_format_utilities,
    bench_memory_estimation,
    bench_database_references,
    bench_critical_workflows,
    bench_string_operations
);
criterion_main!(benches);
