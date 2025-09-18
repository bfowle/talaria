use clap::Args;
use crossterm::{
    cursor,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, Clear, ClearType},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Args)]
pub struct InteractiveArgs {
    /// Start in a specific mode
    #[arg(long)]
    pub mode: Option<String>,
}

// Shared flag for cleanup
static CLEANUP_DONE: AtomicBool = AtomicBool::new(false);

/// Cleanup function to restore terminal state
fn cleanup_terminal() {
    if CLEANUP_DONE.swap(true, Ordering::SeqCst) {
        return; // Already cleaned up
    }

    let mut stdout = io::stdout();

    // Best effort cleanup - ignore errors
    let _ = execute!(
        stdout,
        Clear(ClearType::All),
        DisableMouseCapture,
        LeaveAlternateScreen,
        cursor::Show
    );
    let _ = disable_raw_mode();
    let _ = stdout.flush();
}

pub fn run(_args: InteractiveArgs) -> anyhow::Result<()> {
    // Set up panic handler
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        cleanup_terminal();
        default_panic(panic_info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Hide cursor
    terminal.hide_cursor()?;

    // Run app
    let res = run_app(&mut terminal);

    // Restore terminal (improved cleanup)
    terminal.clear()?;
    terminal.show_cursor()?;
    execute!(
        terminal.backend_mut(),
        Clear(ClearType::All),
        DisableMouseCapture,
        LeaveAlternateScreen,
        cursor::Show
    )?;
    disable_raw_mode()?;

    // Ensure all escape sequences are flushed
    terminal.backend_mut().flush()?;

    // Mark cleanup as done
    CLEANUP_DONE.store(true, Ordering::SeqCst);

    // Restore default panic handler
    let _ = std::panic::take_hook();

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>) -> anyhow::Result<()> {
    let mut selected_index = 0;
    let menu_items = vec![
        ("▼ Download databases", "Download and manage biological databases"),
        ("▶ Reduce a FASTA file", "Intelligently reduce a FASTA file for indexing"),
        ("■ View statistics", "Analyze FASTA files and reduction results"),
        ("◆ Setup wizard", "Interactive setup for first-time users"),
        ("• Configure settings", "Modify Talaria configuration"),
        ("□ View documentation", "Browse built-in documentation"),
        ("× Exit", "Exit Talaria interactive mode"),
    ];

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints([
                    Constraint::Length(5),
                    Constraint::Min(10),
                    Constraint::Length(3),
                ])
                .split(f.area());

            // Header
            let header = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("▶ ", Style::default()),
                    Span::styled(
                        "TALARIA",
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" - Intelligent FASTA Reduction Tool"),
                ]),
                Line::from(""),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .alignment(Alignment::Center);
            f.render_widget(header, chunks[0]);

            // Menu
            let items: Vec<ListItem> = menu_items
                .iter()
                .enumerate()
                .map(|(i, (title, desc))| {
                    let content = if i == selected_index {
                        vec![
                            Line::from(vec![
                                Span::styled("▶ ", Style::default().fg(Color::Yellow)),
                                Span::styled(
                                    *title,
                                    Style::default()
                                        .fg(Color::Yellow)
                                        .add_modifier(Modifier::BOLD),
                                ),
                            ]),
                            Line::from(vec![
                                Span::raw("  "),
                                Span::styled(
                                    *desc,
                                    Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC),
                                ),
                            ]),
                        ]
                    } else {
                        vec![
                            Line::from(vec![
                                Span::raw("  "),
                                Span::raw(*title),
                            ]),
                            Line::from(vec![
                                Span::raw("  "),
                                Span::styled(
                                    *desc,
                                    Style::default().fg(Color::DarkGray),
                                ),
                            ]),
                        ]
                    };
                    ListItem::new(content)
                })
                .collect();

            let menu = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Main Menu ")
                        .border_style(Style::default().fg(Color::White)),
                );
            f.render_widget(menu, chunks[1]);

            // Footer
            let footer = Paragraph::new(vec![Line::from(vec![
                Span::raw("Use "),
                Span::styled("↑↓", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" to navigate, "),
                Span::styled("Enter", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" to select, "),
                Span::styled("q", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" to quit"),
            ])])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .alignment(Alignment::Center);
            f.render_widget(footer, chunks[2]);
        })?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Esc => return Ok(()),
                KeyCode::Down => {
                    selected_index = (selected_index + 1) % menu_items.len();
                }
                KeyCode::Up => {
                    if selected_index > 0 {
                        selected_index -= 1;
                    } else {
                        selected_index = menu_items.len() - 1;
                    }
                }
                KeyCode::Enter => {
                    match selected_index {
                        0 => {
                            // Download databases
                            terminal.clear()?;
                            crate::cli::interactive::download::run_download_wizard(terminal)?;
                        }
                        1 => {
                            // Reduce FASTA
                            terminal.clear()?;
                            crate::cli::interactive::reduce::run_reduce_wizard(terminal)?;
                        }
                        2 => {
                            // View statistics
                            terminal.clear()?;
                            crate::cli::interactive::stats::run_stats_viewer(terminal)?;
                        }
                        3 => {
                            // Setup wizard - need to temporarily exit raw mode for dialoguer
                            terminal.clear()?;
                            
                            // Temporarily disable raw mode for dialoguer
                            disable_raw_mode()?;
                            terminal.show_cursor()?;
                            
                            let config = crate::cli::interactive::wizard::run_setup_wizard()?;
                            save_wizard_config(config)?;
                            
                            // Re-enable raw mode
                            enable_raw_mode()?;
                            terminal.hide_cursor()?;
                            terminal.clear()?;
                        }
                        4 => {
                            // Configure settings
                            terminal.clear()?;
                            crate::cli::interactive::config_editor::run_config_editor(terminal)?;
                        }
                        5 => {
                            // View documentation
                            terminal.clear()?;
                            crate::cli::interactive::docs_viewer::run_docs_viewer(terminal)?;
                        }
                        6 => return Ok(()), // Exit
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

fn save_wizard_config(wizard_config: crate::cli::interactive::wizard::WizardConfig) -> anyhow::Result<()> {
    use dialoguer::{Confirm, Input, theme::ColorfulTheme};
    use crate::core::config::{Config, ReductionConfig, AlignmentConfig, OutputConfig, PerformanceConfig, DatabaseConfig, save_config};
    
    // Ask if user wants to save
    let should_save = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Save this configuration for future use?")
        .default(true)
        .interact()?;
    
    if !should_save {
        return Ok(());
    }
    
    // Get save path
    let default_path = dirs::config_dir()
        .map(|p| p.join("talaria").join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("config.toml"));
    
    let path_str: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Configuration file path")
        .default(default_path.to_string_lossy().to_string())
        .interact_text()?;
    
    let path = PathBuf::from(path_str);
    
    // Create directory if needed
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    // Convert WizardConfig to Config
    let config = Config {
        reduction: ReductionConfig {
            target_ratio: 0.3, // Default, not in wizard
            min_sequence_length: 50, // Default
            max_delta_distance: 100, // Default
            similarity_threshold: wizard_config.clustering_threshold,
            taxonomy_aware: wizard_config.preserve_taxonomy,
        },
        alignment: AlignmentConfig {
            gap_penalty: -11,
            gap_extension: -1,
            algorithm: "needleman-wunsch".to_string(),
        },
        output: OutputConfig {
            format: "fasta".to_string(),
            include_metadata: true,
            compress_output: false,
        },
        performance: PerformanceConfig {
            chunk_size: 10000,
            batch_size: 1000,
            cache_alignments: true,
        },
        database: DatabaseConfig {
            database_dir: None,
            retention_count: 3,
            auto_update_check: false,
            preferred_mirror: Some("ebi".to_string()),
        },
    };
    
    // Save config
    save_config(&path, &config)?;
    println!("Configuration saved to: {}", path.display());
    
    Ok(())
}