//! C2 phase 0: async DB helpers built on `diesel-async` + bb8.
//!
//! This module is intentionally tiny: it exposes the bare minimum so that
//! service modules can be ported to `AsyncPgConnection` one at a time
//! without rewriting `app_state` or every handler signature in the same PR.
//!
//! Usage from a ported module:
//!
//! ```ignore
//! use crate::service::async_db::with_async_conn;
//!
//! pub async fn list_things() -> Result<Vec<Thing>, AppError> {
//!     with_async_conn(|conn| {
//!         Box::pin(async move {
//!             use crate::schema::things::dsl::*;
//!             things.select(Thing::as_select()).load(conn).await
//!                 .map_err(Into::into)
//!         })
//!     }).await
//! }
//! ```
//!
//! The closure-based shape lets callers borrow the connection without
//! exposing bb8's `PooledConnection` lifetime through their own signatures.

use std::future::Future;
use std::pin::Pin;

use diesel_async::AsyncPgConnection;

use crate::error::AppError;
use crate::state::app_state;

/// Borrow an async DB connection from the bb8 pool and run `f` against it.
///
/// Errors map to `AppError::Internal` for pool acquisition failures and
/// preserve any `AppError` returned from the body.
pub async fn with_async_conn<F, T>(f: F) -> Result<T, AppError>
where
    F: for<'c> FnOnce(
        &'c mut AsyncPgConnection,
    ) -> Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'c>>,
{
    let pool = app_state().async_pool.clone();
    let mut conn = pool
        .get()
        .await
        .map_err(|error| AppError::Internal(format!("async pool: {error}")))?;
    f(&mut conn).await
}
