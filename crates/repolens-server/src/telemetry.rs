//! Tracing setup, shared by all three binaries.

use tracing_subscriber::EnvFilter;

/// Installs the process-wide tracing subscriber.
///
/// Human-readable output for now. Cloud Run parses structured JSON logs but
/// keys severity off a `severity` field rather than tracing's `level`, so
/// deployed environments need a small custom formatter or every line lands as
/// "default" severity and log-based alerting is worthless. That formatter
/// belongs with the deployment work (issue #9), not here.
///
/// # Panics
///
/// If a global subscriber is already installed.
pub fn init() {
    // A per-target default would have to name every binary as well as the
    // library, because `server`, `worker`, and `migrate` each log under their
    // own target. Getting that list wrong silently swallows startup lines, so
    // the default is a plain level and `RUST_LOG` narrows it when needed.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}
