use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, ContentArrangement, Table};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap}, Terminal,
};
use std::io;
use termimad::{Area, MadSkin};

use super::themes::get_markdown_skin;

pub struct MarkdownRenderer {
    skin: MadSkin,
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        MarkdownRenderer {
            skin: get_markdown_skin(),
        }
    }
    
    pub fn render(&self, markdown: &str) -> String {
        let width = termimad::terminal_size().0 as usize;
        let area = Area::new(0, 0, width.min(120) as u16, 50);
        
        self.skin
            .area_text(markdown, &area)
            .to_string()
    }
    
    pub fn render_to_terminal(&self, markdown: &str) {
        let width = termimad::terminal_size().0 as usize;
        let area = Area::new(0, 0, width.min(120) as u16, 50);
        
        print!("{}", self.skin.area_text(markdown, &area));
    }
    
    pub fn render_inline(&self, text: &str) -> String {
        self.skin.inline(text).to_string()
    }
}

pub fn render_table(headers: Vec<&str>, rows: Vec<Vec<String>>) -> String {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);
    
    table.set_header(headers);
    
    for row in rows {
        table.add_row(row);
    }
    
    table.to_string()
}

pub fn render_code_block(language: &str, code: &str) -> String {
    let border = "─".repeat(60);
    format!(
        "┌{}┐\n│ {} │\n├{}┤\n{}\n└{}┘",
        border,
        language.to_uppercase(),
        border,
        code.lines()
            .map(|line| format!("│ {} │", line))
            .collect::<Vec<_>>()
            .join("\n"),
        border
    )
}

pub fn render_list(items: Vec<String>, ordered: bool) -> String {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            if ordered {
                format!("  {}. {}", i + 1, item)
            } else {
                format!("  • {}", item)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn render_link(text: &str, url: &str) -> String {
    format!("\x1b[4m{}\x1b[0m ({})", text, url)
}

pub fn render_bold(text: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", text)
}

pub fn render_italic(text: &str) -> String {
    format!("\x1b[3m{}\x1b[0m", text)
}

pub fn render_underline(text: &str) -> String {
    format!("\x1b[4m{}\x1b[0m", text)
}

pub fn render_strikethrough(text: &str) -> String {
    format!("\x1b[9m{}\x1b[0m", text)
}

pub fn render_color(text: &str, color: Color) -> String {
    let color_code = match color {
        Color::Black => 30,
        Color::Red => 31,
        Color::Green => 32,
        Color::Yellow => 33,
        Color::Blue => 34,
        Color::Magenta => 35,
        Color::Cyan => 36,
        Color::White => 37,
        Color::Gray => 90,
        Color::LightRed => 91,
        Color::LightGreen => 92,
        Color::LightYellow => 93,
        Color::LightBlue => 94,
        Color::LightMagenta => 95,
        Color::LightCyan => 96,
        _ => 37,
    };
    
    format!("\x1b[{}m{}\x1b[0m", color_code, text)
}

pub fn display_markdown_in_tui<B: Backend>(
    terminal: &mut Terminal<B>,
    markdown: &str,
    title: &str,
) -> io::Result<()> {
    let renderer = MarkdownRenderer::new();
    let rendered = renderer.render(markdown);
    
    terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(0)])
            .split(f.area());
        
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan));
        
        let paragraph = Paragraph::new(rendered)
            .block(block)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::White));
        
        f.render_widget(paragraph, chunks[0]);
    })?;
    
    Ok(())
}