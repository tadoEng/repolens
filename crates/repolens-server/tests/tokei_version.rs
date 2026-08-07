//! The gate that keeps `TOKEI_VERSION` from lying.
//!
//! `TOKEI_VERSION` is persisted as report metadata and is part of what makes
//! two counts comparable — it records which language definitions decided that a
//! file was Rust. It is also a string written by hand next to a version pinned
//! in `Cargo.toml`, which makes it a duplication, and a duplication in
//! provenance data is a report that can claim counts from a release that did
//! not produce them.
//!
//! Tokei exposes no constant to read and a build script is a lot of machinery
//! for one string, so the duplication stays and this makes it loud: bump the
//! dependency without touching the constant and these fail.
//!
//! Both files are read, because they fail differently. `Cargo.toml` is what a
//! contributor edits; `Cargo.lock` is what actually built. A requirement that
//! stopped being an exact pin would let the lock move on its own, so that is
//! checked too.

use std::path::{Path, PathBuf};

use repolens_server::infrastructure::composition::counter::TOKEI_VERSION;

/// The workspace root, from this crate's manifest directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate sits two levels below the workspace root")
        .to_path_buf()
}

#[test]
fn the_recorded_version_is_the_one_that_was_built() {
    // `Cargo.lock` is the authority on what actually linked. A constant that
    // disagrees with it describes a different program than the one that ran.
    let lock = std::fs::read_to_string(workspace_root().join("Cargo.lock"))
        .expect("the workspace lock file is readable");

    let locked = lock
        .split("[[package]]")
        .find(|block| block.contains("name = \"tokei\""))
        .and_then(|block| {
            block
                .lines()
                .find_map(|line| line.trim().strip_prefix("version = "))
        })
        .map(|value| value.trim().trim_matches('"').to_owned())
        .expect("tokei is in the lock file");

    assert_eq!(
        locked, TOKEI_VERSION,
        "\nCargo.lock builds tokei {locked}, and every report claims {TOKEI_VERSION}. \
         Update TOKEI_VERSION in infrastructure::composition::counter.\n"
    );
}

#[test]
fn the_dependency_is_pinned_exactly_and_to_the_recorded_version() {
    /*
     * Two claims in one, because they fail for different reasons.
     *
     * A requirement of `"14.0.0"` rather than `"=14.0.0"` is a caret range in
     * Cargo, so `cargo update` could move to 14.1 and the constant would keep
     * saying 14.0.0 while the lock test still passed on the day of the change
     * and failed only later, for whoever ran it next.
     *
     * Tokei's language detection is the definition of what counts as Rust, so
     * a minor release reclassifying one extension changes every report. That is
     * a change a reproducibility key has to be able to explain, which it cannot
     * do if the version moved without anybody deciding it should.
     */
    let manifest = std::fs::read_to_string(workspace_root().join("Cargo.toml"))
        .expect("the workspace manifest is readable");

    let requirement = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("tokei = "))
        .expect("tokei is a workspace dependency");

    assert!(
        requirement.contains("\"=") || requirement.contains("version = \"="),
        "\ntokei must be pinned exactly, not left to a caret range: {}\n",
        requirement.trim()
    );
    assert!(
        requirement.contains(&format!("={TOKEI_VERSION}")),
        "\nthe manifest pins a different tokei than TOKEI_VERSION ({TOKEI_VERSION}): {}\n",
        requirement.trim()
    );
}
