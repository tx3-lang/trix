use super::*;

#[test]
fn init_runs_without_error() {
    let ctx = TestContext::new();
    let result = ctx.run_trix(&["init", "--yes"]);

    assert_success(&result);
    ctx.assert_file_exists("trix.toml");
}
