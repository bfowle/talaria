use crate::casg::{CASGRepository, CASGVerifier, VerificationResult};
use crate::core::paths;
use clap::Args;
use colored::*;

#[derive(Args)]
pub struct VerifyArgs {
    /// Database name to verify (verifies all if not specified)
    #[arg(value_name = "DATABASE")]
    pub database: Option<String>,

    /// Verify only structure without reading chunk contents
    #[arg(long, short = 's')]
    pub structure_only: bool,

    /// Verify chunk content hashes
    #[arg(long, short = 'c')]
    pub verify_content: bool,

    /// Verify Merkle tree integrity
    #[arg(long, short = 'm')]
    pub verify_merkle: bool,

    /// Verify all aspects (equivalent to -c -m)
    #[arg(long, short = 'a')]
    pub all: bool,

    /// Show detailed verification output
    #[arg(long, short = 'v')]
    pub verbose: bool,
}

pub fn run(args: VerifyArgs) -> anyhow::Result<()> {
    let base_path = if let Some(db_name) = &args.database {
        paths::talaria_databases_dir().join("data").join(db_name)
    } else {
        paths::talaria_databases_dir()
    };

    if !base_path.exists() {
        return Err(anyhow::anyhow!(
            "Database path does not exist: {}",
            base_path.display()
        ));
    }

    println!(
        "{} Verifying database integrity at {}...",
        "►".cyan().bold(),
        base_path.display()
    );

    let repo = CASGRepository::open(&base_path)?;

    let result = if args.structure_only {
        verify_structure(&repo)?
    } else {
        // Default to full verification or based on flags
        repo.verify()?
    };

    display_results(&result, args.verbose)?;

    if result.valid {
        println!(
            "\n{} Database verification completed successfully!",
            "✓".green().bold()
        );
        Ok(())
    } else {
        println!(
            "\n{} Database verification failed with {} invalid chunks",
            "✗".red().bold(),
            result.invalid_chunks.len()
        );
        Err(anyhow::anyhow!("Database verification failed"))
    }
}

fn verify_structure(repo: &CASGRepository) -> anyhow::Result<VerificationResult> {
    let manifest_data = repo.manifest.get_data()
        .ok_or_else(|| anyhow::anyhow!("No manifest loaded"))?;

    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    // Check manifest structure
    if manifest_data.chunk_index.is_empty() {
        warnings.push("Manifest contains no chunks".to_string());
    }

    // Check for orphaned chunks
    let stored_chunks = repo.storage.list_all_chunks()?;
    let manifest_chunks: std::collections::HashSet<_> = manifest_data.chunk_index
        .iter()
        .map(|c| c.hash.clone())
        .collect();

    for chunk_hash in &stored_chunks {
        if !manifest_chunks.contains(chunk_hash) {
            warnings.push(format!(
                "Orphaned chunk found: {}",
                chunk_hash.to_hex()
            ));
        }
    }

    // Check for missing chunks
    for chunk in &manifest_data.chunk_index {
        if !stored_chunks.contains(&chunk.hash) {
            errors.push(chunk.hash.to_hex());
        }
    }

    Ok(VerificationResult {
        valid: errors.is_empty(),
        chunks_verified: manifest_data.chunk_index.len(),
        invalid_chunks: errors,
        merkle_root_valid: true, // Not checked in structure-only mode
    })
}

fn display_results(result: &VerificationResult, verbose: bool) -> anyhow::Result<()> {
    println!("\n{}", "─".repeat(60));
    println!("{:^60}", "VERIFICATION RESULTS");
    println!("{}", "─".repeat(60));

    println!(
        "{} {}",
        "Status:".bold(),
        if result.valid {
            "VALID".green().bold()
        } else {
            "INVALID".red().bold()
        }
    );

    println!("{} {}", "Chunks verified:".bold(), result.chunks_verified);
    println!(
        "{} {}",
        "Merkle root:".bold(),
        if result.merkle_root_valid {
            "VALID".green().bold()
        } else {
            "INVALID".red().bold()
        }
    );

    if !result.invalid_chunks.is_empty() {
        println!("\n{} ({}):", "Invalid chunks".red().bold(), result.invalid_chunks.len());
        for (i, chunk_hash) in result.invalid_chunks.iter().enumerate() {
            if !verbose && i >= 5 {
                println!("  ... and {} more", result.invalid_chunks.len() - 5);
                break;
            }
            println!("  • {}", chunk_hash);
        }
    }

    Ok(())
}