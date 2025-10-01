//! Structured output formatting utilities

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
            Status::InProgress => write!(f, "[*]"),
            Status::Complete => write!(f, "[✓]"),
            Status::Failed => write!(f, "[✗]"),
            Status::Skipped => write!(f, "[⊘]"),
        }
    }
}

/// A section in the output
pub struct Section {
    title: String,
    items: Vec<Item>,
    status: Status,
}

impl Section {
    /// Create a new section
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
            status: Status::Pending,
        }
    }

    /// Add an item to the section
    pub fn add_item(&mut self, item: Item) {
        self.items.push(item);
    }

    /// Set the status of the section
    pub fn set_status(&mut self, status: Status) {
        self.status = status;
    }

    /// Render the section to string
    pub fn render(&self) -> String {
        let mut output = String::new();

        // Section header
        let header = format!("{} {}", self.status, self.title.bold());
        output.push_str(&header);
        output.push('\n');

        // Section items
        for item in &self.items {
            output.push_str(&item.render(1));
            output.push('\n');
        }

        output
    }
}

/// An item within a section
pub struct Item {
    label: String,
    value: Option<String>,
    status: Option<Status>,
    sub_items: Vec<Item>,
}

impl Item {
    /// Create a new item
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: None,
            status: None,
            sub_items: Vec::new(),
        }
    }

    /// Set the value
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Set the status
    pub fn with_status(mut self, status: Status) -> Self {
        self.status = Some(status);
        self
    }

    /// Add a sub-item
    pub fn add_sub_item(&mut self, item: Item) {
        self.sub_items.push(item);
    }

    /// Render the item with indentation
    pub fn render(&self, indent_level: usize) -> String {
        let mut output = String::new();
        let indent = "  ".repeat(indent_level);

        // Main item
        if let Some(status) = self.status {
            output.push_str(&format!("{}{} {}", indent, status, self.label));
        } else {
            output.push_str(&format!("{}• {}", indent, self.label));
        }

        if let Some(ref value) = self.value {
            output.push_str(&format!(": {}", value));
        }

        // Sub-items
        for sub_item in &self.sub_items {
            output.push('\n');
            output.push_str(&sub_item.render(indent_level + 1));
        }

        output
    }
}

/// Output formatter that manages sections
pub struct OutputFormatter {
    sections: Vec<Section>,
    current_section: Option<usize>,
}

impl OutputFormatter {
    /// Create a new formatter
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
            current_section: None,
        }
    }

    /// Start a new section
    pub fn start_section(&mut self, title: impl Into<String>) -> &mut Section {
        let section = Section::new(title);
        self.sections.push(section);
        self.current_section = Some(self.sections.len() - 1);
        &mut self.sections[self.current_section.unwrap()]
    }

    /// Add item to current section
    pub fn add_item(&mut self, item: Item) {
        if let Some(idx) = self.current_section {
            self.sections[idx].add_item(item);
        }
    }

    /// Update current section status
    pub fn update_status(&mut self, status: Status) {
        if let Some(idx) = self.current_section {
            self.sections[idx].set_status(status);
        }
    }

    /// Render all sections
    pub fn render(&self) -> String {
        let mut output = String::new();
        for (i, section) in self.sections.iter().enumerate() {
            if i > 0 {
                output.push('\n');
            }
            output.push_str(&section.render());
        }
        output
    }
}

impl Default for OutputFormatter {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for types that can report their status
pub trait StatusReporter {
    /// Get the current status
    fn status(&self) -> Status;

    /// Get a summary message
    fn summary(&self) -> String;
}

/// Trait for types that can be formatted as output
pub trait OutputFormattable {
    /// Format as a section
    fn format_section(&self) -> Section;

    /// Format as an item
    fn format_item(&self) -> Item;
}
