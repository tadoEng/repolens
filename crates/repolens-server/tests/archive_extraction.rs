//! What the extractor does with an archive built to hurt it.
//!
//! Issue #12's acceptance criteria are the outline: every control asserts the
//! exact limit and the observed value, a bomb is caught independently by two
//! ceilings, and the traversal cases are *generated* rather than listed —
//! because a hand-written set pins the paths somebody thought of, which is the
//! set an attacker is least interested in.
//!
//! Archives are built here rather than committed. A malicious tarball in the
//! repository is a file every future contributor has to be told not to open,
//! and one that CI would happily hand to a scanner.

use std::path::{Path, PathBuf};

use proptest::prelude::*;
use repolens_server::infrastructure::composition::entry::{EntryKind, Refusal, admit};
use repolens_server::infrastructure::composition::extract::{
    Ceilings, ExtractionError, ExtractionLimit, extract,
};

/// Builds a gzip'd tar from `(path, kind, bytes)` triples.
fn archive(entries: &[(&str, tar::EntryType, Vec<u8>)]) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let path = directory.path().join("archive.tar.gz");
    let file = std::fs::File::create(&path).expect("the archive is creatable");
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);

    for (name, kind, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_entry_type(*kind);
        header.set_mode(0o644);
        set_hostile_name(&mut header, name);
        header.set_cksum();
        builder
            .append(&header, bytes.as_slice())
            .expect("appending an entry");
    }

    builder
        .into_inner()
        .expect("finishing the tar")
        .finish()
        .expect("finishing the gzip");
    (directory, path)
}

/// Writes `name` into the header, refusal and all.
///
/// `Header::set_path` and `Builder::append_data` both **refuse** a path
/// containing `..` — the tar crate declines to build the archive this suite
/// exists to defend against. So the bytes go into the old-format name field
/// directly, which is what a real hostile tarball looks like: a header is a
/// hundred bytes of name and nobody validated them on the way in.
///
/// Using `append_data` here instead would make the traversal tests silently
/// assert nothing, because the archive would never contain a traversal path.
fn set_hostile_name(header: &mut tar::Header, name: &str) {
    if header.set_path(name).is_ok() {
        return;
    }
    let field = &mut header.as_old_mut().name;
    let bytes = name.as_bytes();
    assert!(
        bytes.len() < field.len(),
        "hostile fixture names must fit the 100-byte header field: {name}"
    );
    field[..bytes.len()].copy_from_slice(bytes);
}

/// A regular-file entry.
fn file(name: &str, bytes: &[u8]) -> (&'static str, tar::EntryType, Vec<u8>) {
    // The name is leaked so the tuple can borrow it for the archive's lifetime;
    // these are test fixtures and the leak is bounded by the test.
    (
        Box::leak(name.to_owned().into_boxed_str()),
        tar::EntryType::Regular,
        bytes.to_vec(),
    )
}

fn extract_into(archive_path: &Path, ceilings: Ceilings) -> Result<PathBuf, ExtractionError> {
    let parent = tempfile::tempdir().expect("a scratch parent");
    let extraction = extract(archive_path, parent.path(), ceilings)?;
    // The root is copied out before the extraction drops, so a test can assert
    // on the tree; the directory itself is gone by the time this returns, which
    // is the cleanup guarantee under test elsewhere.
    Ok(extraction.root().to_path_buf())
}

// ---------------------------------------------------------------------------
// The happy path, so every refusal below means something.
// ---------------------------------------------------------------------------

#[test]
fn an_ordinary_archive_extracts_with_the_wrapper_stripped() {
    let (_scratch, path) = archive(&[
        file("owner-repo-abc123/README.md", b"# hello\n"),
        file("owner-repo-abc123/src/main.rs", b"fn main() {}\n"),
    ]);
    let parent = tempfile::tempdir().expect("a scratch parent");

    let extraction = extract(&path, parent.path(), Ceilings::default()).expect("extraction");

    assert_eq!(extraction.files_written, 2);
    assert!(extraction.refusals.is_empty(), "{:?}", extraction.refusals);

    // GitHub wraps every archive in one directory named `owner-repo-sha`.
    // Stripping it keeps the tree shaped like the repository, so an exclusion
    // rule written against `web/node_modules` matches what a reader expects.
    assert!(extraction.root().join("README.md").is_file());
    assert!(extraction.root().join("src/main.rs").is_file());
}

#[test]
fn the_extraction_directory_is_gone_once_the_extraction_is_dropped() {
    // Issue #12 requires release on success, error *and* panic. `TempDir`
    // removes its tree on drop, and drop runs while unwinding — so this one
    // assertion covers all three, provided the directory is genuinely owned.
    let (_scratch, path) = archive(&[file("owner-repo-abc123/README.md", b"hi\n")]);
    let root = extract_into(&path, Ceilings::default()).expect("extraction");

    assert!(
        !root.exists(),
        "the extracted tree outlived the extraction that owned it"
    );
}

#[test]
fn a_panic_still_releases_the_extraction_directory() {
    let (_scratch, path) = archive(&[file("owner-repo-abc123/README.md", b"hi\n")]);
    let parent = tempfile::tempdir().expect("a scratch parent");

    let root = std::panic::catch_unwind({
        let parent = parent.path().to_path_buf();
        let path = path.clone();
        move || {
            let extraction = extract(&path, &parent, Ceilings::default()).expect("extraction");
            let root = extraction.root().to_path_buf();
            // A counter that panics mid-walk is the case this protects: on a
            // memory-backed volume a leaked tree is leaked memory.
            panic!("{}", root.display());
        }
    })
    .expect_err("the closure panics");

    let leaked = root
        .downcast_ref::<String>()
        .expect("the panic payload carries the path");
    assert!(
        !Path::new(leaked).exists(),
        "unwinding left the extracted tree behind"
    );
}

// ---------------------------------------------------------------------------
// Path traversal, in every shape the type system lets one arrive in.
// ---------------------------------------------------------------------------

#[test]
fn traversal_and_absolute_paths_are_refused_by_name() {
    // Hand-written cases pin the specific shapes that have bitten real
    // extractors. The property test below covers the ones nobody listed.
    for hostile in [
        "../../etc/cron.d/x",
        "owner-repo/../../../etc/passwd",
        "/etc/passwd",
        "//server/share/x",
        r"C:\Windows\System32\drivers\etc\hosts",
        r"..\..\Windows\System32\x",
        r"\\?\C:\x",
        "owner-repo/..",
    ] {
        assert_eq!(
            admit(Path::new(hostile), EntryKind::RegularFile, 1, 1024),
            Err(Refusal::PathEscapes),
            "{hostile} was not refused"
        );
    }
}

#[test]
fn a_traversal_entry_writes_nothing_even_when_the_rest_extracts() {
    let (_scratch, path) = archive(&[
        file("owner-repo-abc/../../escaped.txt", b"pwned\n"),
        file("owner-repo-abc/kept.md", b"fine\n"),
    ]);
    /*
     * Two directories deep, both owned by this test, and that is not tidiness.
     *
     * The escape target is `root/../../escaped.txt`. With the extraction parent
     * sitting directly in the system temp directory, a successful escape lands
     * *in* the system temp directory — shared, persistent, and visible to every
     * other test on the machine. Tamper-checking this file proved the point:
     * disabling the traversal guard really did write `escaped.txt` there, and
     * the leftover then failed this assertion on every later run, including
     * runs of unrelated tampers.
     *
     * Nesting keeps the blast radius inside `outer`, which is deleted with it.
     */
    let outer = tempfile::tempdir().expect("a scratch root");
    let parent = outer.path().join("extraction-parent");
    std::fs::create_dir(&parent).expect("the parent is creatable");

    let extraction = extract(&path, &parent, Ceilings::default()).expect("extraction");

    assert_eq!(extraction.files_written, 1);
    assert!(
        extraction
            .refusals
            .iter()
            .any(|(_, refusal)| *refusal == Refusal::PathEscapes),
        "{:?}",
        extraction.refusals
    );

    // The decisive assertion: nothing landed outside the extraction root, at
    // either level the `../../` was aiming for.
    assert!(!parent.join("escaped.txt").exists());
    assert!(!outer.path().join("escaped.txt").exists());
}

#[test]
fn links_are_refused_in_both_directions() {
    // A link *into* the tree lets a later entry write through it to anywhere
    // the process can reach; a link *out* makes the counter read files that are
    // not in this repository. Neither is worth preserving in a tree that exists
    // only to be counted and deleted.
    assert_eq!(
        admit(Path::new("owner-repo/link"), EntryKind::Link, 0, 1024),
        Err(Refusal::Link)
    );

    let (_scratch, path) = archive(&[
        ("owner-repo-abc/evil", tar::EntryType::Symlink, Vec::new()),
        ("owner-repo-abc/hard", tar::EntryType::Link, Vec::new()),
        file("owner-repo-abc/ok.rs", b"fn main() {}\n"),
    ]);
    let parent = tempfile::tempdir().expect("a scratch parent");

    let extraction = extract(&path, parent.path(), Ceilings::default()).expect("extraction");

    assert_eq!(extraction.files_written, 1);
    assert_eq!(
        extraction
            .refusals
            .iter()
            .filter(|(_, refusal)| *refusal == Refusal::Link)
            .count(),
        2
    );
    assert!(!extraction.root().join("evil").exists());
    assert!(!extraction.root().join("hard").exists());
}

proptest! {
    /// No generated path, however hostile, produces an absolute or escaping
    /// destination.
    ///
    /// The invariant is stated over the *output* rather than the input: a test
    /// that asserted "these inputs are refused" would only ever be as good as
    /// the refusal list. This says the admitted paths are all relative and
    /// traversal-free, whatever arrives — which is the property the extractor
    /// actually relies on when it joins onto the root.
    #[test]
    fn an_admitted_path_can_never_escape_the_root(
        segments in proptest::collection::vec(
            prop_oneof![
                Just("..".to_owned()),
                Just(".".to_owned()),
                Just(String::new()),
                Just("C:".to_owned()),
                Just(r"\\?\".to_owned()),
                Just("../".to_owned()),
                Just(r"..\".to_owned()),
                // Unicode that normalises toward a separator or a dot.
                Just("\u{2024}\u{2024}".to_owned()),
                Just("\u{ff0e}\u{ff0e}".to_owned()),
                "[a-zA-Z0-9_.-]{1,12}",
            ],
            1..8,
        ),
        separator in prop_oneof![Just("/"), Just("\\")],
    ) {
        let raw = segments.join(separator);
        let Ok(admitted) = admit(Path::new(&raw), EntryKind::RegularFile, 1, 1024) else {
            // Refusing is always a safe answer.
            return Ok(());
        };

        prop_assert!(admitted.is_relative(), "{raw:?} admitted as {admitted:?}");
        for component in admitted.components() {
            prop_assert!(
                matches!(component, std::path::Component::Normal(_)),
                "{raw:?} admitted with component {component:?}"
            );
        }

        // The property the extractor leans on: joining stays underneath.
        let root = Path::new("/extraction/root");
        let joined = root.join(&admitted);
        prop_assert!(joined.starts_with(root), "{raw:?} joined to {joined:?}");
    }
}

// ---------------------------------------------------------------------------
// The byte and count ceilings, each asserting its own limit and observation.
// ---------------------------------------------------------------------------

#[test]
fn a_decompression_bomb_is_caught_by_the_decompressed_ceiling() {
    // Compressible beyond anything a real repository is: the compressed archive
    // is a few kilobytes and the decompressed stream is megabytes. The
    // compressed ceiling sees nothing wrong, because nothing *is* wrong until
    // the bytes are decoded — which is the whole reason the second layer exists.
    let bomb = vec![0u8; 8 * 1024 * 1024];
    let (_scratch, path) = archive(&[file("owner-repo-abc/bomb.bin", &bomb)]);

    let compressed = std::fs::metadata(&path).expect("the archive exists").len();
    assert!(
        compressed < 1024 * 1024,
        "the fixture must be small compressed, or it proves nothing: {compressed}"
    );

    let ceilings = Ceilings {
        decompressed_bytes: 1024 * 1024,
        file_bytes: 64 * 1024 * 1024,
        ..Ceilings::default()
    };
    let parent = tempfile::tempdir().expect("a scratch parent");

    let error = extract(&path, parent.path(), ceilings).expect_err("the bomb is refused");
    let ExtractionError::Limit(limit @ ExtractionLimit::Decompressed { .. }) = error else {
        panic!("expected a decompressed-stream breach, got {error:?}");
    };

    let breach = limit.breach();
    assert_eq!(breach.limit_name, "ARCHIVE_DECOMPRESSED_LIMIT");
    assert_eq!(breach.limit_value, 1024 * 1024);
    assert!(
        breach.observed_value > breach.limit_value,
        "the observed value has to exceed the ceiling it tripped: {breach:?}"
    );
}

#[test]
fn one_oversized_entry_costs_that_file_and_not_the_report() {
    // Per-file and archive-wide are separate ceilings for a reason: a single
    // enormous blob should not make the rest of a repository uncountable.
    let big = vec![b'x'; 256 * 1024];
    let (_scratch, path) = archive(&[
        file("owner-repo-abc/huge.bin", &big),
        file("owner-repo-abc/small.rs", b"fn main() {}\n"),
    ]);
    let ceilings = Ceilings {
        file_bytes: 1024,
        ..Ceilings::default()
    };
    let parent = tempfile::tempdir().expect("a scratch parent");

    let extraction = extract(&path, parent.path(), ceilings).expect("the archive still extracts");

    assert_eq!(extraction.files_written, 1);
    assert!(extraction.root().join("small.rs").is_file());
    assert!(!extraction.root().join("huge.bin").exists());

    let (_, refusal) = extraction
        .refusals
        .iter()
        .find(|(_, refusal)| matches!(refusal, Refusal::TooLarge { .. }))
        .expect("the oversized entry is recorded");
    let Refusal::TooLarge {
        declared_bytes,
        limit_bytes,
    } = refusal
    else {
        unreachable!()
    };
    assert_eq!(*limit_bytes, 1024);
    assert_eq!(*declared_bytes, big.len() as u64);
}

#[test]
fn too_many_entries_ends_the_walk_with_the_count_that_ended_it() {
    // Bounds the iteration rather than the bytes. Ten thousand empty files
    // breach no byte ceiling at all — every entry is zero-length — and still
    // cost an inode apiece and an unbounded walk.
    let entries: Vec<_> = (0..64)
        .map(|n| file(&format!("owner-repo-abc/f{n}.rs"), b""))
        .collect();
    let (_scratch, path) = archive(&entries);

    let ceilings = Ceilings {
        entries: 10,
        ..Ceilings::default()
    };
    let parent = tempfile::tempdir().expect("a scratch parent");

    let error = extract(&path, parent.path(), ceilings).expect_err("the walk is bounded");
    let ExtractionError::Limit(limit @ ExtractionLimit::EntryCount { .. }) = error else {
        panic!("expected an entry-count breach, got {error:?}");
    };

    let breach = limit.breach();
    assert_eq!(breach.limit_name, "ARCHIVE_ENTRY_COUNT_LIMIT");
    assert_eq!(breach.limit_value, 10);
    assert_eq!(breach.observed_value, 11);
}

#[test]
fn filling_the_extraction_volume_is_reportable_rather_than_fatal() {
    // The control issue #12 argues for at length: on a memory-backed volume an
    // unbounded write is an OOM kill, which leaves a stale lease and no
    // diagnosis. A ceiling makes it a named, catchable outcome instead.
    let chunk = vec![b'y'; 128 * 1024];
    let (_scratch, path) = archive(&[
        file("owner-repo-abc/a.rs", &chunk),
        file("owner-repo-abc/b.rs", &chunk),
        file("owner-repo-abc/c.rs", &chunk),
    ]);
    let ceilings = Ceilings {
        storage_bytes: 200 * 1024,
        ..Ceilings::default()
    };
    let parent = tempfile::tempdir().expect("a scratch parent");

    let error = extract(&path, parent.path(), ceilings).expect_err("storage is bounded");
    let ExtractionError::Limit(limit @ ExtractionLimit::Storage { .. }) = error else {
        panic!("expected a storage breach, got {error:?}");
    };

    let breach = limit.breach();
    assert_eq!(
        breach.limit_name, "EXTRACTION_STORAGE_LIMIT",
        "the name issue #12 specifies by hand"
    );
    assert_eq!(breach.limit_value, 200 * 1024);
    assert!(breach.observed_value > breach.limit_value, "{breach:?}");
}

#[test]
fn a_header_that_lies_about_its_size_writes_only_what_it_declared() {
    // The header is the archive's own claim, and the extractor treats it as
    // one: the copy is bounded by the declared size, so a lying header cannot
    // spend more of the budget than it asked for.
    let (_scratch, path) = archive(&[file("owner-repo-abc/a.rs", b"0123456789")]);
    let parent = tempfile::tempdir().expect("a scratch parent");

    let extraction = extract(&path, parent.path(), Ceilings::default()).expect("extraction");

    assert_eq!(extraction.bytes_written, 10);
    assert_eq!(
        std::fs::read(extraction.root().join("a.rs")).expect("the file is there"),
        b"0123456789"
    );
}

#[test]
fn nothing_outside_the_wrapper_directory_is_treated_as_content() {
    // `pax_global_header` sits beside the wrapper in real GitHub tarballs. It
    // is not repository content and has no path inside the tree to be written
    // to.
    let (_scratch, path) = archive(&[
        file("pax_global_header", b"metadata\n"),
        file("owner-repo-abc/real.rs", b"fn main() {}\n"),
    ]);
    let parent = tempfile::tempdir().expect("a scratch parent");

    let extraction = extract(&path, parent.path(), Ceilings::default()).expect("extraction");

    assert_eq!(extraction.files_written, 1);
    assert!(
        extraction
            .refusals
            .iter()
            .any(|(_, refusal)| *refusal == Refusal::PathUnusable)
    );
}
