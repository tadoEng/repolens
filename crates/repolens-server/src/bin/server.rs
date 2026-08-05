//! HTTP API role.
//!
//! One of three binaries sharing a single container image; Cloud Run selects
//! this one by overriding the entrypoint on the Service.

use anyhow::Context as _;
use repolens_server::{api, config, telemetry};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init();

    let address = config::bind_address().context("resolving the bind address")?;
    let (app, _openapi) = api::build();

    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("binding {address}"))?;

    tracing::info!(%address, "repolens server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("serving HTTP")?;

    tracing::info!("repolens server stopped");
    Ok(())
}

/// Resolves when the platform asks the process to stop.
///
/// Cloud Run sends `SIGTERM` before reclaiming an instance. Ignoring it means
/// in-flight requests are cut rather than drained on every revision rollout.
async fn shutdown_signal() {
    let interrupt = async {
        tokio::signal::ctrl_c()
            .await
            .expect("installing the Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("installing the SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => tracing::info!("interrupt received, draining"),
        () = terminate => tracing::info!("SIGTERM received, draining"),
    }
}
