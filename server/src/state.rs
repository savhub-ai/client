use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use once_cell::sync::OnceCell;
use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::config::Config;
use crate::db::{PgPool, run_migrations};

/// Real-time event broadcast to WebSocket clients.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    #[serde(rename = "index_progress")]
    IndexProgress {
        job_id: Uuid,
        status: String,
        progress_pct: i32,
        progress_message: String,
        result_data: serde_json::Value,
        error_message: Option<String>,
    },
}

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pool: PgPool,
    pub events_tx: broadcast::Sender<WsEvent>,
    /// Lock that serialises all writes to the registry git repo.
    pub registry_lock: Arc<Mutex<()>>,
    /// Per-repo locks that serialise clone/pull operations so that
    /// `collect_skill_candidates` never runs against a partially-cloned checkout.
    pub repo_checkout_locks: Arc<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>>,
    /// Semaphore limiting concurrent AI chat requests (flock/repo metadata).
    pub ai_chat_semaphore: Arc<tokio::sync::Semaphore>,
    /// Semaphore limiting concurrent AI security scan requests.
    pub ai_security_semaphore: Arc<tokio::sync::Semaphore>,
}

impl AppState {
    /// Returns a per-repo async lock so that only one clone/pull can run at a
    /// time for a given checkout directory.  Callers should hold the returned
    /// guard until the checkout is fully ready (clone finished, HEAD resolved).
    pub fn repo_checkout_lock(&self, repo_dir: &PathBuf) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = self
            .repo_checkout_locks
            .lock()
            .expect("repo_checkout_locks poisoned");
        map.entry(repo_dir.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }
}

static APP_STATE: OnceCell<Arc<AppState>> = OnceCell::new();

pub fn init_state(config: Config, pool: PgPool) -> Result<Arc<AppState>> {
    let (events_tx, _) = broadcast::channel::<WsEvent>(256);
    let ai_chat_concurrency = config.ai_chat_concurrency.max(1);
    let ai_security_concurrency = config.ai_security_concurrency.max(1);
    let state = Arc::new(AppState {
        config,
        pool,
        events_tx,
        registry_lock: Arc::new(Mutex::new(())),
        repo_checkout_locks: Arc::new(Mutex::new(HashMap::new())),
        ai_chat_semaphore: Arc::new(tokio::sync::Semaphore::new(ai_chat_concurrency)),
        ai_security_semaphore: Arc::new(tokio::sync::Semaphore::new(ai_security_concurrency)),
    });
    APP_STATE
        .set(state.clone())
        .map_err(|_| anyhow!("application state already initialized"))?;
    Ok(state)
}

pub fn app_state() -> &'static Arc<AppState> {
    APP_STATE
        .get()
        .expect("application state accessed before initialization")
}

pub fn migrate_app_db() -> Result<()> {
    let state = app_state();
    let mut conn = state
        .pool
        .get()
        .map_err(|error| anyhow!("failed to get a DB connection for migrations: {error}"))?;
    run_migrations(&mut conn)
}
