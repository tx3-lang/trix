//! The version-compat gate (`spawn::compat`) end-to-end: trix probes
//! `tx3c --version` and enforces the support window before the first real
//! invocation. The fake reports any version we ask — coverage the journeys
//! can't provide, since they only ever see real released versions.
//!
//! The window arithmetic itself is unit-tested in `src/spawn/compat.rs`;
//! these pin the process-level behavior: probe order, error surface, the
//! project floor, and the escape hatch.

use crate::harness::*;

#[test]
fn below_floor_is_rejected_before_any_real_invocation() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix_with_fake_tx3c(&["check"], &[("FAKE_TX3C_VERSION", "0.21.0")]);
    assert_failure_mentioning(&result, "incompatible tx3 toolchain");

    // The gate stopped everything after the probe.
    let invocations = ctx.tx3c_invocations();
    assert!(
        invocations.iter().all(|i| i[0] == "--version"),
        "no real invocation may follow a failed version gate: {invocations:?}"
    );
}

#[test]
fn next_major_is_rejected_as_too_new() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix_with_fake_tx3c(&["check"], &[("FAKE_TX3C_VERSION", "1.0.0")]);
    assert_failure_mentioning(&result, "newer than this trix supports");
}

#[test]
fn unparseable_version_is_rejected() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix_with_fake_tx3c(&["check"], &[("FAKE_TX3C_VERSION", "garbage")]);
    assert_failure_mentioning(&result, "cannot parse tx3c version");
}

/// `trix.toml [toolchain]` raises the floor above the built-in matrix.
#[test]
fn project_toolchain_floor_is_enforced() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let mut content = ctx.read_file("trix.toml");
    content.push_str("\n[toolchain]\ntx3c = \"0.25.0\"\n");
    ctx.write_file("trix.toml", &content);

    // 0.24.0 satisfies the built-in matrix but not the project floor.
    let result = ctx.run_trix_with_fake_tx3c(&["check"], &[("FAKE_TX3C_VERSION", "0.24.0")]);
    assert_failure_mentioning(&result, "this protocol requires");
}

/// The development escape hatch: `TX3_SKIP_COMPAT_CHECK` bypasses the
/// window entirely (an unreleased tool reports a pre-bump version).
#[test]
fn skip_env_bypasses_the_gate() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix_with_fake_tx3c(
        &["check"],
        &[
            ("FAKE_TX3C_VERSION", "0.0.1"),
            ("TX3_SKIP_COMPAT_CHECK", "1"),
        ],
    );
    assert_success(&result);
    assert_output_contains(&result, "check passed");
}
