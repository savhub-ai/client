use anyhow::{Context, Result};
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

pub type PgPool = Pool<ConnectionManager<PgConnection>>;
pub type PgPooledConnection = PooledConnection<ConnectionManager<PgConnection>>;
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
        .build(manager)
        .context("failed to create PostgreSQL pool")
}

pub fn run_migrations(conn: &mut PgConnection) -> Result<()> {
    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|error| anyhow::anyhow!("failed to run diesel migrations: {error}"))?;
    Ok(())
}
