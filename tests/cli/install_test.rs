//! Integration tests for the `mcpg install` command

use mcpg::config::PROJECT_CONFIG_FILE;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn mcpg_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mcpg"))
}

#[test]
fn test_install_no_agent_config() {
    let temp_dir = TempDir::new().unwrap();

    let output = mcpg_bin()
        .current_dir(temp_dir.path())
        .arg("install")
        .arg("--yes")
        .env("HOME", temp_dir.path()) // No agent config here
        .output()
        .expect("Failed to run mcpg");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No supported AI agent"),
        "Expected error about missing config, got: {}",
        stderr
    );
}

#[test]
fn test_install_dry_run_creates_no_files() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a minimal .mcp.json (project-level Claude config)
    let mcp_config = r#"{
        "mcpServers": {
            "test-server": {
                "command": "echo",
                "args": ["hello"]
            }
        }
    }"#;
    fs::write(project_path.join(".mcp.json"), mcp_config).unwrap();

    let output = mcpg_bin()
        .current_dir(project_path)
        .arg("install")
        .arg("--dry-run")
        .arg("--yes")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed
    assert!(
        output.status.success(),
        "Expected success, got stderr: {}",
        stderr
    );

    // Should mention dry run
    assert!(
        stdout.contains("Dry run") || stdout.contains("dry run") || stdout.contains("No changes"),
        "Expected dry run message, got stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    // Should NOT create .mcpg.json
    assert!(
        !project_path.join(PROJECT_CONFIG_FILE).exists(),
        "Dry run should not create {}",
        PROJECT_CONFIG_FILE
    );
}

#[test]
fn test_install_creates_mcpg_config() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a minimal .mcp.json (project-level Claude config)
    let mcp_config = r#"{
        "mcpServers": {
            "test-server": {
                "command": "echo",
                "args": ["hello"]
            }
        }
    }"#;
    fs::write(project_path.join(".mcp.json"), mcp_config).unwrap();

    let output = mcpg_bin()
        .current_dir(project_path)
        .arg("install")
        .arg("--yes")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed
    assert!(
        output.status.success(),
        "Expected success, got stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    // Should create .mcpg.json
    assert!(
        project_path.join(PROJECT_CONFIG_FILE).exists(),
        "{} should be created",
        PROJECT_CONFIG_FILE
    );

    // Verify .mcpg.json contains the server
    let mcpg_config = fs::read_to_string(project_path.join(PROJECT_CONFIG_FILE)).unwrap();
    assert!(
        mcpg_config.contains("test-server"),
        "{} should contain test-server",
        PROJECT_CONFIG_FILE
    );
}

#[test]
fn test_install_adds_mcpg_to_claude_config() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a minimal .mcp.json (project-level Claude config)
    let mcp_config = r#"{
        "mcpServers": {
            "test-server": {
                "command": "echo",
                "args": ["hello"]
            }
        }
    }"#;
    fs::write(project_path.join(".mcp.json"), mcp_config).unwrap();

    let output = mcpg_bin()
        .current_dir(project_path)
        .arg("install")
        .arg("--yes")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg");

    assert!(output.status.success());

    // Verify .mcp.json now contains mcpg
    let updated_config = fs::read_to_string(project_path.join(".mcp.json")).unwrap();
    assert!(
        updated_config.contains("mcpg"),
        ".mcp.json should contain mcpg after install"
    );
}

#[test]
fn test_install_with_no_servers_still_adds_mcpg() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create an empty .mcp.json
    let mcp_config = r#"{
        "mcpServers": {}
    }"#;
    fs::write(project_path.join(".mcp.json"), mcp_config).unwrap();

    let output = mcpg_bin()
        .current_dir(project_path)
        .arg("install")
        .arg("--yes")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed even with no servers
    assert!(
        output.status.success(),
        "Expected success with no servers, got stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    // Should still add mcpg to .mcp.json
    let updated_config = fs::read_to_string(project_path.join(".mcp.json")).unwrap();
    assert!(
        updated_config.contains("mcpg"),
        ".mcp.json should contain mcpg even with no other servers"
    );
}

#[test]
fn test_help_shows_install_command() {
    let output = mcpg_bin()
        .arg("--help")
        .output()
        .expect("Failed to run mcpg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("install"),
        "Help should mention install command"
    );
    assert!(
        stdout.contains("serve"),
        "Help should mention serve command"
    );
}

#[test]
fn test_install_help_shows_options() {
    let output = mcpg_bin()
        .arg("install")
        .arg("--help")
        .output()
        .expect("Failed to run mcpg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("--dry-run"),
        "Install help should show --dry-run option"
    );
    assert!(
        stdout.contains("--yes") || stdout.contains("-y"),
        "Install help should show --yes/-y option"
    );
}

#[test]
fn test_install_already_configured() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create .mcp.json with mcpg already configured
    let mcp_config = r#"{
        "mcpServers": {
            "mcpg": {
                "command": "mcpg"
            }
        }
    }"#;
    fs::write(project_path.join(".mcp.json"), mcp_config).unwrap();

    let output = mcpg_bin()
        .current_dir(project_path)
        .arg("install")
        .arg("--yes")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed but indicate already configured
    assert!(output.status.success());
    assert!(
        stdout.contains("already configured"),
        "Should indicate mcpg is already configured, got: {}",
        stdout
    );
}
