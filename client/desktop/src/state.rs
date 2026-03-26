use std::path::PathBuf;

use dioxus::prelude::*;
use savhub_local::config::SecurityLevel;
use savhub_shared::UserSummary;

use crate::api::{ApiClient, ApiCompatibility};
use crate::i18n::Language;

/// Fallback API base if nothing is configured.
const DEFAULT_API_BASE: &str = "https://savhub.ai/api/v1";

fn default_workdir() -> PathBuf {
    savhub_local::clients::home_dir().join(".savhub")
}

/// Read global config.
///
/// API base URL priority (highest first):
///   1. `~/.savhub/config.toml` → `api_base`
///   2. Default fallback
fn load_config() -> (
    String,
    Option<String>,
    Language,
    PathBuf,
    Vec<String>,
    SecurityLevel,
) {
    // Highest priority: api_base from config.toml.
    let api_override = savhub_local::registry::read_api_base_url();

    let cfg = savhub_local::config::read_global_config()
        .ok()
        .flatten()
        .unwrap_or_default();

    let token = cfg.token;
    let lang = Language::from_code(cfg.language.as_deref().unwrap_or("en"));
    let workdir = cfg
        .workdir
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_workdir);
    let agents = cfg.agents;
    let security_level = cfg.security_level;

    let registry = api_override.unwrap_or_else(|| DEFAULT_API_BASE.to_string());

    (registry, token, lang, workdir, agents, security_level)
}

/// Read just the language setting from config (used before full state init).
pub fn read_language() -> Language {
    let cfg = savhub_local::config::read_global_config()
        .ok()
        .flatten()
        .unwrap_or_default();
    Language::from_code(cfg.language.as_deref().unwrap_or("en"))
}

/// Shared application state accessible from all pages.
#[derive(Clone, Copy)]
pub struct AppState {
    pub api_base: Signal<String>,
    pub token: Signal<Option<String>>,
    pub workdir: Signal<PathBuf>,
    pub current_user: Signal<Option<UserSummary>>,
    pub status_message: Signal<String>,
    pub lang: Signal<Language>,
    /// Registry API version compatibility status.
    pub registry_compat: Signal<ApiCompatibility>,
    pub agents: Signal<Vec<String>>,
    /// Minimum security level for fetching skills/flocks.
    pub security_level: Signal<SecurityLevel>,
    /// Incremented when an external config change is detected via the signal file.
    pub config_version: Signal<u64>,
}

impl AppState {
    pub fn init() -> Self {
        let (registry, token, lang, workdir, agents, security_level) = load_config();
        Self {
            api_base: Signal::new(registry),
            token: Signal::new(token),
            workdir: Signal::new(workdir),
            current_user: Signal::new(None),
            status_message: Signal::new(String::new()),
            lang: Signal::new(lang),
            registry_compat: Signal::new(ApiCompatibility::Unknown),
            agents: Signal::new(agents),
            security_level: Signal::new(security_level),
            config_version: Signal::new(0),
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
