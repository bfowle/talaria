use anyhow::Result;
/// Unified progress tracking system for download and processing operations
///
/// This module provides a clean, non-overlapping progress display that shows
/// meaningful information at each stage of the operation.
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

/// Different stages of a download and processing operation
#[derive(Debug, Clone, PartialEq)]
pub enum OperationStage {
    /// Discovering existing downloads
    Discovery,
    /// Downloading the file
    Download {
        bytes_current: u64,
        bytes_total: u64,
    },
    /// Processing/chunking sequences
    Processing {
        sequences_processed: usize,
        sequences_total: Option<usize>,
        batches_processed: usize,
    },
    /// Storing sequences
    Storing {
        sequences_stored: usize,
        sequences_new: usize,
        sequences_dedup: usize,
    },
    /// Building indices
    IndexBuilding,
    /// Creating manifest
    ManifestCreation,
    /// Finalizing
    Finalization,
    /// Completed
    Complete { total_time: Duration },
}

/// Unified progress tracker for database operations
pub struct UnifiedProgressTracker {
    multi: Arc<MultiProgress>,
    main_bar: Arc<Mutex<ProgressBar>>,
    detail_bar: Arc<Mutex<Option<ProgressBar>>>,
    stage: Arc<Mutex<OperationStage>>,
    start_time: std::time::Instant,
    silent: bool,
}

impl UnifiedProgressTracker {
    /// Create a new progress tracker
    pub fn new(operation_name: &str, silent: bool) -> Self {
        if silent {
            // Create hidden progress bars for silent mode (no visual output)
            let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
            let main_bar = ProgressBar::hidden();

            return Self {
                multi: Arc::new(multi),
                main_bar: Arc::new(Mutex::new(main_bar)),
                detail_bar: Arc::new(Mutex::new(None)),
                stage: Arc::new(Mutex::new(OperationStage::Discovery)),
                start_time: std::time::Instant::now(),
                silent,
            };
        }

        let multi = MultiProgress::new();

        // Main progress bar showing overall operation
        let main_bar = ProgressBar::new(100);
        main_bar.set_style(
            ProgressStyle::default_bar()
                .template("╭─ {msg}\n╰─ [{elapsed_precise}] [{bar:50.cyan/blue}] {pos}% ETA: {eta}")
                .unwrap()
                .progress_chars("█▓▒░ "),
        );
        main_bar.set_message(format!("🔄 {}", operation_name));

        let main_bar = multi.add(main_bar);
        // Don't use steady_tick - causes ETA miscalculation

        Self {
            multi: Arc::new(multi),
            main_bar: Arc::new(Mutex::new(main_bar)),
            detail_bar: Arc::new(Mutex::new(None)),
            stage: Arc::new(Mutex::new(OperationStage::Discovery)),
            start_time: std::time::Instant::now(),
            silent,
        }
    }

    /// Update the current stage
    pub fn set_stage(&self, new_stage: OperationStage) -> Result<()> {
        if self.silent {
            return Ok(());
        }

        let mut stage = self.stage.lock();
        *stage = new_stage.clone();

        let main_bar = self.main_bar.lock();

        // Clear any existing detail bar
        if let Some(bar) = self.detail_bar.lock().take() {
            bar.finish_and_clear();
        }

        match &new_stage {
            OperationStage::Discovery => {
                main_bar.set_message("🔍 Checking for existing downloads...");
                main_bar.set_position(5);
            }

            OperationStage::Download {
                bytes_current,
                bytes_total,
            } => {
                let percent = if *bytes_total > 0 {
                    (*bytes_current as f64 / *bytes_total as f64 * 30.0) as u64 + 5
                } else {
                    5
                };
                main_bar.set_position(percent);

                // Create detail bar for download progress
                let detail = ProgressBar::new(*bytes_total);
                detail.set_style(
                    ProgressStyle::default_bar()
                        .template("     📥 Downloading: [{bar:40.green/white}] {bytes}/{total_bytes} ({bytes_per_sec}) {msg}")
                        .unwrap()
                        .progress_chars("═╾─")
                );
                detail.set_position(*bytes_current);

                let detail = self.multi.add(detail);
                // Don't use steady_tick - causes ETA miscalculation
                *self.detail_bar.lock() = Some(detail);

                main_bar.set_message("📥 Downloading database...");
            }

            OperationStage::Processing {
                sequences_processed,
                sequences_total,
                batches_processed,
            } => {
                let percent = if let Some(total) = sequences_total {
                    if *total > 0 {
                        (*sequences_processed as f64 / *total as f64 * 40.0) as u64 + 35
                    } else {
                        35
                    }
                } else {
                    // Estimate based on batches (assuming ~100 batches typical)
                    ((*batches_processed as f64 / 100.0).min(1.0) * 40.0) as u64 + 35
                };
                main_bar.set_position(percent);

                // Create detail bar for processing
                let detail = if let Some(total) = sequences_total {
                    let bar = ProgressBar::new(*total as u64);
                    bar.set_style(
                        ProgressStyle::default_bar()
                            .template("     🧬 Processing: [{bar:40.yellow/white}] {pos}/{len} sequences (batch {msg})")
                            .unwrap()
                            .progress_chars("▰▱ ")
                    );
                    bar.set_position(*sequences_processed as u64);
                    bar.set_message(format!("{}", batches_processed));
                    bar
                } else {
                    let bar = ProgressBar::new_spinner();
                    bar.set_style(
                        ProgressStyle::default_spinner()
                            .template("     🧬 Processing: {spinner:.yellow} {pos} sequences (batch {msg})")
                            .unwrap()
                    );
                    bar.set_position(*sequences_processed as u64);
                    bar.set_message(format!("{}", batches_processed));
                    bar
                };

                let detail = self.multi.add(detail);
                // Don't use steady_tick - causes ETA miscalculation
                *self.detail_bar.lock() = Some(detail);

                main_bar.set_message("🧬 Processing sequences...");
            }

            OperationStage::Storing {
                sequences_stored,
                sequences_new,
                sequences_dedup,
            } => {
                main_bar.set_position(80);

                let detail = ProgressBar::new_spinner();
                detail.set_style(
                    ProgressStyle::default_spinner()
                        .template("     💾 Storing: {spinner:.cyan} {msg}")
                        .unwrap(),
                );
                detail.set_message(format!(
                    "{} total ({} new, {} deduplicated)",
                    format_number(*sequences_stored),
                    format_number(*sequences_new),
                    format_number(*sequences_dedup)
                ));

                let detail = self.multi.add(detail);
                // Don't use steady_tick - causes ETA miscalculation
                *self.detail_bar.lock() = Some(detail);

                main_bar.set_message("💾 Storing canonical sequences...");
            }

            OperationStage::IndexBuilding => {
                main_bar.set_position(85);
                main_bar.set_message("🔨 Building indices...");
            }

            OperationStage::ManifestCreation => {
                main_bar.set_position(90);
                main_bar.set_message("📋 Creating manifest...");
            }

            OperationStage::Finalization => {
                main_bar.set_position(95);
                main_bar.set_message("🏁 Finalizing...");
            }

            OperationStage::Complete { total_time } => {
                main_bar.set_position(100);
                main_bar.finish_with_message(format!(
                    "✅ Complete in {}",
                    format_duration(*total_time)
                ));

                if let Some(bar) = self.detail_bar.lock().take() {
                    bar.finish_and_clear();
                }
            }
        }

        Ok(())
    }

    /// Update download progress
    pub fn update_download(&self, bytes_current: u64, bytes_total: u64) -> Result<()> {
        if self.silent {
            return Ok(());
        }

        self.set_stage(OperationStage::Download {
            bytes_current,
            bytes_total,
        })?;

        if let Some(ref bar) = *self.detail_bar.lock() {
            bar.set_position(bytes_current);

            // Add helpful context
            if bytes_total > 0 {
                let percent = (bytes_current as f64 / bytes_total as f64 * 100.0) as u32;
                bar.set_message(format!("{}%", percent));
            }
        }

        Ok(())
    }

    /// Update processing progress
    pub fn update_processing(
        &self,
        sequences_processed: usize,
        sequences_total: Option<usize>,
        batches_processed: usize,
    ) -> Result<()> {
        if self.silent {
            return Ok(());
        }

        self.set_stage(OperationStage::Processing {
            sequences_processed,
            sequences_total,
            batches_processed,
        })?;

        Ok(())
    }

    /// Update storing progress
    pub fn update_storing(
        &self,
        sequences_stored: usize,
        sequences_new: usize,
        sequences_dedup: usize,
    ) -> Result<()> {
        if self.silent {
            return Ok(());
        }

        self.set_stage(OperationStage::Storing {
            sequences_stored,
            sequences_new,
            sequences_dedup,
        })?;

        Ok(())
    }

    /// Print a status message (appears above progress bars)
    pub fn print_status(&self, message: &str) {
        if self.silent {
            return;
        }

        // Use the multi-progress suspend feature to print without disrupting bars
        self.multi.suspend(|| {
            println!("  {}", message);
        });
    }

    /// Complete the operation
    pub fn complete(&self) -> Result<()> {
        let total_time = self.start_time.elapsed();
        self.set_stage(OperationStage::Complete { total_time })?;
        Ok(())
    }

    /// Finish with an error
    pub fn error(&self, err: &str) {
        if self.silent {
            return;
        }

        let main_bar = self.main_bar.lock();
        main_bar.abandon_with_message(format!("❌ Error: {}", err));

        if let Some(bar) = self.detail_bar.lock().take() {
            bar.abandon();
        }
    }
}

impl Drop for UnifiedProgressTracker {
    fn drop(&mut self) {
        // Clean up any remaining bars
        {
            let main_bar = self.main_bar.lock();
            if !main_bar.is_finished() {
                main_bar.abandon();
            }
        }

        {
            let detail_opt = self.detail_bar.lock();
            if let Some(ref bar) = *detail_opt {
                if !bar.is_finished() {
                    bar.abandon();
                }
            }
        }
    }
}

/// Helper to format numbers with commas
fn format_number(n: usize) -> String {
    use talaria_utils::display::output::format_number;
    format_number(n)
}

/// Helper to format duration
fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m {}s", secs / 3600, (secs % 3600) / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_progress_stages() {
        let tracker = UnifiedProgressTracker::new("Test Operation", true); // silent mode for tests

        // Test stage transitions
        assert!(tracker.set_stage(OperationStage::Discovery).is_ok());
        assert!(tracker
            .set_stage(OperationStage::Download {
                bytes_current: 100,
                bytes_total: 1000
            })
            .is_ok());
        assert!(tracker.update_download(500, 1000).is_ok());
        assert!(tracker
            .set_stage(OperationStage::Processing {
                sequences_processed: 50,
                sequences_total: Some(100),
                batches_processed: 1,
            })
            .is_ok());
        assert!(tracker.complete().is_ok());
    }

    #[test]
    fn test_concurrent_updates() {
        let tracker = Arc::new(UnifiedProgressTracker::new("Concurrent Test", true));

        let t1 = tracker.clone();
        let h1 = thread::spawn(move || {
            for i in 0..10 {
                t1.update_download(i * 100, 1000).unwrap();
                thread::sleep(Duration::from_millis(10));
            }
        });

        let t2 = tracker.clone();
        let h2 = thread::spawn(move || {
            for i in 0..5 {
                t2.print_status(&format!("Status update {}", i));
                thread::sleep(Duration::from_millis(20));
            }
        });

        h1.join().unwrap();
        h2.join().unwrap();

        assert!(tracker.complete().is_ok());
    }
}
