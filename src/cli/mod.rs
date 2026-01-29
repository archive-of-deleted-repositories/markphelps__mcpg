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
    Init(InitArgs),
}

#[derive(clap::Args, Debug)]
pub struct InitArgs {
    /// Show what would change without writing
    #[arg(long)]
    pub dry_run: bool,
    /// Skip confirmation prompts
    #[arg(short, long)]
    pub yes: bool,
}

pub mod init;
pub mod style;

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
    fn test_init_command() {
        let cli = Cli::parse_from(["mcpg", "init"]);
        match cli.command {
            Some(Commands::Init(args)) => {
                assert!(!args.dry_run);
                assert!(!args.yes);
            }
            _ => panic!("Expected Init command"),
        }
    }

    #[test]
    fn test_init_with_dry_run() {
        let cli = Cli::parse_from(["mcpg", "init", "--dry-run"]);
        match cli.command {
            Some(Commands::Init(args)) => assert!(args.dry_run),
            _ => panic!("Expected Init command"),
        }
    }

    #[test]
    fn test_init_with_yes_short() {
        let cli = Cli::parse_from(["mcpg", "init", "-y"]);
        match cli.command {
            Some(Commands::Init(args)) => assert!(args.yes),
            _ => panic!("Expected Init command"),
        }
    }

    #[test]
    fn test_init_with_yes_long() {
        let cli = Cli::parse_from(["mcpg", "init", "--yes"]);
        match cli.command {
            Some(Commands::Init(args)) => assert!(args.yes),
            _ => panic!("Expected Init command"),
        }
    }

    #[test]
    fn test_verbose_global_flag() {
        let cli = Cli::parse_from(["mcpg", "--verbose", "init"]);
        assert!(cli.verbose);
    }

    #[test]
    fn test_verbose_short_flag() {
        let cli = Cli::parse_from(["mcpg", "-v", "init"]);
        assert!(cli.verbose);
    }
}
