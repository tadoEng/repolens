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

/// Bytes in the `kB` that `/proc` writes.
///
/// The kernel spells it `kB` and means kibibytes. Taking the label at its word
/// would report every memory figure 2.4% low — small enough to look like normal
/// variation and never be questioned, which is the reason the conversion is a
/// named constant instead of a literal in the multiplication.
const BYTES_PER_KIB: u64 = 1024;

/// Resident set size in bytes, where the platform can answer.
///
/// Linux only, and `None` everywhere else **on purpose**. Development happens on
/// Windows, which has no `/proc`; a plausible-looking zero, or a figure borrowed
/// from a different measurement, would turn "we cannot read this here" into "the
/// process uses no memory". Unknown is not zero — the same rule the report
/// contract keeps for a truncated tree.
///
/// # Why `/proc/self/status`
///
/// Three files answer this, and the choice between them is a choice about what
/// the figure depends on. `/proc/self/statm` counts *pages*, so it needs the
/// host's page size — a kernel configuration no safe Rust can ask for, since
/// `sysconf(_SC_PAGESIZE)` is FFI and this workspace forbids `unsafe`. Reading
/// it therefore meant hard-coding 4 KiB: correct on the x86-64 hosts this is
/// deployed to, and quietly short by the ratio on a kernel built with larger
/// pages. `VmRSS:` is already in kibibytes, which removes the page size from the
/// calculation rather than documenting an assumption about it.
///
/// `/proc/self/smaps_rollup` reports the same quantity in the same unit and
/// loses on availability, which is the axis that matters for a figure read on a
/// host we do not choose: it needs a 4.14 kernel with `CONFIG_PROC_PAGE_MONITOR`
/// and is produced by walking the process's page tables, where `VmRSS:` is a
/// counter the kernel already maintains. Its extra per-mapping detail has
/// nowhere to go in a single gauge.
#[must_use]
pub fn resident_bytes() -> Option<u64> {
    parse_vm_rss(&read_status()?)
}

/// Reads `/proc/self/status`, where such a file exists.
#[cfg(target_os = "linux")]
fn read_status() -> Option<String> {
    std::fs::read_to_string("/proc/self/status").ok()
}

/// There is no `/proc` here, and inventing one would be inventing the number.
#[cfg(not(target_os = "linux"))]
fn read_status() -> Option<String> {
    None
}

/// The `VmRSS:` field of `/proc/self/status`, in bytes.
///
/// A pure function of the file's contents, so the parsing is testable on the
/// platform this is developed on, where the file does not exist.
///
/// Every step is allowed to decline, and the unit is checked rather than
/// assumed. `/proc` is a kernel-formatted text file, not a stable wire format:
/// the padding between the label, the number and the unit is column alignment,
/// and a field that arrived in some other unit would otherwise be multiplied as
/// though it were kibibytes and be wrong by a factor no reader could detect.
/// `None` is a value a dashboard can render as unknown; a wrong number is not.
fn parse_vm_rss(contents: &str) -> Option<u64> {
    // Located by stripping the label off the front of a line, so the match is on
    // the whole field name in the position the kernel writes it. The file also
    // carries `RssAnon`, `RssFile` and `RssShmem`, which are components of this
    // figure rather than substitutes for it; a looser search is how a parser
    // starts answering with one of those after a kernel reorders the file.
    let value = contents
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("VmRSS:"))?;

    let mut fields = value.split_ascii_whitespace();
    let kibibytes: u64 = fields.next()?.parse().ok()?;
    if !fields.next()?.eq_ignore_ascii_case("kB") {
        return None;
    }

    Some(kibibytes.saturating_mul(BYTES_PER_KIB))
}

#[cfg(test)]
mod tests {
    use super::{mark_start, parse_vm_rss, resident_bytes, uptime};

    /// An excerpt of `/proc/self/status`, keeping the fields that sit around
    /// `VmRSS:` and the tab-then-padding the kernel writes.
    const STATUS: &str = "Name:\tserver\n\
         State:\tR (running)\n\
         VmPeak:\t  108764 kB\n\
         VmSize:\t  102400 kB\n\
         VmRSS:\t   12345 kB\n\
         RssAnon:\t    9000 kB\n\
         RssFile:\t    3345 kB\n\
         RssShmem:\t       0 kB\n\
         Threads:\t8\n";

    #[test]
    fn the_resident_field_is_read_as_kibibytes_and_not_a_neighbouring_one() {
        // The expected figure is written out rather than expressed with the
        // conversion constant, so this checks the arithmetic instead of agreeing
        // with it: a `kB` read as a thousand bytes has to fail somewhere.
        assert_eq!(
            parse_vm_rss(STATUS),
            Some(12_345 * 1024),
            "VmRSS is the resident figure; RssAnon and RssFile are parts of it"
        );
    }

    #[test]
    fn the_padding_around_the_value_is_not_part_of_the_format() {
        // Column alignment is the kernel's presentation choice and has varied.
        // A parser that depends on it reports None on a host it was never tried
        // on, which looks exactly like a platform that has no /proc.
        for contents in [
            "VmRSS:\t   12345 kB\n",
            "VmRSS: 12345 kB\n",
            "VmRSS:\t12345\tkB\n",
            "VmRSS:12345 kB\n",
            "  VmRSS:  12345  kB",
        ] {
            assert_eq!(
                parse_vm_rss(contents),
                Some(12_345 * 1024),
                "should read a figure from {contents:?}"
            );
        }
    }

    #[test]
    fn a_status_file_without_the_field_is_unknown_rather_than_zero() {
        // Built by removing the line from the fixture, so it also asserts that
        // nothing else in the file can stand in for it.
        let without = STATUS
            .lines()
            .filter(|line| !line.starts_with("VmRSS:"))
            .collect::<Vec<_>>()
            .join("\n");

        assert_eq!(
            parse_vm_rss(&without),
            None,
            "an absent field is not a process using no memory"
        );
    }

    #[test]
    fn a_field_that_cannot_be_read_is_unknown_rather_than_wrong() {
        for contents in [
            "",
            "\n",
            "VmRSS:\n",
            "VmRSS:\t kB\n",
            "VmRSS:\tnotanumber kB\n",
            "VmRSS:\t-1 kB\n",
            "VmRSS:\t12345\n",
            "VmRSS:\t12345 MB\n",
            "VmRSS:\t12345 B\n",
            "NotVmRSS:\t12345 kB\n",
        ] {
            assert_eq!(
                parse_vm_rss(contents),
                None,
                "should not invent a figure from {contents:?}"
            );
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn resident_memory_is_readable_on_linux() {
        let bytes = resident_bytes().expect("/proc/self/status exists on Linux");
        assert!(
            bytes > 0,
            "a running process is resident somewhere; zero means the field was misread"
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
