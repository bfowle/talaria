#![allow(dead_code)]

/// Structured output formatter for CLI commands
/// Provides Claude Code-style formatting with sections and progress tracking
use colored::*;
use std::fmt;

/// Status indicator for progress items
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    Pending,
    InProgress,
    Complete,
    Failed,
    Skipped,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Pending => write!(f, "[ ]"),
            Status::InProgress => write!(f, "[⏳]"),
            Status::Complete => write!(f, "[✓]"),
            Status::Failed => write!(f, "[✗]"),
            Status::Skipped => write!(f, "[⊘]"),
        }
    }
}

/// A section in the output (e.g., "Preprocessing", "Reference Selection")
pub struct Section {
    title: String,
    status: Status,
    details: Vec<String>,
    subsections: Vec<Section>,
    indent_level: usize,
}

impl Section {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            status: Status::Pending,
            details: Vec::new(),
            subsections: Vec::new(),
            indent_level: 0,
        }
    }

    pub fn with_status(mut self, status: Status) -> Self {
        self.status = status;
        self
    }

    pub fn add_detail(&mut self, detail: impl Into<String>) {
        self.details.push(detail.into());
    }

    pub fn add_subsection(&mut self, mut subsection: Section) {
        subsection.indent_level = self.indent_level + 1;
        self.subsections.push(subsection);
    }

    pub fn set_status(&mut self, status: Status) {
        self.status = status;
    }

    /// Print the section with proper formatting
    pub fn print(&self) {
        let indent = "  ".repeat(self.indent_level);
        let status_str = match self.status {
            Status::Complete => format!("{}", self.status).green().to_string(),
            Status::Failed => format!("{}", self.status).red().to_string(),
            Status::InProgress => format!("{}", self.status).yellow().to_string(),
            Status::Skipped => format!("{}", self.status).dimmed().to_string(),
            _ => format!("{}", self.status),
        };

        // Print section header
        println!("{}{} {}", indent, status_str, self.title.bold());

        // Print details
        for detail in &self.details {
            println!("{}    {}", indent, detail.dimmed());
        }

        // Print subsections
        for subsection in &self.subsections {
            subsection.print();
        }
    }
}

/// Main pipeline formatter for reduction operations
pub struct PipelineFormatter {
    title: String,
    sections: Vec<Section>,
}

impl PipelineFormatter {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            sections: Vec::new(),
        }
    }

    pub fn add_section(&mut self, section: Section) {
        self.sections.push(section);
    }

    pub fn update_section(&mut self, index: usize, status: Status) {
        if let Some(section) = self.sections.get_mut(index) {
            section.set_status(status);
        }
    }

    pub fn print_header(&self) {
        println!("\n╭─ {} ─╮", self.title.bold().cyan());
        println!("│");
    }

    pub fn print_sections(&self) {
        for (i, section) in self.sections.iter().enumerate() {
            if i == 0 {
                print!("├─ ");
            } else if i == self.sections.len() - 1 {
                print!("└─ ");
            } else {
                print!("├─ ");
            }
            section.print();
            if i < self.sections.len() - 1 {
                println!("│");
            }
        }
    }

    pub fn print(&self) {
        self.print_header();
        self.print_sections();
        println!();
    }

    /// Update and reprint (for live updates)
    pub fn update(&self) {
        // Clear previous output (simplified - in practice would track lines)
        print!("\x1B[2J\x1B[1;1H"); // Clear screen and move to top
        self.print();
    }
}

/// Progress item for detailed progress tracking
pub struct ProgressItem {
    pub label: String,
    pub current: usize,
    pub total: usize,
    pub status: Status,
}

impl ProgressItem {
    pub fn new(label: impl Into<String>, total: usize) -> Self {
        Self {
            label: label.into(),
            current: 0,
            total,
            status: Status::InProgress,
        }
    }

    pub fn increment(&mut self) {
        self.current += 1;
        if self.current >= self.total {
            self.status = Status::Complete;
        }
    }

    pub fn print(&self, indent: usize) {
        let indent_str = "  ".repeat(indent);
        let progress = if self.total > 0 {
            format!("{}/{}", self.current, self.total)
        } else {
            "...".to_string()
        };

        let status_color = match self.status {
            Status::Complete => progress.green(),
            Status::InProgress => progress.yellow(),
            Status::Failed => progress.red(),
            _ => progress.normal(),
        };

        println!(
            "{}{} {} ({})",
            indent_str, self.status, self.label, status_color
        );
    }
}

/// Helper functions for common formatting patterns
pub fn print_info(message: &str) {
    println!("ℹ️  {}", message.blue());
}

// Re-export functions from formatter module

pub fn print_debug(message: &str) {
    if std::env::var("TALARIA_DEBUG").is_ok() {
        eprintln!("🔍 {}", message.dimmed());
    }
}

/// Format statistics in a clean table
pub fn print_stats(title: &str, stats: &[(String, String)]) {
    println!("\n{}", title.bold());
    println!("{}", "─".repeat(40));
    for (key, value) in stats {
        println!("{:<20} {}", key, value.bright_white());
    }
    println!("{}", "─".repeat(40));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_display() {
        assert_eq!(format!("{}", Status::Pending), "[ ]");
        assert_eq!(format!("{}", Status::Complete), "[✓]");
        assert_eq!(format!("{}", Status::Failed), "[✗]");
    }

    #[test]
    fn test_section_creation() {
        let mut section = Section::new("Test Section");
        assert_eq!(section.status, Status::Pending);

        section.set_status(Status::Complete);
        assert_eq!(section.status, Status::Complete);

        section.add_detail("Detail 1");
        assert_eq!(section.details.len(), 1);
    }

    #[test]
    fn test_pipeline_formatter() {
        let mut formatter = PipelineFormatter::new("Test Pipeline");

        let section1 = Section::new("Step 1").with_status(Status::Complete);
        let section2 = Section::new("Step 2").with_status(Status::InProgress);

        formatter.add_section(section1);
        formatter.add_section(section2);

        assert_eq!(formatter.sections.len(), 2);
    }
}
