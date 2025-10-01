use crate::cli::TargetAligner;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    io,
    sync::{Arc, Mutex},
};

pub struct ReduceWizard {
    state: WizardState,
    input_file: String,
    output_file: String,
    aligner: TargetAligner,
    options: ReductionOptions,
    progress: f64,
    message: String,
    list_state: ListState,
}

#[derive(Default)]
struct ReductionOptions {
    clustering_threshold: f64,
    min_identity: f64,
    preserve_taxonomy: bool,
    remove_redundant: bool,
    optimize_for_memory: bool,
}

enum WizardState {
    InputFile,
    SelectAligner,
    ConfigureOptions,
    Review,
    Processing,
    Complete,
}

impl ReduceWizard {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            state: WizardState::InputFile,
            input_file: String::new(),
            output_file: String::from("output.reduced.fasta"),
            aligner: TargetAligner::Lambda,
            options: ReductionOptions {
                clustering_threshold: 0.9,
                min_identity: 0.8,
                preserve_taxonomy: true,
                remove_redundant: true,
                optimize_for_memory: false,
            },
            progress: 0.0,
            message: String::from("Welcome to the FASTA Reduction Wizard"),
            list_state,
        }
    }

    fn next(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                let max = match self.state {
                    WizardState::SelectAligner => 5,
                    WizardState::ConfigureOptions => 4,
                    _ => 0,
                };
                if i >= max {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                let max = match self.state {
                    WizardState::SelectAligner => 5,
                    WizardState::ConfigureOptions => 4,
                    _ => 0,
                };
                if i == 0 {
                    max
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn advance(&mut self) {
        match self.state {
            WizardState::InputFile => {
                // Use dialoguer for file input
                use dialoguer::{theme::ColorfulTheme, Input};
                match Input::<String>::with_theme(&ColorfulTheme::default())
                    .with_prompt("Enter FASTA file path")
                    .default("example.fasta".into())
                    .interact_text()
                {
                    Ok(path) => {
                        self.input_file = path;
                        self.state = WizardState::SelectAligner;
                        self.message = "Select your target aligner".to_string();
                    }
                    Err(_) => {
                        self.input_file = "example.fasta".to_string();
                        self.state = WizardState::SelectAligner;
                        self.message = "Select your target aligner".to_string();
                    }
                }
            }
            WizardState::SelectAligner => {
                let idx = self.list_state.selected().unwrap_or(0);
                self.aligner = match idx {
                    0 => TargetAligner::Lambda,
                    1 => TargetAligner::Blast,
                    2 => TargetAligner::Diamond,
                    3 => TargetAligner::MMseqs2,
                    4 => TargetAligner::Kraken,
                    _ => TargetAligner::Generic,
                };
                self.state = WizardState::ConfigureOptions;
                self.list_state.select(Some(0));
                self.message = "Configure reduction options".to_string();
            }
            WizardState::ConfigureOptions => {
                self.state = WizardState::Review;
                self.message = "Review your settings".to_string();
            }
            WizardState::Review => {
                self.state = WizardState::Processing;
                self.message = "Processing FASTA file...".to_string();
            }
            WizardState::Processing => {
                self.state = WizardState::Complete;
                self.message = "Reduction complete!".to_string();
            }
            _ => {}
        }
    }

    fn toggle_option(&mut self) {
        if matches!(self.state, WizardState::ConfigureOptions) {
            let idx = self.list_state.selected().unwrap_or(0);
            match idx {
                2 => self.options.preserve_taxonomy = !self.options.preserve_taxonomy,
                3 => self.options.remove_redundant = !self.options.remove_redundant,
                4 => self.options.optimize_for_memory = !self.options.optimize_for_memory,
                _ => {}
            }
        }
    }
}

pub fn run_reduce_wizard<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let mut wizard = ReduceWizard::new();

    loop {
        terminal.draw(|f| draw_wizard(f, &mut wizard))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => break,
                KeyCode::Down | KeyCode::Char('j') => wizard.next(),
                KeyCode::Up | KeyCode::Char('k') => wizard.previous(),
                KeyCode::Enter => {
                    wizard.advance();
                    if matches!(wizard.state, WizardState::Complete) {
                        break;
                    }
                }
                KeyCode::Char(' ') => wizard.toggle_option(),
                _ => {}
            }
        }

        // Actual processing
        if matches!(wizard.state, WizardState::Processing) {
            // Prepare for actual reduction
            let input_path = std::path::PathBuf::from(&wizard.input_file);
            let output_path = std::path::PathBuf::from(&wizard.output_file);

            // Check if input file exists
            if !input_path.exists() {
                wizard.message = format!("Error: Input file '{}' not found", wizard.input_file);
                wizard.state = WizardState::Complete;
                terminal.draw(|f| draw_wizard(f, &mut wizard))?;
                continue;
            }

            // Show initial progress
            wizard.progress = 0.1;
            wizard.message = "Loading sequences...".to_string();
            terminal.draw(|f| draw_wizard(f, &mut wizard))?;

            // Load sequences
            match talaria_bio::parse_fasta(&input_path) {
                Ok(sequences) => {
                    wizard.progress = 0.3;
                    wizard.message = format!("Loaded {} sequences, reducing...", sequences.len());
                    terminal.draw(|f| draw_wizard(f, &mut wizard))?;

                    // Create config
                    let mut config = talaria_core::config::default_config();
                    config.reduction.similarity_threshold = wizard.options.clustering_threshold;
                    // min_identity is part of similarity_threshold
                    config.reduction.taxonomy_aware = wizard.options.preserve_taxonomy;

                    // Run reduction with progress callback
                    let progress_clone = Arc::new(Mutex::new(0.0));
                    let message_clone = Arc::new(Mutex::new(String::new()));

                    let progress_callback = {
                        let progress = progress_clone.clone();
                        let message = message_clone.clone();
                        move |msg: &str, pct: f64| {
                            *progress.lock().unwrap() = pct / 100.0;
                            *message.lock().unwrap() = msg.to_string();
                        }
                    };

                    let mut reducer = talaria_sequoia::Reducer::new(config)
                        .with_progress_callback(progress_callback)
                        .with_silent(false);

                    // Convert CLI TargetAligner to SEQUOIA TargetAligner
                    let target_aligner = match wizard.aligner {
                        crate::cli::TargetAligner::Lambda => talaria_sequoia::TargetAligner::Lambda,
                        crate::cli::TargetAligner::Blast => talaria_sequoia::TargetAligner::Blast,
                        crate::cli::TargetAligner::Kraken => talaria_sequoia::TargetAligner::Kraken,
                        crate::cli::TargetAligner::Diamond => {
                            talaria_sequoia::TargetAligner::Diamond
                        }
                        crate::cli::TargetAligner::MMseqs2 => {
                            talaria_sequoia::TargetAligner::MMseqs2
                        }
                        crate::cli::TargetAligner::Generic => {
                            talaria_sequoia::TargetAligner::Generic
                        }
                    };
                    match reducer.reduce(sequences, 0.5, target_aligner) {
                        Ok((references, _deltas, _)) => {
                            wizard.progress = 0.8;
                            wizard.message =
                                format!("Writing {} reference sequences...", references.len());
                            terminal.draw(|f| draw_wizard(f, &mut wizard))?;

                            // Write output
                            match talaria_bio::write_fasta(&output_path, &references) {
                                Ok(_) => {
                                    wizard.progress = 1.0;
                                    wizard.message = format!(
                                        "Successfully reduced to {} sequences",
                                        references.len()
                                    );
                                    wizard.state = WizardState::Complete;
                                }
                                Err(e) => {
                                    wizard.message = format!("Error writing output: {}", e);
                                    wizard.state = WizardState::Complete;
                                }
                            }
                        }
                        Err(e) => {
                            wizard.message = format!("Error during reduction: {}", e);
                            wizard.state = WizardState::Complete;
                        }
                    }
                }
                Err(e) => {
                    wizard.message = format!("Error loading sequences: {}", e);
                    wizard.state = WizardState::Complete;
                }
            }

            terminal.draw(|f| draw_wizard(f, &mut wizard))?;
        }
    }

    Ok(())
}

fn draw_wizard(f: &mut Frame, wizard: &mut ReduceWizard) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new("FASTA Reduction Wizard")
        .style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Main content
    match wizard.state {
        WizardState::InputFile => {
            let para = Paragraph::new(
                "Enter input FASTA file path:\n\n(Press Enter to use example.fasta)",
            )
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center)
            .block(Block::default().title("Input File").borders(Borders::ALL));
            f.render_widget(para, chunks[1]);
        }
        WizardState::SelectAligner => {
            let items = vec![
                ListItem::new("LAMBDA - Fast protein aligner"),
                ListItem::new("BLAST - Traditional sequence aligner"),
                ListItem::new("Diamond - Ultra-fast protein aligner"),
                ListItem::new("MMseqs2 - Sensitive sequence search"),
                ListItem::new("Kraken - Taxonomic classifier"),
                ListItem::new("Generic - No specific optimizations"),
            ];
            let list = List::new(items)
                .block(
                    Block::default()
                        .title("Select Target Aligner")
                        .borders(Borders::ALL),
                )
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().fg(Color::Black).bg(Color::Green))
                .highlight_symbol("> ");
            f.render_stateful_widget(list, chunks[1], &mut wizard.list_state);
        }
        WizardState::ConfigureOptions => {
            let items = vec![
                ListItem::new(format!(
                    "Clustering threshold: {:.1}",
                    wizard.options.clustering_threshold
                )),
                ListItem::new(format!("Min identity: {:.1}", wizard.options.min_identity)),
                ListItem::new(format!(
                    "[{}] Preserve taxonomy",
                    if wizard.options.preserve_taxonomy {
                        "●"
                    } else {
                        " "
                    }
                )),
                ListItem::new(format!(
                    "[{}] Remove redundant",
                    if wizard.options.remove_redundant {
                        "●"
                    } else {
                        " "
                    }
                )),
                ListItem::new(format!(
                    "[{}] Optimize for memory",
                    if wizard.options.optimize_for_memory {
                        "●"
                    } else {
                        " "
                    }
                )),
            ];
            let list = List::new(items)
                .block(
                    Block::default()
                        .title("Configure Options (Space to toggle)")
                        .borders(Borders::ALL),
                )
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().fg(Color::Black).bg(Color::Yellow))
                .highlight_symbol("> ");
            f.render_stateful_widget(list, chunks[1], &mut wizard.list_state);
        }
        WizardState::Review => {
            let review_text = format!(
                "Input: {}\nOutput: {}\nAligner: {:?}\nClustering: {:.1}\nMin Identity: {:.1}\nPreserve Taxonomy: {}\n\nPress Enter to start reduction",
                wizard.input_file,
                wizard.output_file,
                wizard.aligner,
                wizard.options.clustering_threshold,
                wizard.options.min_identity,
                wizard.options.preserve_taxonomy
            );
            let para = Paragraph::new(review_text)
                .style(Style::default().fg(Color::Cyan))
                .alignment(Alignment::Left)
                .wrap(Wrap { trim: true })
                .block(
                    Block::default()
                        .title("Review Settings")
                        .borders(Borders::ALL),
                );
            f.render_widget(para, chunks[1]);
        }
        WizardState::Processing => {
            let gauge = Gauge::default()
                .block(Block::default().title("Processing").borders(Borders::ALL))
                .gauge_style(Style::default().fg(Color::Green))
                .percent((wizard.progress * 100.0) as u16)
                .label(format!("{}%", (wizard.progress * 100.0) as u16));
            f.render_widget(gauge, chunks[1]);
        }
        WizardState::Complete => {
            let color = if wizard.message.contains("Error") {
                Color::Red
            } else {
                Color::Green
            };
            let symbol = if wizard.message.contains("Error") {
                "■"
            } else {
                "●"
            };
            let text = format!(
                "{} {}\n\nOutput: {}\n\nPress Esc to exit",
                symbol, wizard.message, wizard.output_file
            );
            let para = Paragraph::new(text)
                .style(Style::default().fg(color))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(para, chunks[1]);
        }
    }

    // Help text
    let help = match wizard.state {
        WizardState::ConfigureOptions => {
            "↑/↓: Navigate | Space: Toggle | Enter: Continue | Esc: Exit"
        }
        _ => "↑/↓: Navigate | Enter: Continue | Esc: Exit",
    };
    let help_widget = Paragraph::new(help)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help_widget, chunks[2]);
}
