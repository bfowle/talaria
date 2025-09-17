/// ASCII chart visualization for terminal output
/// Provides simple but effective visualizations for reduction statistics

use colored::*;
use std::collections::HashMap;

/// ASCII bar chart for terminal display
pub struct AsciiBarChart {
    title: String,
    data: Vec<(String, f64)>,
    width: usize,
    show_values: bool,
}

impl AsciiBarChart {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            data: Vec::new(),
            width: 50,
            show_values: true,
        }
    }

    pub fn with_width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    pub fn add_bar(&mut self, label: &str, value: f64) {
        self.data.push((label.to_string(), value));
    }

    pub fn render(&self) -> String {
        let mut output = String::new();

        // Title
        output.push_str(&format!("\n{}\n", self.title.bold().cyan()));
        output.push_str(&"─".repeat(self.width + 20));
        output.push('\n');

        if self.data.is_empty() {
            return output;
        }

        // Find max value for scaling
        let max_value = self.data.iter()
            .map(|(_, v)| *v)
            .fold(0.0, f64::max);

        // Find max label length for alignment
        let max_label_len = self.data.iter()
            .map(|(l, _)| l.len())
            .max()
            .unwrap_or(10);

        // Render each bar
        for (label, value) in &self.data {
            let bar_width = if max_value > 0.0 {
                ((value / max_value) * self.width as f64) as usize
            } else {
                0
            };

            let bar = "█".repeat(bar_width);
            let padding = " ".repeat(self.width - bar_width);

            let formatted_label = format!("{:width$}", label, width = max_label_len);

            if self.show_values {
                output.push_str(&format!(
                    "{} │{}{} {:.1}%\n",
                    formatted_label.yellow(),
                    bar.green(),
                    padding,
                    value
                ));
            } else {
                output.push_str(&format!(
                    "{} │{}{}\n",
                    formatted_label.yellow(),
                    bar.green(),
                    padding
                ));
            }
        }

        output
    }
}

/// ASCII line chart for showing trends
pub struct AsciiLineChart {
    title: String,
    data: Vec<(String, f64)>,
    height: usize,
    width: usize,
}

impl AsciiLineChart {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            data: Vec::new(),
            height: 10,
            width: 60,
        }
    }

    pub fn add_point(&mut self, label: &str, value: f64) {
        self.data.push((label.to_string(), value));
    }

    pub fn render(&self) -> String {
        let mut output = String::new();

        // Title
        output.push_str(&format!("\n{}\n", self.title.bold().cyan()));
        output.push_str(&"─".repeat(self.width));
        output.push('\n');

        if self.data.len() < 2 {
            output.push_str("(Need at least 2 data points)\n");
            return output;
        }

        // Find min and max values
        let max_value = self.data.iter().map(|(_, v)| *v).fold(0.0, f64::max);
        let min_value = self.data.iter().map(|(_, v)| *v).fold(max_value, f64::min);
        let value_range = max_value - min_value;

        // Create grid
        let mut grid = vec![vec![' '; self.width]; self.height];

        // Calculate points
        let x_step = self.width as f64 / (self.data.len() - 1) as f64;

        for (i, (_, value)) in self.data.iter().enumerate() {
            let x = (i as f64 * x_step) as usize;
            let y = if value_range > 0.0 {
                self.height - 1 - (((*value - min_value) / value_range) * (self.height - 1) as f64) as usize
            } else {
                self.height / 2
            };

            if x < self.width && y < self.height {
                grid[y][x] = '●';
            }

            // Draw connecting lines
            if i > 0 {
                let prev_value = self.data[i - 1].1;
                let prev_x = ((i - 1) as f64 * x_step) as usize;
                let prev_y = if value_range > 0.0 {
                    self.height - 1 - (((prev_value - min_value) / value_range) * (self.height - 1) as f64) as usize
                } else {
                    self.height / 2
                };

                // Simple line drawing
                let steps = (x as i32 - prev_x as i32).abs().max((y as i32 - prev_y as i32).abs()) as usize;
                for step in 1..steps {
                    let t = step as f64 / steps as f64;
                    let inter_x = (prev_x as f64 + t * (x as f64 - prev_x as f64)) as usize;
                    let inter_y = (prev_y as f64 + t * (y as f64 - prev_y as f64)) as usize;

                    if inter_x < self.width && inter_y < self.height {
                        if grid[inter_y][inter_x] == ' ' {
                            grid[inter_y][inter_x] = '·';
                        }
                    }
                }
            }
        }

        // Render grid with axis
        output.push_str(&format!("{:>6.1} ┤", max_value));
        for row in grid.iter() {
            if row == grid.first().unwrap() {
                // Top row - already has axis label
            } else if row == grid.last().unwrap() {
                output.push_str(&format!("\n{:>6.1} ┤", min_value));
            } else {
                output.push_str(&format!("\n{:>6} │", ""));
            }

            for &cell in row {
                output.push(cell);
            }
        }

        // X-axis
        output.push_str(&format!("\n{:>6} └", ""));
        output.push_str(&"─".repeat(self.width));
        output.push('\n');

        // X-axis labels (show first and last)
        if let (Some(first), Some(last)) = (self.data.first(), self.data.last()) {
            let label_spacing = self.width - first.0.len() - last.0.len();
            output.push_str(&format!(
                "{:>6} {} {} {}\n",
                "",
                first.0.yellow(),
                " ".repeat(label_spacing.saturating_sub(2)),
                last.0.yellow()
            ));
        }

        output
    }
}

/// ASCII pie chart approximation using blocks
pub struct AsciiPieChart {
    title: String,
    data: Vec<(String, f64)>,
    radius: usize,
}

impl AsciiPieChart {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            data: Vec::new(),
            radius: 8,
        }
    }

    pub fn add_slice(&mut self, label: &str, value: f64) {
        self.data.push((label.to_string(), value));
    }

    pub fn render(&self) -> String {
        let mut output = String::new();

        // Title
        output.push_str(&format!("\n{}\n", self.title.bold().cyan()));
        output.push_str(&"─".repeat(self.radius * 4));
        output.push('\n');

        if self.data.is_empty() {
            return output;
        }

        // Calculate percentages
        let total: f64 = self.data.iter().map(|(_, v)| v).sum();
        let percentages: Vec<_> = self.data.iter()
            .map(|(label, value)| (label.clone(), (value / total) * 100.0))
            .collect();

        // Create simple block representation
        let blocks = vec!['█', '▓', '▒', '░', '╬', '╪', '╫', '┼'];
        let width = 40;

        output.push_str("\n");
        for (i, (label, percentage)) in percentages.iter().enumerate() {
            let block = blocks[i % blocks.len()];
            let bar_width = ((percentage / 100.0) * width as f64) as usize;
            let bar = format!("{}", block).repeat(bar_width);

            output.push_str(&format!(
                "{:>15} │{:<40} {:.1}%\n",
                label.yellow(),
                bar.color(Self::get_color(i)),
                percentage
            ));
        }

        output
    }

    fn get_color(index: usize) -> colored::Color {
        let colors = vec![
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::Red,
            Color::BrightGreen,
            Color::BrightYellow,
        ];
        colors[index % colors.len()]
    }
}

/// Histogram for distribution visualization
pub struct AsciiHistogram {
    title: String,
    data: Vec<f64>,
    bins: usize,
    width: usize,
}

impl AsciiHistogram {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            data: Vec::new(),
            bins: 10,
            width: 50,
        }
    }

    pub fn with_bins(mut self, bins: usize) -> Self {
        self.bins = bins;
        self
    }

    pub fn add_value(&mut self, value: f64) {
        self.data.push(value);
    }

    pub fn add_values(&mut self, values: &[f64]) {
        self.data.extend_from_slice(values);
    }

    pub fn render(&self) -> String {
        let mut output = String::new();

        // Title
        output.push_str(&format!("\n{}\n", self.title.bold().cyan()));
        output.push_str(&"─".repeat(self.width + 20));
        output.push('\n');

        if self.data.is_empty() {
            return output;
        }

        // Calculate histogram
        let min = self.data.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = self.data.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let range = max - min;

        if range == 0.0 {
            output.push_str("All values are identical\n");
            return output;
        }

        let mut bins = vec![0usize; self.bins];
        let bin_width = range / self.bins as f64;

        for &value in &self.data {
            let bin = ((value - min) / bin_width).min((self.bins - 1) as f64) as usize;
            bins[bin] += 1;
        }

        let max_count = *bins.iter().max().unwrap();

        // Render histogram
        for (i, &count) in bins.iter().enumerate() {
            let bin_start = min + i as f64 * bin_width;
            let bin_end = bin_start + bin_width;
            let bar_width = if max_count > 0 {
                (count as f64 / max_count as f64 * self.width as f64) as usize
            } else {
                0
            };

            let bar = "█".repeat(bar_width);
            let padding = " ".repeat(self.width - bar_width);

            output.push_str(&format!(
                "{:>6.1}-{:<6.1} │{}{} {}\n",
                bin_start,
                bin_end,
                bar.green(),
                padding,
                count
            ));
        }

        output
    }
}

/// Create a reduction summary chart
pub fn create_reduction_summary_chart(
    original_count: usize,
    reference_count: usize,
    delta_count: usize,
    coverage: f64,
) -> String {
    let mut chart = AsciiBarChart::new("== Reduction Summary ==");

    let original = 100.0;
    let ref_percent = (reference_count as f64 / original_count as f64) * 100.0;
    let delta_percent = (delta_count as f64 / original_count as f64) * 100.0;

    chart.add_bar("Original", original);
    chart.add_bar("References", ref_percent);
    chart.add_bar("Deltas", delta_percent);
    chart.add_bar("Coverage", coverage);

    chart.render()
}

/// Create a taxonomic distribution chart
pub fn create_taxonomy_distribution_chart(taxonomy_counts: &HashMap<String, usize>) -> String {
    let mut chart = AsciiPieChart::new("== Taxonomic Distribution ==");

    // Get top 8 taxa
    let mut sorted_taxa: Vec<_> = taxonomy_counts.iter().collect();
    sorted_taxa.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    sorted_taxa.truncate(8);

    for (taxon, count) in sorted_taxa {
        chart.add_slice(taxon, *count as f64);
    }

    chart.render()
}

/// Create a sequence length distribution histogram
pub fn create_length_histogram(lengths: &[usize]) -> String {
    let mut histogram = AsciiHistogram::new("== Sequence Length Distribution ==")
        .with_bins(10);

    for &length in lengths {
        histogram.add_value(length as f64);
    }

    histogram.render()
}

/// Create coverage progression chart
pub fn create_coverage_chart(coverage_history: &[(usize, f64)]) -> String {
    let mut chart = AsciiLineChart::new("📈 Coverage Progression");

    for (refs, coverage) in coverage_history {
        chart.add_point(&format!("{}", refs), *coverage);
    }

    chart.render()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_chart() {
        let mut chart = AsciiBarChart::new("Test Chart");
        chart.add_bar("Item 1", 75.0);
        chart.add_bar("Item 2", 50.0);
        chart.add_bar("Item 3", 100.0);

        let output = chart.render();
        assert!(output.contains("Test Chart"));
        assert!(output.contains("Item 1"));
        assert!(output.contains("75.0%"));
    }

    #[test]
    fn test_histogram() {
        let mut histogram = AsciiHistogram::new("Test Histogram");
        histogram.add_values(&[1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 5.0]);

        let output = histogram.render();
        assert!(output.contains("Test Histogram"));
    }

    #[test]
    fn test_line_chart() {
        let mut chart = AsciiLineChart::new("Test Line Chart");
        chart.add_point("Start", 10.0);
        chart.add_point("Middle", 50.0);
        chart.add_point("End", 30.0);

        let output = chart.render();
        assert!(output.contains("Test Line Chart"));
    }
}