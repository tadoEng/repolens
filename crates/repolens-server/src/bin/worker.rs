//! Durable worker role.
//!
//! **Always run-once.** A Cloud Run Job starts one execution and exits, so there
//! is no `--once` flag and no argument parser to hold one — which is precisely
//! why this workspace needs no CLI-parsing dependency. An always-on polling
//! worker would need `min-instances >= 1`, which bills continuously and would
//! falsify the free-stack thesis RepoLens exists to test.
//!
//! There is deliberately no loop mode, not even behind an environment variable.
//! A worker that can be a daemon under some configuration is a worker whose
//! shutdown, lease-renewal, and failure semantics have to be correct in two
//! shapes rather than one — and the second shape would be exercised only on a
//! developer's machine, which is the worst place to discover a lease bug. Local
//! iteration repeats the process instead of the body:
//!
//! ```sh
//! while cargo run --bin worker; do sleep 5; done
//! ```
//!
//! That runs the exact code path Cloud Run runs, including startup and exit.

use repolens_server::{config, telemetry};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // First, before anything that can panic. The default panic hook prints the
    // payload to stderr *before* unwinding, so a panic while parsing a `.env`
    // line would copy that line into the log. See `telemetry::install_panic_hook`.
    telemetry::install_panic_hook();

    // Before telemetry, so RUST_LOG from .env.local is honoured by the
    // subscriber this call installs. Reporting is deferred until after, because
    // anything logged here would precede the subscriber and be discarded.
    let dotenv = config::load_dotenv();
    telemetry::init();
    config::report_dotenv(&dotenv);
    run_once().await
}

/// Claims at most one queued analysis, runs it, persists the result, and
/// returns.
///
/// The durable state machine behind this — `SELECT ... FOR UPDATE SKIP LOCKED`
/// claims, explicit leases, abandoned-lease recovery, bounded retries,
/// idempotent effects — is issue #7, and none of it exists yet. Lease recovery
/// matters *more* under Cloud Run Jobs, not less: an execution can be killed by
/// a job timeout or by memory exhaustion during archive extraction, stranding a
/// lease that nothing else will release.
#[expect(
    clippy::unused_async,
    reason = "the signature is the contract; the body awaits PostgreSQL from #7 onward"
)]
async fn run_once() -> anyhow::Result<()> {
    tracing::info!("worker execution finished: no analysis queue exists yet");
    Ok(())
}
