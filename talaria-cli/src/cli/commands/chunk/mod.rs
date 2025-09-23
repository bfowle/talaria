pub mod inspect;
pub mod lookup;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum ChunkCommands {
    /// Look up chunk information by hash, taxonomy, or accession
    Lookup(lookup::LookupArgs),

    /// Inspect chunk distribution and taxonomic organization of a database
    Inspect(inspect::InspectArgs),
}

pub fn run(command: ChunkCommands) -> anyhow::Result<()> {
    match command {
        ChunkCommands::Lookup(args) => lookup::run(args),
        ChunkCommands::Inspect(args) => inspect::run(args),
    }
}
