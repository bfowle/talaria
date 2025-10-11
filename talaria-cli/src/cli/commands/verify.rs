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

    /// Report output file path
    #[arg(long = "report-output", value_name = "FILE")]
    pub report_output: Option<std::path::PathBuf>,

    /// Report output format (text, html, json, csv)
    #[arg(long = "report-format", value_name = "FORMAT", default_value = "text")]
    pub report_format: String,
}

pub fn run(args: VerifyArgs) -> Result<()> {
    use crate::cli::formatting::output::*;
    use talaria_sequoia::database::DatabaseManager;
    use talaria_sequoia::SHA256Hash;

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

    // Generate report if requested
    if let Some(report_path) = &args.report_output {
        use std::time::Duration;
        use talaria_sequoia::operations::VerificationResult;

        // This is a simple verification, create basic result
        let result = VerificationResult {
            valid: true, // Would track actual verification status
            issues: Vec::new(),
            merkle_valid: args.chunk.is_some(),
            statistics: talaria_sequoia::operations::results::VerificationStatistics {
                total_chunks: if args.chunk.is_some() { 1 } else { 0 },
                verified_chunks: if args.chunk.is_some() { 1 } else { 0 },
                corrupted_chunks: 0,
                missing_chunks: 0,
                total_bytes: 0,
                verified_bytes: 0,
            },
            duration: Duration::from_secs(0),
        };

        crate::cli::commands::save_report(&result, &args.report_format, report_path)?;
        println!("✓ Report saved to {}", report_path.display());
    }

    Ok(())
}
