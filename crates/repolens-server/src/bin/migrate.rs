//! Migration role.
//!
//! Migrations are compiled into this binary by `sqlx::migrate!`, so a deployed
//! environment needs no migration files on disk and no CLI in the image.
//! `sqlx-cli` authors migrations locally and must never ship in the container.
//!
//! Runs against the **direct** database endpoint: schema changes rely on
//! session-level behaviour that a connection pooler restricts.

use anyhow::Context as _;
use repolens_server::{config, telemetry};
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;

/// Migrations live at the workspace root rather than inside this crate, since
/// they describe the product's schema and not this binary's implementation.
static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Before telemetry, so RUST_LOG from .env.local is honoured by the
    // subscriber this call installs.
    config::load_dotenv();
    telemetry::init();

    let url = config::database_direct_url().context("resolving the direct database URL")?;

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .context("connecting to the database (direct endpoint)")?;

    MIGRATOR
        .run(&pool)
        .await
        .context("applying pending migrations")?;

    pool.close().await;

    tracing::info!(applied = MIGRATOR.iter().len(), "migrations up to date");
    Ok(())
}
