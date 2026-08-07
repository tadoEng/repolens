//! Extracting a commit archive without trusting a byte of it.
//!
//! Anyone can submit any public repository, so the tarball is hostile input in
//! the ordinary case, not the exceptional one. The whole design is one rule:
//! **nothing is written until it has been validated**, and validation is
//! [`super::entry::admit`], which touches no filesystem and can therefore be
//! attacked by `proptest` rather than by imagination.
//!
//! # The two independent byte ceilings
//!
//! `Read::take` is applied at two layers, and the layering is the point:
//!
//! ```text
//! file on disk ──take(compressed cap)──▶ gzip ──take(decompressed cap)──▶ tar
//! ```
//!
//! The inner cap is what a compression bomb defeats when it is absent. Four
//! mebibytes of zeroes is an unremarkable download and expands to gigabytes;
//! the compressed ceiling sees nothing wrong, because nothing is wrong until
//! the bytes are decoded. Capping only the decompressed side is equally
//! insufficient — the download itself is unbounded — so both exist, with
//! separate names, separate values, and separate tests.
//!
//! # Cleanup
//!
//! [`tempfile::TempDir`] removes its tree on drop, and drop runs while
//! unwinding. That covers success, early return and panic without a `finally`
//! that a future edit could step around.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use repolens_core::CompositionLimitBreach;

use super::entry::{EntryKind, Refusal, admit};
use super::limits;

/// One archive extracted into a directory that deletes itself.
#[derive(Debug)]
pub struct Extraction {
    /// The directory holding the tree. Removed when this value is dropped.
    directory: tempfile::TempDir,
    /// Files actually written.
    pub files_written: u64,
    /// Decompressed bytes actually written.
    pub bytes_written: u64,
    /// Entries refused, with the reason each time.
    ///
    /// Kept rather than counted, because "we skipped 12 things" is not a
    /// finding a reader can act on and "we refused a symlink at `web/link`" is.
    pub refusals: Vec<(PathBuf, Refusal)>,
}

impl Extraction {
    /// Where the extracted tree lives.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.directory.path()
    }
}

/// A ceiling that ended the extraction.
///
/// Distinct from [`Refusal`], which skips one entry and carries on. Reaching a
/// limit means the machine cannot finish this archive at all, so the analysis
/// answers `UNABLE_TO_VERIFY` with the ceiling and the observed value rather
/// than reporting counts drawn from a partial tree.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExtractionLimit {
    /// Decompressed bytes exceeded the archive-wide ceiling.
    #[error("decompressed {observed} bytes, over the {limit} the extractor accepts")]
    Decompressed {
        /// The ceiling.
        limit: u64,
        /// What was seen when it tripped.
        observed: u64,
    },
    /// The archive held more entries than the walk accepts.
    #[error("archive holds more than {limit} entries")]
    EntryCount {
        /// The ceiling.
        limit: usize,
        /// What was seen when it tripped.
        observed: usize,
    },
    /// One entry declared more bytes than any single file may hold.
    ///
    /// Ends the run rather than skipping the file, and the distinction is the
    /// whole point of counting lines. A policy exclusion — vendored code,
    /// generated output — is a decision about what *should* be counted, and a
    /// report can state it. A source file left out because the extractor could
    /// not process it makes the number itself wrong, and a "counted" result
    /// that quietly means "counted, minus whatever we choked on" is the failure
    /// mode LOC reporting is most prone to.
    #[error("an entry declares {observed} bytes, over the {limit} any one file may hold")]
    FileSize {
        /// The ceiling.
        limit: u64,
        /// What the entry declared.
        observed: u64,
    },
    /// The bounded extraction volume could not take another byte.
    #[error("extraction storage filled at {observed} bytes, against a {limit} ceiling")]
    Storage {
        /// The ceiling.
        limit: u64,
        /// What was seen when it tripped.
        observed: u64,
    },
}

impl ExtractionLimit {
    /// The breach as the report publishes it.
    #[must_use]
    pub fn breach(&self) -> CompositionLimitBreach {
        let (name, limit, observed) = match *self {
            Self::Decompressed { limit, observed } => {
                (limits::names::DECOMPRESSED_STREAM, limit, observed)
            }
            Self::EntryCount { limit, observed } => {
                (limits::names::ENTRY_COUNT, limit as u64, observed as u64)
            }
            Self::FileSize { limit, observed } => (limits::names::FILE_BYTES, limit, observed),
            Self::Storage { limit, observed } => {
                (limits::names::EXTRACTION_STORAGE, limit, observed)
            }
        };
        CompositionLimitBreach {
            limit_name: name.to_owned(),
            limit_value: limit,
            observed_value: observed,
        }
    }
}

/// Anything that stopped the extraction.
#[derive(Debug, thiserror::Error)]
pub enum ExtractionError {
    /// A ceiling was reached. Reportable, not a fault.
    #[error(transparent)]
    Limit(#[from] ExtractionLimit),
    /// The archive is not readable as gzip'd tar, or the disk refused a write.
    ///
    /// The message is the source's, never the archive's content: a tarball can
    /// name its entries anything, and an error string is a place that text
    /// would otherwise reach a log unfiltered.
    #[error("could not read the archive")]
    Io(#[source] io::Error),
}

/// Where extracted bytes are put.
///
/// A seam, and one issue #12 effectively asks for. The storage ceiling below is
/// a *prediction* — it refuses a write it can see will not fit — and a
/// prediction is not the same control as a volume that is genuinely full. A
/// real mount fills during `create_dir_all`, `File::create`, `copy` or `flush`,
/// and without this trait every one of those arrives as an anonymous
/// [`ExtractionError::Io`]: the analysis would report "could not read the
/// archive" for a machine that simply ran out of room.
///
/// With the seam, a test can hand the extractor a volume that fills, and the
/// translation to `EXTRACTION_STORAGE_LIMIT` is exercised rather than asserted.
pub trait ExtractionStorage {
    /// Creates a directory and its parents.
    ///
    /// # Errors
    ///
    /// Whatever the underlying filesystem reports.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;

    /// Writes `source` to `path`, returning the bytes written.
    ///
    /// # Errors
    ///
    /// Whatever the underlying filesystem reports.
    fn write_file(&self, path: &Path, source: &mut dyn Read) -> io::Result<u64>;
}

/// The real filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct Filesystem;

impl ExtractionStorage for Filesystem {
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn write_file(&self, path: &Path, source: &mut dyn Read) -> io::Result<u64> {
        let mut sink = fs::File::create(path)?;
        let written = io::copy(source, &mut sink)?;
        // Flushed explicitly, because `File`'s own drop swallows the error —
        // and a write that failed on flush is exactly the shape a full volume
        // takes.
        sink.flush()?;
        Ok(written)
    }
}

/// Whether an I/O failure means the volume has no room left.
///
/// `StorageFull` and `QuotaExceeded` are both "the machine ran out", differing
/// only in who imposed the ceiling, and a reader does not need that difference.
fn is_out_of_space(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded
    )
}

/// Every ceiling one extraction runs under.
///
/// Passed rather than read from [`limits`] directly so a test can drive a
/// control from its own side without building a half-gigabyte fixture.
#[derive(Debug, Clone, Copy)]
pub struct Ceilings {
    /// Decompressed bytes across the whole archive.
    pub decompressed_bytes: u64,
    /// Entries the walk will examine.
    pub entries: usize,
    /// Bytes accepted from any one entry.
    pub file_bytes: u64,
    /// Bytes the extraction directory may hold.
    pub storage_bytes: u64,
}

impl Default for Ceilings {
    fn default() -> Self {
        Self {
            decompressed_bytes: limits::MAX_DECOMPRESSED_BYTES,
            entries: limits::MAX_ENTRIES,
            file_bytes: limits::MAX_FILE_BYTES,
            storage_bytes: limits::MAX_EXTRACTION_STORAGE_BYTES,
        }
    }
}

/// Extracts `archive` into a self-deleting directory under `parent`.
///
/// `parent` is where the bounded volume is mounted in production, so the
/// storage ceiling here is a *second* line rather than the only one: the volume
/// makes an over-large write fail, and this makes it fail with a name.
///
/// # Errors
///
/// [`ExtractionError::Limit`] when a ceiling is reached, which the caller turns
/// into `UNABLE_TO_VERIFY`; [`ExtractionError::Io`] when the archive cannot be
/// read at all.
pub fn extract(
    archive: &Path,
    parent: &Path,
    ceilings: Ceilings,
) -> Result<Extraction, ExtractionError> {
    extract_to(archive, parent, ceilings, &Filesystem)
}

/// [`extract`], against a caller-supplied storage.
///
/// Exists so a test can supply a volume that fills. Production always passes
/// [`Filesystem`].
///
/// # Errors
///
/// As [`extract`].
pub fn extract_to(
    archive: &Path,
    parent: &Path,
    ceilings: Ceilings,
    storage: &dyn ExtractionStorage,
) -> Result<Extraction, ExtractionError> {
    let file = fs::File::open(archive).map_err(ExtractionError::Io)?;

    // Layer one: the compressed stream. The download already enforced this, and
    // it is enforced again here because a file on disk is not proof of how it
    // got there — a future caller extracting an archive from anywhere else
    // would otherwise inherit no ceiling at all.
    let capped_compressed = file.take(limits::MAX_COMPRESSED_BYTES);

    // Layer two: the decompressed stream. One byte over the ceiling is enough
    // to detect it, so the cap is set one past, and the reader is watched below
    // rather than trusted to stop politely.
    let decoder = flate2::read::GzDecoder::new(capped_compressed);
    let counted = CountingReader::new(decoder.take(ceilings.decompressed_bytes.saturating_add(1)));
    let total_read = counted.total.clone();

    let directory = tempfile::Builder::new()
        .prefix("repolens-archive-")
        .tempdir_in(parent)
        .map_err(ExtractionError::Io)?;

    let mut extraction = Extraction {
        directory,
        files_written: 0,
        bytes_written: 0,
        refusals: Vec::new(),
    };

    // Any read failure is checked against the ceiling before it is reported.
    //
    // This is not defensive tidying. Capping the decompressed stream *is* how a
    // bomb is stopped, and the way it stops is that the tar reader runs out of
    // bytes mid-entry — which surfaces as "unexpected EOF during skip", an I/O
    // error. Reporting that verbatim would tell the reader the archive was
    // corrupt when in fact it was too large, and issue #12 requires the exact
    // limit and observed value instead. The first version of this function did
    // exactly that, and the bomb test is what said so.
    let classify = |error: io::Error| -> ExtractionError {
        let observed = total_read.get();
        if observed > ceilings.decompressed_bytes {
            ExtractionLimit::Decompressed {
                limit: ceilings.decompressed_bytes,
                observed,
            }
            .into()
        } else {
            ExtractionError::Io(error)
        }
    };

    let mut tar = tar::Archive::new(counted);
    let entries = tar.entries().map_err(&classify)?;

    let mut examined = 0usize;
    for entry in entries {
        let mut entry = entry.map_err(&classify)?;

        examined += 1;
        if examined > ceilings.entries {
            return Err(ExtractionLimit::EntryCount {
                limit: ceilings.entries,
                observed: examined,
            }
            .into());
        }

        // Checked every iteration rather than at the end. An archive that
        // exceeds the ceiling should stop costing memory at the moment it does,
        // not after it has finished expanding.
        let read_so_far = total_read.get();
        if read_so_far > ceilings.decompressed_bytes {
            return Err(ExtractionLimit::Decompressed {
                limit: ceilings.decompressed_bytes,
                observed: read_so_far,
            }
            .into());
        }

        let declared = entry.path().map_err(&classify)?.into_owned();
        let kind = kind_of(entry.header().entry_type());
        // `u64::MAX` when the header is unreadable, so an entry that will not
        // say how big it is trips the per-file ceiling rather than sliding
        // under it.
        let size = entry.header().size().unwrap_or(u64::MAX);

        let relative = match admit(&declared, kind) {
            Ok(path) => path,
            Err(refusal) => {
                extraction.refusals.push((declared, refusal));
                continue;
            }
        };

        // Checked after admission, so a link or a directory that happens to
        // declare an absurd size is refused as what it is rather than ending
        // the run. Only a *file* we would otherwise have counted can breach
        // this ceiling, which is what makes the breach mean the counts are
        // incomplete.
        if size > ceilings.file_bytes {
            return Err(ExtractionLimit::FileSize {
                limit: ceilings.file_bytes,
                observed: size,
            }
            .into());
        }

        // Joined onto the root only after `admit` has proved the path is a
        // plain relative one. The order matters: `Path::join` with an absolute
        // right-hand side silently discards the left, which is precisely the
        // escape this sequencing prevents.
        let destination = extraction.root().join(&relative);
        if extraction.bytes_written.saturating_add(size) > ceilings.storage_bytes {
            return Err(ExtractionLimit::Storage {
                limit: ceilings.storage_bytes,
                observed: extraction.bytes_written.saturating_add(size),
            }
            .into());
        }

        let written = write_entry(
            storage,
            &destination,
            &mut entry.by_ref().take(size),
            ceilings.storage_bytes,
            extraction.bytes_written,
        )
        .map_err(|error| match error {
            WriteFailure::OutOfSpace(limit) => limit.into(),
            WriteFailure::Io(error) => classify(error),
        })?;

        extraction.files_written += 1;
        extraction.bytes_written = extraction.bytes_written.saturating_add(written);
    }

    // The final check. An archive whose last entry pushed it over the ceiling
    // has to fail, even though the loop had no next iteration to notice in.
    let read_total = total_read.get();
    if read_total > ceilings.decompressed_bytes {
        return Err(ExtractionLimit::Decompressed {
            limit: ceilings.decompressed_bytes,
            observed: read_total,
        }
        .into());
    }

    Ok(extraction)
}

/// What a write can fail as.
enum WriteFailure {
    /// The volume said no more, whoever imposed the ceiling.
    OutOfSpace(ExtractionLimit),
    /// Anything else, which the caller still checks against the byte counter.
    Io(io::Error),
}

/// Writes one entry, distinguishing a full volume from every other failure.
///
/// Split out so the loop stays readable, but the distinction is the reason it
/// exists: a real mount fills during `create_dir_all`, `File::create`, `copy`
/// or `flush`, and reporting any of those as an unexplained I/O error would
/// tell a reader the archive was unreadable when the machine had run out of
/// room.
fn write_entry(
    storage: &dyn ExtractionStorage,
    destination: &Path,
    source: &mut dyn Read,
    storage_ceiling: u64,
    written_so_far: u64,
) -> Result<u64, WriteFailure> {
    let out_of_space = |error: io::Error| {
        if is_out_of_space(&error) {
            WriteFailure::OutOfSpace(ExtractionLimit::Storage {
                limit: storage_ceiling,
                observed: written_so_far,
            })
        } else {
            WriteFailure::Io(error)
        }
    };

    if let Some(parent) = destination.parent() {
        storage.create_dir_all(parent).map_err(&out_of_space)?;
    }
    // `source` is already bounded by the declared size, so a header that lies
    // about it cannot write more than it asked for.
    storage
        .write_file(destination, source)
        .map_err(out_of_space)
}

/// Maps tar's entry types onto the three this extractor distinguishes.
fn kind_of(entry_type: tar::EntryType) -> EntryKind {
    if entry_type.is_file() {
        EntryKind::RegularFile
    } else if entry_type.is_symlink() || entry_type.is_hard_link() {
        EntryKind::Link
    } else {
        EntryKind::Other
    }
}

/// A reader that remembers how much has passed through it.
///
/// The decompressed ceiling is enforced from this rather than from the sum of
/// entry sizes, because entry sizes are the archive's own claims. A header can
/// declare one byte and deliver a million; only counting what was actually
/// decoded measures the thing the ceiling is about.
struct CountingReader<R> {
    inner: R,
    total: Counter,
}

impl<R: Read> CountingReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            total: Counter::default(),
        }
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.total.add(read as u64);
        Ok(read)
    }
}

/// A shared byte counter.
///
/// `Rc<Cell<_>>` rather than an atomic: extraction is single-threaded by
/// construction — it is called inside `spawn_blocking` — and an atomic here
/// would advertise a concurrency that does not exist.
#[derive(Debug, Clone, Default)]
struct Counter(std::rc::Rc<std::cell::Cell<u64>>);

impl Counter {
    fn add(&self, bytes: u64) {
        self.0.set(self.0.get().saturating_add(bytes));
    }

    fn get(&self) -> u64 {
        self.0.get()
    }
}
