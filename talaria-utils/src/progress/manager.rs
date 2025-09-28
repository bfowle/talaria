/// Unified progress management system
///
/// This module provides a single, consistent interface for all progress reporting
/// throughout Talaria, replacing the fragmented progress implementations.

use indicatif::{MultiProgress, ProgressBar, ProgressStyle, ProgressDrawTarget};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Operation types that can report progress
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OperationType {
    Download,
    Decompress,
    Process,
    Store,
    Index,
    Verify,
    Custom(String),
}

impl OperationType {
    pub fn display_name(&self) -> &str {
        match self {
            Self::Download => "Downloading",
            Self::Decompress => "Decompressing",
            Self::Process => "Processing",
            Self::Store => "Storing",
            Self::Index => "Indexing",
            Self::Verify => "Verifying",
            Self::Custom(name) => name,
        }
    }
}

/// Progress information for an operation
#[derive(Debug, Clone)]
pub struct ProgressInfo {
    /// Current progress value
    pub current: u64,
    /// Total expected value (0 for indeterminate)
    pub total: u64,
    /// Human-readable message
    pub message: String,
    /// Optional rate information
    pub rate: Option<f64>,
    /// Time operation started
    pub started: Instant,
}

impl ProgressInfo {
    pub fn new(total: u64, message: impl Into<String>) -> Self {
        Self {
            current: 0,
            total,
            message: message.into(),
            rate: None,
            started: Instant::now(),
        }
    }

    /// Calculate percentage complete
    pub fn percentage(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.current as f64 / self.total as f64) * 100.0
        }
    }

    /// Get elapsed time
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Estimate time remaining
    pub fn eta(&self) -> Option<Duration> {
        if self.current == 0 || self.total == 0 {
            return None;
        }

        let elapsed = self.elapsed().as_secs_f64();
        let rate = self.current as f64 / elapsed;
        if rate == 0.0 {
            return None;
        }

        let remaining = (self.total - self.current) as f64;
        let eta_secs = remaining / rate;

        // Don't show insane ETAs
        if eta_secs > 86400.0 * 365.0 {
            None
        } else {
            Some(Duration::from_secs_f64(eta_secs))
        }
    }
}

/// Unified progress manager for all operations
pub struct ProgressManager {
    /// Multi-progress container for all bars
    multi: Arc<MultiProgress>,
    /// Active progress bars by operation type
    bars: Arc<Mutex<HashMap<OperationType, ProgressBar>>>,
    /// Progress information by operation type
    info: Arc<Mutex<HashMap<OperationType, ProgressInfo>>>,
    /// Whether to show progress (false for quiet mode)
    visible: bool,
    /// Main operation bar (optional)
    main_bar: Option<Arc<Mutex<ProgressBar>>>,
}

impl ProgressManager {
    /// Create a new progress manager
    pub fn new(visible: bool) -> Self {
        let multi = if visible {
            MultiProgress::new()
        } else {
            MultiProgress::with_draw_target(ProgressDrawTarget::hidden())
        };

        Self {
            multi: Arc::new(multi),
            bars: Arc::new(Mutex::new(HashMap::new())),
            info: Arc::new(Mutex::new(HashMap::new())),
            visible,
            main_bar: None,
        }
    }

    /// Create a progress manager with a main operation bar
    pub fn with_main_operation(operation: impl Into<String>, visible: bool) -> Self {
        let mut manager = Self::new(visible);

        if visible {
            let main_bar = ProgressBar::new(100);
            main_bar.set_style(
                ProgressStyle::default_bar()
                    .template("{msg}\n[{elapsed_precise}] [{bar:50.cyan/blue}] {pos}% {eta_precise}")
                    .unwrap()
                    .progress_chars("━━─")
            );
            main_bar.set_message(operation.into());

            let main_bar = manager.multi.add(main_bar);
            manager.main_bar = Some(Arc::new(Mutex::new(main_bar)));
        }

        manager
    }

    /// Start tracking a new operation
    pub fn start_operation(&self, op_type: OperationType, total: u64, message: impl Into<String>) {
        if !self.visible {
            return;
        }

        let info = ProgressInfo::new(total, message.into());

        // Create appropriate progress bar based on whether total is known
        let bar = if total > 0 {
            let bar = ProgressBar::new(total);
            bar.set_style(
                ProgressStyle::default_bar()
                    .template("  {msg} [{bar:40.cyan/blue}] {human_pos}/{human_len} ({eta_precise})")
                    .unwrap()
                    .progress_chars("━━─")
                    .with_key("human_pos", |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
                        use crate::display::output::format_number;
                        write!(w, "{:>9}", format_number(state.pos() as usize)).unwrap()
                    })
                    .with_key("human_len", |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
                        use crate::display::output::format_number;
                        write!(w, "{}", format_number(state.len().unwrap_or(0) as usize)).unwrap()
                    })
                    .with_key("eta_precise", |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
                        if state.pos() == 0 {
                            write!(w, "calculating").unwrap()
                        } else {
                            let eta = state.eta();
                            let secs = eta.as_secs();
                            if secs < 60 {
                                write!(w, "{}s", secs).unwrap()
                            } else if secs < 3600 {
                                write!(w, "{}m", secs / 60).unwrap()
                            } else if secs < 86400 {
                                write!(w, "{}h {}m", secs / 3600, (secs % 3600) / 60).unwrap()
                            } else if secs < 31536000 {
                                write!(w, "{}d", secs / 86400).unwrap()
                            } else {
                                write!(w, "-").unwrap()
                            }
                        }
                    })
            );
            bar
        } else {
            // Spinner for indeterminate progress
            let bar = ProgressBar::new_spinner();
            bar.set_style(
                ProgressStyle::default_spinner()
                    .template("  {spinner:.cyan} {msg}")
                    .unwrap()
            );
            bar
        };

        bar.set_message(info.message.clone());
        let bar = self.multi.add(bar);

        // Store bar and info
        self.bars.lock().unwrap().insert(op_type.clone(), bar.clone());
        self.info.lock().unwrap().insert(op_type, info);

        // Manually tick to show immediately
        bar.tick();
    }

    /// Update progress for an operation
    pub fn update_progress(&self, op_type: &OperationType, current: u64) {
        if !self.visible {
            return;
        }

        // Update info
        if let Some(info) = self.info.lock().unwrap().get_mut(op_type) {
            info.current = current;
        }

        // Update bar
        if let Some(bar) = self.bars.lock().unwrap().get(op_type) {
            bar.set_position(current);
        }

        // Update main bar if present
        self.update_main_bar();
    }

    /// Update message for an operation
    pub fn update_message(&self, op_type: &OperationType, message: impl Into<String>) {
        if !self.visible {
            return;
        }

        let message = message.into();

        // Update info
        if let Some(info) = self.info.lock().unwrap().get_mut(op_type) {
            info.message = message.clone();
        }

        // Update bar
        if let Some(bar) = self.bars.lock().unwrap().get(op_type) {
            bar.set_message(message);
        }
    }

    /// Increment progress for an operation
    pub fn increment(&self, op_type: &OperationType, delta: u64) {
        if !self.visible {
            return;
        }

        // Get current value
        let current = self.info.lock().unwrap()
            .get(op_type)
            .map(|info| info.current)
            .unwrap_or(0);

        self.update_progress(op_type, current + delta);
    }

    /// Finish an operation
    pub fn finish_operation(&self, op_type: &OperationType, message: Option<String>) {
        if !self.visible {
            return;
        }

        // Remove from tracking
        self.info.lock().unwrap().remove(op_type);

        // Finish and remove bar
        if let Some(bar) = self.bars.lock().unwrap().remove(op_type) {
            if let Some(msg) = message {
                bar.finish_with_message(msg);
            } else {
                bar.finish_with_message(format!("✓ {} complete", op_type.display_name()));
            }
        }

        // Update main bar
        self.update_main_bar();
    }

    /// Update main progress bar based on sub-operations
    fn update_main_bar(&self) {
        if let Some(main_bar) = &self.main_bar {
            let info = self.info.lock().unwrap();
            if info.is_empty() {
                return;
            }

            // Calculate overall progress as average of all operations
            let mut total_percentage = 0.0;
            let mut count = 0;

            for progress_info in info.values() {
                total_percentage += progress_info.percentage();
                count += 1;
            }

            if count > 0 {
                let avg_percentage = total_percentage / count as f64;
                main_bar.lock().unwrap().set_position(avg_percentage as u64);
            }
        }
    }

    /// Tick a spinner operation
    pub fn tick(&self, op_type: &OperationType) {
        if !self.visible {
            return;
        }

        if let Some(bar) = self.bars.lock().unwrap().get(op_type) {
            bar.tick();
        }
    }

    /// Clear all progress bars
    pub fn clear(&self) {
        if !self.visible {
            return;
        }

        // Finish all operations
        let ops: Vec<_> = self.bars.lock().unwrap().keys().cloned().collect();
        for op in ops {
            self.finish_operation(&op, None);
        }

        // Clear multi-progress
        let _ = self.multi.clear();
    }

    /// Check if any operations are active
    pub fn has_active_operations(&self) -> bool {
        !self.info.lock().unwrap().is_empty()
    }

    /// Get a snapshot of all operation progress
    pub fn get_progress_snapshot(&self) -> HashMap<OperationType, ProgressInfo> {
        self.info.lock().unwrap().clone()
    }
}

/// Convenience builder for progress manager
pub struct ProgressManagerBuilder {
    main_operation: Option<String>,
    visible: bool,
}

impl ProgressManagerBuilder {
    pub fn new() -> Self {
        Self {
            main_operation: None,
            visible: true,
        }
    }

    pub fn with_main_operation(mut self, operation: impl Into<String>) -> Self {
        self.main_operation = Some(operation.into());
        self
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn build(self) -> ProgressManager {
        if let Some(main_op) = self.main_operation {
            ProgressManager::with_main_operation(main_op, self.visible)
        } else {
            ProgressManager::new(self.visible)
        }
    }
}

impl Default for ProgressManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_info_percentage() {
        let mut info = ProgressInfo::new(100, "Test");
        assert_eq!(info.percentage(), 0.0);

        info.current = 50;
        assert_eq!(info.percentage(), 50.0);

        info.current = 100;
        assert_eq!(info.percentage(), 100.0);
    }

    #[test]
    fn test_progress_info_eta() {
        let mut info = ProgressInfo::new(100, "Test");
        assert!(info.eta().is_none());

        // Simulate some progress
        info.current = 10;
        // Force a time difference
        info.started = Instant::now() - Duration::from_secs(10);

        let eta = info.eta();
        assert!(eta.is_some());
        // Should estimate ~90 seconds remaining (90 items at 1 item/sec)
    }

    #[test]
    fn test_progress_manager_operations() {
        let manager = ProgressManager::new(false); // Hidden for tests

        // Start an operation
        manager.start_operation(OperationType::Download, 100, "Downloading");
        assert!(manager.has_active_operations());

        // Update progress
        manager.update_progress(&OperationType::Download, 50);

        // Get snapshot
        let snapshot = manager.get_progress_snapshot();
        assert_eq!(snapshot.get(&OperationType::Download).unwrap().current, 50);

        // Finish operation
        manager.finish_operation(&OperationType::Download, None);
        assert!(!manager.has_active_operations());
    }

    #[test]
    fn test_builder() {
        let manager = ProgressManagerBuilder::new()
            .with_main_operation("Test Operation")
            .visible(true)
            .build();

        assert!(manager.main_bar.is_some());
    }
}