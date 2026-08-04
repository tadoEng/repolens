//! Durable worker role.
//!
//! **Always run-once.** A Cloud Run Job starts one execution and exits, so
//! there is no `--once` flag and no argument parser to hold one — which is
//! precisely why this workspace needs no CLI-parsing dependency. An always-on
//! polling worker would need `min-instances >= 1`, which bills continuously and
//! would falsify the free-stack thesis RepoLens exists to test.
//!
//! For local iteration set `REPOLENS_WORKER_LOOP=1` to repeat the same
//! run-once body on an interval. That is a development convenience and is never
//! set in a deployed environment.

use std::time::Duration;

use repolens_server::telemetry;

/// Interval between iterations when looping locally.
const LOCAL_POLL_INTERVAL: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init();

    let repeat = std::env::var("REPOLENS_WORKER_LOOP")
        .is_ok_and(|value| matches!(value.trim(), "1" | "true" | "TRUE"));

    loop {
        run_once().await?;

        if !repeat {
            break;
        }

        tokio::time::sleep(LOCAL_POLL_INTERVAL).await;
    }

    Ok(())
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
