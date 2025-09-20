use crate::core::config::{default_config, load_config, save_config, Config};
use crossterm::event::{self, Event, KeyCode};
use dirs;
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::{io, path::PathBuf};

pub struct ConfigEditor {
    config: Config,
    config_path: Option<PathBuf>,
    selected_field: usize,
    editing: bool,
    message: String,
    list_state: ListState,
    temp_value: String,
}

impl ConfigEditor {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        // Try to load existing config or use default
        let default_path = dirs::config_dir().map(|p| p.join("talaria").join("config.toml"));

        let (config, path) = if let Some(ref p) = default_path {
            if p.exists() {
                match load_config(p) {
                    Ok(c) => (c, Some(p.clone())),
                    Err(_) => (default_config(), Some(p.clone())),
                }
            } else {
                (default_config(), Some(p.clone()))
            }
        } else {
            (default_config(), None)
        };

        Self {
            config,
            config_path: path,
            selected_field: 0,
            editing: false,
            message: String::from("Press Enter to edit, 's' to save, 'q' to quit"),
            list_state,
            temp_value: String::new(),
        }
    }

    fn get_field_count(&self) -> usize {
        12 // Total number of editable fields
    }

    fn get_field_name(&self, index: usize) -> &str {
        match index {
            0 => "Target Ratio",
            1 => "Min Sequence Length",
            2 => "Max Delta Distance",
            3 => "Similarity Threshold",
            4 => "Taxonomy Aware",
            5 => "Gap Penalty",
            6 => "Gap Extension",
            7 => "Algorithm",
            8 => "Output Format",
            9 => "Include Metadata",
            10 => "Compress Output",
            11 => "Chunk Size",
            _ => "Unknown",
        }
    }

    fn get_field_value(&self, index: usize) -> String {
        match index {
            0 => format!("{:.2}", self.config.reduction.target_ratio),
            1 => self.config.reduction.min_sequence_length.to_string(),
            2 => self.config.reduction.max_delta_distance.to_string(),
            3 => format!("{:.2}", self.config.reduction.similarity_threshold),
            4 => if self.config.reduction.taxonomy_aware {
                "[✓]"
            } else {
                "[ ]"
            }
            .to_string(),
            5 => self.config.alignment.gap_penalty.to_string(),
            6 => self.config.alignment.gap_extension.to_string(),
            7 => self.config.alignment.algorithm.clone(),
            8 => self.config.output.format.clone(),
            9 => if self.config.output.include_metadata {
                "[✓]"
            } else {
                "[ ]"
            }
            .to_string(),
            10 => if self.config.output.compress_output {
                "[✓]"
            } else {
                "[ ]"
            }
            .to_string(),
            11 => self.config.performance.chunk_size.to_string(),
            _ => String::new(),
        }
    }

    fn set_field_value(&mut self, index: usize, value: &str) -> Result<(), String> {
        match index {
            0 => {
                let v: f64 = value.parse().map_err(|_| "Invalid number")?;
                if v <= 0.0 || v > 1.0 {
                    return Err("Must be between 0 and 1".to_string());
                }
                self.config.reduction.target_ratio = v;
            }
            1 => {
                let v: usize = value.parse().map_err(|_| "Invalid number")?;
                self.config.reduction.min_sequence_length = v;
            }
            2 => {
                let v: usize = value.parse().map_err(|_| "Invalid number")?;
                self.config.reduction.max_delta_distance = v;
            }
            3 => {
                let v: f64 = value.parse().map_err(|_| "Invalid number")?;
                if v <= 0.0 || v > 1.0 {
                    return Err("Must be between 0 and 1".to_string());
                }
                self.config.reduction.similarity_threshold = v;
            }
            4 => {
                self.config.reduction.taxonomy_aware = !self.config.reduction.taxonomy_aware;
            }
            5 => {
                let v: i32 = value.parse().map_err(|_| "Invalid number")?;
                self.config.alignment.gap_penalty = v;
            }
            6 => {
                let v: i32 = value.parse().map_err(|_| "Invalid number")?;
                self.config.alignment.gap_extension = v;
            }
            7 => {
                self.config.alignment.algorithm = value.to_string();
            }
            8 => {
                self.config.output.format = value.to_string();
            }
            9 => {
                self.config.output.include_metadata = !self.config.output.include_metadata;
            }
            10 => {
                self.config.output.compress_output = !self.config.output.compress_output;
            }
            11 => {
                let v: usize = value.parse().map_err(|_| "Invalid number")?;
                self.config.performance.chunk_size = v;
            }
            _ => {}
        }
        Ok(())
    }

    fn toggle_boolean(&mut self) {
        match self.selected_field {
            4 => self.config.reduction.taxonomy_aware = !self.config.reduction.taxonomy_aware,
            9 => self.config.output.include_metadata = !self.config.output.include_metadata,
            10 => self.config.output.compress_output = !self.config.output.compress_output,
            _ => {}
        }
    }

    fn is_boolean_field(&self, index: usize) -> bool {
        matches!(index, 4 | 9 | 10)
    }

    fn save_config(&mut self) {
        if let Some(ref path) = self.config_path {
            // Create directory if it doesn't exist
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }

            match save_config(path, &self.config) {
                Ok(_) => {
                    self.message = format!("Configuration saved to {}", path.display());
                }
                Err(e) => {
                    self.message = format!("Failed to save: {}", e);
                }
            }
        } else {
            self.message = "No config path set".to_string();
        }
    }

    fn load_config(&mut self) {
        use dialoguer::{theme::ColorfulTheme, Input};

        let path_str: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Enter config file path")
            .default(
                self.config_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "config.toml".to_string()),
            )
            .interact_text()
            .unwrap_or_else(|_| "config.toml".to_string());

        let path = PathBuf::from(path_str);
        match load_config(&path) {
            Ok(config) => {
                self.config = config;
                self.config_path = Some(path.clone());
                self.message = format!("Loaded config from {}", path.display());
            }
            Err(e) => {
                self.message = format!("Failed to load: {}", e);
            }
        }
    }

    fn reset_to_default(&mut self) {
        self.config = default_config();
        self.message = "Reset to default configuration".to_string();
    }
}

pub fn run_config_editor<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let mut editor = ConfigEditor::new();

    loop {
        terminal.draw(|f| draw_editor(f, &mut editor))?;

        if let Event::Key(key) = event::read()? {
            if editor.editing {
                match key.code {
                    KeyCode::Enter => {
                        let field = editor.selected_field;
                        let value = editor.temp_value.clone();
                        if let Err(e) = editor.set_field_value(field, &value) {
                            editor.message = format!("Error: {}", e);
                        } else {
                            editor.message = "Value updated".to_string();
                        }
                        editor.editing = false;
                        editor.temp_value.clear();
                    }
                    KeyCode::Esc => {
                        editor.editing = false;
                        editor.temp_value.clear();
                        editor.message = "Edit cancelled".to_string();
                    }
                    KeyCode::Backspace => {
                        editor.temp_value.pop();
                    }
                    KeyCode::Char(c) => {
                        editor.temp_value.push(c);
                    }
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => break,
                    KeyCode::Down | KeyCode::Char('j') => {
                        let count = editor.get_field_count();
                        editor.selected_field = (editor.selected_field + 1) % count;
                        editor.list_state.select(Some(editor.selected_field));
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let count = editor.get_field_count();
                        if editor.selected_field == 0 {
                            editor.selected_field = count - 1;
                        } else {
                            editor.selected_field -= 1;
                        }
                        editor.list_state.select(Some(editor.selected_field));
                    }
                    KeyCode::Enter => {
                        if editor.is_boolean_field(editor.selected_field) {
                            editor.toggle_boolean();
                            editor.message = "Value toggled".to_string();
                        } else {
                            editor.editing = true;
                            editor.temp_value = editor.get_field_value(editor.selected_field);
                            editor.message = "Enter new value (Esc to cancel)".to_string();
                        }
                    }
                    KeyCode::Char(' ') => {
                        if editor.is_boolean_field(editor.selected_field) {
                            editor.toggle_boolean();
                            editor.message = "Value toggled".to_string();
                        }
                    }
                    KeyCode::Char('s') => {
                        editor.save_config();
                    }
                    KeyCode::Char('l') => {
                        terminal.clear()?;
                        editor.load_config();
                    }
                    KeyCode::Char('r') => {
                        editor.reset_to_default();
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn draw_editor(f: &mut Frame, editor: &mut ConfigEditor) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Title
    let title_text = if let Some(ref path) = editor.config_path {
        format!("Talaria Configuration Editor - {}", path.display())
    } else {
        "Talaria Configuration Editor - (no file)".to_string()
    };

    let title = Paragraph::new(title_text)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Configuration fields
    if editor.editing {
        // Show edit dialog
        let edit_block = Block::default()
            .title(format!(
                " Editing: {} ",
                editor.get_field_name(editor.selected_field)
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let input = Paragraph::new(editor.temp_value.as_str())
            .style(Style::default().fg(Color::White))
            .block(edit_block);

        // Center the edit dialog
        let area = centered_rect(60, 20, chunks[1]);
        f.render_widget(input, area);
    } else {
        // Show field list
        let items: Vec<ListItem> = (0..editor.get_field_count())
            .map(|i| {
                let name = editor.get_field_name(i);
                let value = editor.get_field_value(i);
                let style = if i == editor.selected_field {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let content = if i == editor.selected_field {
                    format!("▶ {:<25} {}", name, value)
                } else {
                    format!("  {:<25} {}", name, value)
                };

                ListItem::new(content).style(style)
            })
            .collect();

        // Section headers could be added here later
        // let sections = vec![
        //     ListItem::new("── Reduction Settings ──").style(Style::default().fg(Color::Cyan)),
        // ];

        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Configuration ")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(Color::White));
        f.render_stateful_widget(list, chunks[1], &mut editor.list_state);
    }

    // Status/Help bar
    let help_text = if editor.editing {
        "Enter: Save | Esc: Cancel"
    } else {
        "↑/↓: Navigate | Enter/Space: Edit/Toggle | s: Save | l: Load | r: Reset | q: Quit"
    };

    let status = Paragraph::new(vec![
        Line::from(vec![Span::styled(
            editor.message.as_str(),
            Style::default().fg(Color::Green),
        )]),
        Line::from(vec![Span::styled(
            help_text,
            Style::default().fg(Color::DarkGray),
        )]),
    ])
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center);
    f.render_widget(status, chunks[2]);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
