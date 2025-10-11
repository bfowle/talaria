/// Report generation utilities
///
/// Provides a generic reporting framework that works across all Talaria commands.
///
/// ## Generic Framework
/// - `Report`, `Reportable` trait - work with any command
/// - Generic renderers in `renderers/` module
///
/// ## Usage
/// 1. Implement `Reportable` for your command's result type
/// 2. Call `result.to_report()` to get a generic `Report`
/// 3. Use renderers (`render_html`, `render_json`, etc.) to output in desired format
pub mod core;
pub mod renderers;
pub mod types;

// Re-export core generic types
pub use core::{
    Cell, CellStyle, ChangeDirection, ChartData, ChartType, Dataset, Metric, MetricChange,
    MetricSeverity, Report, ReportBuilder, Reportable, Section, SectionContent, Table,
};

// Re-export specific types
pub use types::*;

// Re-export generic renderers
pub use renderers::{render_csv, render_html, render_json, render_text};
