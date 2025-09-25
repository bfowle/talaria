//! Output formatting and visualization utilities

use colored::*;
use comfy_table::{Cell, CellAlignment, Table};
use std::fmt::Display;

/// Tree node for hierarchical visualization
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    /// Create a new tree node
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            children: Vec::new(),
        }
    }

    /// Add a child node (returns self for chaining)
    pub fn add_child(mut self, child: TreeNode) -> Self {
        self.children.push(child);
        self
    }

    /// Render the tree to a string
    pub fn render(&self) -> String {
        self.render_internal("", true, true)
    }

    fn render_internal(&self, prefix: &str, is_last: bool, is_root: bool) -> String {
        let mut result = String::new();

        if !is_root {
            result.push_str(prefix);
            result.push_str(if is_last { "└─ " } else { "├─ " });
        }
        result.push_str(&self.name);
        result.push('\n');

        let child_prefix = if is_root {
            String::new()
        } else {
            format!("{}{}", prefix, if is_last { "   " } else { "│  " })
        };

        for (i, child) in self.children.iter().enumerate() {
            let child_is_last = i == self.children.len() - 1;
            result.push_str(&child.render_internal(&child_prefix, child_is_last, false));
        }

        result
    }
}

/// Format a number with thousands separators
pub fn format_number<T: Display>(n: T) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;

    for c in s.chars().rev() {
        if count == 3 {
            result.push(',');
            count = 0;
        }
        result.push(c);
        count += 1;
    }

    result.chars().rev().collect()
}

/// Print a warning message
pub fn warning(msg: &str) {
    eprintln!("{} {}", "⚠".yellow(), msg.yellow());
}

/// Print an info message
pub fn info(msg: &str) {
    eprintln!("{} {}", "ℹ".blue(), msg);
}

/// Print a success message
pub fn success(msg: &str) {
    eprintln!("{} {}", "✓".green(), msg.green());
}

/// Print an error message
pub fn error(msg: &str) {
    eprintln!("{} {}", "✗".red(), msg.red());
}

/// Create a tree section with items
pub fn tree_section(title: &str, items: Vec<(&str, String)>, last: bool) {
    let prefix = if last { "└─" } else { "├─" };
    println!("{} {}", prefix, title.bold());

    let indent = if last { "   " } else { "│  " };
    for (i, (key, value)) in items.iter().enumerate() {
        let item_prefix = if i == items.len() - 1 { "└─" } else { "├─" };
        println!("{}  {} {}: {}", indent, item_prefix, key, value);
    }
}

/// Create a standard table with consistent styling
pub fn create_standard_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(comfy_table::presets::UTF8_FULL)
        .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS);
    table
}

/// Create a header cell with center alignment
pub fn header_cell(text: &str) -> Cell {
    Cell::new(text)
        .set_alignment(CellAlignment::Center)
        .add_attribute(comfy_table::Attribute::Bold)
}