//! `trix codegen` ↔ `tx3c build --emit tii` + `tx3c codegen`.
//!
//! Trix's side of the contract: which TII feeds each target (project built
//! from source, interfaces from their cached published TII) and the
//! unconditional per-protocol output layout `gen/<name>/`.

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

// The `[[codegen]].options` channel
//
// What trix *sends*: the merged options ride as one JSON object under
// `--options`, and nothing else about the invocation changes. What the
// merge itself resolves to (plugin defaults, user override precedence) is
// a rule, unit-tested on `CodegenConfig::resolved_options` in
// `src/config/convention.rs`; these are the wiring probes.

/// The `--options` value of the first `tx3c codegen` invocation, parsed.
/// `None` when the flag was not passed at all.
fn forwarded_options(ctx: &TestContext) -> Option<serde_json::Value> {
    let invocations = ctx.tx3c_invocations();
    let codegen = invocations
        .iter()
        .find(|i| i[0] == "codegen")
        .unwrap_or_else(|| panic!("codegen must delegate to tx3c codegen: {invocations:?}"));
    codegen
        .windows(2)
        .find(|w| w[0] == "--options")
        .map(|w| serde_json::from_str(&w[1]).expect("--options must carry a JSON object"))
}

/// A project that asks for nothing gets exactly the argv trix sent before
/// the options channel existed: no `--options` at all. That keeps the
/// no-options path compatible with a `tx3c` that predates the flag.
#[test]
fn no_options_and_no_plugin_defaults_forwards_no_options_flag() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));
    ctx.declare_codegen();

    assert_success(&ctx.run_trix_with_fake_tx3c(&["codegen"], &[]));

    assert_eq!(
        forwarded_options(&ctx),
        None,
        "an empty option set must not put --options on the command line"
    );
}

/// A plugin trix knows nothing about contributes no defaults, so the
/// project's own options reach `tx3c` unchanged — including nested values.
#[test]
fn plugin_without_defaults_forwards_user_options_unchanged() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));
    ctx.declare_codegen_with_options(Some(
        r#"{ standalone = false, package_name = "acme", extra = { nested = 1 } }"#,
    ));

    assert_success(&ctx.run_trix_with_fake_tx3c(&["codegen"], &[]));

    assert_eq!(
        forwarded_options(&ctx),
        Some(serde_json::json!({
            "standalone": false,
            "package_name": "acme",
            "extra": { "nested": 1 },
        })),
    );
}

/// The built-in `ts-client` default reaching the wire: with no `options`
/// in `trix.toml`, `tx3c codegen` still receives `{"standalone":true}` —
/// the knob the `codegen-v1beta0` templates gate `package.json.hbs` and
/// `tsconfig.json.hbs` on.
#[test]
fn ts_client_default_injects_standalone_on_the_wire() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));
    ctx.stage_local_ts_client_templates();

    assert_success(&ctx.run_trix_with_fake_tx3c(&["codegen", "--plugin", "ts-client"], &[]));

    assert_eq!(
        forwarded_options(&ctx),
        Some(serde_json::json!({ "standalone": true })),
    );
}

/// …and the project overrides it: an explicit `standalone = false` is
/// forwarded verbatim rather than being masked by the plugin default. This
/// is the host-package consumption mode.
#[test]
fn ts_client_explicit_standalone_false_overrides_the_default() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));
    ctx.stage_local_ts_client_templates();

    let mut trix_toml = ctx.read_file("trix.toml");
    trix_toml.push_str("\n[[codegen]]\nplugin = \"ts-client\"\noptions = { standalone = false }\n");
    ctx.write_file("trix.toml", &trix_toml);

    assert_success(&ctx.run_trix_with_fake_tx3c(&["codegen"], &[]));

    assert_eq!(
        forwarded_options(&ctx),
        Some(serde_json::json!({ "standalone": false })),
    );
}
