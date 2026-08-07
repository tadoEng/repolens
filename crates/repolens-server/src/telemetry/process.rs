//! Process-level facts, read without a dependency.
//!
//! Two numbers, both of which a dashboard needs to say whether a measurement
//! describes a warm process or a cold one. Neither is worth a crate: one is a
//! subtraction, the other is a line of `/proc`.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

/// When this process started, as observed by the first caller.
static STARTED_AT: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Anchors the uptime clock at the current instant.
///
/// Called once, at the top of `main`. Without it the anchor is wherever
/// [`uptime`] is first read — which for this service would be the first request
/// that asks, making a process up for days report an uptime of seconds. A
/// [`LazyLock`] cannot be re-anchored, so calling this twice is harmless and
/// calling it late is the only mistake available.
pub fn mark_start() {
    LazyLock::force(&STARTED_AT);
}

/// How long since [`mark_start`], or since the first read if it was never
/// called.
///
/// A monotonic clock, so it is unaffected by the wall clock being corrected
/// underneath it.
#[must_use]
pub fn uptime() -> Duration {
    STARTED_AT.elapsed()
}

/// Assumed page size for [`resident_bytes`].
///
/// `/proc/self/statm` counts *pages*, and the page size is a kernel
/// configuration this process cannot read without `sysconf(_SC_PAGESIZE)` —
/// which is an FFI call, and this workspace forbids `unsafe`. Four kibibytes is
/// correct on the x86-64 Linux the service is deployed to, and would
/// under-report by the ratio on a kernel built with larger pages. Written down
/// here rather than left implicit in a multiplication, because a figure that is
/// wrong by 4× on some hosts and right on others is worth being able to find.
const ASSUMED_PAGE_SIZE_BYTES: u64 = 4096;

/// Resident set size in bytes, where the platform can answer.
///
/// Linux only, and `None` everywhere else **on purpose**. Development happens on
/// Windows, which has no `/proc`; a plausible-looking zero, or a figure borrowed
/// from a different measurement, would turn "we cannot read this here" into "the
/// process uses no memory". Unknown is not zero — the same rule the report
/// contract keeps for a truncated tree.
#[must_use]
pub fn resident_bytes() -> Option<u64> {
    parse_statm(&read_statm()?)
}

/// Reads `/proc/self/statm`, where such a file exists.
#[cfg(target_os = "linux")]
fn read_statm() -> Option<String> {
    std::fs::read_to_string("/proc/self/statm").ok()
}

/// There is no `/proc` here, and inventing one would be inventing the number.
#[cfg(not(target_os = "linux"))]
fn read_statm() -> Option<String> {
    None
}

/// Resident pages — the second field of `/proc/self/statm` — as bytes.
///
/// A pure function of the file's contents, so the parsing is testable on the
/// platform this is developed on, where the file does not exist. The first field
/// is the total program size and is deliberately not used: it counts address
/// space rather than resident memory, and reporting it as RSS would overstate a
/// process holding a large mapping it never touched.
fn parse_statm(contents: &str) -> Option<u64> {
    let resident_pages: u64 = contents.split_ascii_whitespace().nth(1)?.parse().ok()?;
    Some(resident_pages.saturating_mul(ASSUMED_PAGE_SIZE_BYTES))
}

#[cfg(test)]
mod tests {
    use super::{ASSUMED_PAGE_SIZE_BYTES, mark_start, parse_statm, resident_bytes, uptime};

    #[test]
    fn statm_reports_the_resident_field_rather_than_the_first_one() {
        // A real line: size, resident, shared, text, lib, data, dirty. Reading
        // the first field would report address space as memory in use.
        let parsed = parse_statm("100000 2500 1800 12 0 3000 0\n").expect("a well-formed line");
        assert_eq!(parsed, 2500 * ASSUMED_PAGE_SIZE_BYTES);
    }

    #[test]
    fn a_statm_line_that_cannot_be_read_is_unknown_rather_than_zero() {
        for contents in [
            "",
            "100000",
            "not a number at all",
            "100000 notanumber",
            "\n",
        ] {
            assert_eq!(
                parse_statm(contents),
                None,
                "should not invent a figure from {contents:?}"
            );
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn resident_memory_is_readable_on_linux() {
        let bytes = resident_bytes().expect("/proc/self/statm exists on Linux");
        assert!(
            bytes > 0,
            "a running process has resident pages; zero means the field was misread"
        );
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn resident_memory_is_absent_off_linux() {
        assert_eq!(
            resident_bytes(),
            None,
            "there is no /proc here, and a faked figure would be worse than none"
        );
    }

    #[test]
    fn a_second_anchor_does_not_restart_the_clock() {
        // The property worth having: `main` calls `mark_start` once, but an
        // anchor that could be moved would report a fresh uptime to whoever
        // called last. That this cannot happen is a consequence of `LazyLock`,
        // which is exactly the kind of guarantee that survives until somebody
        // swaps it for a `Mutex<Instant>` they can also assign to.
        mark_start();
        let first = uptime();
        std::thread::sleep(std::time::Duration::from_millis(5));
        mark_start();
        let second = uptime();

        assert!(
            second > first,
            "re-anchoring reset the clock: {second:?} is not after {first:?}"
        );
        assert!(
            second >= std::time::Duration::from_millis(5),
            "uptime {second:?} is shorter than the sleep that preceded it, so the \
             anchor moved"
        );
    }
}
