use super::InitArgs;
use super::style;
use crate::agents::{self, Agent, ServerWithSource};
use crate::config::{Config, ServerConfig, TransportConfig};
use anyhow::{Context, Result, bail};
use console::Term;
use dialoguer::{Confirm, MultiSelect};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::debug;
use tracing_subscriber::EnvFilter;

/// Result of the init command planning phase
pub struct InitPlan {
    pub project_path: PathBuf,
    pub servers: HashMap<String, ServerWithSource>,
    pub selected_servers: Vec<String>,
    pub agent: Box<dyn Agent>,
}

/// Run the init command
pub fn run(args: InitArgs, verbose: bool) -> Result<()> {
    // Initialize tracing when verbose mode is enabled
    if verbose {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new("mcpg=debug"))
            .with_writer(std::io::stderr)
            .without_time()
            .init();
    }

    let project_path = std::env::current_dir().context("Failed to get current directory")?;

    debug!(path = %project_path.display(), "Project path");

    // Discover available agents
    let agents = agents::discover_agents(&project_path);

    debug!(count = agents.len(), "Found agents");

    if agents.is_empty() {
        bail!(
            "No supported AI agent configuration found.\n\
             Make sure Claude Code is installed and has been used in this project."
        );
    }

    // For now, use the first agent (Claude Code)
    let agent = agents.into_iter().next().unwrap();

    debug!(agent = agent.name(), "Using agent");

    // Log config sources being checked
    for (i, source) in agent.config_sources(&project_path).iter().enumerate() {
        debug!(
            priority = i + 1,
            path = %source.display(),
            exists = source.exists(),
            "Config source"
        );
    }

    println!();
    println!(
        "{}",
        style::info(&format!("Detected {} configuration", agent.name()))
    );

    // Check if mcpg is already configured
    if let Some(mcpg_source) = agent.mcpg_configured_at(&project_path) {
        println!();
        println!(
            "{}",
            style::info("mcpg is already configured for this project.")
        );
        println!(
            "{}",
            style::dim(&format!(
                "To reconfigure, first remove mcpg from {}",
                mcpg_source.display()
            ))
        );
        return Ok(());
    }

    // Get servers from the agent
    debug!("Getting servers from agent");
    let servers = agent.get_servers(&project_path)?;

    debug!(count = servers.len(), "Found servers");
    for (name, server) in &servers {
        debug!(
            name = %name,
            source = %server.source.display(),
            "Server"
        );
    }

    // Select servers to route through mcpg (if any exist)
    let selected_servers: Vec<String> = if servers.is_empty() {
        debug!("No existing servers to configure");
        Vec::new()
    } else {
        let server_names: Vec<String> = servers.keys().cloned().collect();
        let selected_indices = if args.yes {
            // Select all servers when -y is passed
            (0..server_names.len()).collect()
        } else {
            select_servers(&server_names)?
        };

        selected_indices
            .iter()
            .map(|&i| server_names[i].clone())
            .collect()
    };

    let plan = InitPlan {
        project_path: project_path.clone(),
        servers: servers.clone(),
        selected_servers: selected_servers.clone(),
        agent,
    };

    // Show dry run output
    show_plan(&plan)?;

    if args.dry_run {
        println!();
        println!("{}", style::info("Dry run complete. No changes made."));
        return Ok(());
    }

    // Confirm unless -y was passed
    if !args.yes {
        println!();
        let confirmed = Confirm::with_theme(&style::theme())
            .with_prompt("Proceed?")
            .default(false)
            .interact()?;

        if !confirmed {
            println!("{}", style::info("Aborted."));
            return Ok(());
        }
    }

    // Execute the plan
    execute_plan(&plan)?;

    Ok(())
}

fn select_servers(server_names: &[String]) -> Result<Vec<usize>> {
    println!();
    println!("Found {} MCP server(s) configured:", server_names.len());
    println!();

    let defaults = vec![true; server_names.len()];

    MultiSelect::with_theme(&style::theme())
        .with_prompt("Select servers to migrate (Space to toggle, Enter to confirm)")
        .items(server_names)
        .defaults(&defaults)
        .interact()
        .context("Failed to get server selection")
}

fn show_plan(plan: &InitPlan) -> Result<()> {
    let term = Term::stderr();
    let width = term.size().1 as usize;

    println!();
    println!("{}", style::separator(width));
    println!("{}", style::header("Plan"));
    println!("{}", style::separator(width));
    println!();

    // .mcpg.json changes (mcpg gateway config)
    let mcpg_config_path = plan.project_path.join(".mcpg.json");
    let mcpg_config_exists = mcpg_config_path.exists();

    if mcpg_config_exists {
        println!(
            "{}: {}",
            style::key("Will overwrite"),
            mcpg_config_path.display()
        );
    } else {
        println!(
            "{}: {}",
            style::key("Will create"),
            mcpg_config_path.display()
        );
    }

    for name in &plan.selected_servers {
        if let Some(server) = plan.servers.get(name) {
            let desc = describe_server(&server.config);
            let source_display = server
                .source
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| server.source.display().to_string());
            println!(
                "{}",
                style::addition(&format!(
                    "{}: {} {}",
                    name,
                    desc,
                    style::dim(&format!("(from {})", source_display))
                ))
            );
        }
    }

    println!();

    // Agent config changes
    let mcp_json_path = plan.project_path.join(".mcp.json");
    println!("{}: {}", style::key("Will modify"), mcp_json_path.display());
    println!("{}", style::addition("Add mcpg as MCP server"));

    // Group servers by source for cleaner display
    let mut servers_by_source: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for name in &plan.selected_servers {
        if let Some(server) = plan.servers.get(name) {
            servers_by_source
                .entry(server.source.clone())
                .or_default()
                .push(name.clone());
        }
    }

    for (source, server_names) in &servers_by_source {
        let source_display = source
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| source.display().to_string());
        println!(
            "{}",
            style::addition(&format!(
                "Disable in {}: {}",
                source_display,
                server_names.join(", ")
            ))
        );
    }

    println!();

    // Backup info
    for source in servers_by_source.keys() {
        println!("{}: {}.backup", style::key("Will backup"), source.display());
    }

    println!();
    println!("{}", style::separator(width));

    Ok(())
}

fn describe_server(server: &ServerConfig) -> String {
    match &server.transport {
        TransportConfig::Stdio { command, args, .. } => {
            let args_str = args.as_ref().map(|a| a.join(" ")).unwrap_or_default();
            if args_str.is_empty() {
                command.clone()
            } else {
                format!("{} {}", command, args_str)
            }
        }
        TransportConfig::Http { url, .. } => url.clone(),
    }
}

fn execute_plan(plan: &InitPlan) -> Result<()> {
    // Check for existing .mcpg.json
    let mcpg_config_path = plan.project_path.join(".mcpg.json");
    if mcpg_config_path.exists() {
        println!("{}", style::warning("Overwriting existing .mcpg.json"));
    }

    // Build the new config with just the ServerConfig (not ServerWithSource)
    let mut new_servers: HashMap<String, ServerConfig> = HashMap::new();
    for name in &plan.selected_servers {
        if let Some(server) = plan.servers.get(name) {
            new_servers.insert(name.clone(), server.config.clone());
        }
    }

    let new_config = Config {
        servers: new_servers,
    };

    // Write .mcpg.json
    let config_json =
        serde_json::to_string_pretty(&new_config).context("Failed to serialize .mcpg.json")?;
    fs::write(&mcpg_config_path, config_json)
        .with_context(|| format!("Failed to write {}", mcpg_config_path.display()))?;
    println!(
        "{}",
        style::success(&format!("Created {}", mcpg_config_path.display()))
    );

    // Apply changes to agent configs (backup is always created)
    plan.agent
        .apply_changes(&plan.project_path, &plan.selected_servers)?;
    println!(
        "{}",
        style::success("Updated agent configuration (backups created)")
    );

    // Success message
    println!();
    println!("{}", style::header("Done!"));
    println!();
    println!("Restart Claude Code to use mcpg.");
    println!();
    println!(
        "{}",
        style::dim("Note: Ensure 'mcpg' is in your PATH, or update .mcp.json")
    );
    println!("{}", style::dim("to use the full path to the mcpg binary."));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_describe_server_stdio() {
        let server = ServerConfig {
            transport: TransportConfig::Stdio {
                command: "npx".to_string(),
                args: Some(vec!["@mcp/server".to_string()]),
                env: None,
            },
            timeout_secs: 30,
        };

        let desc = describe_server(&server);
        assert_eq!(desc, "npx @mcp/server");
    }

    #[test]
    fn test_describe_server_stdio_no_args() {
        let server = ServerConfig {
            transport: TransportConfig::Stdio {
                command: "mcp-server".to_string(),
                args: None,
                env: None,
            },
            timeout_secs: 30,
        };

        let desc = describe_server(&server);
        assert_eq!(desc, "mcp-server");
    }

    #[test]
    fn test_describe_server_http() {
        let server = ServerConfig {
            transport: TransportConfig::Http {
                url: "http://localhost:8080".to_string(),
                headers: None,
            },
            timeout_secs: 30,
        };

        let desc = describe_server(&server);
        assert_eq!(desc, "http://localhost:8080");
    }
}
