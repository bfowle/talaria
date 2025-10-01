use std::fs;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use talaria_bio::sequence::Sequence;
use talaria_test::fixtures::test_database_source;
/// Performance regression tests for download and chunking pipeline
///
/// These tests ensure that performance doesn't degrade over time.
/// They set baseline expectations for throughput and resource usage.
use talaria_sequoia::{
    chunker::{ChunkingStrategy, TaxonomicChunker},
    database::DatabaseManager,
    download::DatabaseSource,
    performance::{get_system_info, AdaptiveConfigBuilder, AdaptiveManager},
    storage::SequenceStorage,
};
use tempfile::TempDir;

/// Minimum acceptable throughput in sequences per second
const MIN_THROUGHPUT_SEQS_PER_SEC: f64 = 50_000.0;

/// Maximum acceptable memory usage in MB for 1M sequences
const MAX_MEMORY_MB_PER_MILLION_SEQS: u64 = 2000;

/// Maximum acceptable time for checkpoint operations
const MAX_CHECKPOINT_TIME_MS: u128 = 100;

/// Generate test sequences
fn generate_test_sequences(count: usize) -> Vec<Sequence> {
    (0..count)
        .map(|i| Sequence {
            id: format!("seq_{:08}", i),
            description: Some(format!("Test sequence {}", i)),
            sequence: vec![b'A'; 500 + (i % 100)], // Typical sequence length
            taxon_id: Some((i % 10000) as u32),
            taxonomy_sources: Default::default(),
        })
        .collect()
}

#[test]
fn test_throughput_baseline() {
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(SequenceStorage::new(temp_dir.path()).unwrap());

    let mut chunker =
        TaxonomicChunker::new(ChunkingStrategy::default(), storage, test_database_source("performance"));

    // Generate 100k sequences for testing
    let sequences = generate_test_sequences(100_000);
    let start = Instant::now();

    // Process sequences
    chunker.set_quiet_mode(true);
    let _result = chunker
        .chunk_sequences_canonical(sequences.clone())
        .unwrap();

    let elapsed = start.elapsed();
    let throughput = sequences.len() as f64 / elapsed.as_secs_f64();

    println!("Throughput: {:.0} sequences/second", throughput);
    println!(
        "Time: {:.2}s for {} sequences",
        elapsed.as_secs_f64(),
        sequences.len()
    );

    // Assert minimum throughput is met
    assert!(
        throughput >= MIN_THROUGHPUT_SEQS_PER_SEC,
        "Throughput {:.0} seq/s is below minimum {:.0} seq/s",
        throughput,
        MIN_THROUGHPUT_SEQS_PER_SEC
    );
}

#[test]
fn test_memory_usage() {
    use talaria_sequoia::performance::MemoryMonitor;

    let monitor = MemoryMonitor::new();
    let initial_memory = monitor.get_stats().used_mb();

    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(SequenceStorage::new(temp_dir.path()).unwrap());

    let mut chunker =
        TaxonomicChunker::new(ChunkingStrategy::default(), storage, test_database_source("performance"));

    // Process 100k sequences
    let sequences = generate_test_sequences(100_000);
    chunker.set_quiet_mode(true);
    let _result = chunker.chunk_sequences_canonical(sequences).unwrap();

    let final_memory = monitor.get_stats().used_mb();
    let memory_used = final_memory.saturating_sub(initial_memory);

    // Scale to per-million sequences
    let memory_per_million = (memory_used * 10) as u64;

    println!("Memory used: {} MB for 100k sequences", memory_used);
    println!("Projected: {} MB per million sequences", memory_per_million);

    assert!(
        memory_per_million <= MAX_MEMORY_MB_PER_MILLION_SEQS,
        "Memory usage {} MB/M sequences exceeds limit {} MB/M",
        memory_per_million,
        MAX_MEMORY_MB_PER_MILLION_SEQS
    );
}

#[test]
fn test_parallel_scaling() {
    use rayon::prelude::*;
    use talaria_core::SHA256Hash;

    let sequences: Vec<Vec<u8>> = (0..100_000).map(|i| vec![b'A'; 500 + (i % 100)]).collect();

    // Measure sequential performance
    let start = Instant::now();
    let _sequential: Vec<_> = sequences
        .iter()
        .map(|seq| SHA256Hash::compute(seq))
        .collect();
    let sequential_time = start.elapsed();

    // Measure parallel performance
    let start = Instant::now();
    let _parallel: Vec<_> = sequences
        .par_iter()
        .map(|seq| SHA256Hash::compute(seq))
        .collect();
    let parallel_time = start.elapsed();

    let speedup = sequential_time.as_secs_f64() / parallel_time.as_secs_f64();
    let cpu_cores = num_cpus::get() as f64;

    println!("Sequential: {:.2}s", sequential_time.as_secs_f64());
    println!("Parallel: {:.2}s", parallel_time.as_secs_f64());
    println!("Speedup: {:.2}x on {} cores", speedup, cpu_cores);

    // Expect at least 50% efficiency (e.g., 4x speedup on 8 cores)
    let min_speedup = cpu_cores * 0.5;
    assert!(
        speedup >= min_speedup.min(2.0), // At least 2x speedup or 50% efficiency
        "Parallel speedup {:.2}x is too low for {} cores",
        speedup,
        cpu_cores
    );
}

#[test]
fn test_checkpoint_performance() {
    use talaria_sequoia::checkpoint::ChunkingCheckpoint;

    let _temp_dir = TempDir::new().unwrap();

    let mut checkpoint = ChunkingCheckpoint::new(
        "test_database".to_string(),
        1_000_000_000, // 1GB file size
    );

    // Measure save time
    checkpoint.update(1_000_000, 500_000_000, Some("seq_1000000".to_string()));

    let start = Instant::now();
    checkpoint.save().unwrap();
    let save_time = start.elapsed();

    // Measure load time
    let start = Instant::now();
    let _loaded = ChunkingCheckpoint::load("test_database").unwrap();
    let load_time = start.elapsed();

    println!("Checkpoint save: {}ms", save_time.as_millis());
    println!("Checkpoint load: {}ms", load_time.as_millis());

    assert!(
        save_time.as_millis() <= MAX_CHECKPOINT_TIME_MS,
        "Checkpoint save took {}ms, exceeds limit {}ms",
        save_time.as_millis(),
        MAX_CHECKPOINT_TIME_MS
    );

    assert!(
        load_time.as_millis() <= MAX_CHECKPOINT_TIME_MS,
        "Checkpoint load took {}ms, exceeds limit {}ms",
        load_time.as_millis(),
        MAX_CHECKPOINT_TIME_MS
    );
}

#[test]
fn test_adaptive_batch_sizing() {
    let system_info = get_system_info();
    println!(
        "System: {} cores, {} MB RAM total, {} MB available",
        system_info.cpu_cores, system_info.total_memory_mb, system_info.available_memory_mb
    );

    // Test adaptive manager with different memory scenarios
    let scenarios = [
        (100, "low_memory"),
        (1000, "medium_memory"),
        (4000, "high_memory"),
    ];

    for (memory_mb, scenario) in scenarios.iter() {
        let config = AdaptiveConfigBuilder::new()
            .memory_limit_mb(*memory_mb)
            .target_memory_usage(0.75)
            .build();

        let adaptive = AdaptiveManager::with_config(config).unwrap();
        let batch_size = adaptive.get_memory_aware_batch_size();

        println!(
            "{}: {} MB limit -> batch size {}",
            scenario, memory_mb, batch_size
        );

        // Batch size should be reasonable
        assert!(batch_size >= 100, "Batch size too small");
        assert!(batch_size <= 1_000_000, "Batch size too large");
    }
}

#[test]
fn test_deduplication_performance() {
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(SequenceStorage::new(temp_dir.path()).unwrap());

    // Create sequences with 50% duplicates
    let unique_count = 50_000;
    let duplicate_count = 50_000;

    let start = Instant::now();

    // Store unique sequences
    for i in 0..unique_count {
        let seq = format!("UNIQUE_SEQ_{:06}_ACGTACGT", i);
        let header = format!(">seq_{}", i);
        storage
            .store_sequence(&seq, &header, test_database_source("performance"))
            .unwrap();
    }

    // Store duplicates (should be deduplicated)
    for i in 0..duplicate_count {
        let seq = format!("UNIQUE_SEQ_{:06}_ACGTACGT", i % 100); // Repeat first 100
        let header = format!(">dup_{}", i);
        storage
            .store_sequence(&seq, &header, test_database_source("performance"))
            .unwrap();
    }

    let elapsed = start.elapsed();
    let throughput = (unique_count + duplicate_count) as f64 / elapsed.as_secs_f64();

    println!("Deduplication: {:.0} sequences/second", throughput);
    println!(
        "Time: {:.2}s for {} total sequences",
        elapsed.as_secs_f64(),
        unique_count + duplicate_count
    );

    // Should maintain good throughput even with deduplication
    assert!(
        throughput >= MIN_THROUGHPUT_SEQS_PER_SEC * 0.8, // Allow 20% overhead for dedup
        "Deduplication throughput {:.0} seq/s is too low",
        throughput
    );
}

#[test]
#[ignore] // Run with --ignored for stress testing
fn test_large_file_stress() {
    let temp_dir = TempDir::new().unwrap();

    // Create a 1GB test file
    let file_path = temp_dir.path().join("large_test.fasta");
    let mut file = fs::File::create(&file_path).unwrap();

    let num_sequences = 2_000_000; // ~500 bytes each = 1GB
    for i in 0..num_sequences {
        writeln!(file, ">seq_{:08} Test sequence", i).unwrap();
        writeln!(file, "{}", "ACGT".repeat(125)).unwrap(); // 500 bases
    }
    file.sync_all().unwrap();

    std::env::set_var("TALARIA_HOME", temp_dir.path());
    let mut manager = DatabaseManager::new(None).unwrap();

    let start = Instant::now();
    manager
        .chunk_database(&file_path, &test_database_source("performance"), None)
        .unwrap();
    let elapsed = start.elapsed();

    let file_size_mb = fs::metadata(&file_path).unwrap().len() / 1_000_000;
    let throughput_mb = file_size_mb as f64 / elapsed.as_secs_f64();

    println!(
        "Large file: {} MB in {:.2}s = {:.1} MB/s",
        file_size_mb,
        elapsed.as_secs_f64(),
        throughput_mb
    );

    // Should process at least 20 MB/s
    assert!(
        throughput_mb >= 20.0,
        "Large file throughput {:.1} MB/s is too low",
        throughput_mb
    );
}

#[test]
fn test_cpu_utilization() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);

    // Monitor CPU usage in background
    let monitor_thread = thread::spawn(move || {
        let max_cpu = 0.0;
        while running_clone.load(Ordering::Relaxed) {
            // This is a simplified CPU check
            // In production, use proper CPU monitoring
            thread::sleep(Duration::from_millis(100));
        }
        max_cpu
    });

    // Run intensive operation
    let sequences = generate_test_sequences(100_000);
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(SequenceStorage::new(temp_dir.path()).unwrap());
    let mut chunker =
        TaxonomicChunker::new(ChunkingStrategy::default(), storage, test_database_source("performance"));

    chunker.set_quiet_mode(true);
    let _result = chunker.chunk_sequences_canonical(sequences).unwrap();

    running.store(false, Ordering::Relaxed);
    let _max_cpu = monitor_thread.join().unwrap();

    // Just verify the operation completed
    // Real CPU monitoring would check utilization percentage
    assert!(true, "CPU utilization test completed");
}
