use crate::cli::TargetAligner;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use std::io;

pub struct WizardConfig {
    pub aligner: TargetAligner,
    pub input_path: String,
    pub output_path: String,
    pub clustering_threshold: f64,
    pub min_identity: f64,
    pub preserve_taxonomy: bool,
}

pub fn run_setup_wizard() -> anyhow::Result<WizardConfig> {
    let theme = ColorfulTheme::default();

    println!("\n🧙 Welcome to the Talaria Setup Wizard!\n");

    let aligner_options = vec![
        "LAMBDA - Fast protein aligner",
        "BLAST - Traditional sequence aligner",
        "Kraken - Taxonomic classifier",
        "Diamond - Fast protein aligner",
        "MMseqs2 - Sensitive sequence search",
        "Generic - No specific optimizations",
    ];

    let aligner_idx = Select::with_theme(&theme)
        .with_prompt("Select your target aligner")
        .items(&aligner_options)
        .default(0)
        .interact()?;

    let aligner = match aligner_idx {
        0 => TargetAligner::Lambda,
        1 => TargetAligner::Blast,
        2 => TargetAligner::Kraken,
        3 => TargetAligner::Diamond,
        4 => TargetAligner::MMseqs2,
        _ => TargetAligner::Generic,
    };

    let input_path: String = Input::with_theme(&theme)
        .with_prompt("Input FASTA file path")
        .validate_with(|input: &String| {
            if std::path::Path::new(input).exists() {
                Ok(())
            } else {
                Err("File does not exist")
            }
        })
        .interact_text()?;

    let output_path: String = Input::with_theme(&theme)
        .with_prompt("Output path for reduced FASTA")
        .default("output.reduced.fasta".to_string())
        .interact_text()?;

    let clustering_threshold: f64 = Input::with_theme(&theme)
        .with_prompt("Clustering threshold (0.0-1.0)")
        .default(0.9)
        .validate_with(|input: &f64| {
            if *input >= 0.0 && *input <= 1.0 {
                Ok(())
            } else {
                Err("Value must be between 0.0 and 1.0")
            }
        })
        .interact_text()?;

    let min_identity: f64 = Input::with_theme(&theme)
        .with_prompt("Minimum sequence identity (0.0-1.0)")
        .default(0.8)
        .validate_with(|input: &f64| {
            if *input >= 0.0 && *input <= 1.0 {
                Ok(())
            } else {
                Err("Value must be between 0.0 and 1.0")
            }
        })
        .interact_text()?;

    let preserve_taxonomy = Confirm::with_theme(&theme)
        .with_prompt("Preserve taxonomic diversity?")
        .default(true)
        .interact()?;

    Ok(WizardConfig {
        aligner,
        input_path,
        output_path,
        clustering_threshold,
        min_identity,
        preserve_taxonomy,
    })
}

pub fn display_progress<B: Backend>(
    terminal: &mut Terminal<B>,
    title: &str,
    current: usize,
    total: usize,
    message: &str,
) -> io::Result<()> {
    terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(5),
            ])
            .split(f.area());

        let title_block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan));

        let progress_text = format!("{}/{} ({}%)", current, total, current * 100 / total.max(1));
        let progress = Paragraph::new(progress_text)
            .block(title_block)
            .alignment(Alignment::Center);

        f.render_widget(progress, chunks[0]);

        let message_block = Block::default().title("Status").borders(Borders::ALL);

        let message_widget = Paragraph::new(message)
            .block(message_block)
            .wrap(Wrap { trim: true });

        f.render_widget(message_widget, chunks[1]);
    })?;

    Ok(())
}

pub fn display_results<B: Backend>(
    terminal: &mut Terminal<B>,
    results: Vec<String>,
) -> io::Result<()> {
    terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(0)])
            .split(f.area());

        let items: Vec<ListItem> = results.iter().map(|r| ListItem::new(r.as_str())).collect();

        let results_list = List::new(items)
            .block(
                Block::default()
                    .title("Results")
                    .borders(Borders::ALL)
                    .style(Style::default().fg(Color::Green)),
            )
            .style(Style::default().fg(Color::White));

        f.render_widget(results_list, chunks[0]);
    })?;

    Ok(())
}
