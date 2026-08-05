//! Application state.
//!
//! Bounded on purpose: everything reachable from a handler is listed here, so
//! "what can this endpoint touch?" is answered by reading one struct rather
//! than by tracing imports.

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// The commit this binary was built from.
///
/// Resolved at compile time from `REPOLENS_BUILD_SHA`, which the container
/// build sets. `option_env!` rather than `env!` so a local `cargo run` still
/// compiles — a developer build genuinely does not have a deployed SHA, and
/// reporting `unknown` is more honest than failing to build or inventing one.
pub const BUILD_SHA: &str = match option_env!("REPOLENS_BUILD_SHA") {
    Some(sha) => sha,
    None => "unknown",
};

/// Ceiling on pooled connections held by one instance.
///
/// Neon's free plan allows a small connection budget shared across every
/// compute, and Cloud Run may run several instances. A pool that sized itself
/// for one machine would exhaust the project's budget as soon as it scaled, so
/// this is deliberately small and is measured in issue #9 rather than guessed
/// at again later.
const MAX_POOL_CONNECTIONS: u32 = 5;

/// Everything a request handler may reach.
#[derive(Clone)]
pub struct AppState {
    /// Pooled database handle, absent when `DATABASE_URL` is not configured.
    ///
    /// Optional so the server still starts for frontend work without a
    /// database. The probe then reports the database as unavailable, which is
    /// the truthful answer — better than refusing to boot and blocking UI
    /// development, and better than pretending the database is fine.
    pool: Option<PgPool>,
}

impl AppState {
    /// Builds state with a live pool.
    #[must_use]
    pub fn with_pool(pool: PgPool) -> Self {
        Self { pool: Some(pool) }
    }

    /// Builds state with no database configured.
    #[must_use]
    pub fn without_database() -> Self {
        Self { pool: None }
    }

    /// Borrows the pool, if one was configured.
    #[must_use]
    pub fn pool(&self) -> Option<&PgPool> {
        self.pool.as_ref()
    }

    /// Connects lazily to `url`.
    ///
    /// Lazy on purpose: Neon scale-to-zero suspends an idle compute, so the
    /// first connection can take seconds while it resumes. Connecting eagerly
    /// at startup would push that latency into container start, where Cloud Run
    /// counts it against the startup probe rather than against a request.
    ///
    /// # Errors
    ///
    /// Returns an error when the URL cannot be parsed into connection options.
    pub fn connect_lazy(url: &str) -> Result<PgPool, sqlx::Error> {
        PgPoolOptions::new()
            .max_connections(MAX_POOL_CONNECTIONS)
            .connect_lazy(url)
    }
}
