use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::clients::{ClientKind, DetectedClient, home_dir};

const MCP_SERVER_NAME: &str = "savhub";

/// Returns the path to the MCP binary (savhub-mcp), assumed to be next to the
/// currently running executable.
pub fn mcp_binary_path() -> Result<PathBuf> {
    let current_exe =
        std::env::current_exe().context("could not determine current executable path")?;
    let dir = current_exe
        .parent()
        .context("executable has no parent directory")?;
    let name = if cfg!(windows) {
        "savhub-mcp.exe"
    } else {
        "savhub-mcp"
    };
    let path = dir.join(name);
    Ok(path)
}

/// Build the MCP server entry JSON for a given client.
fn mcp_server_entry(mcp_bin: &Path) -> Value {
    let bin_str = mcp_bin.to_string_lossy().replace('\\', "/");
    json!({
        "command": bin_str,
        "args": [],
        "env": {}
    })
}

/// VS Code uses a slightly different format with `type: "stdio"`.
fn vscode_server_entry(mcp_bin: &Path) -> Value {
    let bin_str = mcp_bin.to_string_lossy().replace('\\', "/");
    json!({
        "type": "stdio",
        "command": bin_str,
        "args": [],
        "env": {}
    })
}

/// Get the MCP config file path for a client.
pub fn mcp_config_path(client: &DetectedClient) -> Option<PathBuf> {
    let home = home_dir();
    match client.kind {
        ClientKind::ClaudeCode => Some(home.join(".claude.json")),
        ClientKind::Cursor => Some(home.join(".cursor").join("mcp.json")),
        ClientKind::Windsurf => {
            let windsurf_dir = home.join(".codeium").join("windsurf");
            if windsurf_dir.is_dir() {
                Some(windsurf_dir.join("mcp_config.json"))
            } else {
                Some(home.join(".windsurf").join("mcp_config.json"))
            }
        }
        ClientKind::VsCode => Some(home.join(".vscode").join("mcp.json")),
        ClientKind::Continue => {
            let dir = home.join(".continue").join("mcpServers");
            Some(dir.join("savhub.json"))
        }
        ClientKind::Codex => None,
    }
}

/// Root key used by each client for MCP servers.
fn root_key(kind: ClientKind) -> &'static str {
    match kind {
        ClientKind::VsCode => "servers",
        _ => "mcpServers",
    }
}

/// Read an existing MCP config file, or return an empty object.
fn read_mcp_config(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("invalid JSON in {}", path.display()))
}

/// Write MCP config, creating parent directories as needed.
fn write_mcp_config(path: &Path, config: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(config)?;
    fs::write(path, format!("{payload}\n"))?;
    Ok(())
}

/// Generate a project-level `.mcp.json` so AI agents pick up the MCP server
/// with the correct `--workdir` when opening this project.
pub fn register_mcp_for_project(workdir: &Path) -> Result<()> {
    let mcp_bin = mcp_binary_path()?;
    let workdir_str = workdir.to_string_lossy().replace('\\', "/");
    let bin_str = mcp_bin.to_string_lossy().replace('\\', "/");

    let config = json!({
        "mcpServers": {
            MCP_SERVER_NAME: {
                "command": bin_str,
                "args": ["--workdir", workdir_str],
                "env": {}
            }
        }
    });

    let config_path = workdir.join(".mcp.json");
    write_mcp_config(&config_path, &config)
}

/// Register the savhub MCP server with a client.
pub fn register_mcp(client: &DetectedClient) -> Result<()> {
    if !client.kind.supports_mcp_prompts() {
        bail!("{} does not support MCP prompts, skipping", client.name);
    }

    let mcp_bin = mcp_binary_path()?;
    let config_path = mcp_config_path(client).context("could not determine MCP config path")?;

    let mut config = read_mcp_config(&config_path)?;
    let key = root_key(client.kind);

    // Ensure the root key exists as an object
    if !config.get(key).is_some_and(Value::is_object) {
        config[key] = json!({});
    }

    let entry = if client.kind == ClientKind::VsCode {
        vscode_server_entry(&mcp_bin)
    } else {
        mcp_server_entry(&mcp_bin)
    };

    config[key][MCP_SERVER_NAME] = entry;
    write_mcp_config(&config_path, &config)
}

/// Unregister the savhub MCP server from a client.
pub fn unregister_mcp(client: &DetectedClient) -> Result<()> {
    let config_path = mcp_config_path(client).context("could not determine MCP config path")?;

    if !config_path.exists() {
        return Ok(());
    }

    let mut config = read_mcp_config(&config_path)?;
    let key = root_key(client.kind);

    if let Some(servers) = config.get_mut(key).and_then(Value::as_object_mut) {
        servers.remove(MCP_SERVER_NAME);
    }

    // For Continue, just delete the file
    if client.kind == ClientKind::Continue {
        let _ = fs::remove_file(&config_path);
        return Ok(());
    }

    write_mcp_config(&config_path, &config)
}

/// Check if the savhub MCP server is registered with a client.
pub fn is_registered(client: &DetectedClient) -> bool {
    let Some(config_path) = mcp_config_path(client) else {
        return false;
    };
    let Ok(config) = read_mcp_config(&config_path) else {
        return false;
    };
    let key = root_key(client.kind);
    config
        .get(key)
        .and_then(Value::as_object)
        .is_some_and(|servers| servers.contains_key(MCP_SERVER_NAME))
}

/// Registration status for display.
#[derive(Debug)]
pub struct McpRegistrationStatus {
    pub client_name: String,
    pub kind: ClientKind,
    pub installed: bool,
    pub supports_prompts: bool,
    pub registered: bool,
    pub config_path: Option<PathBuf>,
}

/// Get registration status for all detected clients.
pub fn get_all_registration_status() -> Vec<McpRegistrationStatus> {
    let clients = crate::clients::detect_clients();
    clients
        .iter()
        .map(|client| McpRegistrationStatus {
            client_name: client.name.clone(),
            kind: client.kind,
            installed: client.installed,
            supports_prompts: client.kind.supports_mcp_prompts(),
            registered: is_registered(client),
            config_path: mcp_config_path(client),
        })
        .collect()
}
