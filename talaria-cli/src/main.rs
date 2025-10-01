use clap::Parser;
use colored::*;
use std::process;
use tracing_subscriber::EnvFilter;

mod cli;

use crate::cli::{Cli, Commands};
use talaria_core::TalariaError;

fn main() {
    // Initialize logging with TALARIA_LOG environment variable support
    let log_level = std::env::var("TALARIA_LOG").unwrap_or_else(|_| "warn".to_string());

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_level)),
        )
        .init();

    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("{} {}", "Error:".red().bold(), e);

        // Use appropriate exit codes based on error type
        let exit_code = match e.downcast_ref::<TalariaError>() {
            Some(TalariaError::Configuration(_)) => 2,
            Some(TalariaError::Io(_)) => 3,
            Some(TalariaError::Parse(_)) => 4,
            Some(TalariaError::Database(_)) => 5,
            _ => 1,
        };
        process::exit(exit_code);
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    // Configure thread pool
    let num_threads = if cli.threads == 0 {
        num_cpus::get()
    } else {
        cli.threads
    };

    // Set global thread count
    std::env::set_var("TALARIA_THREADS", num_threads.to_string());

    // Set global verbose level for access throughout the application
    std::env::set_var("TALARIA_VERBOSE", cli.verbose.to_string());

    // Initialize Rayon thread pool
    rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build_global()
        .expect("Failed to initialize thread pool");

    // Log thread configuration
    if cli.verbose > 0 {
        eprintln!("Using {} threads", num_threads);
    }

    match cli.command {
        Commands::Reduce(args) => crate::cli::commands::reduce::run(args),
        Commands::Reconstruct(args) => crate::cli::commands::reconstruct::run(args),
        Commands::Validate(args) => crate::cli::commands::validate::run(args),
        Commands::Stats(args) => crate::cli::commands::stats::run(args),
        Commands::Database(args) => crate::cli::commands::database::run(args),
        Commands::Tools(args) => crate::cli::commands::tools::run(args),
        Commands::Interactive(args) => crate::cli::commands::interactive::run(args),
        Commands::Verify(args) => crate::cli::commands::verify::run(args),
        Commands::Temporal(args) => crate::cli::commands::temporal::run(args),
        Commands::Chunk { command } => crate::cli::commands::chunk::run(command),
        Commands::Sequoia(args) => crate::cli::commands::sequoia::run(args),
    }
}
