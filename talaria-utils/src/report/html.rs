#![allow(dead_code)]

use super::ComparisonResult;
use super::ReportOptions;
use anyhow::Result;

pub fn generate_html_report(result: &ComparisonResult, options: &ReportOptions) -> Result<String> {
    let mut html = String::new();

    html.push_str(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Database Comparison Report</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 20px; background: #f5f5f5; }
        .container { max-width: 1200px; margin: 0 auto; background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
        h1 { color: #333; border-bottom: 3px solid #4CAF50; padding-bottom: 10px; }
        h2 { color: #555; margin-top: 30px; }
        .summary { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 20px; margin: 20px 0; }
        .stat-card { background: #f9f9f9; padding: 15px; border-radius: 6px; border-left: 4px solid #4CAF50; }
        .stat-card h3 { margin: 0 0 10px 0; color: #666; font-size: 14px; text-transform: uppercase; }
        .stat-card .value { font-size: 24px; font-weight: bold; color: #333; }
        .added { border-left-color: #4CAF50; }
        .removed { border-left-color: #f44336; }
        .modified { border-left-color: #FF9800; }
        .unchanged { border-left-color: #2196F3; }
        table { width: 100%; border-collapse: collapse; margin: 20px 0; }
        th, td { padding: 12px; text-align: left; border-bottom: 1px solid #ddd; }
        th { background: #f5f5f5; font-weight: 600; }
        tr:hover { background: #f9f9f9; }
        .chart-container { margin: 30px 0; padding: 20px; background: #f9f9f9; border-radius: 6px; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Database Comparison Report</h1>
        <p><strong>Generated:</strong> "#);

    html.push_str(&chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

    html.push_str(
        r#"</p>
        <h2>Databases Compared</h2>
        <table>
            <tr>
                <th>Version</th>
                <th>Path</th>
                <th>Sequences</th>
            </tr>
            <tr>
                <td>Old</td>
                <td>"#,
    );

    html.push_str(&format!("{}", result.old_path.display()));
    html.push_str(&format!("</td><td>{}</td></tr>", result.old_count));

    html.push_str(
        r#"<tr>
                <td>New</td>
                <td>"#,
    );

    html.push_str(&format!("{}", result.new_path.display()));
    html.push_str(&format!("</td><td>{}</td></tr>", result.new_count));

    html.push_str(
        r#"
        </table>
        
        <h2>Summary</h2>
        <div class="summary">
            <div class="stat-card added">
                <h3>Added</h3>
                <div class="value">"#,
    );

    html.push_str(&format!("{}", result.added.len()));

    html.push_str(
        r#"</div>
            </div>
            <div class="stat-card removed">
                <h3>Removed</h3>
                <div class="value">"#,
    );

    html.push_str(&format!("{}", result.removed.len()));

    html.push_str(
        r#"</div>
            </div>
            <div class="stat-card modified">
                <h3>Modified</h3>
                <div class="value">"#,
    );

    html.push_str(&format!("{}", result.modified.len()));

    html.push_str(
        r#"</div>
            </div>
            <div class="stat-card unchanged">
                <h3>Unchanged</h3>
                <div class="value">"#,
    );

    html.push_str(&format!("{}", result.unchanged_count));

    html.push_str(
        r#"</div>
            </div>
        </div>"#,
    );

    if options.include_visuals {
        html.push_str(
            r#"
        <div class="chart-container">
            <canvas id="changesChart"></canvas>
        </div>
        
        <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
        <script>
            const ctx = document.getElementById('changesChart').getContext('2d');
            new Chart(ctx, {
                type: 'doughnut',
                data: {
                    labels: ['Added', 'Removed', 'Modified', 'Unchanged'],
                    datasets: [{
                        data: ["#,
        );

        html.push_str(&format!(
            "{}, {}, {}, {}",
            result.added.len(),
            result.removed.len(),
            result.modified.len(),
            result.unchanged_count
        ));

        html.push_str(
            r#"],
                        backgroundColor: ['#4CAF50', '#f44336', '#FF9800', '#2196F3']
                    }]
                },
                options: {
                    responsive: true,
                    maintainAspectRatio: false,
                    plugins: {
                        title: {
                            display: true,
                            text: 'Sequence Changes Distribution'
                        }
                    }
                }
            });
        </script>"#,
        );
    }

    if options.include_details && !result.added.is_empty() {
        html.push_str(
            r#"
        <h2>Added Sequences (Top 20)</h2>
        <table>
            <tr>
                <th>ID</th>
                <th>Length</th>
                <th>Description</th>
            </tr>"#,
        );

        for seq in result.added.iter().take(20) {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                seq.id,
                seq.length,
                seq.description.as_deref().unwrap_or("")
            ));
        }

        html.push_str("</table>");
    }

    html.push_str(
        r#"
    </div>
</body>
</html>"#,
    );

    Ok(html)
}
