//! Regression tests for previously-fixed defects, so they can never
//! silently return:
//!
//! - npm-style `@` version separators are forbidden in protocol refs; the
//!   canonical grammar maps the version slot to `:` (`src/refs.rs`).
//! - `trix use` accepts a published image carrying an `image/png` logo
//!   layer (PR #118 — before it, the pull rejected the manifest with an
//!   incompatible-layer-media-type error).
//! - OCI repository paths are addressed in lowercase while the original
//!   scope case is preserved for identity (PR #120 — uppercase GitHub
//!   owners like `SundaeSwap-finance` broke registry addressing).
//! - `trix codegen` places every file the template set emits — notably
//!   `package.json` — next to the generated `protocol.ts` (user report:
//!   "package.json not created" in the TS output).
//!
//! The `trix use` tests run fully offline against the in-process OCI
//! registry stub (`super::oci_stub`); the codegen tests use a local
//! template-dir fixture and need a real `tx3c`, like `codegen_deps`.

use super::oci_stub::{OciRegistryStub, StubProtocolImage};
use super::*;
use std::path::PathBuf;
use trix::interfaces::oci::{
    LOGO_PNG_MEDIA_TYPE, MARKDOWN_MEDIA_TYPE, PNG_MAGIC, PROTOCOL_MEDIA_TYPE, TII_MEDIA_TYPE,
};

fn fixture_bytes(relative: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/e2e/fixtures")
        .join(relative);
    std::fs::read(&path).unwrap_or_else(|e| panic!("reading fixture {}: {e}", path.display()))
}

/// Layer set mirroring what `trix publish` pushes: source + TII + README,
/// plus (optionally) the PNG logo layer that PR #118 taught the pull side
/// to accept.
fn protocol_layers(with_logo: bool) -> Vec<(String, Vec<u8>)> {
    let mut layers = vec![
        (
            PROTOCOL_MEDIA_TYPE.to_string(),
            fixture_bytes("use-stub/acme/widget/0.1.0/main.tx3"),
        ),
        (
            TII_MEDIA_TYPE.to_string(),
            fixture_bytes("use-stub/acme/widget/0.1.0/main.tii"),
        ),
        (
            MARKDOWN_MEDIA_TYPE.to_string(),
            fixture_bytes("use-stub/acme/widget/0.1.0/README.md"),
        ),
    ];
    if with_logo {
        let mut png = PNG_MAGIC.to_vec();
        png.extend_from_slice(b"stub-logo-payload");
        layers.push((LOGO_PNG_MEDIA_TYPE.to_string(), png));
    }
    layers
}

/// JSON in the shape of `trix::interfaces::oci::ImageMetadata`, as written
/// by `trix publish` into the OCI config blob. `version` must be concrete
/// so `trix use` can pin.
fn image_metadata(scope: &str, name: &str, version: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "scope": scope,
        "published_date": 1700000000,
        "repository_url": null,
        "description": "stub protocol for regression tests",
        "version": version,
    })
}

/// npm-style `@` version separators must be rejected at parse time — before
/// any registry traffic. The canonical grammar is `scope/name:version`
/// (`:` in the version slot; `@` is not a valid identifier character).
#[test]
fn use_rejects_npm_style_at_version_separator() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    // Point at a live stub so we can prove no request is ever made.
    let image = StubProtocolImage {
        repo: "acme/widget".to_string(),
        tag: "0.1.0".to_string(),
        metadata: image_metadata("acme", "widget", "0.1.0"),
        layers: protocol_layers(false),
    };
    let stub = OciRegistryStub::serve(image.routes());
    ctx.set_registry_url(&stub.url());

    let result = ctx.run_trix(&["use", "acme/widget@0.1.0"]);
    assert!(
        !result.success(),
        "npm-style '@' ref should be rejected, got success:\n{}",
        result.stdout
    );
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("invalid"),
        "error should call the reference invalid:\n{combined}"
    );
    assert!(
        combined.contains("widget@0.1.0"),
        "error should echo the offending reference:\n{combined}"
    );

    assert!(
        stub.requested_paths().is_empty(),
        "rejection must happen before any registry request, but the stub saw: {:?}",
        stub.requested_paths()
    );
}

/// The positive half of the `@` → `:` mapping: the same coordinates with
/// the canonical `:` separator resolve, pull, cache, and pin end to end.
#[test]
fn use_accepts_colon_version_separator() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let image = StubProtocolImage {
        repo: "acme/widget".to_string(),
        tag: "0.1.0".to_string(),
        metadata: image_metadata("acme", "widget", "0.1.0"),
        layers: protocol_layers(false),
    };
    let stub = OciRegistryStub::serve(image.routes());
    ctx.set_registry_url(&stub.url());

    let result = ctx.run_trix(&["use", "acme/widget:0.1.0"]);
    assert_success(&result);

    ctx.assert_file_exists(".tx3/tii/acme/widget/0.1.0/main.tii");
    ctx.assert_file_exists(".tx3/tii/acme/widget/0.1.0/main.tx3");

    let config = ctx.load_trix_config();
    let entry = config
        .interfaces
        .get("widget")
        .expect("interface 'widget' should be pinned in trix.toml");
    assert_eq!(entry.reference.to_string(), "acme/widget:0.1.0");
}

/// Regression for PR #118: a published image carrying an `image/png` logo
/// layer must still pull. Before the fix, `oci::pull`'s accepted media
/// types did not include `image/png`, so the whole manifest was rejected
/// for consumers as soon as the publisher attached a logo.
#[test]
fn use_accepts_image_with_png_logo_layer() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let image = StubProtocolImage {
        repo: "acme/widget".to_string(),
        tag: "0.1.0".to_string(),
        metadata: image_metadata("acme", "widget", "0.1.0"),
        layers: protocol_layers(true),
    };
    let stub = OciRegistryStub::serve(image.routes());
    ctx.set_registry_url(&stub.url());

    let result = ctx.run_trix(&["use", "acme/widget:0.1.0"]);
    assert_success(&result);

    // The pull must succeed and cache the protocol artifacts; the logo
    // layer itself is not part of the interface cache.
    ctx.assert_file_exists(".tx3/tii/acme/widget/0.1.0/main.tii");
    ctx.assert_file_exists(".tx3/tii/acme/widget/0.1.0/main.tx3");
    ctx.assert_file_exists(".tx3/tii/acme/widget/0.1.0/README.md");

    let config = ctx.load_trix_config();
    assert!(
        config.interfaces.get("widget").is_some(),
        "interface should be pinned despite the logo layer"
    );
}

/// Regression for PR #120: scopes mirror GitHub owners and may carry
/// capitals (`SundaeSwap-finance`), but OCI repository paths must be
/// lowercase. The registry must be addressed with the lowercased path
/// while trix.toml and the cache keep the original case for identity.
/// The stub serves the image ONLY under the lowercase path, so any
/// uppercase request 404s and fails the test.
#[test]
fn use_addresses_registry_repo_path_in_lowercase() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let image = StubProtocolImage {
        repo: "sundaeswap-finance/sundae-v3".to_string(),
        tag: "0.1.0".to_string(),
        metadata: image_metadata("SundaeSwap-finance", "sundae-v3", "0.1.0"),
        layers: protocol_layers(false),
    };
    let stub = OciRegistryStub::serve(image.routes());
    ctx.set_registry_url(&stub.url());

    let result = ctx.run_trix(&["use", "SundaeSwap-finance/sundae-v3:0.1.0"]);
    assert_success(&result);

    // Every repository-scoped request must have used the lowercase path.
    let repo_requests: Vec<String> = stub
        .requested_paths()
        .into_iter()
        .filter(|line| line.contains("/manifests/") || line.contains("/blobs/"))
        .collect();
    assert!(
        !repo_requests.is_empty(),
        "expected manifest/blob requests against the stub"
    );
    for line in &repo_requests {
        assert!(
            line.contains("/v2/sundaeswap-finance/sundae-v3/"),
            "registry request should use the lowercase repository path: {line}"
        );
    }

    // Identity keeps the original case: the pinned ref and the cache path.
    let config = ctx.load_trix_config();
    let entry = config
        .interfaces
        .get("sundae-v3")
        .expect("interface 'sundae-v3' should be pinned in trix.toml");
    assert_eq!(
        entry.reference.to_string(),
        "SundaeSwap-finance/sundae-v3:0.1.0"
    );
    ctx.assert_file_exists(".tx3/tii/SundaeSwap-finance/sundae-v3/0.1.0/main.tii");
}

/// Regression lock for the user report "trix codegen: package.json not
/// created": when the template set emits a `package.json`, `trix codegen`
/// must place it next to the generated `protocol.ts` in the per-protocol
/// output subdir. Uses the local `ts-client-lib` fixture (same shape as
/// web-sdk's `.trix/client-lib`, with `package.json.hbs` emitting
/// unconditionally) so the test stays offline. Requires a real `tx3c`,
/// like the `codegen_deps` tests.
#[test]
fn codegen_emits_package_json_next_to_protocol_ts() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let tx3c_path = ctx
        .tx3c_path()
        .expect("tx3c should be available in PATH or TX3_TX3C_PATH");
    assert!(tx3c_path.is_file(), "tx3c path should exist");

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/e2e/fixtures/ts-client-lib")
        .to_str()
        .expect("fixture path should be valid UTF-8")
        .to_string();

    let mut trix_toml = ctx.read_file("trix.toml");
    trix_toml.push_str(&format!(
        "\n[[codegen]]\noutput_dir = \"gen\"\nplugin = {{ repo = \"{template_dir}\", path = \".\" }}\n",
    ));
    ctx.write_file("trix.toml", &trix_toml);

    let project_name = ctx.load_trix_config().protocol.name;

    let result = ctx.run_trix(&["codegen"]);
    assert_success(&result);

    ctx.assert_file_exists(format!("gen/{project_name}/protocol.ts"));
    ctx.assert_file_exists(format!("gen/{project_name}/package.json"));

    // The emitted package.json must be valid JSON naming the protocol.
    let package_json = ctx.read_file(format!("gen/{project_name}/package.json"));
    let parsed: serde_json::Value =
        serde_json::from_str(&package_json).expect("generated package.json should be valid JSON");
    assert_eq!(
        parsed.get("name").and_then(|v| v.as_str()),
        Some(project_name.as_str()),
        "package.json name should match the protocol name"
    );
}

/// The full journey behind the original report: the BUILT-IN `ts-client`
/// plugin must emit a `package.json` next to `protocol.ts`.
///
/// Ignored in the default suite because it needs network access (trix
/// fetches the web-sdk templates from GitHub at the `codegen-v1beta0` tag)
/// plus a real `tx3c`. As of this writing it also still FAILS: the
/// web-sdk templates gate `package.json.hbs` behind
/// `{{#if options.standalone}}`, but the tx3c-delegated codegen path
/// passes no `options` into the template data (and `tx3c codegen` has no
/// CLI surface to receive them), so the file renders empty and is
/// skipped. Un-ignore once the options channel / template gating decision
/// lands and the fix ships.
#[test]
#[ignore = "needs network (GitHub template fetch) + tx3c; currently reproduces the missing-package.json defect"]
fn codegen_ts_client_emits_package_json_from_github_tag() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let tx3c_path = ctx
        .tx3c_path()
        .expect("tx3c should be available in PATH or TX3_TX3C_PATH");
    assert!(tx3c_path.is_file(), "tx3c path should exist");

    let project_name = ctx.load_trix_config().protocol.name;

    let result = ctx.run_trix(&["codegen", "--plugin", "ts-client"]);
    assert_success(&result);

    ctx.assert_file_exists(format!(".tx3/codegen/ts-client/{project_name}/protocol.ts"));
    ctx.assert_file_exists(format!(
        ".tx3/codegen/ts-client/{project_name}/package.json"
    ));
}
