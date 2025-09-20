use clap::Parser;
use colored::*;
use std::process;
use talaria::cli::{Cli, Commands};
use tracing_subscriber::EnvFilter;

fn main() {
    // Initialize logging with TALARIA_LOG environment variable support
    let log_level = std::env::var("TALARIA_LOG").unwrap_or_else(|_| "info".to_string());

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_level)),
        )
        .init();

    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("{} {}", "Error:".red().bold(), e);

        // Use appropriate exit codes based on error type
        let exit_code = match e.downcast_ref::<talaria::TalariaError>() {
            Some(talaria::TalariaError::Config(_)) => 2,
            Some(talaria::TalariaError::Io(_)) => 3,
            Some(talaria::TalariaError::Parse(_)) | Some(talaria::TalariaError::Alignment(_)) => 4,
            Some(talaria::TalariaError::Database(_)) => 5,
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
        Commands::Reduce(args) => talaria::cli::commands::reduce::run(args),
        Commands::Reconstruct(args) => talaria::cli::commands::reconstruct::run(args),
        Commands::Validate(args) => talaria::cli::commands::validate::run(args),
        Commands::Stats(args) => talaria::cli::commands::stats::run(args),
        Commands::Database(args) => talaria::cli::commands::database::run(args),
        Commands::Tools(args) => talaria::cli::commands::tools::run(args),
        Commands::Interactive(args) => talaria::cli::commands::interactive::run(args),
        Commands::Verify(args) => talaria::cli::commands::verify::run(args),
        Commands::Temporal(args) => talaria::cli::commands::temporal::run(args),
        Commands::Chunk { command } => talaria::cli::commands::chunk::run(command),
    }
}
