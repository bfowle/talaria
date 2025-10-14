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

    // Handle negative numbers
    let (is_negative, digits) = if let Some(stripped) = s.strip_prefix('-') {
        (true, stripped)
    } else {
        (false, s.as_str())
    };

    let mut result = String::new();

    for (i, c) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }

    if is_negative {
        result.push('-');
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
        let item_prefix = if i == items.len() - 1 {
            "└─"
        } else {
            "├─"
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_node_creation() {
        let node = TreeNode::new("root");
        assert_eq!(node.name, "root");
        assert!(node.children.is_empty());
    }

    #[test]
    fn test_tree_node_add_child() {
        let child1 = TreeNode::new("child1");
        let child2 = TreeNode::new("child2");

        let root = TreeNode::new("root").add_child(child1).add_child(child2);

        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].name, "child1");
        assert_eq!(root.children[1].name, "child2");
    }

    #[test]
    fn test_tree_node_render_single() {
        let node = TreeNode::new("root");
        let rendered = node.render();
        assert_eq!(rendered, "root\n");
    }

    #[test]
    fn test_tree_node_render_with_children() {
        let root = TreeNode::new("root")
            .add_child(TreeNode::new("child1"))
            .add_child(TreeNode::new("child2"));

        let rendered = root.render();
        assert!(rendered.contains("root\n"));
        assert!(rendered.contains("├─ child1\n"));
        assert!(rendered.contains("└─ child2\n"));
    }

    #[test]
    fn test_tree_node_render_nested() {
        let grandchild = TreeNode::new("grandchild");
        let child = TreeNode::new("child").add_child(grandchild);
        let root = TreeNode::new("root").add_child(child);

        let rendered = root.render();
        assert!(rendered.contains("root\n"));
        assert!(rendered.contains("└─ child\n"));
        assert!(rendered.contains("   └─ grandchild\n"));
    }

    #[test]
    fn test_tree_node_render_complex() {
        let root = TreeNode::new("root")
            .add_child(
                TreeNode::new("branch1")
                    .add_child(TreeNode::new("leaf1"))
                    .add_child(TreeNode::new("leaf2")),
            )
            .add_child(TreeNode::new("branch2").add_child(TreeNode::new("leaf3")));

        let rendered = root.render();
        assert!(rendered.contains("root\n"));
        assert!(rendered.contains("├─ branch1\n"));
        assert!(rendered.contains("│  ├─ leaf1\n"));
        assert!(rendered.contains("│  └─ leaf2\n"));
        assert!(rendered.contains("└─ branch2\n"));
        assert!(rendered.contains("   └─ leaf3\n"));
    }

    #[test]
    fn test_format_number_small() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
    }

    #[test]
    fn test_format_number_thousands() {
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(10000), "10,000");
        assert_eq!(format_number(999999), "999,999");
    }

    #[test]
    fn test_format_number_millions() {
        assert_eq!(format_number(1000000), "1,000,000");
        assert_eq!(format_number(123456789), "123,456,789");
    }

    #[test]
    fn test_format_number_negative() {
        assert_eq!(format_number(-1000), "-1,000");
        assert_eq!(format_number(-999999), "-999,999");
    }

    #[test]
    fn test_create_standard_table() {
        let _table = create_standard_table();
        // Just verify it creates without panic
        // Table is created successfully
    }

    #[test]
    fn test_header_cell() {
        let cell = header_cell("Test Header");
        // Can't easily test Cell internals, but verify it doesn't panic
        let _ = format!("{:?}", cell);
    }

    #[test]
    fn test_message_functions() {
        // These functions print to stderr, so we just verify they don't panic
        warning("test warning");
        info("test info");
        success("test success");
        error("test error");
    }

    #[test]
    fn test_tree_section() {
        let items = vec![
            ("key1", "value1".to_string()),
            ("key2", "value2".to_string()),
        ];

        // This prints to stdout, so just verify it doesn't panic
        tree_section("Test Section", items.clone(), false);
        tree_section("Test Section", items, true);
    }

    #[test]
    fn test_tree_node_empty_name() {
        let node = TreeNode::new("");
        assert_eq!(node.render(), "\n");
    }

    #[test]
    fn test_tree_node_special_characters() {
        let node = TreeNode::new("Node with 特殊字符 and émojis 🌳");
        let rendered = node.render();
        assert!(rendered.contains("Node with 特殊字符 and émojis 🌳"));
    }

    #[test]
    fn test_tree_deep_nesting() {
        // Build a deeply nested tree
        let mut root = TreeNode::new("level0");
        let level1 = TreeNode::new("level1");
        let level2 = TreeNode::new("level2");
        let level3 = TreeNode::new("level3");
        let level4 = TreeNode::new("level4");

        // Create nested structure
        root = root.add_child(level1.add_child(level2.add_child(level3.add_child(level4))));

        let rendered = root.render();

        // Should handle deep nesting without stack overflow
        assert!(rendered.contains("level0"));
        assert!(rendered.contains("level1"));
        assert!(rendered.contains("level2"));
        assert!(rendered.contains("level3"));
        assert!(rendered.contains("level4"));
    }
}
