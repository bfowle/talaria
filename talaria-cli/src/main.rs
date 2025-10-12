use clap::Parser;
use colored::*;
use std::path::PathBuf;
use std::process;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

mod cli;

use crate::cli::{Cli, Commands};
use talaria_core::audit::{AuditConfig, AuditLogger};
use talaria_core::TalariaError;

fn main() {
    let cli = Cli::parse();

    // Initialize logging with TALARIA_LOG environment variable support
    // When audit is enabled, ensure at least INFO level to capture spans
    let log_level = if cli.audit {
        let env_level = std::env::var("TALARIA_LOG").unwrap_or_else(|_| "info".to_string());
        // Ensure at least INFO level for audit to work properly
        if env_level == "error" || env_level == "warn" {
            "info".to_string()
        } else {
            env_level
        }
    } else {
        std::env::var("TALARIA_LOG").unwrap_or_else(|_| "warn".to_string())
    };

    // Set up base tracing subscriber
    // Use RUST_LOG if set, otherwise use our adjusted log level
    let env_filter = if std::env::var("RUST_LOG").is_ok() {
        EnvFilter::try_from_default_env().unwrap()
    } else {
        EnvFilter::new(&log_level)
    };

    // Check if flame tracing is requested via environment variable
    let flame_enabled = std::env::var("TALARIA_FLAME").unwrap_or_default() == "1";

    #[cfg(not(feature = "flame"))]
    if flame_enabled {
        eprintln!("{} TALARIA_FLAME=1 set but flame feature not enabled. Rebuild with: cargo build --features flame", "Warning:".yellow());
    }

    // Initialize audit logger if requested (independent of flame tracing)
    let audit_enabled = cli.audit;
    if audit_enabled {
        let mut audit_config = AuditConfig::default();

        // Use custom audit file path if provided
        if let Some(ref audit_file) = cli.audit_file {
            audit_config.log_path = PathBuf::from(audit_file);
        }

        // Initialize the audit logger
        if let Err(e) = AuditLogger::init(audit_config.clone()) {
            eprintln!(
                "{} Failed to initialize audit logger: {}",
                "Warning:".yellow(),
                e
            );
        } else if cli.verbose > 0 {
            eprintln!(
                "Audit logging enabled to: {}",
                audit_config.log_path.display()
            );
        }
    }

    // Build layered subscriber with all requested features
    #[cfg(feature = "flame")]
    let flame_guard = if flame_enabled {
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let flame_file = format!("flame-{}.folded", timestamp);

        match tracing_flame::FlameLayer::with_file(&flame_file) {
            Ok((flame_layer, guard)) => {
                eprintln!("Flame tracing enabled to: {}", flame_file);

                let trace_level = std::env::var("TALARIA_FLAME_LEVEL").unwrap_or_else(|_| "debug".to_string());
                eprintln!("Tracing all talaria crates at {} level", trace_level.to_uppercase());
                eprintln!("Tip: Set TALARIA_FLAME_LEVEL=trace for more detailed traces");

                // Build comprehensive env filter for flame tracing
                let flame_filter = EnvFilter::new("warn")
                    .add_directive(format!("talaria={}", trace_level).parse().unwrap())
                    .add_directive(format!("talaria_cli={}", trace_level).parse().unwrap())
                    .add_directive(format!("talaria_herald={}", trace_level).parse().unwrap())
                    .add_directive(format!("talaria_core={}", trace_level).parse().unwrap())
                    .add_directive(format!("talaria_bio={}", trace_level).parse().unwrap())
                    .add_directive(format!("talaria_storage={}", trace_level).parse().unwrap())
                    .add_directive(format!("talaria_tools={}", trace_level).parse().unwrap())
                    .add_directive(format!("talaria_utils={}", trace_level).parse().unwrap());

                // Build the subscriber with all layers
                let fmt_layer = tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .with_thread_ids(false)
                    .with_level(false);

                let registry = tracing_subscriber::registry()
                    .with(flame_filter)
                    .with(fmt_layer)
                    .with(flame_layer);

                // Add audit layer if requested
                if audit_enabled {
                    let audit_layer = talaria_core::audit::layer::AuditLayer::new(cli.audit_trace);
                    registry.with(audit_layer).init();
                } else {
                    registry.init();
                }

                Some(guard)
            }
            Err(e) => {
                eprintln!("{} Failed to create flame file: {}", "Warning:".yellow(), e);
                None
            }
        }
    } else {
        None
    };

    // If flame wasn't initialized but we still need to set up tracing
    #[cfg(feature = "flame")]
    let tracing_initialized = flame_guard.is_some();
    #[cfg(not(feature = "flame"))]
    let tracing_initialized = false;

    if !tracing_initialized {
        if audit_enabled {
            // Set up tracing with audit layer only
            let fmt_layer = tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_thread_ids(false);

            let audit_layer = talaria_core::audit::layer::AuditLayer::new(cli.audit_trace);

            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .with(audit_layer)
                .init();
        } else {
            // Standard tracing without audit or flame
            tracing_subscriber::fmt().with_env_filter(env_filter).init();
        }
    }

    // Keep flame guard alive for duration of program
    #[cfg(feature = "flame")]
    let _flame_guard = flame_guard;

    let result = run(cli);

    // Explicitly flush flame guard before exit
    #[cfg(feature = "flame")]
    if let Some(guard) = _flame_guard {
        if let Err(e) = guard.flush() {
            eprintln!("{} Failed to flush flame trace: {}", "Warning:".yellow(), e);
        }
    }

    if let Err(e) = result {
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
        Commands::Herald(args) => crate::cli::commands::herald::run(args),
    }
}
