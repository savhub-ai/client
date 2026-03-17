use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use dioxus::prelude::*;
use savhub_shared::UserSummary;

use crate::api::{ApiClient, ApiCompatibility};
use crate::i18n::Language;

/// Fallback API base if nothing is configured.
const DEFAULT_API_BASE: &str = "http://127.0.0.1:5006/api/v1";

fn default_workdir() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().join(".savhub"))
        .unwrap_or_else(|| PathBuf::from(".savhub"))
}

/// Read global config.
///
/// API base URL priority (highest first):
///   1. `~/.savhub/config.toml` → `[rest_api] base_url` (user override via registry)
///   2. `~/.savhub/registry.json` → `rest_api.base_url`
///   3. `~/.savhub/config.toml` → `registry`
///   4. Default fallback
fn load_config() -> (String, Option<String>, Language, PathBuf, Vec<String>) {
    // Highest priority: config.toml / registry.json via read_api_base_url()
    let api_override = savhub_local::registry::read_api_base_url();

    // Read config.toml from ~/.savhub/
    let config_path = savhub_local::config::get_config_dir()
        .ok()
        .map(|d| d.join("config.toml"));

    let mut token = None;
    let mut lang = Language::English;
    let mut workdir = default_workdir();
    let mut agents: Vec<String> = Vec::new();
    let mut config_registry = None;

    if let Some(path) = config_path {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = toml::from_str::<toml::Value>(&raw) {
                config_registry = cfg
                    .get("registry")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                token = cfg.get("token").and_then(|v| v.as_str()).map(String::from);
                lang = Language::from_code(
                    cfg.get("language").and_then(|v| v.as_str()).unwrap_or("en"),
                );
                workdir = cfg
                    .get("workdir")
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.trim().is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(default_workdir);
                agents = cfg
                    .get("agents")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
            }
        }
    }

    // Priority: config.toml [rest_api] override > registry.json > config.toml registry > default
    let registry = api_override
        .or(config_registry)
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string());

    (registry, token, lang, workdir, agents)
}

/// Read just the language setting from config (used before full state init).
pub fn read_language() -> Language {
    let config_path = savhub_local::config::get_config_dir()
        .ok()
        .map(|d| d.join("config.toml"));
    if let Some(path) = config_path {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = toml::from_str::<toml::Value>(&raw) {
                return Language::from_code(
                    cfg.get("language").and_then(|v| v.as_str()).unwrap_or("en"),
                );
            }
        }
    }
    Language::English
}

/// Handle to MCP server child process, shared across components.
pub type McpProcessHandle = Arc<Mutex<Option<std::process::Child>>>;

/// Shared application state accessible from all pages.
#[derive(Clone, Copy)]
pub struct AppState {
    pub api_base: Signal<String>,
    pub token: Signal<Option<String>>,
    pub workdir: Signal<PathBuf>,
    pub current_user: Signal<Option<UserSummary>>,
    pub status_message: Signal<String>,
    pub lang: Signal<Language>,
    /// Whether the MCP server process is currently running.
    pub mcp_running: Signal<bool>,
    /// Shared handle to the MCP child process (for stop/cleanup).
    pub mcp_process: Signal<McpProcessHandle>,
    /// Registry API version compatibility status.
    pub registry_compat: Signal<ApiCompatibility>,
    pub agents: Signal<Vec<String>>,
}

impl AppState {
    pub fn init() -> Self {
        let (registry, token, lang, workdir, agents) = load_config();
        Self {
            api_base: Signal::new(registry),
            token: Signal::new(token),
            workdir: Signal::new(workdir),
            current_user: Signal::new(None),
            status_message: Signal::new(String::new()),
            lang: Signal::new(lang),
            mcp_running: Signal::new(false),
            mcp_process: Signal::new(Arc::new(Mutex::new(None))),
            registry_compat: Signal::new(ApiCompatibility::Unknown),
            agents: Signal::new(agents),
        }
    }

    pub fn api_client(&self) -> ApiClient {
        ApiClient::new(self.api_base.read().clone(), self.token.read().clone())
    }

    /// Whether the user is logged in (has a token and user info).
    #[allow(dead_code)]
    pub fn is_logged_in(&self) -> bool {
        self.token.read().is_some() && self.current_user.read().is_some()
    }
}
