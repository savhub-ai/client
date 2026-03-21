use anyhow::Result;
use salvo::cors::{Any, Cors};
use salvo::http::Method;
use salvo::prelude::*;

use server::config::Config;
use server::db::{new_pool, run_migrations};
use server::seed::ensure_seed_data;
use server::state::init_state;

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

    // check git is available
    match std::process::Command::new("git").arg("--version").output() {
        Ok(out) => tracing::info!(
            "  git             = {}",
            String::from_utf8_lossy(&out.stdout).trim()
        ),
        Err(e) => tracing::error!("  git             = NOT FOUND: {e}"),
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
    // Backfill repos.git_rev for any rows that are still NULL.
    {
        let pool = pool.clone();
        tokio::spawn(async move {
            if let Err(e) = server::service::upgrade::backfill_repo_git_rev(&pool).await {
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

    let _worker = server::worker::spawn_worker(pool.clone());

    let acceptor = TcpListener::new(config.bind.clone()).bind().await;
    tracing::info!("savhub backend listening on {}", config.bind);
    Server::new(acceptor)
        .serve(server::routing::router().hoop(cors))
        .await;
    Ok(())
}
