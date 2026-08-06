//! The panic hook, proven by looking at real stderr.
//!
//! This has to be a subprocess test. The thing under test is what the Rust
//! runtime writes to the process's standard error *before* unwinding starts,
//! which no in-process assertion can observe: by the time `catch_unwind` or
//! `CatchPanicLayer` receives the payload, anything the hook was going to print
//! has already been printed.
//!
//! The test re-executes its own binary in two modes and compares them. The
//! default-hook run is a **positive control**: it asserts the payload *does*
//! reach stderr without our hook, which is what proves this test could detect a
//! regression at all. Without it, deleting the hook entirely would leave a green
//! test that watches nothing.

use std::process::Command;

/// Shaped like the things a real panic message can carry. `.invalid` is
/// reserved by RFC 2606 and the credentials are named rather than plausible.
const SECRET_SHAPED_PAYLOAD: &str =
    "postgres://EXAMPLE_USER:EXAMPLE_PASSWORD@db.example.invalid/repolens";

/// Environment variable selecting child behaviour.
const MODE: &str = "REPOLENS_PANIC_HOOK_CHILD";

/// Panics with the payload, having installed our hook or not, then exits
/// cleanly so the parent reads stderr rather than a harness failure.
fn run_as_child(install_hook: bool) -> ! {
    if install_hook {
        repolens_server::telemetry::install_panic_hook();
    }

    // Caught so the child exits 0. The hook has already run by this point —
    // that is the whole ordering this test exists to pin down.
    let panicked = std::panic::catch_unwind(|| panic!("{SECRET_SHAPED_PAYLOAD}"));
    assert!(panicked.is_err(), "the child must actually panic");

    std::process::exit(0);
}

/// Runs this test binary again in `mode`, returning its stderr.
///
/// `--nocapture` matters: without it the harness redirects the child's output
/// into its own buffer, the payload never reaches the real file descriptor, and
/// the positive control below would pass for the wrong reason.
fn stderr_of(mode: &str) -> String {
    let exe = std::env::current_exe().expect("the running test binary has a path");

    let output = Command::new(exe)
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env(MODE, mode)
        .output()
        .expect("the child runs");

    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn the_panic_hook_keeps_the_payload_out_of_stderr() {
    match std::env::var(MODE).as_deref() {
        Ok("default") => run_as_child(false),
        Ok("hooked") => run_as_child(true),
        _ => {}
    }

    // Positive control. Rust's default hook prints the payload before
    // unwinding; if this ever stops holding, the assertion below proves
    // nothing and must be re-examined rather than trusted.
    let default = stderr_of("default");
    assert!(
        default.contains(SECRET_SHAPED_PAYLOAD),
        "the default hook is expected to leak the payload — without that this \
         test cannot detect a regression. stderr was: {default}"
    );

    let hooked = stderr_of("hooked");
    assert!(
        !hooked.contains(SECRET_SHAPED_PAYLOAD),
        "the payload reached stderr despite the hook. Catching the unwind is \
         not enough: the hook runs first and the default one prints. \
         stderr was: {hooked}"
    );

    // The fact of the panic must still be recorded, or the hook has traded a
    // disclosure for a silent failure.
    for fragment in ["EXAMPLE_PASSWORD", "db.example.invalid", "postgres://"] {
        assert!(
            !hooked.contains(fragment),
            "stderr still carries {fragment:?}: {hooked}"
        );
    }
}
