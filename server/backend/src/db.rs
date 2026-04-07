use anyhow::{Context, Result};
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::bb8::Pool as AsyncPool;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

pub type PgPool = Pool<ConnectionManager<PgConnection>>;
pub type PgPooledConnection = PooledConnection<ConnectionManager<PgConnection>>;

/// C2 phase 0: async pool that lives alongside the sync `PgPool`. Modules
/// are ported to AsyncPgConnection one at a time; until the migration is
/// finished both pools share the same database URL but are independent so
/// each handler can pick the appropriate flavor.
pub type AsyncPgPool = AsyncPool<AsyncPgConnection>;

pub const DEFAULT_DATABASE_POOL_MAX_SIZE: u32 = 32;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

pub fn configured_pool_max_size() -> u32 {
    std::env::var("DATABASE_POOL_MAX_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_DATABASE_POOL_MAX_SIZE)
}

pub fn new_pool(database_url: &str) -> Result<PgPool> {
    let max_size = configured_pool_max_size();
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::builder()
        .max_size(max_size)
        .connection_timeout(std::time::Duration::from_secs(5))
        .build(manager)
        .context("failed to create PostgreSQL pool")
}

/// Build an async bb8 pool. Sized identically to the sync pool by default;
/// callers can tune via `DATABASE_POOL_MAX_SIZE` (shared during migration).
pub async fn new_async_pool(database_url: &str) -> Result<AsyncPgPool> {
    let max_size = configured_pool_max_size();
    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url.to_string());
    AsyncPool::builder()
        .max_size(max_size)
        .connection_timeout(std::time::Duration::from_secs(5))
        .build(manager)
        .await
        .context("failed to create async PostgreSQL pool")
}

pub fn run_migrations(conn: &mut PgConnection) -> Result<()> {
    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|error| anyhow::anyhow!("failed to run diesel migrations: {error}"))?;
    Ok(())
}
