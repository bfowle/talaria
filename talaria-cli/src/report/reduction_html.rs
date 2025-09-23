#![allow(dead_code)]

use talaria_bio::Sequence;
use crate::core::reference_selector::SelectionResult;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

/// Generate an HTML report for reduction results
pub fn generate_reduction_html_report(
    input_path: &Path,
    output_path: &Path,
    original_sequences: &[Sequence],
    selection_result: &SelectionResult,
    coverage_percent: f64,
    taxonomic_stats: Option<&HashMap<String, TaxonomicReductionStats>>,
) -> Result<String> {
    let mut html = String::new();

    // Calculate statistics
    let original_count = original_sequences.len();
    let reference_count = selection_result.references.len();
    let delta_count = selection_result
        .children
        .values()
        .map(|v| v.len())
        .sum::<usize>();
    let discarded_count = selection_result.discarded.len();
    let reduction_rate = if original_count > 0 {
        ((original_count - reference_count) as f64 / original_count as f64) * 100.0
    } else {
        0.0
    };

    // Calculate size statistics
    let original_size: usize = original_sequences.iter().map(|s| s.len()).sum();
    let reference_size: usize = selection_result.references.iter().map(|s| s.len()).sum();
    let size_reduction = if original_size > 0 {
        ((original_size - reference_size) as f64 / original_size as f64) * 100.0
    } else {
        0.0
    };

    html.push_str(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Talaria Reduction Report</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            padding: 40px 20px;
        }
        .container {
            max-width: 1400px;
            margin: 0 auto;
            background: white;
            border-radius: 20px;
            box-shadow: 0 20px 60px rgba(0,0,0,0.3);
            overflow: hidden;
        }
        .header {
            background: linear-gradient(135deg, #764ba2 0%, #667eea 100%);
            color: white;
            padding: 40px;
            text-align: center;
        }
        .header h1 {
            font-size: 2.5rem;
            margin-bottom: 10px;
            font-weight: 300;
            letter-spacing: -1px;
        }
        .header .subtitle {
            opacity: 0.9;
            font-size: 1.1rem;
        }
        .content {
            padding: 40px;
        }

        /* Summary Cards */
        .summary-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 20px;
            margin-bottom: 40px;
        }
        .metric-card {
            background: linear-gradient(135deg, #f5f7fa 0%, #c3cfe2 100%);
            border-radius: 15px;
            padding: 25px;
            position: relative;
            overflow: hidden;
        }
        .metric-card::before {
            content: '';
            position: absolute;
            top: 0;
            left: 0;
            width: 4px;
            height: 100%;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
        }
        .metric-card.primary {
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
        }
        .metric-card h3 {
            font-size: 0.9rem;
            text-transform: uppercase;
            letter-spacing: 1px;
            margin-bottom: 10px;
            opacity: 0.7;
        }
        .metric-card .value {
            font-size: 2.5rem;
            font-weight: 700;
            line-height: 1;
        }
        .metric-card .unit {
            font-size: 0.9rem;
            opacity: 0.7;
            margin-left: 5px;
        }
        .metric-card .change {
            margin-top: 10px;
            font-size: 0.9rem;
            opacity: 0.8;
        }

        /* Charts Section */
        .charts-section {
            margin: 40px 0;
        }
        .section-title {
            font-size: 1.8rem;
            color: #333;
            margin-bottom: 25px;
            padding-bottom: 10px;
            border-bottom: 2px solid #e0e0e0;
        }
        .charts-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(400px, 1fr));
            gap: 30px;
            margin-bottom: 30px;
        }
        .chart-container {
            background: #f9f9f9;
            border-radius: 15px;
            padding: 25px;
            box-shadow: 0 4px 6px rgba(0,0,0,0.07);
            position: relative;
            height: 400px; /* Fixed height for charts */
        }
        .chart-container h4 {
            margin-bottom: 20px;
            color: #555;
            font-weight: 500;
        }
        .chart-wrapper {
            position: relative;
            height: 350px; /* Fixed height for chart wrapper */
        }
        canvas {
            max-width: 100%;
            max-height: 100%;
        }

        /* Details Table */
        .table-container {
            margin: 40px 0;
            overflow-x: auto;
        }
        table {
            width: 100%;
            border-collapse: collapse;
            background: white;
            box-shadow: 0 2px 4px rgba(0,0,0,0.05);
            border-radius: 10px;
            overflow: hidden;
        }
        th, td {
            padding: 15px;
            text-align: left;
        }
        th {
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            font-weight: 500;
            text-transform: uppercase;
            font-size: 0.85rem;
            letter-spacing: 0.5px;
        }
        tr:nth-child(even) {
            background: #f8f9fa;
        }
        tr:hover {
            background: #e9ecef;
            transition: background 0.3s;
        }

        /* Progress Bar */
        .progress-bar {
            height: 30px;
            background: #e0e0e0;
            border-radius: 15px;
            overflow: hidden;
            margin: 20px 0;
        }
        .progress-fill {
            height: 100%;
            background: linear-gradient(90deg, #667eea 0%, #764ba2 100%);
            border-radius: 15px;
            display: flex;
            align-items: center;
            padding: 0 15px;
            color: white;
            font-weight: 500;
            transition: width 1s ease-out;
        }

        /* Footer */
        .footer {
            background: #f5f7fa;
            padding: 30px 40px;
            text-align: center;
            color: #666;
            font-size: 0.9rem;
        }
        .footer a {
            color: #667eea;
            text-decoration: none;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>Talaria Reduction Report</h1>
            <div class="subtitle">Sequence Database Reduction Analysis</div>
        </div>

        <div class="content">
            <!-- File Information -->
            <div style="background: #f0f4f8; border-radius: 10px; padding: 20px; margin-bottom: 30px;">
                <p><strong>Input:</strong> <code>"#);

    html.push_str(&input_path.display().to_string());
    html.push_str(
        r#"</code></p>
                <p><strong>Output:</strong> <code>"#,
    );

    html.push_str(&output_path.display().to_string());
    html.push_str(
        r#"</code></p>
                <p><strong>Generated:</strong> "#,
    );

    html.push_str(&chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

    html.push_str(
        r#"</p>
            </div>

            <!-- Summary Metrics -->
            <div class="summary-grid">
                <div class="metric-card primary">
                    <h3>Reduction Rate</h3>
                    <div class="value">"#,
    );

    html.push_str(&format!("{:.1}", reduction_rate));
    html.push_str(
        r#"<span class="unit">%</span></div>
                    <div class="change">↓ "#,
    );
    html.push_str(&format!("{}", original_count - reference_count));
    html.push_str(
        r#" sequences</div>
                </div>

                <div class="metric-card">
                    <h3>Original Sequences</h3>
                    <div class="value">"#,
    );
    html.push_str(&format_number(original_count));
    html.push_str(
        r#"</div>
                    <div class="change">"#,
    );
    html.push_str(&format_size(original_size));
    html.push_str(
        r#" total</div>
                </div>

                <div class="metric-card">
                    <h3>References Selected</h3>
                    <div class="value">"#,
    );
    html.push_str(&format_number(reference_count));
    html.push_str(
        r#"</div>
                    <div class="change">"#,
    );
    html.push_str(&format_size(reference_size));
    html.push_str(
        r#" total</div>
                </div>

                <div class="metric-card">
                    <h3>Coverage</h3>
                    <div class="value">"#,
    );
    html.push_str(&format!("{:.1}", coverage_percent));
    html.push_str(
        r#"<span class="unit">%</span></div>
                    <div class="change">"#,
    );
    html.push_str(&format!("{} deltas", delta_count));
    html.push_str(
        r#"</div>
                </div>
            </div>

            <!-- Progress Bar -->
            <div style="margin: 30px 0;">
                <h4 style="margin-bottom: 10px; color: #666;">Size Reduction</h4>
                <div class="progress-bar">
                    <div class="progress-fill" style="width: "#,
    );
    html.push_str(&format!("{:.1}%", size_reduction));
    html.push_str(
        r#";">
                        "#,
    );
    html.push_str(&format!("{:.1}% size reduced", size_reduction));
    html.push_str(
        r#"
                    </div>
                </div>
            </div>

            <!-- Charts Section -->
            <div class="charts-section">
                <h2 class="section-title">Visual Analysis</h2>
                <div class="charts-grid">
                    <div class="chart-container">
                        <h4>Sequence Distribution</h4>
                        <div class="chart-wrapper">
                            <canvas id="distributionChart"></canvas>
                        </div>
                    </div>
                    <div class="chart-container">
                        <h4>Size Comparison</h4>
                        <div class="chart-wrapper">
                            <canvas id="sizeChart"></canvas>
                        </div>
                    </div>
                </div>"#,
    );

    // Add taxonomic breakdown if available
    if let Some(stats) = taxonomic_stats {
        if !stats.is_empty() {
            html.push_str(
                r#"
                <div class="chart-container" style="margin-top: 30px; height: 400px;">
                    <h4>Taxonomic Breakdown</h4>
                    <div class="chart-wrapper">
                        <canvas id="taxonomyChart"></canvas>
                    </div>
                </div>"#,
            );
        }
    }

    html.push_str(
        r#"
            </div>

            <!-- Sequence Length Distribution -->
            <div class="charts-section">
                <h2 class="section-title">Length Distribution</h2>
                <div class="chart-container">
                    <div class="chart-wrapper">
                        <canvas id="lengthHistogram"></canvas>
                    </div>
                </div>
            </div>"#,
    );

    // Add top sequences table
    if !selection_result.references.is_empty() {
        html.push_str(
            r#"
            <!-- Top Reference Sequences -->
            <div class="table-container">
                <h2 class="section-title">Top Reference Sequences</h2>
                <table>
                    <thead>
                        <tr>
                            <th>Sequence ID</th>
                            <th>Length</th>
                            <th>Children</th>
                            <th>Description</th>
                        </tr>
                    </thead>
                    <tbody>"#,
        );

        let mut sorted_refs: Vec<_> = selection_result
            .references
            .iter()
            .map(|seq| {
                let children_count = selection_result
                    .children
                    .get(&seq.id)
                    .map(|v| v.len())
                    .unwrap_or(0);
                (seq, children_count)
            })
            .collect();
        sorted_refs.sort_by(|a, b| b.1.cmp(&a.1));

        for (seq, children_count) in sorted_refs.iter().take(20) {
            html.push_str(&format!(
                r#"
                        <tr>
                            <td><code>{}</code></td>
                            <td>{}</td>
                            <td>{}</td>
                            <td>{}</td>
                        </tr>"#,
                seq.id,
                seq.len(),
                children_count,
                seq.description.as_deref().unwrap_or("-")
            ));
        }

        html.push_str(
            r#"
                    </tbody>
                </table>
            </div>"#,
        );
    }

    html.push_str(
        r#"
        </div>

        <div class="footer">
            <p>Generated by <a href="https://github.com/yourusername/talaria">Talaria</a> •
            Fast and efficient sequence database reduction</p>
        </div>
    </div>

    <!-- Chart.js Scripts -->
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
    <script>
        // Distribution Chart
        const distributionCtx = document.getElementById('distributionChart').getContext('2d');
        new Chart(distributionCtx, {
            type: 'doughnut',
            data: {
                labels: ['References', 'Deltas', 'Discarded'],
                datasets: [{
                    data: ["#,
    );

    html.push_str(&format!(
        "{}, {}, {}",
        reference_count, delta_count, discarded_count
    ));

    html.push_str(
        r#"],
                    backgroundColor: [
                        'rgba(102, 126, 234, 0.8)',
                        'rgba(118, 75, 162, 0.8)',
                        'rgba(237, 100, 166, 0.8)'
                    ],
                    borderWidth: 0
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: true,
                aspectRatio: 1.5,
                plugins: {
                    legend: {
                        position: 'bottom',
                        labels: {
                            padding: 20
                        }
                    },
                    tooltip: {
                        callbacks: {
                            label: function(context) {
                                const label = context.label || '';
                                const value = context.parsed || 0;
                                const total = context.dataset.data.reduce((a, b) => a + b, 0);
                                const percentage = ((value / total) * 100).toFixed(1);
                                return `${label}: ${value} (${percentage}%)`;
                            }
                        }
                    }
                }
            }
        });

        // Size Chart
        const sizeCtx = document.getElementById('sizeChart').getContext('2d');
"#,
    );

    // Dynamically choose unit based on file size
    let max_size = original_size.max(reference_size);
    let (unit, divisor) = if max_size < 1024 {
        ("B", 1.0)
    } else if max_size < 1024 * 1024 {
        ("KB", 1024.0)
    } else {
        ("MB", 1_048_576.0)
    };

    // Ensure minimum visibility - if value would be 0.00, show 0.01
    let orig_val = (original_size as f64 / divisor).max(if original_size > 0 { 0.01 } else { 0.0 });
    let ref_val =
        (reference_size as f64 / divisor).max(if reference_size > 0 { 0.01 } else { 0.0 });

    html.push_str(&format!(
        r#"        new Chart(sizeCtx, {{
            type: 'bar',
            data: {{
                labels: ['Original', 'References'],
                datasets: [{{
                    label: 'Size ({})',
                    data: [{:.2}, {:.2}],"#,
        unit, orig_val, ref_val
    ));

    html.push_str(
        r#"
                    backgroundColor: [
                        'rgba(102, 126, 234, 0.8)',
                        'rgba(118, 75, 162, 0.8)'
                    ],
                    borderWidth: 0
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: true,
                aspectRatio: 2,
                scales: {
                    y: {
                        beginAtZero: true
                    }
                },
                plugins: {
                    legend: {
                        display: false
                    }
                }
            }
        });

        // Length Histogram
        const lengthCtx = document.getElementById('lengthHistogram').getContext('2d');
        const lengths = ["#,
    );

    // Create histogram data
    let length_buckets = create_length_histogram(&selection_result.references, 20);
    for (i, bucket) in length_buckets.iter().enumerate() {
        if i > 0 {
            html.push_str(", ");
        }
        html.push_str(&bucket.count.to_string());
    }

    html.push_str(
        r#"];
        const labels = ["#,
    );

    for (i, bucket) in length_buckets.iter().enumerate() {
        if i > 0 {
            html.push_str(", ");
        }
        html.push_str(&format!("'{}-{}'", bucket.min, bucket.max));
    }

    html.push_str(
        r#"];
        new Chart(lengthCtx, {
            type: 'bar',
            data: {
                labels: labels,
                datasets: [{
                    label: 'Number of Sequences',
                    data: lengths,
                    backgroundColor: 'rgba(102, 126, 234, 0.8)',
                    borderWidth: 0
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: true,
                aspectRatio: 2,
                scales: {
                    y: {
                        beginAtZero: true
                    },
                    x: {
                        ticks: {
                            maxRotation: 45,
                            minRotation: 45
                        }
                    }
                },
                plugins: {
                    legend: {
                        display: false
                    }
                }
            }
        });"#,
    );

    // Add taxonomic chart if data is available
    if let Some(stats) = taxonomic_stats {
        if !stats.is_empty() {
            html.push_str(
                r#"

        // Taxonomic Chart
        const taxonomyCtx = document.getElementById('taxonomyChart').getContext('2d');
        const taxonLabels = ["#,
            );

            let mut sorted_taxa: Vec<_> = stats.iter().collect();
            sorted_taxa.sort_by(|a, b| b.1.original_count.cmp(&a.1.original_count));

            for (i, (name, _)) in sorted_taxa.iter().take(10).enumerate() {
                if i > 0 {
                    html.push_str(", ");
                }
                html.push_str(&format!("'{}'", name));
            }

            html.push_str(
                r#"];
        const originalCounts = ["#,
            );

            for (i, (_, stat)) in sorted_taxa.iter().take(10).enumerate() {
                if i > 0 {
                    html.push_str(", ");
                }
                html.push_str(&stat.original_count.to_string());
            }

            html.push_str(
                r#"];
        const referenceCounts = ["#,
            );

            for (i, (_, stat)) in sorted_taxa.iter().take(10).enumerate() {
                if i > 0 {
                    html.push_str(", ");
                }
                html.push_str(&stat.reference_count.to_string());
            }

            html.push_str(
                r#"];
        new Chart(taxonomyCtx, {
            type: 'bar',
            data: {
                labels: taxonLabels,
                datasets: [{
                    label: 'Original',
                    data: originalCounts,
                    backgroundColor: 'rgba(102, 126, 234, 0.6)'
                }, {
                    label: 'References',
                    data: referenceCounts,
                    backgroundColor: 'rgba(118, 75, 162, 0.8)'
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: true,
                aspectRatio: 2,
                scales: {
                    y: {
                        beginAtZero: true
                    },
                    x: {
                        ticks: {
                            maxRotation: 45,
                            minRotation: 45
                        }
                    }
                }
            }
        });"#,
            );
        }
    }

    html.push_str(
        r#"
    </script>
</body>
</html>"#,
    );

    Ok(html)
}

/// Statistics for taxonomic reduction
#[derive(Debug, Clone)]
pub struct TaxonomicReductionStats {
    pub original_count: usize,
    pub reference_count: usize,
    pub reduction_rate: f64,
}

/// Helper function to format numbers with commas
fn format_number(n: usize) -> String {
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

/// Helper function to format byte sizes
fn format_size(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", size as usize, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

/// Create histogram buckets for sequence lengths
#[derive(Debug)]
struct HistogramBucket {
    min: usize,
    max: usize,
    count: usize,
}

fn create_length_histogram(sequences: &[Sequence], num_buckets: usize) -> Vec<HistogramBucket> {
    if sequences.is_empty() {
        return vec![];
    }

    let lengths: Vec<usize> = sequences.iter().map(|s| s.len()).collect();
    let min_len = *lengths.iter().min().unwrap_or(&0);
    let max_len = *lengths.iter().max().unwrap_or(&0);

    if min_len == max_len {
        return vec![HistogramBucket {
            min: min_len,
            max: max_len,
            count: sequences.len(),
        }];
    }

    let range = max_len - min_len;
    let bucket_size = range.div_ceil(num_buckets); // Round up

    let mut buckets = Vec::new();
    for i in 0..num_buckets {
        let bucket_min = min_len + i * bucket_size;
        let bucket_max = if i == num_buckets - 1 {
            max_len
        } else {
            min_len + (i + 1) * bucket_size - 1
        };

        let count = lengths
            .iter()
            .filter(|&&len| len >= bucket_min && len <= bucket_max)
            .count();

        if count > 0 {
            buckets.push(HistogramBucket {
                min: bucket_min,
                max: bucket_max,
                count,
            });
        }
    }

    buckets
}
