//! Progress bar and spinner utilities

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::time::Duration;

/// Create a standard progress bar with consistent styling
pub fn create_progress_bar(total: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {pos:>7}/{len:7} ({eta})")
            .unwrap()
            .progress_chars("━━─"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Create a spinner with consistent styling
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Manager for multiple progress bars
pub struct ProgressBarManager {
    multi: MultiProgress,
}

impl ProgressBarManager {
    /// Create a new progress bar manager
    pub fn new() -> Self {
        Self {
            multi: MultiProgress::new(),
        }
    }

    /// Add a progress bar to the manager
    pub fn add(&self, pb: ProgressBar) -> ProgressBar {
        self.multi.add(pb)
    }

    /// Create and add a progress bar
    pub fn create_progress_bar(&self, total: u64, message: &str) -> ProgressBar {
        let pb = create_progress_bar(total, message);
        self.add(pb)
    }

    /// Create and add a spinner
    pub fn create_spinner(&self, message: &str) -> ProgressBar {
        let pb = create_spinner(message);
        self.add(pb)
    }

    /// Clear all progress bars
    pub fn clear(&self) -> Result<(), std::io::Error> {
        self.multi.clear()
    }
}

impl Default for ProgressBarManager {
    fn default() -> Self {
        Self::new()
    }
}