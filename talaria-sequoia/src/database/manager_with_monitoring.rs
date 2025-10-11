/// Example of how to integrate ThroughputMonitor into DatabaseManager
///
/// This shows how to add performance monitoring to the chunk_database_streaming method

use crate::database::DatabaseManager;
use crate::performance::ThroughputMonitor;
use crate::download::DatabaseSource;
use std::path::Path;
use anyhow::Result;

impl DatabaseManager {
    /// Chunk database with performance monitoring
    pub fn chunk_database_with_monitoring(
        &mut self,
        file_path: &Path,
        source: &DatabaseSource,
    ) -> Result<()> {
        // Create throughput monitor
        let monitor = ThroughputMonitor::new();

        // Get file size for estimation
        let file_size = std::fs::metadata(file_path)?.len();
        tracing::info!("Starting processing of {:.1} GB file", file_size as f64 / 1_073_741_824.0);

        // Call the existing streaming method with monitoring
        self.chunk_database_streaming_with_monitor(file_path, source, monitor.clone())?;

        // Generate and display final report
        let report = monitor.generate_report();
        tracing::info!("\n{}", report.format());

        // Save report to file
        let report_path = Path::new("performance_report.json");
        let json = report.to_json()?;
        std::fs::write(report_path, json)?;
        tracing::info!("Performance report saved to: {}", report_path.display());

        // Check for bottlenecks
        if !report.bottlenecks.is_empty() {
            tracing::info!("\n⚠️  Performance bottlenecks detected:");
            for bottleneck in &report.bottlenecks {
                tracing::info!("  - {:?}", bottleneck);
            }
        }

        // Show recommendations
        if !report.recommendations.is_empty() {
            tracing::info!("\n💡 Performance recommendations:");
            for rec in &report.recommendations {
                tracing::info!("  • {}", rec);
            }
        }

        Ok(())
    }

    /// Internal method that actually does the monitoring
    fn chunk_database_streaming_with_monitor(
        &mut self,
        file_path: &Path,
        source: &DatabaseSource,
        monitor: ThroughputMonitor,
    ) -> Result<()> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};
        use talaria_bio::sequence::Sequence;

        let file = File::open(file_path)?;
        let reader = BufReader::new(file);

        let mut sequences_batch = Vec::with_capacity(1000);
        let mut total_sequences = 0;
        let mut total_bytes = 0;

        for line in reader.lines() {
            let line = line?;

            if line.starts_with('>') {
                // Process previous sequence if exists
                if !sequences_batch.is_empty() && sequences_batch.len() >= 1000 {
                    // Record metrics
                    let batch_bytes: usize = sequences_batch.iter()
                        .map(|s: &Sequence| s.sequence.len())
                        .sum();

                    monitor.record_sequences(sequences_batch.len(), batch_bytes);
                    total_sequences += sequences_batch.len();
                    total_bytes += batch_bytes;

                    // Process the batch
                    self.chunk_sequences_direct(&sequences_batch, source)?;

                    // Record chunks (example - would need actual count)
                    monitor.record_chunks(10);

                    // Update batch size if adaptive
                    monitor.update_batch_size(sequences_batch.len());

                    // Clear batch
                    sequences_batch.clear();

                    // Check for bottlenecks periodically
                    if total_sequences % 10000 == 0 {
                        let bottlenecks = monitor.detect_bottlenecks();
                        if !bottlenecks.is_empty() {
                            tracing::info!("Bottleneck detected: {:?}", bottlenecks[0]);
                        }
                    }
                }
            }
        }

        // Process remaining sequences
        if !sequences_batch.is_empty() {
            monitor.record_sequences(sequences_batch.len(),
                sequences_batch.iter().map(|s| s.sequence.len()).sum());
            self.chunk_sequences_direct(&sequences_batch, source)?;
        }

        Ok(())
    }
}