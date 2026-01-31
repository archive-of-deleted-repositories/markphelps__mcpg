use super::UninstallArgs;
use super::style;
use crate::agents::{self, Agent};
use crate::config::PROJECT_CONFIG_FILE;
use anyhow::{Context, Result, bail};
use console::Term;
use dialoguer::Confirm;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;
use tracing_subscriber::EnvFilter;

/// Result of the uninstall command planning phase
pub struct UninstallPlan {
    pub mcpg_config_path: PathBuf,
    pub backup_files: Vec<PathBuf>,
}

/// Run the uninstall command
pub fn run(args: UninstallArgs, verbose: bool) -> Result<()> {
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
             Make sure a supported AI agent is installed and has been used in this project."
        );
    }

    // For now, use the first agent (Claude Code)
    let agent = agents.into_iter().next().unwrap();

    debug!(agent = agent.name(), "Using agent");

    // Check if mcpg is configured
    let mcpg_source = agent.mcpg_configured_at(&project_path);
    if mcpg_source.is_none() {
        println!();
        println!(
            "{}",
            style::info("mcpg is not configured for this project.")
        );
        return Ok(());
    }

    println!();
    println!(
        "{}",
        style::info(&format!("Detected {} configuration", agent.name()))
    );

    let mcpg_config_path = project_path.join(PROJECT_CONFIG_FILE);

    // Find backup files to restore
    let backup_files = find_backup_files(&project_path, agent.as_ref())?;

    if backup_files.is_empty() && !mcpg_config_path.exists() {
        println!();
        println!(
            "{}",
            style::warning("No mcpg configuration or backups found.")
        );
        println!("Nothing to uninstall.");
        return Ok(());
    }

    let plan = UninstallPlan {
        mcpg_config_path: mcpg_config_path.clone(),
        backup_files: backup_files.clone(),
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
            .with_prompt("Proceed with uninstall?")
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

/// Find backup files created during install.
///
/// Only looks for `.json.backup` files (the most recent backup).
/// Timestamped backups (`.json.backup.<timestamp>`) are old backups
/// and are not considered for restoration.
fn find_backup_files(project_path: &Path, agent: &dyn Agent) -> Result<Vec<PathBuf>> {
    let mut backups = Vec::new();

    for source in agent.config_sources(project_path) {
        let backup_path = source.with_extension("json.backup");
        if backup_path.exists() {
            debug!(path = %backup_path.display(), "Found backup file");
            backups.push(backup_path);
        }
    }

    Ok(backups)
}

fn show_plan(plan: &UninstallPlan) -> Result<()> {
    let term = Term::stderr();
    let width = term.size().1 as usize;

    println!();
    println!("{}", style::separator(width));
    println!("{}", style::header("Uninstall Plan"));
    println!("{}", style::separator(width));
    println!();

    // Show mcpg.json removal
    if plan.mcpg_config_path.exists() {
        println!(
            "{}: {}",
            style::key("Will remove"),
            plan.mcpg_config_path.display()
        );
    } else {
        println!(
            "{}: {}",
            style::key("Skip (not found)"),
            plan.mcpg_config_path.display()
        );
    }

    // Show backup restoration
    if !plan.backup_files.is_empty() {
        println!();
        println!(
            "{}: {} file(s) to restore",
            style::key("Will restore"),
            plan.backup_files.len()
        );
        for backup in &plan.backup_files {
            let original = remove_backup_extension(backup);
            println!(
                "  {} {}",
                style::addition(&original.file_name().unwrap_or_default().to_string_lossy()),
                style::dim("(from backup)")
            );
        }
    } else {
        println!();
        println!("{}", style::dim("No backup files found to restore"));
    }

    println!();
    println!("{}", style::separator(width));

    Ok(())
}

/// Remove the `.backup` extension from a backup file path.
///
/// Converts `config.json.backup` -> `config.json`.
fn remove_backup_extension(backup_path: &Path) -> PathBuf {
    let path_str = backup_path.to_string_lossy();
    if let Some(original) = path_str.strip_suffix(".backup") {
        return PathBuf::from(original);
    }
    backup_path.to_path_buf()
}

fn execute_plan(plan: &UninstallPlan) -> Result<()> {
    // Remove .mcpg.json if it exists
    if plan.mcpg_config_path.exists() {
        fs::remove_file(&plan.mcpg_config_path)
            .with_context(|| format!("Failed to remove {}", plan.mcpg_config_path.display()))?;
        println!(
            "{}",
            style::success(&format!("Removed {}", plan.mcpg_config_path.display()))
        );
    }

    // Restore each backup file
    for backup_path in &plan.backup_files {
        let original_path = remove_backup_extension(backup_path);

        // Copy first (keeps backup intact), then remove backup only after success
        fs::copy(backup_path, &original_path)
            .with_context(|| format!("Failed to restore from {}", backup_path.display()))?;
        fs::remove_file(backup_path)
            .with_context(|| format!("Failed to remove backup {}", backup_path.display()))?;
        println!(
            "{}",
            style::success(&format!("Restored {}", original_path.display()))
        );
    }

    // Success message
    println!();
    println!("{}", style::header("Done!"));
    println!();
    println!("Restart your AI agent to apply changes.");
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_remove_backup_extension() {
        let backup = PathBuf::from("/path/to/config.json.backup");
        let original = remove_backup_extension(&backup);
        assert_eq!(original, PathBuf::from("/path/to/config.json"));
    }

    #[test]
    fn test_remove_backup_extension_no_suffix() {
        let path = PathBuf::from("/path/to/config.json");
        let result = remove_backup_extension(&path);
        assert_eq!(result, path);
    }

    #[test]
    fn test_uninstall_plan_struct() {
        let temp_dir = TempDir::new().unwrap();
        let plan = UninstallPlan {
            mcpg_config_path: temp_dir.path().join(".mcpg.json"),
            backup_files: Vec::new(),
        };

        assert!(plan.backup_files.is_empty());
    }
}
