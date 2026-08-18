//! `trix use` against the in-process OCI registry stub
//! (`tests/harness/oci_stub.rs`).
//!
//! No helper binary is involved — the pull path is trix's own code talking
//! the distribution protocol — so this stays in the CLI layer. The stub is
//! bound to `127.0.0.1:<random>` and the project is pointed at it through
//! `[registry].url`, keeping the suite offline (`tests/README.md`).
//!
//! Both tests here are regressions re-cut from PR #128 onto the #129
//! layout: they lock defects that were fixed once and must not return.

use crate::harness::oci_stub::{OciRegistryStub, StubProtocolImage};
use crate::harness::*;

use std::path::PathBuf;
use trix::interfaces::oci::{
    LOGO_PNG_MEDIA_TYPE, MARKDOWN_MEDIA_TYPE, PNG_MAGIC, PROTOCOL_MEDIA_TYPE, TII_MEDIA_TYPE,
};

fn fixture_bytes(relative: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative);
    std::fs::read(&path).unwrap_or_else(|e| panic!("reading fixture {}: {e}", path.display()))
}

/// Layer set mirroring what `trix publish` pushes: source + TII + README,
/// plus (optionally) the PNG logo layer PR #118 taught the pull side to
/// accept.
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

/// Regression for PR #118: a published image carrying an `image/png` logo
/// layer must still pull. Before the fix, the accepted media types did not
/// include `image/png`, so the whole manifest was rejected for consumers as
/// soon as the publisher attached a logo.
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
/// lowercase. The registry is addressed with the lowercased path while
/// `trix.toml` and the cache keep the original case for identity. The stub
/// serves the image ONLY under the lowercase path, so any uppercase request
/// 404s and fails the test.
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
