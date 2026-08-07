//! `trix inspect tir` ↔ `tx3c build --emit tir-json` / `tx3c decode`.
//!
//! The trix-side logic under test is *which artifact gets handed to which
//! tx3c subcommand*: the project's authored source is lowered
//! (`build --emit tir-json`), an interface is decoded from its cached
//! published TII (`decode --tii`). The resolution rules themselves are unit
//! tests on `interfaces::resolve`.

use crate::harness::*;

fn assert_json_object_line(result: &CommandResult) {
    // stdout may carry banner/preamble lines; only the JSON line is
    // structured. At least one line must parse as a JSON object.
    let parsed = result
        .stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .find(|v| v.is_object());
    assert!(
        parsed.is_some(),
        "no JSON object found in inspect output:\n{}",
        result.stdout
    );
}

#[test]
fn bare_tx_lowers_project_source() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix_with_fake_tx3c(&["inspect", "tir", "--tx", "transfer"], &[]);
    assert_success(&result);
    assert_json_object_line(&result);

    let invocations = ctx.tx3c_invocations();
    let build = invocations
        .iter()
        .find(|i| i[0] == "build")
        .expect("bare tx must lower the project source via build");
    assert!(
        build[1].ends_with("main.tx3"),
        "project source must be the input: {build:?}"
    );
    assert!(
        build
            .windows(2)
            .any(|w| w[0] == "--tx" && w[1] == "transfer"),
        "tx name must be passed through: {build:?}"
    );
    assert!(
        !invocations.iter().any(|i| i[0] == "decode"),
        "project inspection must not decode any TII"
    );
}

#[test]
fn alias_addresses_cached_interface_tii() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let digest = ctx.prime_interface_cache("acme", "widget", "0.1.0");
    ctx.declare_interface("widget", "acme", "widget", "0.1.0", &digest);

    let result =
        ctx.run_trix_with_fake_tx3c(&["inspect", "tir", "--tx", "widget::widget_transfer"], &[]);
    assert_success(&result);
    assert_json_object_line(&result);

    let invocations = ctx.tx3c_invocations();
    let decode = invocations
        .iter()
        .find(|i| i[0] == "decode")
        .expect("an interface tx must be decoded from its cached TII");
    let tii = decode
        .windows(2)
        .find(|w| w[0] == "--tii")
        .map(|w| w[1].clone())
        .expect("decode must receive --tii");
    for segment in ["acme", "widget", "0.1.0", "main.tii"] {
        assert!(
            tii.contains(segment),
            "decode must target the cache at <root>/acme/widget/0.1.0/main.tii, got: {tii}"
        );
    }
    assert!(
        !invocations.iter().any(|i| i[0] == "build"),
        "an interface is consumed from its published TII, never recompiled"
    );
}

/// The fully-qualified registry form addresses the same cache — and works
/// without any `[registry]` section in trix.toml: a cached interface is
/// consumed offline, the registry URL only matters on a cache miss.
#[test]
fn full_ref_addresses_same_cache_without_registry_section() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    assert!(
        ctx.load_trix_config().registry.is_none(),
        "fresh init should not write a [registry] section"
    );

    let digest = ctx.prime_interface_cache("acme", "widget", "0.1.0");
    ctx.declare_interface("widget", "acme", "widget", "0.1.0", &digest);

    let result = ctx.run_trix_with_fake_tx3c(
        &[
            "inspect",
            "tir",
            "--tx",
            "acme/widget:0.1.0::widget_transfer",
        ],
        &[],
    );
    assert_success(&result);
    assert_json_object_line(&result);

    assert!(
        ctx.tx3c_invocations().iter().any(|i| i[0] == "decode"),
        "full registry ref must resolve to the cached interface TII"
    );
}
