//! HTTP API role.
//!
//! One of three binaries sharing a single container image; Cloud Run selects
//! this one by overriding the entrypoint on the Service.

use anyhow::Context as _;
use repolens_github::{GitHubClientConfig, GitHubRestClient};
use repolens_server::state::AppState;
use repolens_server::{api, config, telemetry};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // First, before anything that can panic. The default panic hook prints the
    // payload to stderr *before* unwinding, so a panic while parsing a `.env`
    // line would copy that line into the log. See `telemetry::install_panic_hook`.
    telemetry::install_panic_hook();

    // Immediately after, so uptime is measured from process start rather than
    // from whenever something first asks. Anchoring it lazily would report a
    // process that had been up for days as one that started seconds ago —
    // exactly backwards for the number a cold-start investigation depends on.
    telemetry::process::mark_start();

    // Before telemetry, so RUST_LOG from .env.local is honoured by the
    // subscriber this call installs. Reporting is deferred until after, because
    // anything logged here would precede the subscriber and be discarded.
    let dotenv = config::load_dotenv();
    telemetry::init();
    config::report_dotenv(&dotenv);

    let address = config::bind_address().context("resolving the bind address")?;

    // A missing DATABASE_URL is a warning, not a fatal error. The probe then
    // reports the database as unavailable — which is true — and the frontend
    // can still be developed against a running API. Deployed environments set
    // the variable, and the probe is what proves they did.
    // Always built. The token only raises GitHub's rate-limit ceiling; it never
    // widens what can be read, because only public repositories are analyzed.
    // A deployment without one is slower, not broken, and finding out which is
    // GitHub's job rather than this function's.
    let token = config::github_token();
    let mut github = GitHubClientConfig::new();
    if let Some(token) = token {
        github = github.with_token(token);
    } else {
        tracing::warn!(
            "GH_ANALYSIS_TOKEN is not set; GitHub requests are unauthenticated and limited to \
             roughly sixty an hour"
        );
    }
    let github = GitHubRestClient::new(github).context("building the GitHub client")?;

    let state = match config::database_url() {
        Ok(url) => {
            let pool = AppState::connect_lazy(&url).context("configuring the database pool")?;
            AppState::with_pool(pool, github)
        }
        Err(error) => {
            tracing::warn!(%error, "starting without a database; the system probe will report it as unavailable");
            AppState::without_database(github)
        }
    };

    // Authentication. Absent configuration leaves creation closed rather than
    // open — `api::authenticated` refuses every create with 503 when no
    // verifier is present, which is the safe direction for a variable somebody
    // can forget.
    let state = if let Some(project) = config::firebase_project_id() {
        match repolens_server::auth::FirebaseVerifier::new(&project) {
            Ok(verifier) => {
                tracing::info!(%project, "analysis creation requires a Firebase ID token");
                state.with_verifier(std::sync::Arc::new(verifier))
            }
            Err(error) => {
                tracing::error!(%error, "could not build the token verifier; creation stays closed");
                state
            }
        }
    } else {
        tracing::warn!(
            "FIREBASE_PROJECT_ID is not set, so analysis creation is closed. Reads remain anonymous."
        );
        state
    };

    // Configuration is read here, at the composition root, and handed to the
    // router. `api::build` deliberately reads no environment of its own.
    let cors_allowed_origin = config::cors_allowed_origin();
    let (app, _openapi) = api::build(state, cors_allowed_origin.as_deref());

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
