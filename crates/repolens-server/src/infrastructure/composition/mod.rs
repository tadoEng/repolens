//! Counting a repository from its commit archive, safely.
//!
//! GitHub's language endpoint reports **bytes, not lines**, so it cannot answer
//! the question a reader is actually asking — how big is this repository, and
//! what is it mostly made of. One archive request can, and it costs less
//! rate-limit budget than the hundreds of blob fetches the same answer would
//! otherwise take.
//!
//! # The archive is transport, never evidence
//!
//! Identity stays where it was: owner, repository, commit SHA, tree SHA. The
//! tarball is a means of getting the bytes and nothing more, and in particular
//! its hash is **never** persisted. GitHub does not guarantee the tarball for a
//! fixed commit is byte-stable over time, so recording its digest would make
//! two honest runs of the same commit disagree — it would break reproducibility
//! while looking exactly like a proof of it.
//!
//! What is persisted instead is the commit SHA, the counter's version, the
//! exclusion-policy version, and the manifest of what was counted. Those are
//! stable, and together they are what makes two runs comparable.
//!
//! # This is a second ingestion mode, stated out loud
//!
//! Issue #4 says the collector does not download a complete repository by
//! default, and that rule still holds for the identity and evidence collectors
//! — they read a tree listing and a bounded set of blobs. Archive download is a
//! separate mode with a separate justification, entered deliberately rather
//! than by quietly reinterpreting the old rule.
//!
//! # Layout
//!
//! * [`limits`] — the seven controls, with what each one is protecting.
//! * [`classification`] — what a counted file is, and where it lives.
//! * [`counter`] — Tokei, behind the domain's counting contract.
//! * [`entry`] — whether one archive entry may be written, as a pure function.
//! * [`exclusion`] — what is left out of the count, and under which rule.
//! * [`extract`] — the bounded, self-cleaning extraction itself.
//! * [`summary`] — the count, projected into what the report publishes.
//!
//! [`compose`] is the production path through them, in that order.

pub mod classification;
pub mod counter;
pub mod entry;
pub mod exclusion;
pub mod extract;
pub mod limits;
pub mod summary;

use std::path::Path;
use std::time::{Duration, Instant};

use repolens_core::{CommitSha, RepositoryCoordinate};
use repolens_core::{CompositionCounter, CompositionLimitBreach};
use repolens_github::{GitHubRepositorySource, GitHubSourceError};

use counter::{CountedRepository, TokeiCounter};
use extract::{Ceilings, ExtractionError};

/// What one composition attempt produced.
///
/// Three outcomes rather than two, because "a ceiling stopped us" and "we could
/// not get the bytes at all" are different facts about a report. The first is
/// often a property of the repository and always has a limit and an observed
/// value to publish; the second is a retrieval that may succeed next time and
/// has neither. Collapsing them would either invent numbers for a network
/// failure or discard the ones a breach carries.
#[derive(Debug)]
pub enum Composed {
    /// Counting completed inside every ceiling.
    ///
    /// Carries the counter rather than leaving it to be looked up. A counted
    /// run always has one, and a variant that let the caller discover otherwise
    /// is how `composition: null` with no limitation became reachable: the
    /// section quietly returned nothing while the limitation logic, correctly,
    /// had nothing to say about a successful count.
    Counted {
        /// The counts and the manifest.
        counted: Box<CountedRepository>,
        /// What produced them, for the key and the published section.
        counter: CompositionCounter,
    },
    /// A ceiling stopped the run, with the limit and what was seen.
    Limited(CompositionLimitBreach),
    /// The archive could not be retrieved or read at all.
    Unavailable,
}

/// Downloads, extracts and counts one commit, inside every configured ceiling.
///
/// Never returns an error, and that is a decision rather than convenience: the
/// findings are already computed by the time this runs, so a failure here costs
/// the report its composition section and nothing else. Turning it into an
/// analysis failure would throw away work that succeeded in order to report a
/// number that is explicitly allowed to be absent.
///
/// `parent` is the scratch root the archive and its extraction are written
/// under. The storage-byte ceiling is enforced in application code whatever
/// `parent` is, and that is the whole of what this function guarantees.
///
/// It does **not** guarantee that `parent` sits on a physically size-limited
/// mount, and an ordinary temporary directory is **not** equivalent protection.
/// A bounded mount makes an over-large write fail even if the application
/// ceiling were wrong, which matters most on a memory-backed filesystem where
/// an unbounded write is an OOM kill rather than a catchable error. Whether one
/// is provisioned is a deployment fact this code cannot observe, and it is
/// attested at deployment rather than claimed here.
pub async fn compose<S>(
    source: &S,
    coordinate: &RepositoryCoordinate,
    commit: &CommitSha,
    parent: &Path,
) -> Composed
where
    S: GitHubRepositorySource,
{
    let started = Instant::now();
    let ceiling = Duration::from_secs(limits::MAX_DURATION_SECONDS);

    // The wall-clock ceiling covers download plus extraction plus counting,
    // because that is the cost being bounded: an archive that decompresses
    // slowly enough to hold a worker breaches no size limit at all.
    //
    // **It bounds how long the analysis waits, not how long the work runs.**
    // Extraction and counting are `spawn_blocking`, and Tokio cannot abort a
    // blocking task once it has started -- it runs to completion whether or not
    // anything is still awaiting it. So on a breach the report stops waiting
    // and says so, while the thread may still be reading an archive and holding
    // its scratch directory until it finishes on its own.
    //
    // That is a real limit of this control and it is written down rather than
    // papered over: the byte and entry ceilings are what actually bound the
    // work, and this bounds the analysis. Making it a hard stop needs
    // cancellation inside the extractor's own loop, which is a change to
    // extraction rather than to its caller.
    match tokio::time::timeout(ceiling, attempt(source, coordinate, commit, parent)).await {
        Ok(composed) => composed,
        Err(_elapsed) => {
            // `observed` is how long the analysis waited, which is the
            // ceiling: it stopped waiting at exactly that point rather than
            // overrunning and then measuring. Reporting a larger number would
            // be inventing one, and the work's own duration is not knowable
            // from here precisely because the task cannot be stopped.
            tracing::info!(
                repository = %coordinate,
                limit_seconds = limits::MAX_DURATION_SECONDS,
                "composition exceeded its wall-clock ceiling; the analysis stopped waiting"
            );
            Composed::Limited(CompositionLimitBreach {
                limit_name: limits::names::DURATION.to_owned(),
                limit_value: limits::MAX_DURATION_SECONDS,
                observed_value: started
                    .elapsed()
                    .as_secs()
                    .max(limits::MAX_DURATION_SECONDS),
            })
        }
    }
}

/// One attempt, without the clock.
async fn attempt<S>(
    source: &S,
    coordinate: &RepositoryCoordinate,
    commit: &CommitSha,
    parent: &Path,
) -> Composed
where
    S: GitHubRepositorySource,
{
    // Self-deleting, and holding both the archive and the extraction. The
    // archive is transport: nothing about it is persisted, and its bytes must
    // not outlive the count that read them.
    let Ok(scratch) = tempfile::Builder::new()
        .prefix("repolens-composition-")
        .tempdir_in(parent)
    else {
        tracing::warn!(repository = %coordinate, "no scratch directory for composition");
        return Composed::Unavailable;
    };

    let archive = scratch.path().join("commit.tar.gz");
    if let Err(error) = source
        .download_archive(coordinate, commit, limits::MAX_COMPRESSED_BYTES, &archive)
        .await
    {
        return match error {
            // Matched on the *variant*, never on the phrase inside it.
            //
            // This read `limit_name == COMPRESSED_STREAM` and was wrong in
            // production: the downloader says "archive compressed bytes", which
            // is a message for a human, while `ARCHIVE_COMPRESSED_LIMIT` is
            // report vocabulary. The two never matched, so a genuinely
            // oversized archive lost both its numbers and reported itself as a
            // retrieval failure. Every component was locally correct; the
            // producer and the consumer disagreed across the boundary.
            //
            // `download_archive` has exactly one ceiling -- the compressed
            // budget this call passed it -- so the translation is total rather
            // than a guess, and the name comes from the contract that publishes
            // it.
            GitHubSourceError::LimitExceeded {
                limit, observed, ..
            } => Composed::Limited(CompositionLimitBreach {
                limit_name: limits::names::COMPRESSED_STREAM.to_owned(),
                limit_value: limit,
                observed_value: observed,
            }),
            // Anything else is a retrieval that may succeed next time. Logged
            // by category; the URL and the response body never reach a log.
            other => {
                tracing::info!(
                    repository = %coordinate,
                    error = %other,
                    "the commit archive could not be downloaded"
                );
                Composed::Unavailable
            }
        };
    }

    // Extraction and counting are CPU- and IO-bound and deliberately
    // synchronous, so they run off the async worker rather than blocking it.
    // The scratch directory is moved in so it outlives the download and is
    // dropped — deleting the archive and the extracted tree — the moment the
    // count returns.
    let repository = coordinate.clone();
    let counted = tokio::task::spawn_blocking(move || {
        let extraction = extract::extract(&archive, scratch.path(), Ceilings::default())?;
        TokeiCounter
            .count(extraction.root())
            .map_err(|error| ExtractionError::Io(std::io::Error::other(error)))
    })
    .await;

    match counted {
        Ok(Ok(counted)) => Composed::Counted {
            counter: CompositionCounter::new(counter::COUNTER_NAME, counted.tokei_version),
            counted: Box::new(counted),
        },
        Ok(Err(ExtractionError::Limit(limit))) => Composed::Limited(limit.breach()),
        Ok(Err(ExtractionError::Io(error))) => {
            tracing::info!(
                repository = %repository,
                error = %error,
                "the commit archive could not be read"
            );
            Composed::Unavailable
        }
        // The blocking task panicked or was cancelled. Neither is a fact about
        // the repository, and neither has a limit to publish.
        Err(error) => {
            tracing::error!(
                repository = %repository,
                error = %error,
                "the composition task did not finish"
            );
            Composed::Unavailable
        }
    }
}
