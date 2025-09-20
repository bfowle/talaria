/// Beautiful CLI output formatting module
/// Provides consistent, modern CLI output styling similar to Claude Code and other modern tools
use colored::*;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Task status for todo-style tracking
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Complete,
    Failed,
    Skipped,
}

impl TaskStatus {
    pub fn symbol(&self) -> &str {
        match self {
            TaskStatus::Pending => "○",
            TaskStatus::InProgress => "●",
            TaskStatus::Complete => "✓",
            TaskStatus::Failed => "✗",
            TaskStatus::Skipped => "─",
        }
    }

    pub fn colored_symbol(&self) -> ColoredString {
        match self {
            TaskStatus::Pending => self.symbol().dimmed(),
            TaskStatus::InProgress => self.symbol().yellow(),
            TaskStatus::Complete => self.symbol().green(),
            TaskStatus::Failed => self.symbol().red(),
            TaskStatus::Skipped => self.symbol().dimmed(),
        }
    }
}

/// A task in the task list
#[derive(Debug, Clone)]
pub struct Task {
    pub description: String,
    pub status: TaskStatus,
    pub progress: Option<ProgressBar>,
    pub message: Option<String>,
}

/// Task list for tracking multi-step operations
pub struct TaskList {
    tasks: Arc<Mutex<Vec<Task>>>,
    multi_progress: MultiProgress,
    current_spinner: Option<ProgressBar>,
    silent: bool,
    printed_lines: usize,
    updates_paused: bool,
}

/// Handle to a task for updating its status
#[derive(Debug, Clone, Copy)]
pub struct TaskHandle(usize);

impl TaskList {
    /// Create a new task list
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
            multi_progress: MultiProgress::new(),
            current_spinner: None,
            silent: false,
            printed_lines: 0,
            updates_paused: false,
        }
    }

    /// Create a silent task list (no output)
    pub fn silent() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
            multi_progress: MultiProgress::with_draw_target(ProgressDrawTarget::hidden()),
            current_spinner: None,
            silent: true,
            printed_lines: 0,
            updates_paused: false,
        }
    }

    /// Print a styled header
    pub fn print_header(&self, title: &str) {
        if self.silent {
            return;
        }
        let width = terminal_size::terminal_size()
            .map(|(terminal_size::Width(w), _)| w as usize)
            .unwrap_or(80);
        let line = "═".repeat(width.min(80));
        println!("\n{}", format!("▶ {}", title).bold().cyan());
        println!("{}", line.dimmed());
    }

    /// Add a new task to the list
    pub fn add_task(&mut self, description: &str) -> TaskHandle {
        let handle = {
            let mut tasks = self.tasks.lock().unwrap();
            let handle = TaskHandle(tasks.len());
            tasks.push(Task {
                description: description.to_string(),
                status: TaskStatus::Pending,
                progress: None,
                message: None,
            });
            handle
        }; // Release the lock here

        if !self.silent {
            self.print_task_list();
        }

        handle
    }

    /// Update task status
    pub fn update_task(&mut self, handle: TaskHandle, status: TaskStatus) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.get_mut(handle.0) {
            task.status = status;

            // Clear spinner if task is complete or failed
            if matches!(
                status,
                TaskStatus::Complete | TaskStatus::Failed | TaskStatus::Skipped
            ) {
                if let Some(ref spinner) = self.current_spinner {
                    spinner.finish_and_clear();
                }
                self.current_spinner = None;
            }
        }
        drop(tasks);

        // Only update display if not paused
        if !self.silent && !self.updates_paused {
            self.print_task_list();
        }
    }

    /// Set a message for a task (shown alongside the task description)
    pub fn set_task_message(&mut self, handle: TaskHandle, message: &str) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.get_mut(handle.0) {
            task.message = Some(message.to_string());
        }
        drop(tasks);

        // Only update display if not paused
        if !self.silent && !self.updates_paused {
            self.print_task_list();
        }
    }

    /// Start a spinner for the current task
    pub fn start_spinner(&mut self, handle: TaskHandle, message: &str) -> Option<ProgressBar> {
        if self.silent {
            return None;
        }

        // Clear any existing spinner
        if let Some(ref spinner) = self.current_spinner {
            spinner.finish_and_clear();
        }

        let spinner = self.multi_progress.add(ProgressBar::new_spinner());
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
        );
        spinner.set_message(message.to_string());
        spinner.enable_steady_tick(Duration::from_millis(100));

        self.current_spinner = Some(spinner.clone());
        self.update_task(handle, TaskStatus::InProgress);

        Some(spinner)
    }

    /// Create a progress bar for a task
    pub fn create_progress_bar(&mut self, handle: TaskHandle, total: u64) -> ProgressBar {
        let pb = if self.silent {
            ProgressBar::hidden()
        } else {
            self.multi_progress.add(ProgressBar::new(total))
        };

        pb.set_style(create_progress_style());

        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.get_mut(handle.0) {
            task.status = TaskStatus::InProgress;
            task.progress = Some(pb.clone());
        }

        pb
    }

    /// Print the current task list
    fn print_task_list(&mut self) {
        if self.silent {
            return;
        }

        use std::io::{self, Write};

        let tasks = self.tasks.lock().unwrap();

        // Clear previous lines using carriage returns and ANSI clear
        if self.printed_lines > 0 {
            // Use simpler approach: move up and clear
            for _ in 0..self.printed_lines {
                // Move up one line and clear it
                print!("\x1B[1A\x1B[2K");
            }
            io::stdout().flush().ok();
        }

        // Print all tasks
        for task in tasks.iter() {
            let desc = if task.status == TaskStatus::InProgress {
                task.description.yellow()
            } else if task.status == TaskStatus::Complete {
                task.description.green()
            } else if task.status == TaskStatus::Failed {
                task.description.red()
            } else {
                task.description.normal()
            };

            if let Some(ref msg) = task.message {
                println!(
                    "  {} {} - {}",
                    task.status.colored_symbol(),
                    desc,
                    msg.dimmed()
                );
            } else {
                println!("  {} {}", task.status.colored_symbol(), desc);
            }
        }

        // Update the count of printed lines
        self.printed_lines = tasks.len();
    }

    /// Mark all remaining tasks as skipped
    pub fn skip_remaining(&mut self) {
        let mut tasks = self.tasks.lock().unwrap();
        for task in tasks.iter_mut() {
            if task.status == TaskStatus::Pending {
                task.status = TaskStatus::Skipped;
            }
        }
        drop(tasks);

        // Clear any active spinner first
        if let Some(ref spinner) = self.current_spinner {
            spinner.finish_and_clear();
        }
        self.current_spinner = None;

        if !self.silent && !self.updates_paused {
            self.print_task_list();
        }
    }

    /// Pause task list updates (useful during download progress bars)
    pub fn pause_updates(&mut self) {
        self.updates_paused = true;
        // Clear any spinners before pausing
        if let Some(ref spinner) = self.current_spinner {
            spinner.finish_and_clear();
        }
        self.current_spinner = None;
    }

    /// Resume task list updates
    pub fn resume_updates(&mut self) {
        self.updates_paused = false;
        // Redraw the task list after resuming
        if !self.silent {
            self.print_task_list();
        }
    }
}

/// Print an info box with bullet points
pub fn info_box(title: &str, items: &[&str]) {
    println!("\n{} {}", "ℹ".cyan(), title.bold());
    for item in items {
        println!("  {} {}", "•".dimmed(), item);
    }
}

/// Print a warning message
pub fn print_warning(message: &str) {
    println!(
        "\n{} {}",
        "⚠".yellow(),
        format!("Warning: {}", message).yellow()
    );
}

/// Print an error message
pub fn print_error(message: &str) {
    eprintln!("\n{} {}", "✗".red(), format!("Error: {}", message).red());
}

/// Print a success message
pub fn print_success(message: &str) {
    println!("\n{} {}", "✓".green().bold(), message);
}

/// Print a tip
pub fn print_tip(message: &str) {
    println!("\n{} {}", "→".cyan(), format!("Tip: {}", message).dimmed());
}

/// Print a section header
pub fn print_section(title: &str) {
    let width = terminal_size::terminal_size()
        .map(|(terminal_size::Width(w), _)| w as usize)
        .unwrap_or(80);
    let line = "─".repeat(width.min(60));
    println!("\n{} {}", "▶".cyan(), title.bold());
    println!("{}", line.dimmed());
}

/// Create a standard progress bar style
pub fn create_progress_style() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
        .unwrap()
        .progress_chars("█▓▒░")
}

/// Create a download progress bar style with speed
pub fn create_download_style() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        .unwrap()
        .progress_chars("█▓▒░")
}

/// Create a spinner style for indeterminate operations
pub fn create_spinner_style() -> ProgressStyle {
    ProgressStyle::default_spinner()
        .template("{spinner:.cyan} {msg}")
        .unwrap()
        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
}

/// Print a statistics table using comfy_table
pub fn print_stats_table(title: &str, stats: Vec<(&str, String)>) {
    use comfy_table::modifiers::UTF8_ROUND_CORNERS;
    use comfy_table::presets::UTF8_FULL;
    use comfy_table::{Attribute, Cell, Color as TableColor, ContentArrangement, Table};

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Add title as header
    table.set_header(vec![
        Cell::new(title)
            .add_attribute(Attribute::Bold)
            .fg(TableColor::Cyan),
        Cell::new("").add_attribute(Attribute::Bold),
    ]);

    // Add stats rows
    for (label, value) in stats {
        table.add_row(vec![
            Cell::new(label),
            Cell::new(value).fg(TableColor::Green),
        ]);
    }

    println!("\n{}", table);
}

/// Format bytes into human-readable string
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Check if colors should be disabled
pub fn colors_enabled() -> bool {
    std::env::var("NO_COLOR").is_err()
        && std::env::var("CLICOLOR").unwrap_or_else(|_| "1".to_string()) != "0"
}

/// Print a structured section with proper formatting
pub fn print_structured_section(title: &str) {
    println!("\n{} {}", "▶".cyan().bold(), title.bold());
}

/// Print a structured item with tree-like formatting
pub fn print_structured_item(message: &str, level: usize, is_last: bool) {
    let prefix = match level {
        0 => {
            if is_last {
                "└─".dimmed()
            } else {
                "├─".dimmed()
            }
        }
        1 => {
            if is_last {
                "  └─".dimmed()
            } else {
                "  ├─".dimmed()
            }
        }
        _ => {
            let indent = "  ".repeat(level);
            if is_last {
                format!("{}└─", indent).dimmed()
            } else {
                format!("{}├─", indent).dimmed()
            }
        }
    };
    println!("  {} {}", prefix, message);
}

/// Print a progress message with a filled circle
pub fn print_progress(message: &str) {
    println!("  {} {}", "●".yellow(), message);
}

/// Print a sub-item with proper indentation
pub fn print_sub_item(message: &str, indent: usize) {
    let spaces = "  ".repeat(indent + 1);
    println!("{}{}", spaces, message.dimmed());
}

/// Print formatted number with thousands separator
pub fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Initialize the formatter (sets up colored output)
pub fn init() {
    if !colors_enabled() {
        colored::control::set_override(false);
    }
}
