use crate::cli::output::*;
use talaria_tools::ToolManager;
use clap::Args;

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
    section_header("Installed Tools");

    let manager = ToolManager::new()?;
    let tools = manager.list_all_tools()?;

    if tools.is_empty() {
        empty("No tools installed");
        info("Install tools with: talaria tools install <tool>");
        info("Available tools: lambda, blast, diamond, mmseqs2");
        return Ok(());
    }

    match args.format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&tools)?;
            println!("{}", json);
        }
        _ => {
            // Tree format for detailed view or table for normal
            if args.all_versions {
                // Tree structure showing all versions
                for (i, (tool, versions)) in tools.iter().enumerate() {
                    let is_last_tool = i == tools.len() - 1;
                    tree_item(false, tool.display_name(), None);

                    for (j, version) in versions.iter().enumerate() {
                        let is_last_version = j == versions.len() - 1;
                        let status = if version.is_current {
                            format!("v{} ✓ current", version.version)
                        } else {
                            format!("v{}", version.version)
                        };

                        if is_last_version {
                            tree_item_continued_last(
                                &status,
                                Some(&version.installed_date.format("%Y-%m-%d").to_string()),
                            );
                        } else {
                            tree_item_continued(
                                &status,
                                Some(&version.installed_date.format("%Y-%m-%d").to_string()),
                            );
                        }
                    }

                    if !is_last_tool {
                        println!();
                    }
                }
            } else {
                // Table format for current versions only
                let mut table = create_standard_table();

                table.set_header(vec![
                    header_cell("Tool"),
                    header_cell("Version"),
                    header_cell("Status"),
                    header_cell("Installed"),
                    header_cell("Path"),
                ]);

                for (tool, versions) in tools {
                    if let Some(current) = versions.iter().find(|v| v.is_current) {
                        use comfy_table::{Cell, Color};
                        table.add_row(vec![
                            Cell::new(tool.display_name()),
                            Cell::new(&current.version),
                            Cell::new("current").fg(Color::Green),
                            Cell::new(current.installed_date.format("%Y-%m-%d").to_string()),
                            Cell::new(current.binary_path.display().to_string()),
                        ]);
                    }
                }

                println!("{}", table);
                info("\nUse --all-versions to see all installed versions");
            }
        }
    }

    Ok(())
}
