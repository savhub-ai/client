use anyhow::Result;
use salvo::cors::{Any, Cors};
use salvo::http::Method;
use salvo::prelude::*;

use savhub_backend::config::Config;
use savhub_backend::db::{new_pool, run_migrations};
use savhub_backend::seed::ensure_seed_data;
use savhub_backend::service::registry_sync::ensure_registry_repo;
use savhub_backend::state::init_state;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "info,backend=debug,sqlx=warn".to_string()),
        )
        .init();

    let config = Config::from_env()?;

    // ── startup diagnostics ──
    tracing::info!("savhub backend starting up");
    tracing::info!("  bind            = {}", config.bind);
    tracing::info!("  frontend_origin = {}", config.frontend_origin);
    tracing::info!("  api_base        = {}", config.api_base);
    tracing::info!("  space_path      = {}", config.space_path);
    tracing::info!("  registry_url    = {}", config.registry_git_url);
    tracing::info!(
        "  registry_auth   = {}",
        if config.registry_git_ssh_key_file.is_some() {
            "ssh-key-file"
        } else if config.registry_git_ssh_key.is_some() {
            "ssh-key-base64"
        } else if config.registry_git_token.is_some() {
            "https-token"
        } else {
            "none (anonymous)"
        }
    );
    if let Some(ref path) = config.registry_git_ssh_key_file {
        let exists = std::path::Path::new(path).exists();
        tracing::info!("  ssh_key_file    = {path} (exists={exists})");
    }

    // check git is available
    match std::process::Command::new("git").arg("--version").output() {
        Ok(out) => tracing::info!(
            "  git             = {}",
            String::from_utf8_lossy(&out.stdout).trim()
        ),
        Err(e) => tracing::error!("  git             = NOT FOUND: {e}"),
    }

    // check ssh is available
    match std::process::Command::new("ssh").arg("-V").output() {
        Ok(out) => {
            let ver = String::from_utf8_lossy(&out.stderr); // ssh -V prints to stderr
            tracing::info!("  ssh             = {}", ver.trim());
        }
        Err(e) => tracing::error!("  ssh             = NOT FOUND: {e}"),
    }

    // if SSH key is configured, test GitHub connectivity
    if config.registry_git_ssh_key_file.is_some() || config.registry_git_ssh_key.is_some() {
        tracing::info!("  testing SSH connection to github.com ...");
        let mut ssh_test = tokio::process::Command::new("ssh");
        ssh_test.args(["-T", "git@github.com", "-o", "ConnectTimeout=10"]);
        // apply the configured SSH key
        if let Some(ref key_file) = config.registry_git_ssh_key_file {
            if std::path::Path::new(key_file).exists() {
                ssh_test.args(["-i", key_file]);
                ssh_test.args([
                    "-o",
                    "StrictHostKeyChecking=no",
                    "-o",
                    "UserKnownHostsFile=/dev/null",
                ]);
            }
        }
        match ssh_test.output().await {
            Ok(out) => {
                let msg = String::from_utf8_lossy(&out.stderr);
                // GitHub returns exit 1 with "Hi <user>!" on success
                if msg.contains("successfully authenticated") {
                    tracing::info!("  github ssh      = OK: {}", msg.trim());
                } else {
                    tracing::warn!("  github ssh      = {}", msg.trim());
                }
            }
            Err(e) => tracing::error!("  github ssh      = connection failed: {e}"),
        }
    }

    let pool = new_pool(&config.database_url)?;
    {
        let mut conn = pool.get()?;
        run_migrations(&mut conn)?;
    }
    tracing::info!("  database        = connected, migrations applied");

    // Security & AI diagnostics
    if config.security_scan_enabled {
        tracing::info!("  security_scan   = enabled");
    } else {
        tracing::warn!("  security_scan   = DISABLED (set SAVHUB_SECURITY_SCAN=true to enable)");
    }
    match (&config.ai_provider, &config.ai_api_key) {
        (Some(provider), Some(_)) => {
            tracing::info!("  ai_provider     = {provider}");
            tracing::info!(
                "  ai_chat_model   = {}",
                config.ai_chat_model.as_deref().unwrap_or("(default)")
            );
            tracing::info!(
                "  ai_security_model = {}",
                config.ai_security_model.as_deref().unwrap_or("(default)")
            );
            tracing::info!("  ai_chat_concurrency = {}", config.ai_chat_concurrency);
            tracing::info!(
                "  ai_security_concurrency = {}",
                config.ai_security_concurrency
            );
        }
        _ => {
            tracing::warn!(
                "  ai_provider     = NOT CONFIGURED (set SAVHUB_AI_PROVIDER and SAVHUB_AI_API_KEY)"
            );
        }
    }

    let _state = init_state(config.clone(), pool.clone())?;
    // Clone/fetch the registry repo in the background so the server starts
    // accepting requests immediately.
    {
        let config = config.clone();
        tokio::spawn(async move {
            if let Err(e) = ensure_registry_repo(&config).await {
                tracing::error!("ensure_registry_repo failed: {e}");
            }
        });
    }
    // Backfill repos.git_rev for any rows that are still NULL.
    {
        let pool = pool.clone();
        tokio::spawn(async move {
            if let Err(e) = savhub_backend::service::upgrade::backfill_repo_git_rev(&pool).await {
                tracing::error!("backfill_repo_git_rev failed: {e}");
            }
        });
    }
    ensure_seed_data(&pool)?;
    let cors = Cors::new()
        .allow_origin(config.frontend_origin.as_str())
        .allow_methods(vec![
            Method::GET,
            Method::POST,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
        .into_handler();

    let _worker = savhub_backend::worker::spawn_worker(pool.clone());

    let acceptor = TcpListener::new(config.bind.clone()).bind().await;
    tracing::info!("savhub backend listening on {}", config.bind);
    Server::new(acceptor)
        .serve(savhub_backend::routing::router().hoop(cors))
        .await;
    Ok(())
}
