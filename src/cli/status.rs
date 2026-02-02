use super::StatusArgs;
use super::style;
use crate::agents::{self, Agent};
use crate::config::{Config, PROJECT_CONFIG_FILE, TransportConfig};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::debug;
use tracing_subscriber::EnvFilter;

/// Run the status command
pub fn run(args: StatusArgs, verbose: bool) -> Result<()> {
    if verbose {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new(format!("{}=debug", crate::config::APP_NAME)))
            .with_writer(std::io::stderr)
            .without_time()
            .init();
    }

    let project_path = std::env::current_dir().context("Failed to get current directory")?;

    debug!(path = %project_path.display(), "Project path");

    // Discover available agents
    let agents = agents::discover_agents(&project_path);

    debug!(count = agents.len(), "Found agents");

    // Check if .mcpg.json exists and can be loaded
    let mcpg_config_path = project_path.join(PROJECT_CONFIG_FILE);
    let mcpg_config = if mcpg_config_path.exists() {
        Config::load(Some(mcpg_config_path.clone())).ok()
    } else {
        None
    };

    // Find the first agent that has mcpg configured
    let configured_agent = agents
        .iter()
        .find(|a| a.mcpg_configured_at(&project_path).is_some());

    let is_installed = mcpg_config.is_some() && configured_agent.is_some();

    println!();
    if is_installed {
        show_installed(&project_path, &args, &agents, &mcpg_config.unwrap())?;
    } else {
        show_not_installed(&project_path, &args, &agents)?;
    }

    Ok(())
}

fn show_installed(
    project_path: &Path,
    args: &StatusArgs,
    agents: &[Box<dyn Agent>],
    config: &Config,
) -> Result<()> {
    println!(
        "{}",
        style::success(&format!("{} is installed", crate::config::APP_NAME))
    );
    println!();

    let agent = agents
        .iter()
        .find(|a| a.mcpg_configured_at(project_path).is_some())
        .expect("should have configured agent");

    let server_count = config.servers.len();

    println!("  {}: {}", style::key("Config"), PROJECT_CONFIG_FILE);
    println!("  {}: {}", style::key("Agent"), agent.name());
    println!(
        "  {}: {} upstream server{} configured",
        style::key("Servers"),
        server_count,
        if server_count == 1 { "" } else { "s" }
    );

    if args.detail {
        show_installed_detail(project_path, agent.as_ref(), config)?;
    }

    println!();
    Ok(())
}

fn show_installed_detail(project_path: &Path, agent: &dyn Agent, config: &Config) -> Result<()> {
    if !config.servers.is_empty() {
        println!();
        println!("  Upstream servers:");

        let mut names: Vec<&String> = config.servers.keys().collect();
        names.sort();

        for name in names {
            if let Some(server) = config.servers.get(name) {
                let desc = describe_server_transport(server);
                println!("    {} {}: {}", style::ARROW, name, desc);
            }
        }
    }

    if let Some(agent_config) = agent.mcpg_configured_at(project_path) {
        let display_path = make_display_path(&agent_config, project_path);
        println!();
        println!("  {}: {}", style::key("Agent config"), display_path);

        let backup_path = agent_config.with_extension("json.backup");
        if backup_path.exists() {
            let backup_display = make_display_path(&backup_path, project_path);
            println!("  {}: {}", style::key("Backup"), backup_display);
        }
    }

    Ok(())
}

fn show_not_installed(
    project_path: &Path,
    args: &StatusArgs,
    agents: &[Box<dyn Agent>],
) -> Result<()> {
    println!(
        "{}",
        style::error(&format!("{} is not installed", crate::config::APP_NAME))
    );

    if args.detail && !agents.is_empty() {
        println!();
        println!("  Detected agents:");
        for agent in agents {
            println!("    {} {}", style::ARROW, agent.name());
        }

        let mut found_any = false;
        for agent in agents {
            if let Ok(servers) = agent.get_servers(project_path) {
                if !servers.is_empty() {
                    if !found_any {
                        println!();
                        println!("  MCP servers found:");
                        found_any = true;
                    }

                    let mut names: Vec<(&String, &PathBuf)> =
                        servers.iter().map(|(name, s)| (name, &s.source)).collect();
                    names.sort_by_key(|(name, _)| name.to_string());

                    for (name, source) in names {
                        let source_display = source
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| source.display().to_string());
                        println!(
                            "    {} {}  {}",
                            style::ARROW,
                            name,
                            style::dim(&format!("(from {})", source_display))
                        );
                    }
                }
            }
        }
    }

    println!();
    println!(
        "  Run '{}' to get started.",
        style::header(&format!("{} install", crate::config::APP_NAME))
    );

    println!();
    Ok(())
}

fn describe_server_transport(server: &crate::config::ServerConfig) -> String {
    match &server.transport {
        TransportConfig::Stdio { command, args, .. } => {
            let args_str = args.as_ref().map(|a| a.join(" ")).unwrap_or_default();
            if args_str.is_empty() {
                format!("stdio: {}", command)
            } else {
                format!("stdio: {} {}", command, args_str)
            }
        }
        TransportConfig::Http { url, .. } => format!("http: {}", url),
    }
}

fn make_display_path(path: &Path, project_path: &Path) -> String {
    path.strip_prefix(project_path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| {
            path.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ServerConfig, TransportConfig};

    #[test]
    fn test_describe_server_transport_stdio() {
        let server = ServerConfig {
            transport: TransportConfig::Stdio {
                command: "npx".to_string(),
                args: Some(vec!["@mcp/server".to_string()]),
                env: None,
            },
            timeout_secs: 30,
        };
        assert_eq!(describe_server_transport(&server), "stdio: npx @mcp/server");
    }

    #[test]
    fn test_describe_server_transport_stdio_no_args() {
        let server = ServerConfig {
            transport: TransportConfig::Stdio {
                command: "mcp-server".to_string(),
                args: None,
                env: None,
            },
            timeout_secs: 30,
        };
        assert_eq!(describe_server_transport(&server), "stdio: mcp-server");
    }

    #[test]
    fn test_describe_server_transport_http() {
        let server = ServerConfig {
            transport: TransportConfig::Http {
                url: "http://localhost:8080".to_string(),
                headers: None,
            },
            timeout_secs: 30,
        };
        assert_eq!(
            describe_server_transport(&server),
            "http: http://localhost:8080"
        );
    }

    #[test]
    fn test_make_display_path_relative() {
        let project = PathBuf::from("/home/user/project");
        let path = PathBuf::from("/home/user/project/.mcp.json");
        assert_eq!(make_display_path(&path, &project), ".mcp.json");
    }

    #[test]
    fn test_make_display_path_outside_project() {
        let project = PathBuf::from("/home/user/project");
        let path = PathBuf::from("/home/user/.claude.json");
        assert_eq!(make_display_path(&path, &project), ".claude.json");
    }
}
