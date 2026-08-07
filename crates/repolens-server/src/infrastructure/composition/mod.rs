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
//! * [`entry`] — whether one archive entry may be written, as a pure function.
//! * [`extract`] — the bounded, self-cleaning extraction itself.

pub mod entry;
pub mod extract;
pub mod limits;
