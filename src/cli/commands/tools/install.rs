use crate::tools::{Tool, ToolManager};
use clap::Args;

#[derive(Args)]
pub struct InstallArgs {
    /// Tool to install (lambda, blast, diamond, mmseqs2)
    pub tool: String,

    /// Specific version to install (latest if not specified)
    #[arg(long)]
    pub version: Option<String>,

    /// Force reinstall even if already installed
    #[arg(short, long)]
    pub force: bool,

    /// Check for and install upgrades
    #[arg(short, long)]
    pub upgrade: bool,
}

pub fn run(args: InstallArgs) -> anyhow::Result<()> {
    let tool: Tool = args.tool.parse()?;
    let manager = ToolManager::new()?;

    // Create async runtime for downloading
    let runtime = tokio::runtime::Runtime::new()?;

    // Check for upgrades if requested
    if args.upgrade {
        let current_version = manager.get_current_version(tool)?;
        if let Some(current) = &current_version {
            println!("Current {} version: {}", tool, current);
            println!("Checking for updates...");

            let new_version = runtime.block_on(async { manager.check_for_upgrade(tool).await })?;

            if let Some(new_ver) = new_version {
                println!("[NEW] New version available: {}", new_ver);
                println!("Upgrading {} from {} to {}...", tool, current, new_ver);
                // Continue with installation of new version
            } else {
                println!("✓ {} is up to date (version {})", tool, current);
                return Ok(());
            }
        } else {
            println!(
                "[!] {} is not installed, installing latest version...",
                tool
            );
        }
    } else if !args.force && manager.is_installed(tool) {
        // Check if already installed (when not upgrading)
        if let Some(version) = manager.get_current_version(tool)? {
            println!("✓ {} is already installed (version {})", tool, version);
            println!("Use --force to reinstall or --upgrade to check for updates");
            return Ok(());
        }
    }

    match tool {
        Tool::Lambda => {
            runtime.block_on(async { manager.install_lambda(args.version.as_deref()).await })?;
        }
        Tool::Blast => {
            anyhow::bail!("BLAST installation not yet implemented");
        }
        Tool::Diamond => {
            anyhow::bail!("DIAMOND installation not yet implemented");
        }
        Tool::Mmseqs2 => {
            anyhow::bail!("MMseqs2 installation not yet implemented");
        }
    }

    // Verify installation
    if let Some(path) = manager.get_tool_path(tool) {
        println!("\n{} installed successfully at: {:?}", tool, path);

        // Test the tool
        if tool == Tool::Lambda {
            use crate::tools::lambda::LambdaAligner;
            let aligner = LambdaAligner::new(path)?;
            if let Ok(version) = aligner.check_version() {
                println!("Version check: {}", version);
            }
        }
    }

    Ok(())
}
