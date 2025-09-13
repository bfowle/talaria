use crate::download::{DatabaseSource, UniProtDatabase, DownloadProgress};
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{io, path::PathBuf, sync::{Arc, Mutex}};

pub struct DownloadWizard {
    state: WizardState,
    selected_source: Option<DatabaseSource>,
    #[allow(dead_code)]
    selected_database: Option<String>,
    output_dir: PathBuf,
    list_state: ListState,
    message: String,
    progress: Arc<Mutex<f64>>,
    download_size: u64,
}

enum WizardState {
    SelectSource,
    SelectDatabase,
    Confirm,
    Downloading,
    Complete,
}

impl DownloadWizard {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        
        Self {
            state: WizardState::SelectSource,
            selected_source: None,
            selected_database: None,
            output_dir: PathBuf::from("data"),
            list_state,
            message: String::from("Welcome to the Database Download Wizard"),
            progress: Arc::new(Mutex::new(0.0)),
            download_size: 0,
        }
    }
    
    fn next(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                let max = match self.state {
                    WizardState::SelectSource => 2,
                    WizardState::SelectDatabase => 4,
                    _ => 0,
                };
                if i >= max { 0 } else { i + 1 }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }
    
    fn previous(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                let max = match self.state {
                    WizardState::SelectSource => 2,
                    WizardState::SelectDatabase => 4,
                    _ => 0,
                };
                if i == 0 { max } else { i - 1 }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }
    
    fn select(&mut self) {
        match self.state {
            WizardState::SelectSource => {
                let _idx = self.list_state.selected().unwrap_or(0);
                // Just store the source type, will set specific database later
                self.state = WizardState::SelectDatabase;
                self.list_state.select(Some(0));
                self.message = "Select specific database".to_string();
            }
            WizardState::SelectDatabase => {
                let db_idx = self.list_state.selected().unwrap_or(0);
                
                // Set the actual database based on selection
                self.selected_source = Some(match db_idx {
                    0 => DatabaseSource::UniProt(UniProtDatabase::SwissProt),
                    1 => DatabaseSource::UniProt(UniProtDatabase::TrEMBL),
                    2 => DatabaseSource::UniProt(UniProtDatabase::UniRef50),
                    3 => DatabaseSource::UniProt(UniProtDatabase::UniRef90),
                    4 => DatabaseSource::UniProt(UniProtDatabase::UniRef100),
                    _ => DatabaseSource::UniProt(UniProtDatabase::SwissProt),
                });
                
                // Get estimated size
                self.download_size = match &self.selected_source {
                    Some(DatabaseSource::UniProt(UniProtDatabase::SwissProt)) => 200_000_000,
                    Some(DatabaseSource::UniProt(UniProtDatabase::TrEMBL)) => 100_000_000_000,
                    Some(DatabaseSource::UniProt(UniProtDatabase::UniRef50)) => 20_000_000_000,
                    Some(DatabaseSource::UniProt(UniProtDatabase::UniRef90)) => 40_000_000_000,
                    Some(DatabaseSource::UniProt(UniProtDatabase::UniRef100)) => 80_000_000_000,
                    _ => 0,
                };
                
                let size_str = format_size(self.download_size);
                self.state = WizardState::Confirm;
                self.message = format!("Ready to download {} (~{}). Press Enter to start or Esc to cancel.", 
                    self.selected_source.as_ref().map(|s| format!("{}", s)).unwrap_or("Unknown".to_string()),
                    size_str);
            }
            WizardState::Confirm => {
                self.state = WizardState::Downloading;
                self.message = "Starting download...".to_string();
            }
            _ => {}
        }
    }
}

pub fn run_download_wizard<B: Backend>(
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let mut wizard = DownloadWizard::new();
    
    loop {
        terminal.draw(|f| draw_wizard::<B>(f, &mut wizard))?;
        
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => break,
                KeyCode::Down | KeyCode::Char('j') => wizard.next(),
                KeyCode::Up | KeyCode::Char('k') => wizard.previous(),
                KeyCode::Enter => {
                    wizard.select();
                    if matches!(wizard.state, WizardState::Complete) {
                        break;
                    }
                }
                _ => {}
            }
        }
        
        // Perform actual download
        if matches!(wizard.state, WizardState::Downloading) {
            // Create output directory if it doesn't exist
            std::fs::create_dir_all(&wizard.output_dir).ok();
            
            if let Some(source) = wizard.selected_source.clone() {
                // Determine output filename
                let output_file = match &source {
                    DatabaseSource::UniProt(db) => wizard.output_dir.join(format!("{}.fasta", db)),
                    DatabaseSource::NCBI(db) => wizard.output_dir.join(format!("{}.fasta", db)),
                    DatabaseSource::Custom(path) => PathBuf::from(path),
                };
                
                wizard.message = format!("Downloading to {}...", output_file.display());
                terminal.draw(|f| draw_wizard::<B>(f, &mut wizard))?;
                
                // Create async runtime for download
                let runtime = tokio::runtime::Runtime::new().unwrap();
                
                // Clone for the async block
                let source_clone = source.clone();
                let progress_clone = wizard.progress.clone();
                
                // Run async download
                let result = runtime.block_on(async {
                    let mut progress = DownloadProgress::new();
                    
                    // Set up progress callback
                    let progress_arc = progress_clone.clone();
                    progress.set_callback(Box::new(move |current, total| {
                        if total > 0 {
                            let pct = (current as f64 / total as f64) * 100.0;
                            *progress_arc.lock().unwrap() = pct;
                        }
                    }));
                    
                    crate::download::download_database(source_clone, &output_file, &mut progress).await
                });
                
                match result {
                    Ok(_) => {
                        wizard.state = WizardState::Complete;
                        wizard.message = format!("Successfully downloaded to {}", output_file.display());
                    }
                    Err(e) => {
                        wizard.state = WizardState::Complete;
                        wizard.message = format!("Download failed: {}", e);
                    }
                }
            } else {
                wizard.state = WizardState::Complete;
                wizard.message = "No database selected".to_string();
            }
        }
    }
    
    Ok(())
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    
    format!("{:.1} {}", size, UNITS[unit_idx])
}

fn draw_wizard<B>(f: &mut Frame, wizard: &mut DownloadWizard) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.area());
    
    // Title
    let title = Paragraph::new("Database Download Wizard")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);
    
    // Main content
    match wizard.state {
        WizardState::SelectSource => {
            let items = vec![
                ListItem::new("UniProt - Protein sequences"),
                ListItem::new("NCBI - Comprehensive databases"),
                ListItem::new("Custom - Local file"),
            ];
            let list = List::new(items)
                .block(Block::default().title("Select Source").borders(Borders::ALL))
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan))
                .highlight_symbol("> ");
            f.render_stateful_widget(list, chunks[1], &mut wizard.list_state);
        }
        WizardState::SelectDatabase => {
            let items = match &wizard.selected_source {
                Some(DatabaseSource::UniProt(_)) => vec![
                    ListItem::new("SwissProt - Manually reviewed (~570K sequences)"),
                    ListItem::new("TrEMBL - Unreviewed (~250M sequences)"),
                    ListItem::new("UniRef50 - Clustered at 50% identity"),
                    ListItem::new("UniRef90 - Clustered at 90% identity"),
                    ListItem::new("UniRef100 - Clustered at 100% identity"),
                ],
                Some(DatabaseSource::NCBI(_)) => vec![
                    ListItem::new("NR - Non-redundant proteins"),
                    ListItem::new("NT - Nucleotide sequences"),
                    ListItem::new("RefSeq - Curated sequences"),
                    ListItem::new("Taxonomy - Classification database"),
                ],
                _ => vec![ListItem::new("Select file...")],
            };
            let list = List::new(items)
                .block(Block::default().title("Select Database").borders(Borders::ALL))
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().fg(Color::Black).bg(Color::Green))
                .highlight_symbol("> ");
            f.render_stateful_widget(list, chunks[1], &mut wizard.list_state);
        }
        WizardState::Confirm => {
            let para = Paragraph::new(wizard.message.clone())
                .style(Style::default().fg(Color::Yellow))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(para, chunks[1]);
        }
        WizardState::Downloading => {
            let progress_val = *wizard.progress.lock().unwrap();
            
            let content = vec![
                wizard.message.clone(),
                format!("\n\nProgress: {:.1}%", progress_val),
            ].join("");
            
            let para = Paragraph::new(content)
                .style(Style::default().fg(Color::Cyan))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
                .block(Block::default().title("Downloading").borders(Borders::ALL));
            f.render_widget(para, chunks[1]);
            
            // Also show a progress gauge
            let gauge_area = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
                .split(chunks[1]);
            
            let gauge = Gauge::default()
                .block(Block::default().borders(Borders::NONE))
                .gauge_style(Style::default().fg(Color::Green))
                .percent(progress_val as u16);
            f.render_widget(gauge, gauge_area[1]);
        }
        WizardState::Complete => {
            let color = if wizard.message.contains("failed") || wizard.message.contains("Error") {
                Color::Red
            } else {
                Color::Green
            };
            let para = Paragraph::new(wizard.message.clone())
                .style(Style::default().fg(color))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(para, chunks[1]);
        }
    }
    
    // Help text
    let help = Paragraph::new("↑/↓: Navigate | Enter: Select | Esc: Exit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[2]);
}