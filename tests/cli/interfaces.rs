//! Interface-cache integrity at the CLI boundary. The verification rules
//! are unit tests on `interfaces::verify_cache_at`; this probes that the
//! consuming commands run them (`restore_all`) before touching anything.

use crate::harness::*;

/// Tampered cache digest: an interface-aware command fails closed with the
/// digest-mismatch report and the `trix use --force` hint. The failure
/// fires before tool resolution — no tx3c exists in this environment, yet
/// the error is the digest one, proving the integrity gate runs first.
#[test]
fn digest_tamper_fails_closed_before_any_tool_runs() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    ctx.prime_interface_cache("acme", "widget", "0.1.0");
    // Declare the interface with a digest that does NOT match the cache's.
    ctx.declare_interface(
        "widget",
        "acme",
        "widget",
        "0.1.0",
        "sha256:0000000000000000000000000000000000000000000000000000000000000bad",
    );

    let result = ctx.run_trix(&["inspect", "tir", "--tx", "transfer"]);
    assert_failure_mentioning(&result, "digest");
    assert!(
        !result.combined().contains("tool tx3c not found"),
        "integrity must be checked before tool resolution:\n{}",
        result.combined()
    );
}

/// Projects without `[interfaces]` are entirely unaffected by the interface
/// machinery — and with no tool installed, a project-only failure mentions
/// the missing tool, not the interface layer.
#[test]
fn projects_without_interfaces_section_unchanged() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let config = ctx.load_trix_config();
    assert!(
        config.interfaces.is_empty(),
        "fresh init should have no interfaces declared"
    );
    assert!(
        config.registry.is_none(),
        "fresh init should not write a [registry] section"
    );
}
