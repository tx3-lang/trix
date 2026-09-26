//! `trix codegen` ↔ `tx3c build --emit tii` + `tx3c codegen`.
//!
//! Trix's side of the contract: which TII feeds each target (project built
//! from source, interfaces from their cached published TII), how a plugin
//! selects its templates (built-in clients by `--language`, custom plugins
//! by `--template`), and the unconditional per-protocol output layout
//! `gen/<name>/`.

use crate::harness::*;

#[test]
fn project_bindings_nest_under_project_subdir() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));
    ctx.declare_codegen();

    let project_name = ctx.load_trix_config().protocol.name;

    let result = ctx.run_trix_with_fake_tx3c(&["codegen"], &[]);
    assert_success(&result);

    // Per-protocol layout, even with zero interfaces: nothing flat.
    ctx.assert_file_exists(format!("gen/{project_name}/bindings.txt"));
    assert!(
        !ctx.file_path("gen/bindings.txt").exists(),
        "unified layout: nothing should be written flat at gen/bindings.txt"
    );

    let invocations = ctx.tx3c_invocations();
    // The project's TII is built from source first…
    assert!(
        invocations
            .iter()
            .any(|i| i[0] == "build" && i.contains(&"tii".to_string())),
        "codegen must compile the project TII: {invocations:?}"
    );
    // …then handed to tx3c codegen with the per-project output dir.
    let codegen = invocations
        .iter()
        .find(|i| i[0] == "codegen")
        .expect("codegen must delegate to tx3c codegen");
    let output = codegen
        .windows(2)
        .find(|w| w[0] == "--output")
        .map(|w| w[1].clone())
        .expect("tx3c codegen must receive --output");
    assert!(
        output.ends_with(&project_name),
        "output dir must be the per-project subdir, got: {output}"
    );
}

#[test]
fn interface_bindings_use_cached_tii_not_a_recompile() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let digest = ctx.prime_interface_cache("acme", "widget", "0.1.0");
    ctx.declare_interface("widget", "acme", "widget", "0.1.0", &digest);
    ctx.declare_codegen();

    let project_name = ctx.load_trix_config().protocol.name;

    let result = ctx.run_trix_with_fake_tx3c(&["codegen"], &[]);
    assert_success(&result);

    // One binding set per protocol, each in its own subdir.
    ctx.assert_file_exists(format!("gen/{project_name}/bindings.txt"));
    ctx.assert_file_exists("gen/widget/bindings.txt");

    // The fake records which TII fed each binding: the widget bindings must
    // come from the cached published artifact, not a rebuild.
    ctx.assert_file_contains("gen/widget/bindings.txt", "acme");

    let invocations = ctx.tx3c_invocations();
    let tii_builds = invocations
        .iter()
        .filter(|i| i[0] == "build" && i.contains(&"tii".to_string()))
        .count();
    assert_eq!(
        tii_builds, 1,
        "only the project compiles; the interface is consumed from cache: {invocations:?}"
    );
}

/// Built-in plugins render tx3c's client for their language: `--language`, with no
/// template download and no `--template` flag.
#[test]
fn builtin_plugins_render_tx3c_clients_by_language() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let mut trix_toml = ctx.read_file("trix.toml");
    trix_toml.push_str("\n[[codegen]]\noutput_dir = \"gen\"\nplugin = \"python-client\"\n");
    ctx.write_file("trix.toml", &trix_toml);

    let project_name = ctx.load_trix_config().protocol.name;

    let result = ctx.run_trix_with_fake_tx3c(&["codegen"], &[]);
    assert_success(&result);
    assert!(
        !result.combined().contains("Reading template from"),
        "built-in plugins must not download templates: {}",
        result.combined()
    );

    ctx.assert_file_contains(
        format!("gen/{project_name}/bindings.txt"),
        "language=python",
    );

    let invocations = ctx.tx3c_invocations();
    let codegen = invocations
        .iter()
        .find(|i| i[0] == "codegen")
        .expect("codegen must delegate to tx3c codegen");
    assert!(
        codegen
            .windows(2)
            .any(|w| w[0] == "--language" && w[1] == "python"),
        "built-in plugin must pass --language: {codegen:?}"
    );
    assert!(
        !codegen.iter().any(|arg| arg == "--template"),
        "built-in plugin must not pass --template: {codegen:?}"
    );
}
