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
fn audit_help_runs_without_error() {
    let ctx = TestContext::new();
    let result = ctx.run_trix(&["audit", "--help"]);

    assert_success(&result);
    assert_output_contains(&result, "vulnerability");
}

#[test]
#[cfg(feature = "unstable")]
fn audit_help_displays_provider_options() {
    let ctx = TestContext::new();
    let result = ctx.run_trix(&["audit", "--help"]);

    assert_success(&result);
    assert_output_contains(&result, "provider");
}
