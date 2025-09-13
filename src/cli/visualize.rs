use colored::*;
use std::collections::HashMap;

/// Create an ASCII histogram
pub fn ascii_histogram(data: &[(String, usize)], width: usize, use_color: bool) -> String {
    let mut output = String::new();
    
    if data.is_empty() {
        return output;
    }
    
    let max_value = data.iter().map(|(_, v)| *v).max().unwrap_or(1);
    let max_label_len = data.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
    
    for (label, value) in data {
        let percentage = (*value as f64 / max_value as f64) * 100.0;
        let bar_width = ((percentage / 100.0) * width as f64) as usize;
        
        let bar = if use_color {
            match percentage as u32 {
                0..=25 => "█".repeat(bar_width).red().to_string(),
                26..=50 => "█".repeat(bar_width).yellow().to_string(),
                51..=75 => "█".repeat(bar_width).blue().to_string(),
                _ => "█".repeat(bar_width).green().to_string(),
            }
        } else {
            "█".repeat(bar_width)
        };
        
        let empty = "░".repeat(width - bar_width);
        
        output.push_str(&format!(
            "{:>width$} {} {:>6} ({:5.1}%)\n",
            label,
            format!("{}{}", bar, empty),
            value,
            percentage,
            width = max_label_len
        ));
    }
    
    output
}

/// Create a sparkline chart
pub fn sparkline(data: &[f64], width: usize) -> String {
    if data.is_empty() {
        return String::new();
    }
    
    let sparks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let min = data.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let max = data.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    let range = max - min;
    
    if range == 0.0 {
        return sparks[4].to_string().repeat(width.min(data.len()));
    }
    
    let step = data.len() as f64 / width as f64;
    let mut result = String::new();
    
    for i in 0..width.min(data.len()) {
        let idx = (i as f64 * step) as usize;
        if idx < data.len() {
            let normalized = (data[idx] - min) / range;
            let spark_idx = ((normalized * 7.0) as usize).min(7);
            result.push(sparks[spark_idx]);
        }
    }
    
    result
}

/// Create a progress bar
pub fn progress_bar(value: f64, max: f64, width: usize, label: &str, use_color: bool) -> String {
    let percentage = (value / max * 100.0).min(100.0).max(0.0);
    let filled = ((percentage / 100.0) * width as f64) as usize;
    let empty = width.saturating_sub(filled);
    
    let bar = if use_color {
        let filled_str = "█".repeat(filled);
        let color_bar = match percentage as u32 {
            0..=25 => filled_str.red(),
            26..=50 => filled_str.yellow(),
            51..=75 => filled_str.blue(),
            _ => filled_str.green(),
        };
        format!("{}{}", color_bar, "░".repeat(empty))
    } else {
        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    };
    
    format!("{:<15} {} {:5.1}%", label, bar, percentage)
}

/// Create a box plot representation
pub fn box_plot(
    min: f64,
    q1: f64,
    median: f64,
    q3: f64,
    max: f64,
    width: usize,
    use_color: bool,
) -> String {
    let range = max - min;
    if range == 0.0 {
        return "─".repeat(width);
    }
    
    let scale = |v: f64| ((v - min) / range * width as f64) as usize;
    
    let min_pos = 0;
    let q1_pos = scale(q1);
    let median_pos = scale(median);
    let q3_pos = scale(q3);
    let max_pos = width - 1;
    
    let mut plot = vec![' '; width];
    
    // Draw whiskers
    for i in min_pos..=q1_pos {
        plot[i] = '─';
    }
    for i in q3_pos..=max_pos {
        plot[i] = '─';
    }
    
    // Draw box
    for i in q1_pos..=q3_pos {
        plot[i] = '█';
    }
    
    // Mark quartiles
    plot[min_pos] = '├';
    plot[q1_pos] = '┤';
    if median_pos < width {
        plot[median_pos] = '│';
    }
    plot[q3_pos] = '├';
    plot[max_pos.min(width - 1)] = '┤';
    
    let result: String = plot.into_iter().collect();
    
    if use_color {
        result.cyan().to_string()
    } else {
        result
    }
}

/// Create a simple heat map
pub fn heat_map(data: &[Vec<f64>], width: usize, height: usize, use_color: bool) -> String {
    if data.is_empty() || data[0].is_empty() {
        return String::new();
    }
    
    let blocks = [' ', '░', '▒', '▓', '█'];
    let mut output = String::new();
    
    // Find min and max for normalization
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for row in data {
        for &val in row {
            min = min.min(val);
            max = max.max(val);
        }
    }
    
    let range = max - min;
    if range == 0.0 {
        return blocks[2].to_string().repeat(width * height);
    }
    
    // Sample data to fit dimensions
    let row_step = data.len() as f64 / height as f64;
    let col_step = data[0].len() as f64 / width as f64;
    
    for i in 0..height {
        let row_idx = (i as f64 * row_step) as usize;
        if row_idx >= data.len() {
            break;
        }
        
        for j in 0..width {
            let col_idx = (j as f64 * col_step) as usize;
            if col_idx >= data[row_idx].len() {
                break;
            }
            
            let normalized = (data[row_idx][col_idx] - min) / range;
            let block_idx = ((normalized * 4.0) as usize).min(4);
            
            if use_color {
                let ch = blocks[block_idx];
                let colored = match block_idx {
                    0 => ch.to_string().blue(),
                    1 => ch.to_string().cyan(),
                    2 => ch.to_string().yellow(),
                    3 => ch.to_string().magenta(),
                    _ => ch.to_string().red(),
                };
                output.push_str(&colored.to_string());
            } else {
                output.push(blocks[block_idx]);
            }
        }
        output.push('\n');
    }
    
    output
}

/// Create a distribution chart
pub fn distribution_chart(
    data: &HashMap<String, f64>,
    width: usize,
    use_color: bool,
) -> String {
    let mut sorted: Vec<_> = data.iter().collect();
    sorted.sort_by_key(|(k, _)| k.as_str());
    
    let converted: Vec<(String, usize)> = sorted
        .into_iter()
        .map(|(k, v)| (k.clone(), (*v * 100.0) as usize))
        .collect();
    
    ascii_histogram(&converted, width, use_color)
}

/// Format a number with thousands separators
pub fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;
    
    for ch in s.chars().rev() {
        if count == 3 {
            result.push(',');
            count = 0;
        }
        result.push(ch);
        count += 1;
    }
    
    result.chars().rev().collect()
}

/// Create a simple ASCII table
pub fn ascii_table(headers: Vec<&str>, rows: Vec<Vec<String>>) -> String {
    use comfy_table::{Table, presets::UTF8_FULL};
    
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(headers);
    
    for row in rows {
        table.add_row(row);
    }
    
    table.to_string()
}

/// Create comparison bars for two values
pub fn comparison_bars(
    label1: &str,
    value1: f64,
    label2: &str,
    value2: f64,
    width: usize,
    use_color: bool,
) -> String {
    let max_val = value1.max(value2);
    let bar1 = progress_bar(value1, max_val, width / 2, label1, use_color);
    let bar2 = progress_bar(value2, max_val, width / 2, label2, use_color);
    
    format!("{}\n{}", bar1, bar2)
}