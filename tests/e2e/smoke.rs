use super::*;

#[test]
fn init_runs_without_error() {
    let ctx = TestContext::new();
    let result = ctx.run_trix(&["init", "--yes"]);

    assert_success(&result);
    ctx.assert_file_exists("trix.toml");
}

#[test]
#[cfg(feature = "unstable")]
fn aiken_help_runs_without_error() {
    let ctx = TestContext::new();
    let result = ctx.run_trix(&["aiken", "--help"]);

    assert_success(&result);
    assert_output_contains(&result, "audit");
}

#[test]
#[cfg(feature = "unstable")]
fn aiken_audit_help_runs_without_error() {
    let ctx = TestContext::new();
    let result = ctx.run_trix(&["aiken", "audit", "--help"]);

    assert_success(&result);
    assert_output_contains(&result, "vulnerability");
}
