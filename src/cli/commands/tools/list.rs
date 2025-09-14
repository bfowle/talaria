use clap::Args;
use comfy_table::{Table, presets::UTF8_FULL, Cell, Attribute, Color};
use crate::tools::ToolManager;

#[derive(Args)]
pub struct ListArgs {
    /// Show all versions (not just current)
    #[arg(long)]
    pub all_versions: bool,
    
    /// Output format (text, json)
    #[arg(short, long, default_value = "text")]
    pub format: String,
}

pub fn run(args: ListArgs) -> anyhow::Result<()> {
    let manager = ToolManager::new()?;
    let tools = manager.list_all_tools()?;
    
    if tools.is_empty() {
        println!("No tools installed");
        println!("\nInstall tools with: talaria tools install <tool>");
        println!("Available tools: lambda, blast, diamond, mmseqs2");
        return Ok(());
    }
    
    match args.format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&tools)?;
            println!("{}", json);
        }
        _ => {
            // Text format with table
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            
            table.set_header(vec![
                Cell::new("Tool").add_attribute(Attribute::Bold),
                Cell::new("Version").add_attribute(Attribute::Bold),
                Cell::new("Status").add_attribute(Attribute::Bold),
                Cell::new("Installed").add_attribute(Attribute::Bold),
                Cell::new("Path").add_attribute(Attribute::Bold),
            ]);
            
            for (tool, versions) in tools {
                if args.all_versions {
                    // Show all versions
                    for version in versions {
                        let status = if version.is_current {
                            Cell::new("current").fg(Color::Green)
                        } else {
                            Cell::new("")
                        };
                        
                        table.add_row(vec![
                            Cell::new(tool.display_name()),
                            Cell::new(&version.version),
                            status,
                            Cell::new(version.installed_date.format("%Y-%m-%d").to_string()),
                            Cell::new(version.binary_path.display().to_string()),
                        ]);
                    }
                } else {
                    // Show only current version
                    if let Some(current) = versions.iter().find(|v| v.is_current) {
                        table.add_row(vec![
                            Cell::new(tool.display_name()),
                            Cell::new(&current.version),
                            Cell::new("current").fg(Color::Green),
                            Cell::new(current.installed_date.format("%Y-%m-%d").to_string()),
                            Cell::new(current.binary_path.display().to_string()),
                        ]);
                    }
                }
            }
            
            println!("\nInstalled Tools");
            println!("{}", table);
            
            if !args.all_versions {
                println!("\nUse --all-versions to see all installed versions");
            }
        }
    }
    
    Ok(())
}