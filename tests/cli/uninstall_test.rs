//! Integration tests for the `mcpg uninstall` command

use mcpg::config::PROJECT_CONFIG_FILE;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn mcpg_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mcpg"))
}

/// Helper: run install in the given project directory to set up state for uninstall tests.
fn run_install(project_path: &std::path::Path, home: &std::path::Path) {
    let output = mcpg_bin()
        .current_dir(project_path)
        .arg("install")
        .arg("--yes")
        .env("HOME", home)
        .output()
        .expect("Failed to run mcpg install");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "install should succeed, got stdout: {}, stderr: {}",
        stdout,
        stderr
    );
}

#[test]
fn test_uninstall_restores_backups() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a .mcp.json with a test server
    let original_config = r#"{
        "mcpServers": {
            "test-server": {
                "command": "echo",
                "args": ["hello"]
            }
        }
    }"#;
    fs::write(project_path.join(".mcp.json"), original_config).unwrap();

    // Run install to create backups and modify config
    run_install(project_path, temp_dir.path());

    // Verify install created a backup
    assert!(
        project_path.join(".mcp.json.backup").exists(),
        "Install should create .mcp.json.backup"
    );

    // Verify install modified .mcp.json (should now contain mcpg)
    let modified_config = fs::read_to_string(project_path.join(".mcp.json")).unwrap();
    assert!(
        modified_config.contains("mcpg"),
        ".mcp.json should contain mcpg after install"
    );

    // Run uninstall
    let output = mcpg_bin()
        .current_dir(project_path)
        .arg("uninstall")
        .arg("--yes")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg uninstall");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "uninstall should succeed, got stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    // Verify backup was removed (consumed by restore)
    assert!(
        !project_path.join(".mcp.json.backup").exists(),
        "Backup should be removed after uninstall"
    );

    // Verify .mcp.json was restored to original content
    let restored_config = fs::read_to_string(project_path.join(".mcp.json")).unwrap();
    assert!(
        restored_config.contains("test-server"),
        "Restored config should contain test-server"
    );
}

#[test]
fn test_uninstall_removes_mcpg_config() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a .mcp.json with a test server
    let mcp_config = r#"{
        "mcpServers": {
            "test-server": {
                "command": "echo",
                "args": ["hello"]
            }
        }
    }"#;
    fs::write(project_path.join(".mcp.json"), mcp_config).unwrap();

    // Run install
    run_install(project_path, temp_dir.path());

    // Verify .mcpg.json exists after install
    assert!(
        project_path.join(PROJECT_CONFIG_FILE).exists(),
        "{} should exist after install",
        PROJECT_CONFIG_FILE
    );

    // Run uninstall
    let output = mcpg_bin()
        .current_dir(project_path)
        .arg("uninstall")
        .arg("--yes")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg uninstall");

    assert!(output.status.success(), "uninstall should succeed");

    // Verify .mcpg.json was removed
    assert!(
        !project_path.join(PROJECT_CONFIG_FILE).exists(),
        "{} should be removed after uninstall",
        PROJECT_CONFIG_FILE
    );
}

#[test]
fn test_uninstall_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a .mcp.json with a test server
    let mcp_config = r#"{
        "mcpServers": {
            "test-server": {
                "command": "echo",
                "args": ["hello"]
            }
        }
    }"#;
    fs::write(project_path.join(".mcp.json"), mcp_config).unwrap();

    // Run install
    run_install(project_path, temp_dir.path());

    // Snapshot the state after install
    let mcpg_config_before = fs::read_to_string(project_path.join(PROJECT_CONFIG_FILE)).unwrap();
    let mcp_config_before = fs::read_to_string(project_path.join(".mcp.json")).unwrap();

    // Run uninstall with --dry-run
    let output = mcpg_bin()
        .current_dir(project_path)
        .arg("uninstall")
        .arg("--dry-run")
        .arg("--yes")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg uninstall");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "uninstall --dry-run should succeed, got stderr: {}",
        stderr
    );

    // Should mention dry run
    assert!(
        stdout.contains("Dry run") || stdout.contains("No changes"),
        "Expected dry run message, got stdout: {}",
        stdout
    );

    // Verify nothing changed
    assert!(
        project_path.join(PROJECT_CONFIG_FILE).exists(),
        "{} should still exist after dry run",
        PROJECT_CONFIG_FILE
    );
    assert!(
        project_path.join(".mcp.json.backup").exists(),
        "Backup should still exist after dry run"
    );

    let mcpg_config_after = fs::read_to_string(project_path.join(PROJECT_CONFIG_FILE)).unwrap();
    assert_eq!(
        mcpg_config_before, mcpg_config_after,
        "{} should not be modified by dry run",
        PROJECT_CONFIG_FILE
    );

    let mcp_config_after = fs::read_to_string(project_path.join(".mcp.json")).unwrap();
    assert_eq!(
        mcp_config_before, mcp_config_after,
        ".mcp.json should not be modified by dry run"
    );
}

#[test]
fn test_uninstall_not_configured() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a .mcp.json without mcpg configured
    let mcp_config = r#"{
        "mcpServers": {
            "test-server": {
                "command": "echo",
                "args": ["hello"]
            }
        }
    }"#;
    fs::write(project_path.join(".mcp.json"), mcp_config).unwrap();

    // Run uninstall without having run install first
    let output = mcpg_bin()
        .current_dir(project_path)
        .arg("uninstall")
        .arg("--yes")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg uninstall");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed but indicate mcpg is not configured
    assert!(output.status.success());
    assert!(
        stdout.contains("not configured"),
        "Should indicate mcpg is not configured, got: {}",
        stdout
    );
}

#[test]
fn test_uninstall_no_agent_config() {
    let temp_dir = TempDir::new().unwrap();

    // Run uninstall with no agent config at all
    let output = mcpg_bin()
        .current_dir(temp_dir.path())
        .arg("uninstall")
        .arg("--yes")
        .env("HOME", temp_dir.path())
        .output()
        .expect("Failed to run mcpg uninstall");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No supported AI agent"),
        "Expected error about missing config, got: {}",
        stderr
    );
}
