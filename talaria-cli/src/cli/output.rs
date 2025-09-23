#![allow(dead_code)]

/// Standard output utilities for consistent command formatting
use colored::*;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Attribute, Cell, Color as TableColor, ContentArrangement, Table};

/// Display a section header with optional underline
pub fn section_header(title: &str) {
    println!("\n{}", title.bold().cyan());
}

pub fn section_header_with_line(title: &str) {
    println!("\n{}", title.bold().cyan());
    println!("{}", "─".repeat(title.len()).dimmed());
}

/// Display a success message
pub fn success(message: &str) {
    println!("{} {}", "✓".green(), message);
}

/// Display an info message
pub fn info(message: &str) {
    println!("{} {}", "●".blue(), message);
}

/// Display a warning message
pub fn warning(message: &str) {
    println!("{} {}", "⚠".yellow(), message);
}

/// Display an error message
pub fn error(message: &str) {
    eprintln!("{} {}", "✗".red(), message);
}

/// Display an empty/none indicator
pub fn empty(message: &str) {
    println!("{} {}", "◌".dimmed(), message);
}

/// Display a process/action message
pub fn action(message: &str) {
    println!("{} {}", "▶".cyan(), message);
}

/// Tree structure item
pub fn tree_item(is_last: bool, label: &str, value: Option<&str>) {
    let prefix = if is_last { "└─" } else { "├─" };
    if let Some(val) = value {
        println!("{} {}: {}", prefix.dimmed(), label, val);
    } else {
        println!("{} {}", prefix.dimmed(), label);
    }
}

/// Tree structure item with continuation
pub fn tree_item_continued(label: &str, value: Option<&str>) {
    if let Some(val) = value {
        println!("{}{} {}: {}", "│  ".dimmed(), "├─".dimmed(), label, val);
    } else {
        println!("{}{} {}", "│  ".dimmed(), "├─".dimmed(), label);
    }
}

/// Last tree item in a continuation
pub fn tree_item_continued_last(label: &str, value: Option<&str>) {
    if let Some(val) = value {
        println!("{}{} {}: {}", "│  ".dimmed(), "└─".dimmed(), label, val);
    } else {
        println!("{}{} {}", "│  ".dimmed(), "└─".dimmed(), label);
    }
}

/// Tree section with nested items
pub fn tree_section(title: &str, items: Vec<(&str, String)>, is_last: bool) {
    tree_item(is_last, title, None);
    let continuation = if is_last { "   " } else { "│  " };

    for (i, (label, value)) in items.iter().enumerate() {
        let is_last_item = i == items.len() - 1;
        let prefix = if is_last_item { "└─" } else { "├─" };
        println!(
            "{}{} {}: {}",
            continuation.dimmed(),
            prefix.dimmed(),
            label,
            value
        );
    }
}

/// Create a standard table with our preferred styling
pub fn create_standard_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table
}

/// Create a standard header cell
pub fn header_cell(text: &str) -> Cell {
    Cell::new(text)
        .add_attribute(Attribute::Bold)
        .fg(TableColor::Cyan)
}

/// Format bytes with appropriate unit
pub fn format_size(bytes: usize) -> String {
    use humansize::{format_size as hs_format, BINARY};
    hs_format(bytes, BINARY)
}

/// Format a number with thousands separator
pub fn format_number(n: usize) -> String {
    // Simple thousands separator implementation
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, c);
    }
    result
}

/// Display a subsection header
pub fn subsection_header(title: &str) {
    println!("\n{} {}", "◆".cyan(), title.bold());
}

/// Display statistics header
pub fn stats_header(title: &str) {
    println!("\n{} {}", "■".blue(), title.bold());
}

/// Tree node structure for complex trees
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub label: String,
    pub value: Option<String>,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: None,
            children: Vec::new(),
        }
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn add_child(mut self, child: TreeNode) -> Self {
        self.children.push(child);
        self
    }
}

/// Render a tree structure
pub fn render_tree(nodes: &[TreeNode], prefix: &str) {
    for (i, node) in nodes.iter().enumerate() {
        let is_last = i == nodes.len() - 1;
        let connector = if is_last { "└─" } else { "├─" };

        if let Some(value) = &node.value {
            println!("{}{} {}: {}", prefix, connector.dimmed(), node.label, value);
        } else {
            println!("{}{} {}", prefix, connector.dimmed(), node.label);
        }

        if !node.children.is_empty() {
            let child_prefix = if is_last {
                format!("{}   ", prefix)
            } else {
                format!("{}│  ", prefix)
            };
            render_tree(&node.children, &child_prefix);
        }
    }
}

/// Helper to build a tree from key-value pairs
pub fn build_tree_section(title: &str, items: Vec<(&str, String)>) -> TreeNode {
    let mut node = TreeNode::new(title);
    for (label, value) in items {
        node.children.push(TreeNode::new(label).with_value(value));
    }
    node
}
