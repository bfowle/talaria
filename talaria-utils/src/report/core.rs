/// Core generic reporting framework
///
/// Provides trait-based reporting that works across all commands.
/// Commands implement `Reportable` to convert their results into generic `Report` structures.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generic report that any command can produce
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub title: String,
    pub command: String,
    pub timestamp: DateTime<Utc>,
    pub sections: Vec<Section>,
    pub metadata: HashMap<String, String>,
}

impl Report {
    pub fn builder(title: impl Into<String>, command: impl Into<String>) -> ReportBuilder {
        ReportBuilder {
            title: title.into(),
            command: command.into(),
            timestamp: Utc::now(),
            sections: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Builder for constructing reports
pub struct ReportBuilder {
    title: String,
    command: String,
    timestamp: DateTime<Utc>,
    sections: Vec<Section>,
    metadata: HashMap<String, String>,
}

impl ReportBuilder {
    pub fn section(mut self, section: Section) -> Self {
        self.sections.push(section);
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> Report {
        Report {
            title: self.title,
            command: self.command,
            timestamp: self.timestamp,
            sections: self.sections,
            metadata: self.metadata,
        }
    }
}

/// A section of a report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub title: String,
    pub content: SectionContent,
}

impl Section {
    pub fn summary(title: impl Into<String>, metrics: Vec<Metric>) -> Self {
        Self {
            title: title.into(),
            content: SectionContent::Metrics(metrics),
        }
    }

    pub fn table(title: impl Into<String>, table: Table) -> Self {
        Self {
            title: title.into(),
            content: SectionContent::Table(table),
        }
    }

    pub fn key_value(title: impl Into<String>, items: Vec<(String, String)>) -> Self {
        Self {
            title: title.into(),
            content: SectionContent::KeyValueList(items),
        }
    }

    pub fn text(title: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            content: SectionContent::Text(text.into()),
        }
    }
}

/// Content types for report sections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SectionContent {
    /// Summary metrics with optional comparison
    Metrics(Vec<Metric>),

    /// Tabular data
    Table(Table),

    /// Key-value pairs
    KeyValueList(Vec<(String, String)>),

    /// Bullet list
    BulletList(Vec<String>),

    /// Chart data
    Chart(ChartData),

    /// Plain text or markdown
    Text(String),
}

/// A metric with optional change indicator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub label: String,
    pub value: String,
    pub change: Option<MetricChange>,
    pub severity: MetricSeverity,
}

impl Metric {
    pub fn new(label: impl Into<String>, value: impl ToString) -> Self {
        Self {
            label: label.into(),
            value: value.to_string(),
            change: None,
            severity: MetricSeverity::Normal,
        }
    }

    pub fn with_change(mut self, from: impl ToString, to: impl ToString) -> Self {
        let from_val = from.to_string();
        let to_val = to.to_string();

        // Try to parse as numbers to determine direction
        let direction = if let (Ok(f), Ok(t)) = (from_val.parse::<f64>(), to_val.parse::<f64>()) {
            if t > f {
                ChangeDirection::Increase
            } else if t < f {
                ChangeDirection::Decrease
            } else {
                ChangeDirection::NoChange
            }
        } else {
            ChangeDirection::NoChange
        };

        self.change = Some(MetricChange {
            from: from_val,
            to: to_val,
            direction,
        });
        self
    }

    pub fn with_severity(mut self, severity: MetricSeverity) -> Self {
        self.severity = severity;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricChange {
    pub from: String,
    pub to: String,
    pub direction: ChangeDirection,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChangeDirection {
    Increase,
    Decrease,
    NoChange,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MetricSeverity {
    Normal,
    Success,
    Warning,
    Error,
    Info,
}

/// Table data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

impl Table {
    pub fn new(headers: Vec<String>) -> Self {
        Self {
            headers,
            rows: Vec::new(),
        }
    }

    pub fn add_row(&mut self, cells: Vec<Cell>) {
        self.rows.push(cells);
    }

    pub fn with_row(mut self, cells: Vec<Cell>) -> Self {
        self.rows.push(cells);
        self
    }
}

/// A table cell with optional styling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub value: String,
    pub style: CellStyle,
}

impl Cell {
    pub fn new(value: impl ToString) -> Self {
        Self {
            value: value.to_string(),
            style: CellStyle::Normal,
        }
    }

    pub fn with_style(mut self, style: CellStyle) -> Self {
        self.style = style;
        self
    }
}

impl From<String> for Cell {
    fn from(value: String) -> Self {
        Cell::new(value)
    }
}

impl From<&str> for Cell {
    fn from(value: &str) -> Self {
        Cell::new(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CellStyle {
    Normal,
    Success,
    Warning,
    Error,
    Highlight,
    Muted,
}

/// Chart data for visualizations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartData {
    pub chart_type: ChartType,
    pub title: String,
    pub labels: Vec<String>,
    pub datasets: Vec<Dataset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub label: String,
    pub data: Vec<f64>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChartType {
    Bar,
    Line,
    Pie,
    Doughnut,
}

/// Trait for types that can be converted to reports
pub trait Reportable {
    fn to_report(&self) -> Report;
}
