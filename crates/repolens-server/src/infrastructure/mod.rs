//! Adapters that give the domain contracts a real implementation.
//!
//! `repolens-core` states what an analyzer needs — a
//! [`RepositoryCompositionCounter`], a rule input, a reproducibility key — in
//! terms that mention no library and no service. The code that satisfies those
//! contracts with actual tar files, actual gzip streams and an actual line
//! counter lives here, one layer out, where a dependency on `tar` or `tokei`
//! costs the domain nothing.
//!
//! That is not layering for its own sake. Issue #12 puts it plainly: the
//! analyzer depends on *counts*, never on Tokei. A rule that reached for the
//! counter directly would make swapping it a change to the meaning of a report
//! rather than a change to how one is produced.
//!
//! [`RepositoryCompositionCounter`]: repolens_core::RepositoryCompositionCounter

pub mod composition;
