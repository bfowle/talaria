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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_progress_bar() {
        let pb = create_progress_bar(100, "Test Progress");

        // Verify initial state
        assert_eq!(pb.length(), Some(100));
        assert_eq!(pb.position(), 0);

        // Test progression
        pb.inc(10);
        assert_eq!(pb.position(), 10);

        pb.set_position(50);
        assert_eq!(pb.position(), 50);

        pb.finish();
        assert!(pb.is_finished());
    }

    #[test]
    fn test_create_spinner() {
        let spinner = create_spinner("Test Spinner");

        // Verify it's a spinner (no length)
        assert_eq!(spinner.length(), None);

        // Test ticking
        spinner.tick();

        // Test message update
        spinner.set_message("Updated Message".to_string());

        spinner.finish_with_message("Done".to_string());
        assert!(spinner.is_finished());
    }

    #[test]
    fn test_progress_bar_manager_creation() {
        let manager = ProgressBarManager::new();
        // Just verify creation doesn't panic
        let _ = manager;
    }

    #[test]
    fn test_progress_bar_manager_default() {
        let manager = ProgressBarManager::default();
        // Verify default trait implementation
        let _ = manager;
    }

    #[test]
    fn test_progress_bar_manager_add() {
        let manager = ProgressBarManager::new();
        let pb = ProgressBar::new(100);

        let managed_pb = manager.add(pb);

        // Verify the returned progress bar works
        managed_pb.inc(1);
        assert_eq!(managed_pb.position(), 1);
    }

    #[test]
    fn test_progress_bar_manager_create_progress_bar() {
        let manager = ProgressBarManager::new();
        let pb = manager.create_progress_bar(200, "Managed Progress");

        assert_eq!(pb.length(), Some(200));
        assert_eq!(pb.position(), 0);

        pb.inc(50);
        assert_eq!(pb.position(), 50);
    }

    #[test]
    fn test_progress_bar_manager_create_spinner() {
        let manager = ProgressBarManager::new();
        let spinner = manager.create_spinner("Managed Spinner");

        assert_eq!(spinner.length(), None);

        spinner.tick();
        spinner.finish();
        assert!(spinner.is_finished());
    }

    #[test]
    fn test_progress_bar_manager_clear() {
        let manager = ProgressBarManager::new();

        // Add some progress bars
        manager.create_progress_bar(100, "Bar 1");
        manager.create_spinner("Spinner 1");

        // Clear should not panic
        let result = manager.clear();
        assert!(result.is_ok());
    }

    #[test]
    fn test_progress_bar_with_zero_total() {
        let pb = create_progress_bar(0, "Empty Progress");
        assert_eq!(pb.length(), Some(0));

        // Setting position on a zero-length bar shouldn't panic
        pb.set_position(0);
        pb.finish();
    }

    #[test]
    fn test_progress_bar_overflow() {
        let pb = create_progress_bar(10, "Small Progress");

        // Setting position beyond length shouldn't panic
        pb.set_position(20);
        assert_eq!(pb.position(), 20);
    }

    #[test]
    fn test_multiple_progress_bars() {
        let manager = ProgressBarManager::new();

        let pb1 = manager.create_progress_bar(100, "Task 1");
        let pb2 = manager.create_progress_bar(200, "Task 2");
        let spinner = manager.create_spinner("Background Task");

        // Simulate concurrent progress
        pb1.inc(25);
        pb2.inc(50);
        spinner.tick();

        assert_eq!(pb1.position(), 25);
        assert_eq!(pb2.position(), 50);

        // Finish all
        pb1.finish();
        pb2.finish();
        spinner.finish();

        assert!(pb1.is_finished());
        assert!(pb2.is_finished());
        assert!(spinner.is_finished());
    }

    #[test]
    fn test_progress_bar_message_updates() {
        let pb = create_progress_bar(100, "Initial");

        pb.set_message("Updated".to_string());
        pb.inc(50);
        pb.set_message("Almost Done".to_string());
        pb.set_position(100);
        pb.finish_with_message("Complete".to_string());

        assert!(pb.is_finished());
    }
}