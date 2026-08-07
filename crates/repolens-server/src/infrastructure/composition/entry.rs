//! What an archive entry has to prove before a single byte of it is written.
//!
//! Every decision here is a pure function of the entry's declared path and
//! type, which is what makes it testable without a filesystem and what lets
//! `proptest` throw hostile paths at it by the thousand. Nothing in this module
//! touches the disk; the caller does that only after these say yes.
//!
//! # Why `unpack()` is forbidden
//!
//! `tar::Archive::unpack` writes whatever the tarball declares. It is one call,
//! it is convenient, and it defeats every control below at once — a single
//! `../../etc/cron.d/x` entry escapes the extraction root, and a symlink
//! followed by a write through it escapes without any `..` at all. Issue #12
//! forbids it by name. The iteration in [`super::extract`] exists so that this
//! module gets to answer first.

use std::path::{Component, Path};

/// Why an entry was refused.
///
/// Refusing is not an error in the analysis: a hostile or unusable entry is
/// skipped and recorded, and the repository is still counted. Only the archive
/// *limits* end a run, because those say the machine cannot finish rather than
/// that one file is not worth reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The path escapes the extraction root, or tries to.
    ///
    /// Covers `..` at any depth, an absolute path, a Windows drive letter, and
    /// a UNC or verbatim prefix. All of them are the same attack: decide where
    /// the bytes land instead of letting the extractor decide.
    PathEscapes,
    /// The path is not usable as a relative path at all.
    ///
    /// An empty path, or one whose components are entirely `.`. Not hostile,
    /// but nothing can be written to it either.
    PathUnusable,
    /// A symlink or a hardlink.
    ///
    /// Never extracted, in either direction. A link *into* the tree lets a
    /// later entry write through it to anywhere the process can reach, and a
    /// link out of it makes the counter read files that are not in the
    /// repository. Neither is worth the convenience of preserving links in a
    /// tree that exists only to be counted and deleted.
    Link,
    /// A directory, device, FIFO, or anything else that is not a regular file.
    ///
    /// Directories are created on demand from the file paths themselves, so an
    /// entry for one carries no information the extractor needs.
    NotARegularFile,
}

impl Refusal {
    /// Stable code for the exclusion ledger.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::PathEscapes => "ARCHIVE_PATH_ESCAPES_ROOT",
            Self::PathUnusable => "ARCHIVE_PATH_UNUSABLE",
            Self::Link => "ARCHIVE_ENTRY_IS_LINK",
            Self::NotARegularFile => "ARCHIVE_ENTRY_NOT_A_FILE",
        }
    }

    /// What a reader needs to know about this class of refusal.
    #[must_use]
    pub const fn explanation(self) -> &'static str {
        match self {
            Self::PathEscapes => {
                "An archive entry named a location outside the extraction directory. It was \
                 refused and not written."
            }
            Self::PathUnusable => {
                "An archive entry carried a path nothing can be written to, such as an empty one."
            }
            Self::Link => {
                "An archive entry was a symbolic or hard link. Links are never extracted: one \
                 pointing into the tree lets a later entry write through it, and one pointing \
                 out makes the counter read files that are not in this repository."
            }
            Self::NotARegularFile => {
                "An archive entry was not a regular file. Directories are created from the file \
                 paths themselves, and nothing else in a tarball is source code."
            }
        }
    }
}

/// The entry kinds that reach [`admit`].
///
/// A narrow mirror of `tar::EntryType`, so this module — and every test of it —
/// stays free of the tar crate. The mapping happens once, at the call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file, the only kind that is ever written.
    RegularFile,
    /// A symbolic or hard link.
    Link,
    /// Anything else: directory, device, FIFO, PAX metadata.
    Other,
}

/// Whether this entry may be extracted, and under what relative path.
///
/// Returns the path to write *relative to the extraction root*, already
/// stripped of the single leading directory GitHub wraps every archive in
/// (`owner-repo-sha/`). Rejecting is the default: a component this function
/// does not understand is a component it refuses.
///
/// Size is deliberately **not** decided here. An entry that is too large is not
/// an entry we chose to leave out — it is one the extractor could not process —
/// and issue #12 lists the individual-file ceiling among the seven controls,
/// every one of which ends the run with `UNABLE_TO_VERIFY`. That check lives
/// with the other ceilings in [`super::extract`], which leaves this function
/// about admissibility alone.
///
/// # Errors
///
/// [`Refusal`], which the caller records rather than treating as a failure of
/// the analysis.
pub fn admit(declared_path: &Path, kind: EntryKind) -> Result<std::path::PathBuf, Refusal> {
    match kind {
        EntryKind::RegularFile => {}
        EntryKind::Link => return Err(Refusal::Link),
        EntryKind::Other => return Err(Refusal::NotARegularFile),
    }

    let safe = safe_relative(declared_path)?;

    // GitHub wraps every archive in exactly one directory named
    // `owner-repo-sha`. Stripping it here rather than at the counter keeps the
    // extracted tree shaped like the repository, so an exclusion rule written
    // against `web/node_modules` matches what a reader would expect.
    //
    // Stripped by *position*, never by pattern: matching on a name would let an
    // archive that happens to start with a directory of the right shape rename
    // its own root.
    let mut components = safe.components();
    if components.next().is_none() {
        return Err(Refusal::PathUnusable);
    }
    let stripped: std::path::PathBuf = components.collect();

    // Empty here means the entry *was* the wrapper, or sat beside it at the top
    // of the archive — `pax_global_header`, say. Neither is repository content,
    // and neither has a path inside the tree to be written to.
    if stripped.as_os_str().is_empty() {
        return Err(Refusal::PathUnusable);
    }
    Ok(stripped)
}

/// The path as a plain relative path, or a refusal.
///
/// Component-wise rather than string-wise, deliberately. A textual check for
/// `"../"` misses `..\\` on Windows, misses a path whose separator is encoded
/// differently, and has to be re-derived every time somebody adds a case.
/// `Path::components` normalises the traversal question into one enum, and this
/// accepts exactly one of its variants.
fn safe_relative(path: &Path) -> Result<std::path::PathBuf, Refusal> {
    let mut out = std::path::PathBuf::new();
    let mut any = false;

    for component in path.components() {
        match component {
            Component::Normal(part) => {
                // A component containing a separator is not a component. It
                // means the path was built by joining strings and this piece
                // was never parsed — exactly how a `..` slips past a check that
                // only looked at the pieces.
                let text = part.to_string_lossy();
                if text.contains('/') || text.contains('\\') {
                    return Err(Refusal::PathEscapes);
                }
                if looks_like_a_drive(&text) {
                    return Err(Refusal::PathEscapes);
                }
                out.push(part);
                any = true;
            }
            // `.` carries no information and is safe to drop.
            Component::CurDir => {}
            // Everything else is a way of saying "not where you put me":
            // `..`, a leading `/`, `C:`, `\\?\`, a UNC share.
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Refusal::PathEscapes);
            }
        }
    }

    if any {
        Ok(out)
    } else {
        Err(Refusal::PathUnusable)
    }
}

/// Whether a component is shaped like a Windows drive specifier.
///
/// Checked by hand rather than left to [`Component::Prefix`], which is the
/// point: `Prefix` is produced **only by the Windows path parser**. Production
/// runs on Linux, where `C:/Windows/x` parses as three perfectly ordinary
/// `Normal` components and sails through a `Prefix` check that never fires.
///
/// The existing `C:\Windows\...` test passed for the wrong reason — the
/// backslashes were caught, not the drive. And the shape that actually arrives
/// is `owner-repo-sha/C:/Windows/x`, which becomes drive-shaped only *after*
/// the wrapper is stripped, so the guard has to be per-component rather than a
/// look at the front of the path.
///
/// `C:` and `C:foo` are both refused: the second is a drive-relative path on
/// Windows, which resolves against that drive's current directory rather than
/// against ours.
fn looks_like_a_drive(component: &str) -> bool {
    let mut bytes = component.bytes();
    matches!(
        (bytes.next(), bytes.next()),
        (Some(letter), Some(b':')) if letter.is_ascii_alphabetic()
    )
}
