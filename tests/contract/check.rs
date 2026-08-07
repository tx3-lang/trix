//! `trix check` ↔ `tx3c build --diagnostics-format json`.

use crate::harness::*;

#[test]
fn clean_diagnostics_pass_and_print() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix_with_fake_tx3c(&["check"], &[]);
    assert_success(&result);
    assert_output_contains(&result, "check passed, no errors found");

    // The compat gate probes --version before the first real invocation,
    // then check delegates to `build … --diagnostics-format json`.
    let invocations = ctx.tx3c_invocations();
    assert_eq!(
        invocations.first().map(|i| i[0].as_str()),
        Some("--version"),
        "first tx3c contact must be the version probe: {invocations:?}"
    );
    let build = invocations
        .iter()
        .find(|i| i[0] == "build")
        .expect("check should invoke tx3c build");
    assert!(
        build.contains(&"--diagnostics-format".to_string()) && build.contains(&"json".to_string()),
        "check must request JSON diagnostics: {build:?}"
    );
    assert!(
        build[1].ends_with("main.tx3"),
        "check must pass the project's main source: {build:?}"
    );
}

#[test]
fn analyzer_errors_render_and_fail() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let diagnostics =
        r#"{"diagnostics":[{"severity":"error","code":"E001","message":"unknown party 'Ghost'"}]}"#;
    let result = ctx.run_trix_with_fake_tx3c(&["check"], &[("FAKE_TX3C_DIAGNOSTICS", diagnostics)]);

    assert_failure_mentioning(&result, "unknown party 'Ghost'");
}

/// tx3c printing something that isn't the JSON envelope (a crash, a stray
/// log line) is a *spawn-contract* failure: trix must say it couldn't parse
/// the diagnostics and carry the tool's stderr for context.
#[test]
fn garbage_stdout_is_a_parse_error_with_stderr_context() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix_with_fake_tx3c(
        &["check"],
        &[("FAKE_TX3C_DIAGNOSTICS", "thread panicked at src/lib.rs")],
    );

    assert_failure_mentioning(&result, "parsing tx3c diagnostics");
}

/// A tool that dies outright (non-zero, nothing useful on stdout) must not
/// masquerade as a passing or empty check.
#[test]
fn tool_failure_does_not_pass_silently() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix_with_fake_tx3c(
        &["check"],
        &[
            ("FAKE_TX3C_EXIT", "101"),
            ("FAKE_TX3C_STDERR", "tx3c blew up"),
        ],
    );

    assert!(!result.success(), "a dead tool must fail the check");
    assert_failure_mentioning(&result, "tx3c blew up");
}

/// `check` is project-only: a declared, cached interface neither helps nor
/// hinders it — and trix must not decode the interface's TII for it.
#[test]
fn check_ignores_declared_interfaces() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let digest = ctx.prime_interface_cache("acme", "widget", "0.1.0");
    ctx.declare_interface("widget", "acme", "widget", "0.1.0", &digest);

    let result = ctx.run_trix_with_fake_tx3c(&["check"], &[]);
    assert_success(&result);
    assert_output_contains(&result, "check passed");

    assert!(
        !ctx.tx3c_invocations().iter().any(|i| i[0] == "decode"),
        "check must never decode an interface TII"
    );
}
