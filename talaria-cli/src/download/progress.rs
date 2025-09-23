use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

pub struct DownloadProgress {
    bar: ProgressBar,
    total: usize,
    current: usize,
    callback: Option<Box<dyn Fn(usize, usize) + Send + Sync>>,
}

impl DownloadProgress {
    pub fn new() -> Self {
        let bar = ProgressBar::new(0);
        bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] \
                     {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
                )
                .unwrap()
                .progress_chars("#>-"),
        );

        // Enable steady tick for smooth spinner animation
        bar.enable_steady_tick(Duration::from_millis(100));

        DownloadProgress {
            bar,
            total: 0,
            current: 0,
            callback: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_message(message: &str) -> Self {
        let mut progress = Self::new();
        progress.set_message(message);
        progress
    }

    pub fn set_total(&mut self, total: usize) {
        self.total = total;
        self.bar.set_length(total as u64);
    }

    pub fn set_current(&mut self, current: usize) {
        self.current = current;
        self.bar.set_position(current as u64);
        if let Some(ref callback) = self.callback {
            callback(current, self.total);
        }
    }

    #[allow(dead_code)]
    pub fn increment(&mut self, delta: usize) {
        self.current += delta;
        self.bar.inc(delta as u64);
        if let Some(ref callback) = self.callback {
            callback(self.current, self.total);
        }
    }

    pub fn set_message(&mut self, message: &str) {
        self.bar.set_message(message.to_string());
    }

    pub fn finish(&mut self) {
        self.bar.finish_with_message("Complete");
    }

    #[allow(dead_code)]
    pub fn finish_with_message(&mut self, message: &str) {
        self.bar.finish_with_message(message.to_string());
    }

    pub fn is_finished(&self) -> bool {
        self.bar.is_finished()
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.current = 0;
        self.bar.reset();
    }

    pub fn set_callback(&mut self, callback: Box<dyn Fn(usize, usize) + Send + Sync>) {
        self.callback = Some(callback);
    }
}

impl Default for DownloadProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DownloadProgress {
    fn drop(&mut self) {
        if !self.is_finished() {
            self.bar.abandon();
        }
    }
}

#[allow(dead_code)]
pub struct MultiProgress {
    multi: indicatif::MultiProgress,
    bars: Vec<ProgressBar>,
}

impl MultiProgress {
    #[allow(dead_code)]
    pub fn new() -> Self {
        MultiProgress {
            multi: indicatif::MultiProgress::new(),
            bars: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn add_bar(&mut self, total: u64, message: &str) -> usize {
        let bar = ProgressBar::new(total);
        bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{prefix:>12.cyan.bold} [{elapsed_precise}] [{bar:40.cyan/blue}] \
                     {bytes}/{total_bytes} ({eta}) {msg}",
                )
                .unwrap()
                .progress_chars("=> "),
        );
        bar.set_prefix(message.to_string());

        let bar = self.multi.add(bar);
        self.bars.push(bar);
        self.bars.len() - 1
    }

    #[allow(dead_code)]
    pub fn set_position(&mut self, index: usize, position: u64) {
        if let Some(bar) = self.bars.get(index) {
            bar.set_position(position);
        }
    }

    #[allow(dead_code)]
    pub fn inc(&mut self, index: usize, delta: u64) {
        if let Some(bar) = self.bars.get(index) {
            bar.inc(delta);
        }
    }

    #[allow(dead_code)]
    pub fn finish(&mut self, index: usize) {
        if let Some(bar) = self.bars.get(index) {
            bar.finish();
        }
    }

    #[allow(dead_code)]
    pub fn finish_all(&mut self) {
        for bar in &self.bars {
            bar.finish();
        }
    }
}

impl Default for MultiProgress {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
pub fn create_spinner(message: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    spinner.set_message(message.to_string());
    spinner.enable_steady_tick(Duration::from_millis(100));
    spinner
}

#[allow(dead_code)]
pub fn bytes_to_human(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}
