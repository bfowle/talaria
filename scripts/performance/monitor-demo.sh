#!/bin/bash

# Demo script showing how to use throughput monitoring with Talaria

cat << 'EOF' > /tmp/monitor_demo.rs
use talaria_sequoia::performance::{ThroughputMonitor, Bottleneck};
use std::thread;
use std::time::Duration;

fn main() {
    println!("Talaria Throughput Monitor Demo\n");

    let monitor = ThroughputMonitor::new();

    println!("Simulating sequence processing...\n");

    // Simulate processing batches
    for batch in 1..=10 {
        // Simulate processing time
        thread::sleep(Duration::from_millis(100));

        // Record sequences processed (1000 sequences, ~500KB each batch)
        monitor.record_sequences(1000, 500_000);
        monitor.record_chunks(5);
        monitor.update_batch_size(1000);

        // Get current throughput
        let (seq_per_sec, mb_per_sec) = monitor.current_throughput();

        println!("Batch {}: {:.0} seq/s, {:.1} MB/s",
                 batch, seq_per_sec, mb_per_sec);

        // Check for bottlenecks
        let bottlenecks = monitor.detect_bottlenecks();
        for bottleneck in bottlenecks {
            match bottleneck {
                Bottleneck::CpuBound { utilization } =>
                    println!("  ⚠️ CPU bound: {:.0}% utilization", utilization * 100.0),
                Bottleneck::MemoryBound { available_mb, .. } =>
                    println!("  ⚠️ Memory pressure: {} MB available", available_mb),
                Bottleneck::IoBound { read_mb_sec, .. } =>
                    println!("  ⚠️ I/O bound: {:.1} MB/s", read_mb_sec),
                Bottleneck::SingleThreaded { cpu_cores, utilized } =>
                    println!("  ⚠️ Using only {} of {} cores", utilized, cpu_cores),
                _ => {}
            }
        }
    }

    // Generate final report
    println!("\n{}", "=".repeat(60));
    let report = monitor.generate_report();
    println!("{}", report.format());

    // Save reports
    if let Ok(json) = report.to_json() {
        std::fs::write("performance_report.json", json).ok();
        println!("\n✓ JSON report saved to performance_report.json");
    }

    let csv = report.metrics_to_csv();
    std::fs::write("performance_metrics.csv", csv).ok();
    println!("✓ CSV metrics saved to performance_metrics.csv");
}
EOF

# Compile and run
echo "Compiling monitor demo..."
rustc /tmp/monitor_demo.rs \
    -L target/release/deps \
    --extern talaria_sequoia=target/release/libtalaria_sequoia.rlib \
    --extern anyhow=target/release/deps/libanyhow*.rlib \
    -o /tmp/monitor_demo 2>/dev/null

if [ -f /tmp/monitor_demo ]; then
    echo "Running demo..."
    /tmp/monitor_demo
else
    echo "Note: To run this demo, first build Talaria with: cargo build --release"
fi