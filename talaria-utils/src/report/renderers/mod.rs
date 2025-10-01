/// Generic renderers for Report types
///
/// These renderers work with the generic `Report` type and can render
/// reports from any command that implements `Reportable`.

pub mod html;
pub mod json;
pub mod text;
pub mod csv;

pub use html::render_html;
pub use json::render_json;
pub use text::render_text;
pub use csv::render_csv;
