//! Dependency-aware codegen lives only in the unstable codegen path
//! (`src/commands/codegen.rs`). These tests are compiled and run only under
//! `cargo test --features unstable`, where `assert_cmd::cargo_bin` resolves
//! the unstable-built `trix` binary. They require a real `tx3c` (like
//! `happy_path::codegen_generates_bindings_from_fixture`).

use super::*;
use std::path::PathBuf;

fn codegen_template_dir() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/e2e/fixtures/codegen-template")
        .to_str()
        .expect("fixture path should be valid UTF-8")
        .to_string()
}

fn append_codegen_block(ctx: &TestContext) {
    let mut trix_toml = ctx.read_file("trix.toml");
    trix_toml.push_str(&format!(
        "\n[[codegen]]\noutput_dir = \"gen\"\nplugin = {{ repo = \"{}\", path = \".\" }}\n",
        codegen_template_dir()
    ));
    ctx.write_file("trix.toml", &trix_toml);
}

/// With a dependency declared + cached, codegen emits one binding set per
/// protocol into per-protocol subdirs: `gen/<project>/` and `gen/<alias>/`.
#[test]
fn codegen_with_dependency_emits_subdirs() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let tx3c_path = ctx
        .tx3c_path()
        .expect("tx3c should be available in PATH or TX3_TX3C_PATH");
    assert!(tx3c_path.is_file(), "tx3c path should exist");

    let digest = ctx.prime_dep_cache("acme", "widget", "0.1.0");
    ctx.declare_dep("widget", "acme", "widget", "0.1.0", &digest);
    append_codegen_block(&ctx);

    let project_name = ctx.load_trix_config().protocol.name;

    let result = ctx.run_trix(&["codegen"]);
    assert_success(&result);

    ctx.assert_file_exists(format!("gen/{project_name}/bindings.txt"));
    ctx.assert_file_exists("gen/widget/bindings.txt");
    ctx.assert_file_contains("gen/widget/bindings.txt", "widget");

    assert!(
        !ctx.file_path("gen/bindings.txt").exists(),
        "unified layout: nothing should be written flat at gen/bindings.txt"
    );
}

/// Even with NO dependencies, the unstable path nests the project under its
/// own subdir — the deliberate break from the old flat layout, confined to
/// the unstable path.
#[test]
fn codegen_without_dependency_still_uses_subdir() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let tx3c_path = ctx
        .tx3c_path()
        .expect("tx3c should be available in PATH or TX3_TX3C_PATH");
    assert!(tx3c_path.is_file(), "tx3c path should exist");

    append_codegen_block(&ctx);

    let project_name = ctx.load_trix_config().protocol.name;

    let result = ctx.run_trix(&["codegen"]);
    assert_success(&result);

    ctx.assert_file_exists(format!("gen/{project_name}/bindings.txt"));
    assert!(
        !ctx.file_path("gen/bindings.txt").exists(),
        "unified layout applies even with zero deps"
    );
}
