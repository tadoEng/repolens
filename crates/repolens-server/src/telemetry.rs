//! Tracing setup, shared by all three binaries.

use tracing_subscriber::EnvFilter;

/// Replaces the process-wide panic hook so a payload never reaches stderr.
///
/// **Catching the unwind is not enough, and this is the part that is easy to
/// get wrong.** Rust runs the panic hook *before* unwinding begins, and the
/// default hook writes the payload — the whole formatted message — to stderr.
/// By the time `CatchPanicLayer` receives the `Box<dyn Any>` and drops it
/// unread, the disclosure has already happened. Dropping it governs the HTTP
/// response; only replacing the hook governs the log.
///
/// A panic message is built by whatever code panicked, so it can carry a
/// connection string, an internal path, or a fragment of a repository file the
/// handler was holding. Container stderr is collected, retained, and searched
/// by whoever runs the deployment.
///
/// The payload is therefore never read here. The location is: it names a file
/// and line in this workspace's own source, which is what makes a panic
/// actionable and cannot contain runtime data.
///
/// Install this **first** in `main`, before loading `.env` files and before the
/// subscriber exists. A panic while parsing a `.env` line is exactly the kind
/// that carries a credential, and the hook has to already be in place for it.
/// Until the subscriber is installed nothing is written at all, which is the
/// safe direction to fail.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        // `info.payload()` and `info.to_string()` both render the message.
        // Neither is called, deliberately.
        let location = info.location().map(ToString::to_string);

        tracing::error!(
            location = location.as_deref().unwrap_or("unknown"),
            "a panic occurred; the payload is deliberately not recorded"
        );
    }));
}

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
