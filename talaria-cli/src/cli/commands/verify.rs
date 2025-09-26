use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct VerifyArgs {
    /// Verify Merkle proof for a chunk
    #[arg(long)]
    pub chunk: Option<String>,

    /// Verify temporal proof for a sequence
    #[arg(long)]
    pub sequence: Option<String>,

    /// Database to verify against (e.g., "uniprot/swissprot")
    #[arg(short, long)]
    pub database: Option<String>,
}

pub fn run(args: VerifyArgs) -> Result<()> {
    use talaria_sequoia::SHA256Hash;
    use crate::cli::formatting::output::*;
    use talaria_sequoia::database::DatabaseManager;

    // Initialize database manager
    let manager = DatabaseManager::new(None)?;

    if let Some(ref chunk_hash_str) = args.chunk {
        // Verify chunk Merkle proof
        action(&format!(
            "Verifying Merkle proof for chunk: {}",
            chunk_hash_str
        ));

        // Parse chunk hash
        let chunk_hash = SHA256Hash::from_hex(chunk_hash_str)?;

        // Verify proof
        match manager.verify_chunk_proof(&chunk_hash) {
            Ok(true) => {
                success("✓ Chunk verification successful");
                info("The chunk is part of the verified Merkle tree");
            }
            Ok(false) => {
                error("✗ Chunk verification failed");
                warning("The chunk is NOT part of the Merkle tree");
            }
            Err(e) => {
                error(&format!("Error verifying chunk: {}", e));
            }
        }
    }

    if let Some(ref sequence_id) = args.sequence {
        // Get temporal history for a sequence
        action(&format!(
            "Retrieving temporal history for sequence: {}",
            sequence_id
        ));

        match manager.get_sequence_history(sequence_id) {
            Ok(history) => {
                if history.is_empty() {
                    warning("No history found for this sequence");
                } else {
                    success(&format!("Found {} version(s) for sequence", history.len()));

                    // Display history in tree format
                    let history_items: Vec<(&str, String)> = history
                        .iter()
                        .map(|record| {
                            let details = format!(
                                "Version {}: Seq: {}, Tax: {}, TaxID: {}",
                                record.version,
                                record.sequence_time.format("%Y-%m-%d"),
                                record.taxonomy_time.format("%Y-%m-%d"),
                                record
                                    .taxon_id
                                    .map_or("unknown".to_string(), |id| id.to_string())
                            );
                            ("", details)
                        })
                        .collect();

                    tree_section("Sequence History", history_items, false);
                }
            }
            Err(e) => {
                error(&format!("Error retrieving history: {}", e));
            }
        }
    }

    if args.chunk.is_none() && args.sequence.is_none() {
        warning("Please specify either --chunk or --sequence to verify");
    }

    Ok(())
}
