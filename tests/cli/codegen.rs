//! `trix codegen` argument parsing that completes before helper resolution.

use crate::harness::*;

#[test]
fn help_lists_java_and_swift_client_plugins() {
    let ctx = TestContext::new();
    let result = ctx.run_trix(&["codegen", "--help"]);

    assert_success(&result);
    assert_output_contains(&result, "java-client");
    assert_output_contains(&result, "swift-client");
}
