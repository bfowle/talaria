pub mod charts;
pub mod commands;
pub mod formatting;
pub mod global_config;
pub mod interactive;
pub mod progress;
pub mod visualize;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "talaria",
    version,
    about = "Intelligent FASTA reduction for aligner index optimization",
    long_about = "Talaria reduces biological sequence databases by selecting representative sequences \
                  and encoding similar sequences as deltas, optimizing for various aligners like \
                  LAMBDA, BLAST, Kraken, and others."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Verbosity level (can be repeated)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Number of threads to use (0 = all available)
    #[arg(short = 'j', long, default_value = "0", global = true)]
    pub threads: usize,

    /// Enable comprehensive audit logging for debugging
    #[arg(
        long,
        global = true,
        help = "Enable audit logging to track all function calls and data flow"
    )]
    pub audit: bool,

    /// Custom audit log file path (defaults to $TALARIA_HOME/logs/audit-{timestamp}.log)
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Custom path for audit log file"
    )]
    pub audit_file: Option<String>,

    /// Include trace-level spans in audit log (very verbose)
    #[arg(
        long,
        global = true,
        help = "Include trace-level information in audit log"
    )]
    pub audit_trace: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Reduce a FASTA file for optimal indexing
    Reduce(commands::reduce::ReduceArgs),

    /// Reconstruct sequences from reference and delta files
    Reconstruct(commands::reconstruct::ReconstructArgs),

    /// Show statistics about a FASTA file or reduction
    Stats(commands::stats::StatsArgs),

    /// Validate reduction quality against original
    Validate(commands::validate::ValidateArgs),

    /// Manage biological databases
    Database(commands::database::DatabaseArgs),

    /// Manage bioinformatics tools (aligners)
    Tools(commands::tools::ToolsArgs),

    /// Interactive mode with TUI
    Interactive(commands::interactive::InteractiveArgs),

    /// Verify Merkle proofs and integrity
    Verify(commands::verify::VerifyArgs),

    /// Query database at specific temporal coordinates
    Temporal(commands::temporal::TemporalArgs),

    /// Look up and inspect chunk information
    Chunk {
        #[command(subcommand)]
        command: commands::chunk::ChunkCommands,
    },

    /// Manage SEQUOIA repository
    Sequoia(commands::sequoia::SequoiaArgs),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TargetAligner {
    Lambda,
    Blast,
    Kraken,
    Diamond,
    MMseqs2,
    Generic,
}

impl std::str::FromStr for TargetAligner {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "lambda" => Ok(TargetAligner::Lambda),
            "blast" => Ok(TargetAligner::Blast),
            "kraken" => Ok(TargetAligner::Kraken),
            "diamond" => Ok(TargetAligner::Diamond),
            "mmseqs2" | "mmseqs" => Ok(TargetAligner::MMseqs2),
            "generic" => Ok(TargetAligner::Generic),
            _ => Err(format!("Unknown aligner: {}", s)),
        }
    }
}
