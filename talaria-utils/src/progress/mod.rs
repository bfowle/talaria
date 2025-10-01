/// Unified progress management module
///
/// This module replaces the fragmented progress implementations with a single,
/// consistent system for all progress reporting in Talaria.
pub mod manager;

pub use manager::{OperationType, ProgressInfo, ProgressManager, ProgressManagerBuilder};

use indicatif::{ProgressBar, ProgressStyle};

/// Create a standard progress bar with consistent styling
///
/// For more complex progress scenarios, consider using `ProgressManager`.
pub fn create_progress_bar(total: u64, message: &str) -> ProgressBar {
    // If total is 0 and message is empty, return a hidden bar (for quiet mode compatibility)
    if total == 0 && message.is_empty() {
        return ProgressBar::hidden();
    }

    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {human_pos}/{human_len} ({eta_precise})")
            .unwrap()
            .progress_chars("━━─")
            .with_key(
                "human_pos",
                |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
                    use crate::display::output::format_number;
                    write!(w, "{:>9}", format_number(state.pos() as usize)).unwrap()
                },
            )
            .with_key(
                "human_len",
                |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
                    use crate::display::output::format_number;
                    write!(w, "{}", format_number(state.len().unwrap_or(0) as usize)).unwrap()
                },
            )
            .with_key(
                "eta_precise",
                |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
                    if state.pos() == 0 {
                        write!(w, "calculating").unwrap()
                    } else {
                        let eta = state.eta();
                        let secs = eta.as_secs();
                        if secs == 0 {
                            write!(w, "-").unwrap()
                        } else if secs < 60 {
                            write!(w, "{}s", secs).unwrap()
                        } else if secs < 3600 {
                            write!(w, "{}m", secs / 60).unwrap()
                        } else if secs < 86400 {
                            write!(w, "{}h {}m", secs / 3600, (secs % 3600) / 60).unwrap()
                        } else if secs < 31536000 {
                            // Less than a year
                            write!(w, "{}d {}h", secs / 86400, (secs % 86400) / 3600).unwrap()
                        } else {
                            // If more than a year, something's wrong, show as unknown
                            write!(w, "-").unwrap()
                        }
                    }
                },
            ),
    );
    pb.set_message(message.to_string());
    // Don't use steady_tick as it causes ETA miscalculation
    pb
}

/// Create a spinner with consistent styling
///
/// For more complex progress scenarios, consider using `ProgressManager`.
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());
    // Enable steady tick for spinners - only progress bars have ETA issues
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb
}

/// Create a hidden progress bar that doesn't display anything
///
/// Used in quiet mode to avoid flickering empty progress bars.
/// For more complex scenarios, consider using `ProgressManager` with `visible=false`.
pub fn create_hidden_progress_bar() -> ProgressBar {
    ProgressBar::hidden()
}
