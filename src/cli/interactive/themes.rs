use crossterm::style::Attribute;
use ratatui::style::{Color, Modifier, Style};
use termimad::MadSkin;

pub struct TalariaTheme {
    pub primary: Color,
    pub secondary: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    pub text: Color,
    pub dim_text: Color,
    pub background: Color,
    pub highlight: Color,
}

impl Default for TalariaTheme {
    fn default() -> Self {
        TalariaTheme {
            primary: Color::Cyan,
            secondary: Color::Blue,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            info: Color::Magenta,
            text: Color::White,
            dim_text: Color::Gray,
            background: Color::Black,
            highlight: Color::LightCyan,
        }
    }
}

impl TalariaTheme {
    pub fn title_style(&self) -> Style {
        Style::default()
            .fg(self.primary)
            .add_modifier(Modifier::BOLD)
    }

    pub fn header_style(&self) -> Style {
        Style::default()
            .fg(self.secondary)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    }

    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error).add_modifier(Modifier::BOLD)
    }

    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    pub fn info_style(&self) -> Style {
        Style::default().fg(self.info)
    }

    pub fn selected_style(&self) -> Style {
        Style::default()
            .bg(self.highlight)
            .fg(self.background)
            .add_modifier(Modifier::BOLD)
    }

    pub fn code_style(&self) -> Style {
        Style::default()
            .fg(Color::Rgb(255, 150, 150))
            .add_modifier(Modifier::ITALIC)
    }
}

pub fn get_markdown_skin() -> MadSkin {
    let mut skin = MadSkin::default();

    // Headers
    skin.set_headers_fg(termimad::rgb(100, 200, 255));
    skin.headers[0].add_attr(Attribute::Bold);
    skin.headers[1].add_attr(Attribute::Bold);
    skin.headers[2].add_attr(Attribute::Underlined);

    // Bold text
    skin.bold.set_fg(termimad::rgb(255, 255, 255));
    skin.bold.add_attr(Attribute::Bold);

    // Italic text
    skin.italic.add_attr(Attribute::Italic);

    // Code blocks
    skin.code_block.set_fg(termimad::rgb(255, 150, 150));
    skin.inline_code.set_fg(termimad::rgb(255, 180, 180));
    skin.inline_code.set_bg(termimad::rgb(40, 40, 40));

    // Tables
    skin.table.set_fg(termimad::rgb(200, 200, 200));

    // Lists
    skin.bullet.set_fg(termimad::rgb(150, 255, 150));

    skin
}

pub fn format_success(text: &str) -> String {
    format!("● {}", text)
}

pub fn format_error(text: &str) -> String {
    format!("■ {}", text)
}

pub fn format_warning(text: &str) -> String {
    format!("[!] {}", text)
}

pub fn format_info(text: &str) -> String {
    format!("▶ {}", text)
}

pub fn format_progress(current: usize, total: usize) -> String {
    let percentage = if total > 0 {
        (current * 100) / total
    } else {
        0
    };

    let filled = (percentage / 5) as usize;
    let empty = 20 - filled;

    format!(
        "[{}{}] {}%",
        "█".repeat(filled),
        "░".repeat(empty),
        percentage
    )
}
