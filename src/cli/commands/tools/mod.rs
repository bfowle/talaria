pub mod install;
pub mod list;

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct ToolsArgs {
    #[command(subcommand)]
    pub command: ToolsCommands,
}

#[derive(Subcommand)]
pub enum ToolsCommands {
    /// Install a bioinformatics tool
    Install(install::InstallArgs),
    
    /// List installed tools
    List(list::ListArgs),
}

pub fn run(args: ToolsArgs) -> anyhow::Result<()> {
    match args.command {
        ToolsCommands::Install(args) => install::run(args),
        ToolsCommands::List(args) => list::run(args),
    }
}