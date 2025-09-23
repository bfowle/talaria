use crate::output::TreeNode;
/// Rendering traits for temporal data visualization
///
/// These traits enable rich terminal visualization of temporal query results
/// using the existing output utilities.
use comfy_table::Table;

/// Trait for rendering temporal data in various formats
pub trait TemporalRenderable {
    /// Render as tree structure for hierarchical display
    fn render_tree(&self) -> Vec<TreeNode>;

    /// Render as table for tabular display
    fn render_table(&self) -> Table;

    /// Render timeline visualization with Unicode characters
    fn render_timeline(&self) -> String;

    /// Get a summary string for quick display
    fn render_summary(&self) -> String;

    /// Render in a specific format
    fn render(&self, format: RenderFormat) -> String {
        match format {
            RenderFormat::Tree => {
                let nodes = self.render_tree();
                render_tree_to_string(&nodes, "")
            }
            RenderFormat::Table => self.render_table().to_string(),
            RenderFormat::Timeline => self.render_timeline(),
            RenderFormat::Summary => self.render_summary(),
        }
    }
}

/// Available rendering formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFormat {
    Tree,
    Table,
    Timeline,
    Summary,
}

/// Helper function to render tree nodes to string
fn render_tree_to_string(nodes: &[TreeNode], prefix: &str) -> String {
    let mut output = String::new();

    for (i, node) in nodes.iter().enumerate() {
        let is_last = i == nodes.len() - 1;
        let connector = if is_last { "└─" } else { "├─" };

        if let Some(value) = &node.value {
            output.push_str(&format!(
                "{}{} {}: {}\n",
                prefix, connector, node.label, value
            ));
        } else {
            output.push_str(&format!("{}{} {}\n", prefix, connector, node.label));
        }

        if !node.children.is_empty() {
            let child_prefix = if is_last {
                format!("{}   ", prefix)
            } else {
                format!("{}│  ", prefix)
            };
            output.push_str(&render_tree_to_string(&node.children, &child_prefix));
        }
    }

    output
}

/// Trait for rendering classification evolution
pub trait EvolutionRenderable {
    /// Render evolution as a timeline with markers
    fn render_evolution_timeline(&self) -> String;

    /// Render evolution as a graph (using box-drawing characters)
    fn render_evolution_graph(&self) -> String;

    /// Render evolution statistics
    fn render_evolution_stats(&self) -> Table;
}

/// Trait for rendering taxonomy diffs
pub trait DiffRenderable {
    /// Render diff in unified format
    fn render_unified_diff(&self) -> String;

    /// Render diff as side-by-side comparison
    fn render_side_by_side(&self) -> String;

    /// Render diff statistics
    fn render_diff_stats(&self) -> Table;
}

/// Helper to create timeline markers
pub fn timeline_marker(change_type: &str) -> &'static str {
    match change_type {
        "rename" => "◆",
        "merge" => "⤝",
        "split" => "⤜",
        "delete" => "✗",
        "create" => "✚",
        "modify" => "◈",
        _ => "●",
    }
}

/// Helper to create timeline with events
pub fn create_timeline(
    start_year: i32,
    end_year: i32,
    events: Vec<(i32, &str, &str)>, // (year, marker, description)
) -> String {
    let mut output = String::new();

    // Header with years
    output.push_str("    ");
    for year in start_year..=end_year {
        output.push_str(&format!("{:^12}", year));
    }
    output.push('\n');

    // Timeline bar
    output.push_str("────");
    for _ in start_year..=end_year {
        output.push_str("┬───────────");
    }
    output.push_str("────\n");

    // Events
    for (year, marker, description) in events {
        let position = (year - start_year) as usize * 12 + 4;
        output.push_str(&format!(
            "{:>width$}{} {}\n",
            "",
            marker,
            description,
            width = position
        ));
    }

    output
}

/// Helper to create a progress bar visualization
pub fn create_progress_visualization(current: usize, total: usize, width: usize) -> String {
    let percentage = (current as f64 / total as f64 * 100.0) as usize;
    let filled = (current as f64 / total as f64 * width as f64) as usize;
    let empty = width - filled;

    format!(
        "[{}{}] {}%",
        "█".repeat(filled),
        "░".repeat(empty),
        percentage
    )
}

/// Helper to create a box around content
pub fn create_box(title: &str, content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let max_width = lines
        .iter()
        .map(|l| l.len())
        .max()
        .unwrap_or(0)
        .max(title.len());

    let mut output = String::new();

    // Top border with title
    output.push_str("╭─");
    output.push_str(title);
    output.push_str(&"─".repeat(max_width - title.len() + 2));
    output.push_str("╮\n");

    // Content
    for line in lines {
        output.push_str("│ ");
        output.push_str(line);
        output.push_str(&" ".repeat(max_width - line.len()));
        output.push_str(" │\n");
    }

    // Bottom border
    output.push('╰');
    output.push_str(&"─".repeat(max_width + 3));
    output.push_str("╯\n");

    output
}
