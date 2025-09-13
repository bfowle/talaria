pub mod download;
pub mod diff;
pub mod list;
pub mod list_sequences;
pub mod info;
pub mod update;
pub mod clean;
pub mod add;

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct DatabaseArgs {
    #[command(subcommand)]
    pub command: DatabaseCommands,
}

#[derive(Subcommand)]
pub enum DatabaseCommands {
    /// Download biological databases
    Download(download::DownloadArgs),

    /// Add a custom database from a local FASTA file
    Add(add::AddArgs),

    /// Compare two database versions for differences
    Diff(diff::DiffArgs),

    /// List downloaded databases
    List(list::ListArgs),

    /// Show information about a database
    Info(info::InfoArgs),

    /// Check for and download database updates
    Update(update::UpdateArgs),

    /// Clean up old database versions
    Clean(clean::CleanArgs),

    /// List sequences in a database reduction
    ListSequences(list_sequences::ListSequencesArgs),
}

pub fn run(args: DatabaseArgs) -> anyhow::Result<()> {
    match args.command {
        DatabaseCommands::Download(args) => download::run(args),
        DatabaseCommands::Add(args) => add::run(args),
        DatabaseCommands::Diff(args) => diff::run(args),
        DatabaseCommands::List(args) => list::run(args),
        DatabaseCommands::Info(args) => info::run(args),
        DatabaseCommands::Update(args) => update::run(args),
        DatabaseCommands::Clean(args) => clean::run(args),
        DatabaseCommands::ListSequences(args) => list_sequences::run(args),
    }
}