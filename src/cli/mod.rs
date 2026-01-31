use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "mcpg", about = "MCP Gateway - aggregate multiple MCP servers")]
pub struct Cli {
    /// Show verbose debug output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[clap(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run the MCP Gateway server (default)
    Serve,
    Install(InstallArgs),
    Uninstall(UninstallArgs),
}

#[derive(clap::Args, Debug)]
pub struct InstallArgs {
    /// Show what would change without writing
    #[arg(long)]
    pub dry_run: bool,
    /// Skip confirmation prompts
    #[arg(short, long)]
    pub yes: bool,
}

#[derive(clap::Args, Debug)]
pub struct UninstallArgs {
    /// Show what would change without writing
    #[arg(long)]
    pub dry_run: bool,
    /// Skip confirmation prompts
    #[arg(short, long)]
    pub yes: bool,
}

pub mod install;
pub mod style;
pub mod uninstall;

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_no_args_defaults_to_none() {
        let cli = Cli::parse_from(["mcpg"]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_serve_command() {
        let cli = Cli::parse_from(["mcpg", "serve"]);
        assert!(matches!(cli.command, Some(Commands::Serve)));
    }

    #[test]
    fn test_install_command() {
        let cli = Cli::parse_from(["mcpg", "install"]);
        match cli.command {
            Some(Commands::Install(args)) => {
                assert!(!args.dry_run);
                assert!(!args.yes);
            }
            _ => panic!("Expected Install command"),
        }
    }

    #[test]
    fn test_install_with_dry_run() {
        let cli = Cli::parse_from(["mcpg", "install", "--dry-run"]);
        match cli.command {
            Some(Commands::Install(args)) => assert!(args.dry_run),
            _ => panic!("Expected Install command"),
        }
    }

    #[test]
    fn test_install_with_yes_short() {
        let cli = Cli::parse_from(["mcpg", "install", "-y"]);
        match cli.command {
            Some(Commands::Install(args)) => assert!(args.yes),
            _ => panic!("Expected Install command"),
        }
    }

    #[test]
    fn test_install_with_yes_long() {
        let cli = Cli::parse_from(["mcpg", "install", "--yes"]);
        match cli.command {
            Some(Commands::Install(args)) => assert!(args.yes),
            _ => panic!("Expected Install command"),
        }
    }

    #[test]
    fn test_verbose_global_flag() {
        let cli = Cli::parse_from(["mcpg", "--verbose", "install"]);
        assert!(cli.verbose);
    }

    #[test]
    fn test_verbose_short_flag() {
        let cli = Cli::parse_from(["mcpg", "-v", "install"]);
        assert!(cli.verbose);
    }

    #[test]
    fn test_uninstall_command() {
        let cli = Cli::parse_from(["mcpg", "uninstall"]);
        match cli.command {
            Some(Commands::Uninstall(args)) => {
                assert!(!args.dry_run);
                assert!(!args.yes);
            }
            _ => panic!("Expected Uninstall command"),
        }
    }

    #[test]
    fn test_uninstall_with_dry_run() {
        let cli = Cli::parse_from(["mcpg", "uninstall", "--dry-run"]);
        match cli.command {
            Some(Commands::Uninstall(args)) => assert!(args.dry_run),
            _ => panic!("Expected Uninstall command"),
        }
    }

    #[test]
    fn test_uninstall_with_yes() {
        let cli = Cli::parse_from(["mcpg", "uninstall", "-y"]);
        match cli.command {
            Some(Commands::Uninstall(args)) => assert!(args.yes),
            _ => panic!("Expected Uninstall command"),
        }
    }

    #[test]
    fn test_uninstall_all_flags() {
        let cli = Cli::parse_from(["mcpg", "uninstall", "-y", "--dry-run"]);
        match cli.command {
            Some(Commands::Uninstall(args)) => {
                assert!(args.dry_run);
                assert!(args.yes);
            }
            _ => panic!("Expected Uninstall command"),
        }
    }
}
