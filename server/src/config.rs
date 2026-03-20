use std::env;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind: String,
    pub frontend_origin: String,
    pub api_base: String,
    pub space_path: String,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_redirect_url: String,
    pub github_admin_logins: Vec<String>,
    pub github_moderator_logins: Vec<String>,
    pub sync_interval_secs: u64,
    pub sync_stale_hours: u64,
    pub registry_git_url: String,
    /// Optional GitHub personal access token for push access to the registry repo.
    pub registry_git_token: Option<String>,
    /// Optional base64-encoded SSH private key for push access to the registry repo.
    /// When set, it is decoded and written to a temp file, then used via GIT_SSH_COMMAND.
    pub registry_git_ssh_key: Option<String>,
    /// Optional path to an SSH private key file (e.g. mounted via Docker volume).
    /// Takes priority over `registry_git_ssh_key` when both are set.
    pub registry_git_ssh_key_file: Option<String>,
    /// AI provider for generating flock metadata. "zhipu" or "doubao".
    pub ai_provider: Option<String>,
    /// API key for the configured AI provider.
    pub ai_api_key: Option<String>,
    /// Model name override for chat completions.
    pub ai_chat_model: Option<String>,
    /// Model name override for LLM security evaluation (defaults to glm-4-plus / doubao-1-5-pro-32k).
    pub ai_security_model: Option<String>,
    pub auto_index_min_interval_secs: u64,
    /// Maximum number of index jobs that may execute in parallel. Default 10.
    pub max_parallel_index_jobs: usize,
    /// Whether to push commits to the remote registry repo. Default true.
    pub registry_git_push: bool,
    /// Enable the enhanced security scanning pipeline (static + LLM). Default false.
    pub security_scan_enabled: bool,
    /// Max concurrent AI chat requests (flock/repo metadata). Default 2.
    pub ai_chat_concurrency: usize,
    /// Max concurrent AI security scan requests. Default 2.
    pub ai_security_concurrency: usize,
}

impl Config {
    pub fn registry_repo_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.space_path).join("registry")
    }

    pub fn repo_checkout_base_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.space_path).join("repos")
    }

    /// Return the registry git URL with credentials embedded when a token is configured.
    /// e.g. `https://x-access-token:{token}@github.com/org/repo.git`
    pub fn registry_git_url_with_auth(&self) -> String {
        match &self.registry_git_token {
            Some(token) => {
                if let Some(rest) = self.registry_git_url.strip_prefix("https://") {
                    format!("https://x-access-token:{token}@{rest}")
                } else {
                    self.registry_git_url.clone()
                }
            }
            None => self.registry_git_url.clone(),
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        // Use .env.local if it exists, otherwise fall back to .env
        if dotenvy::from_filename(".env.local").is_err() {
            dotenvy::dotenv().ok();
        }
        let database_url =
            env::var("DATABASE_URL").context("DATABASE_URL is required to start the backend")?;
        let bind = env::var("SAVHUB_BIND").unwrap_or_else(|_| "127.0.0.1:5006".to_string());
        let frontend_origin = env::var("SAVHUB_FRONTEND_ORIGIN")
            .unwrap_or_else(|_| "http://127.0.0.1:8081".to_string());
        let api_base =
            env::var("SAVHUB_API_BASE").unwrap_or_else(|_| format!("http://{bind}/api/v1"));
        let space_path = env::var("SAVHUB_SPACE_PATH").unwrap_or_else(|_| "./space".to_string());
        let github_client_id = env::var("SAVHUB_GITHUB_CLIENT_ID")
            .context("SAVHUB_GITHUB_CLIENT_ID is required to start the backend")?;
        let github_client_secret = env::var("SAVHUB_GITHUB_CLIENT_SECRET")
            .context("SAVHUB_GITHUB_CLIENT_SECRET is required to start the backend")?;
        let github_redirect_url = env::var("SAVHUB_GITHUB_REDIRECT_URL")
            .context("SAVHUB_GITHUB_REDIRECT_URL is required to start the backend")?;
        Ok(Self {
            database_url,
            bind,
            frontend_origin,
            api_base,
            space_path,
            github_client_id,
            github_client_secret,
            github_redirect_url,
            github_admin_logins: parse_login_list("SAVHUB_GITHUB_ADMIN_LOGINS"),
            github_moderator_logins: parse_login_list("SAVHUB_GITHUB_MODERATOR_LOGINS"),
            sync_interval_secs: env::var("SAVHUB_SYNC_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            sync_stale_hours: env::var("SAVHUB_SYNC_STALE_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(6),
            registry_git_url: env::var("SAVHUB_REGISTRY_GIT_URL")
                .unwrap_or_else(|_| "https://github.com/savhub-ai/registry.git".to_string()),
            registry_git_token: env::var("SAVHUB_REGISTRY_GIT_TOKEN")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            registry_git_ssh_key: env::var("SAVHUB_REGISTRY_GIT_SSH_KEY")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            registry_git_ssh_key_file: env::var("SAVHUB_REGISTRY_GIT_SSH_KEY_FILE")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            ai_provider: env::var("SAVHUB_AI_PROVIDER")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            ai_api_key: env::var("SAVHUB_AI_API_KEY")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            ai_chat_model: env::var("SAVHUB_AI_CHAT_MODEL")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            ai_security_model: env::var("SAVHUB_AI_SECURITY_MODEL")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            auto_index_min_interval_secs: env::var("SAVHUB_AUTO_INDEX_MIN_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600),
            max_parallel_index_jobs: env::var("SAVHUB_MAX_PARALLEL_INDEX_JOBS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            registry_git_push: env::var("SAVHUB_REGISTRY_GIT_PUSH")
                .map(|v| !matches!(v.trim().to_lowercase().as_str(), "false" | "0" | "no"))
                .unwrap_or(true),
            security_scan_enabled: env::var("SAVHUB_SECURITY_SCAN")
                .map(|v| matches!(v.trim().to_lowercase().as_str(), "true" | "1" | "yes"))
                .unwrap_or(false),
            ai_chat_concurrency: env::var("SAVHUB_AI_CHAT_CONCURRENCY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
            ai_security_concurrency: env::var("SAVHUB_AI_SECURITY_CONCURRENCY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
        })
    }
}

fn parse_login_list(name: &str) -> Vec<String> {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(|item| item.trim().to_lowercase())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}
