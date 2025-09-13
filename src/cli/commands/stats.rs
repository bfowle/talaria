use crate::bio::stats::SequenceStats;
use crate::cli::visualize::{ascii_histogram, progress_bar, format_number};
use clap::Args;
use colored::*;
use std::path::PathBuf;

#[derive(Args)]
pub struct StatsArgs {
    /// Input FASTA file
    #[arg(short, long, value_name = "FILE")]
    pub input: PathBuf,
    
    /// Delta metadata file (if analyzing reduction)
    #[arg(short = 'd', long)]
    pub deltas: Option<PathBuf>,
    
    /// Show detailed statistics
    #[arg(long)]
    pub detailed: bool,
    
    /// Output format (text, json, csv)
    #[arg(long, default_value = "text")]
    pub format: String,
    
    /// Show visual charts and graphs
    #[arg(long)]
    pub visual: bool,
    
    /// Launch interactive TUI viewer
    #[arg(long)]
    pub interactive: bool,
}

pub fn run(args: StatsArgs) -> anyhow::Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};
    
    // Show progress bar for loading FASTA
    let loading_pb = ProgressBar::new_spinner();
    loading_pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
    );
    loading_pb.set_message(format!("Loading {}...", args.input.display()));
    
    // Parse input FASTA
    let sequences = crate::bio::fasta::parse_fasta(&args.input)?;
    loading_pb.finish_with_message(format!("Loaded {} sequences", sequences.len()));
    
    // Launch interactive mode if requested
    if args.interactive {
        return launch_interactive_stats(&sequences);
    }
    
    // Calculate comprehensive statistics with progress
    let stats = SequenceStats::calculate_with_progress(&sequences, true);
    
    match args.format.as_str() {
        "json" => print_json_stats(&stats)?,
        "csv" => print_csv_stats(&stats),
        _ => {
            if args.visual {
                print_visual_stats(&stats, args.detailed)?;
            } else {
                print_text_stats(&stats, args.detailed)?;
            }
            
            if let Some(delta_path) = args.deltas {
                print_reduction_stats(&sequences, &delta_path)?;
            }
        }
    }
    
    Ok(())
}

fn print_text_stats(stats: &SequenceStats, detailed: bool) -> anyhow::Result<()> {
    println!("{}", "═══════════════════════════════════════════════════════════".blue());
    println!("{}", "                    FASTA STATISTICS REPORT".blue().bold());
    println!("{}", "═══════════════════════════════════════════════════════════".blue());
    println!();
    
    // Basic metrics
    println!("{} {}", "● SEQUENCE METRICS".yellow().bold(), "");
    println!("  {} {}", "Total Sequences:".green(), format_number(stats.total_sequences));
    println!("  {} {}", "Total Bases:".green(), format_number(stats.total_length));
    println!("  {} {:.1} bp", "Average Length:".green(), stats.average_length);
    println!("  {} {} bp", "Median Length:".green(), format_number(stats.median_length));
    println!("  {} {} bp", "Min/Max Length:".green(), 
             format!("{} / {}", format_number(stats.min_length), format_number(stats.max_length)));
    println!("  {} {} bp", "N50:".green(), format_number(stats.n50));
    println!("  {} {} bp", "N90:".green(), format_number(stats.n90));
    println!();
    
    // Composition
    println!("{} {}", "◆ COMPOSITION ANALYSIS".yellow().bold(), "");
    
    use crate::bio::sequence::SequenceType;
    if stats.primary_type == SequenceType::Nucleotide {
        println!("  {} {:.1}%", "GC Content:".green(), stats.gc_content);
        println!("  {} {:.1}%", "AT Content:".green(), stats.at_content);
        
        if detailed && !stats.nucleotide_frequencies.is_empty() {
            println!("\n  {}:", "Nucleotide Frequencies".cyan());
            let mut nucs: Vec<_> = stats.nucleotide_frequencies.iter().collect();
            nucs.sort_by_key(|(k, _)| **k);
            for (nuc, freq) in nucs {
                if *nuc as char != 'N' && *nuc as char != '-' {
                    println!("    {}: {:.1}%", *nuc as char, freq);
                }
            }
        }
    } else {
        println!("  {} Protein", "Sequence Type:".green());
        
        if detailed && !stats.amino_acid_frequencies.is_empty() {
            println!("\n  {}:", "Amino Acid Frequencies".cyan());
            let mut aas: Vec<_> = stats.amino_acid_frequencies.iter().collect();
            aas.sort_by_key(|(k, _)| **k);
            let mut count = 0;
            for (aa, freq) in aas {
                if *aa as char != 'X' && *aa as char != '*' && *aa as char != '-' {
                    if count % 4 == 0 && count > 0 {
                        println!();
                    }
                    print!("    {}: {:5.1}%", *aa as char, freq);
                    count += 1;
                }
            }
            if count > 0 {
                println!();
            }
        }
    }
    println!();
    
    // Complexity
    println!("{} {}", "■ COMPLEXITY METRICS".yellow().bold(), "");
    println!("  {} {:.2}", "Shannon Entropy:".green(), stats.shannon_entropy);
    println!("  {} {:.4}", "Simpson Diversity:".green(), stats.simpson_diversity);
    println!("  {} {:.1}%", "Low Complexity:".green(), stats.low_complexity_percentage);
    println!("  {} {}", "Ambiguous Bases:".green(), format_number(stats.ambiguous_bases));
    println!("  {} {}", "Gaps:".green(), format_number(stats.gap_count));
    
    Ok(())
}

fn print_visual_stats(stats: &SequenceStats, detailed: bool) -> anyhow::Result<()> {
    println!("{}", "═══════════════════════════════════════════════════════════".blue());
    println!("{}", "                    FASTA STATISTICS REPORT".blue().bold());
    println!("{}", "═══════════════════════════════════════════════════════════".blue());
    println!();
    
    // Basic metrics with visual elements
    println!("{} {}", "● SEQUENCE METRICS".yellow().bold(), "");
    println!("├─ {} {}", "Total Sequences:".green(), format_number(stats.total_sequences));
    println!("├─ {} {}", "Total Bases:".green(), format_number(stats.total_length));
    println!("├─ {} {:.1} bp", "Average Length:".green(), stats.average_length);
    println!("└─ {} {} bp", "N50:".green(), format_number(stats.n50));
    println!();
    
    // Length distribution histogram
    println!("{} {}", "▶ LENGTH DISTRIBUTION".yellow().bold(), "");
    let histogram = ascii_histogram(&stats.length_distribution, 40, true);
    println!("{}", histogram);
    
    // Composition with progress bars
    println!("{} {}", "◆ COMPOSITION ANALYSIS".yellow().bold(), "");
    
    use crate::bio::sequence::SequenceType;
    if stats.primary_type == SequenceType::Nucleotide {
        println!("{}", progress_bar(stats.gc_content, 100.0, 40, "  GC Content", true));
        println!("{}", progress_bar(stats.at_content, 100.0, 40, "  AT Content", true));
        println!();
        
        if detailed && !stats.nucleotide_frequencies.is_empty() {
            println!("  {}:", "Nucleotide Frequencies".cyan());
            let mut nucs: Vec<_> = stats.nucleotide_frequencies.iter()
                .filter(|(k, _)| **k != b'N' && **k != b'-')
                .collect();
            nucs.sort_by_key(|(k, _)| **k);
            
            for (nuc, freq) in nucs {
                let bar = progress_bar(*freq, 100.0, 30, &format!("    {}", *nuc as char), true);
                println!("{}", bar);
            }
            println!();
        }
    } else {
        println!("  {} Protein sequences", "Type:".green());
        println!("  {} {} unique amino acids", "Composition:".green(), stats.amino_acid_frequencies.len());
        
        if detailed && !stats.amino_acid_frequencies.is_empty() {
            println!("\n  {}:", "Top Amino Acids".cyan());
            let mut aas: Vec<_> = stats.amino_acid_frequencies.iter()
                .filter(|(k, _)| **k != b'X' && **k != b'*' && **k != b'-')
                .collect();
            aas.sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap());
            
            for (aa, freq) in aas.iter().take(10) {
                let bar = progress_bar(**freq, 100.0, 30, &format!("    {}", **aa as char), true);
                println!("{}", bar);
            }
        }
        println!();
    }
    
    // GC distribution (only for nucleotides)
    if stats.primary_type == SequenceType::Nucleotide && !stats.gc_distribution.is_empty() {
        println!("{} {}", "▣ GC CONTENT DISTRIBUTION".yellow().bold(), "");
        let gc_hist = ascii_histogram(&stats.gc_distribution, 40, true);
        println!("{}", gc_hist);
    }
    
    // Complexity metrics with visual indicators
    println!("{} {}", "■ COMPLEXITY METRICS".yellow().bold(), "");
    
    let entropy_indicator = match stats.shannon_entropy {
        e if e < 1.0 => "Low complexity [!]".red(),
        e if e < 1.5 => "Medium complexity".yellow(),
        _ => "High complexity ✓".green(),
    };
    println!("  Shannon Entropy: {:.2} {}", stats.shannon_entropy, entropy_indicator);
    
    println!("  Low Complexity Regions: {}", 
             progress_bar(stats.low_complexity_percentage, 100.0, 30, "", true));
    
    if stats.ambiguous_bases > 0 || stats.gap_count > 0 {
        println!();
        println!("  {} {} ambiguous | {} gaps", 
                 "Quality Issues:".yellow(),
                 format_number(stats.ambiguous_bases),
                 format_number(stats.gap_count));
    }
    
    Ok(())
}

fn print_json_stats(stats: &SequenceStats) -> anyhow::Result<()> {
    use crate::bio::sequence::SequenceType;
    
    let json = if stats.primary_type == SequenceType::Nucleotide {
        serde_json::json!({
            "sequence_type": "nucleotide",
            "total_sequences": stats.total_sequences,
            "total_length": stats.total_length,
            "average_length": stats.average_length,
            "median_length": stats.median_length,
            "min_length": stats.min_length,
            "max_length": stats.max_length,
            "n50": stats.n50,
            "n90": stats.n90,
            "gc_content": stats.gc_content,
            "at_content": stats.at_content,
            "shannon_entropy": stats.shannon_entropy,
            "simpson_diversity": stats.simpson_diversity,
            "low_complexity_percentage": stats.low_complexity_percentage,
            "ambiguous_bases": stats.ambiguous_bases,
            "gap_count": stats.gap_count,
            "length_distribution": stats.length_distribution,
            "gc_distribution": stats.gc_distribution,
            "nucleotide_frequencies": stats.nucleotide_frequencies,
        })
    } else {
        serde_json::json!({
            "sequence_type": "protein",
            "total_sequences": stats.total_sequences,
            "total_length": stats.total_length,
            "average_length": stats.average_length,
            "median_length": stats.median_length,
            "min_length": stats.min_length,
            "max_length": stats.max_length,
            "n50": stats.n50,
            "n90": stats.n90,
            "shannon_entropy": stats.shannon_entropy,
            "simpson_diversity": stats.simpson_diversity,
            "low_complexity_percentage": stats.low_complexity_percentage,
            "ambiguous_bases": stats.ambiguous_bases,
            "gap_count": stats.gap_count,
            "length_distribution": stats.length_distribution,
            "amino_acid_frequencies": stats.amino_acid_frequencies,
        })
    };
    
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

fn print_csv_stats(stats: &SequenceStats) {
    println!("metric,value");
    println!("total_sequences,{}", stats.total_sequences);
    println!("total_length,{}", stats.total_length);
    println!("average_length,{:.1}", stats.average_length);
    println!("median_length,{}", stats.median_length);
    println!("min_length,{}", stats.min_length);
    println!("max_length,{}", stats.max_length);
    println!("n50,{}", stats.n50);
    println!("n90,{}", stats.n90);
    println!("gc_content,{:.1}", stats.gc_content);
    println!("at_content,{:.1}", stats.at_content);
    println!("shannon_entropy,{:.2}", stats.shannon_entropy);
    println!("simpson_diversity,{:.4}", stats.simpson_diversity);
    println!("low_complexity_percentage,{:.1}", stats.low_complexity_percentage);
    println!("ambiguous_bases,{}", stats.ambiguous_bases);
    println!("gap_count,{}", stats.gap_count);
}

fn print_reduction_stats(sequences: &[crate::bio::sequence::Sequence], delta_path: &std::path::Path) -> anyhow::Result<()> {
    println!();
    println!("{}", "═══════════════════════════════════════════════════════════".blue());
    println!("{}", "                   REDUCTION STATISTICS".blue().bold());
    println!("{}", "═══════════════════════════════════════════════════════════".blue());
    
    let deltas = crate::storage::metadata::load_metadata(delta_path)?;
    let num_references = sequences.len();
    let num_deltas = deltas.len();
    let total = num_references + num_deltas;
    let reduction_ratio = num_references as f64 / total as f64;
    
    println!();
    println!("  {} {}", "References:".green(), format_number(num_references));
    println!("  {} {}", "Delta-encoded:".green(), format_number(num_deltas));
    println!("  {} {}", "Total Original:".green(), format_number(total));
    println!();
    
    // Visual representation
    let ref_bar = progress_bar(num_references as f64, total as f64, 40, "  References", true);
    let delta_bar = progress_bar(num_deltas as f64, total as f64, 40, "  Deltas", true);
    
    println!("{}", ref_bar);
    println!("{}", delta_bar);
    println!();
    
    println!("  {} {:.1}%", "Compression Ratio:".green().bold(), (1.0 - reduction_ratio) * 100.0);
    println!("  {} {:.2}x", "Space Savings:".green().bold(), 1.0 / reduction_ratio);
    
    Ok(())
}

fn launch_interactive_stats(_sequences: &[crate::bio::sequence::Sequence]) -> anyhow::Result<()> {
    // Launch the interactive TUI stats viewer
    use crate::cli::interactive::stats::run_stats_viewer;
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;
    use std::io;
    
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    
    crossterm::terminal::enable_raw_mode()?;
    let result = run_stats_viewer(&mut terminal);
    crossterm::terminal::disable_raw_mode()?;
    
    Ok(result?)
}