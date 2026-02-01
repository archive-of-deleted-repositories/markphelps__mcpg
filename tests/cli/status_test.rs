//! Integration tests for the `mcpg status` command

use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn mcpg_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mcpg"))
}

#[test]
fn test_status_not_installed() {
    let temp_dir = TempDir::new().unwrap();

    // Create .mcp.json so agent is detected, but no mcpg configured
    let mcp_config = r#"{
        "mcpServers": {
            "test-server": {
                "command": "echo",
                "args": ["hello"]
            }
        }
    }"#;
    fs::write(temp_dir.path().join(".mcp.json"), mcp_config).unwrap();

    let output = mcpg_bin()
        .current_dir(temp_dir.path())
        .arg("status")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(
        stdout.contains("not installed"),
        "Should show not installed, got: {}",
        stdout
    );
    assert!(
        stdout.contains("mcpg install"),
        "Should suggest mcpg install, got: {}",
        stdout
    );
}

#[test]
fn test_status_installed() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // First install mcpg
    let mcp_config = r#"{
        "mcpServers": {
            "test-server": {
                "command": "echo",
                "args": ["hello"]
            }
        }
    }"#;
    fs::write(project_path.join(".mcp.json"), mcp_config).unwrap();

    mcpg_bin()
        .current_dir(project_path)
        .arg("install")
        .arg("--yes")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg install");

    // Now check status
    let output = mcpg_bin()
        .current_dir(project_path)
        .arg("status")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(
        stdout.contains("installed"),
        "Should show installed, got: {}",
        stdout
    );
    assert!(
        stdout.contains("Claude Code"),
        "Should show agent name, got: {}",
        stdout
    );
}

#[test]
fn test_status_no_agent_config() {
    let temp_dir = TempDir::new().unwrap();

    let output = mcpg_bin()
        .current_dir(temp_dir.path())
        .arg("status")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // No agent found - should show not installed
    assert!(output.status.success());
    assert!(
        stdout.contains("not installed"),
        "Should show not installed when no agent, got: {}",
        stdout
    );
}

#[test]
fn test_status_detail_not_installed() {
    let temp_dir = TempDir::new().unwrap();

    let mcp_config = r#"{
        "mcpServers": {
            "test-server": {
                "command": "echo",
                "args": ["hello"]
            }
        }
    }"#;
    fs::write(temp_dir.path().join(".mcp.json"), mcp_config).unwrap();

    let output = mcpg_bin()
        .current_dir(temp_dir.path())
        .arg("status")
        .arg("--detail")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(
        stdout.contains("not installed"),
        "Should show not installed, got: {}",
        stdout
    );
    assert!(
        stdout.contains("Claude Code"),
        "Detailed should show detected agents, got: {}",
        stdout
    );
    assert!(
        stdout.contains("test-server"),
        "Detailed should show found servers, got: {}",
        stdout
    );
}

#[test]
fn test_status_detail_installed() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    let mcp_config = r#"{
        "mcpServers": {
            "test-server": {
                "command": "echo",
                "args": ["hello"]
            }
        }
    }"#;
    fs::write(project_path.join(".mcp.json"), mcp_config).unwrap();

    mcpg_bin()
        .current_dir(project_path)
        .arg("install")
        .arg("--yes")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg install");

    let output = mcpg_bin()
        .current_dir(project_path)
        .arg("status")
        .arg("--detail")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(
        stdout.contains("installed"),
        "Should show installed, got: {}",
        stdout
    );
    assert!(
        stdout.contains("test-server"),
        "Detailed should show upstream servers, got: {}",
        stdout
    );
    assert!(
        stdout.contains("Agent config"),
        "Detailed should show agent config path, got: {}",
        stdout
    );
}

#[test]
fn test_status_help_shows_options() {
    let output = mcpg_bin()
        .arg("status")
        .arg("--help")
        .output()
        .expect("Failed to run mcpg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("--detail"),
        "Status help should show --detail option, got: {}",
        stdout
    );
}
