use super::*;

/// `trix use` rejects an alias-only reference at parse time because aliases
/// don't carry version info.
#[test]
fn use_rejects_alias_only_reference() {
    let ctx = TestContext::new();
    let init = ctx.run_trix(&["init", "--yes"]);
    assert_success(&init);

    let result = ctx.run_trix(&["use", "widget"]);
    assert!(!result.success(), "expected failure, got: {:?}", result.stdout);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("alias")
            || combined.contains("registry reference")
            || combined.contains("scope"),
        "stderr should explain the registry-only requirement:\n{}",
        combined
    );
}

/// Without a `[registry]` section, `trix use` refuses to talk to any
/// registry (no silent default).
#[test]
fn use_fails_without_registry_configured() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix(&["use", "acme/widget:0.1.0"]);
    assert!(!result.success());
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("registry"),
        "stderr should mention the missing [registry]:\n{}",
        combined
    );
}

/// `trix check` succeeds when a dependency is present in the cache and
/// declared in trix.toml.
#[test]
fn check_runs_with_dependency() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let digest = ctx.prime_dep_cache("acme", "widget", "0.1.0");
    ctx.declare_dep("widget", "acme", "widget", "0.1.0", &digest);

    let result = ctx.run_trix(&["check"]);
    assert_success(&result);
    assert_output_contains(&result, "check passed");
}

/// Inspect a transaction that lives inside a dependency, addressed via
/// `<alias>::<tx>`.
#[test]
fn inspect_tir_addresses_dep_transaction_by_alias() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let digest = ctx.prime_dep_cache("acme", "widget", "0.1.0");
    ctx.declare_dep("widget", "acme", "widget", "0.1.0", &digest);

    let result = ctx.run_trix(&["inspect", "tir", "--tx", "widget::widget_transfer"]);
    assert_success(&result);
    // The stdout includes any update-banner preamble; only the JSON line is
    // structured. Confirm at least one line parses as a JSON object.
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

/// Inspect via the fully-qualified registry form.
#[test]
fn inspect_tir_addresses_dep_transaction_by_full_ref() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let digest = ctx.prime_dep_cache("acme", "widget", "0.1.0");
    ctx.declare_dep("widget", "acme", "widget", "0.1.0", &digest);

    let result = ctx.run_trix(&[
        "inspect",
        "tir",
        "--tx",
        "acme/widget:0.1.0::widget_transfer",
    ]);
    assert_success(&result);
}

/// Inspecting a bare tx name continues to target the project's own protocol.
#[test]
fn inspect_tir_bare_tx_targets_project() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let digest = ctx.prime_dep_cache("acme", "widget", "0.1.0");
    ctx.declare_dep("widget", "acme", "widget", "0.1.0", &digest);

    // The default init template defines `tx transfer`.
    let result = ctx.run_trix(&["inspect", "tir", "--tx", "transfer"]);
    assert_success(&result);
}

/// Unknown alias on `inspect tir --tx`: useful error, non-zero exit.
#[test]
fn inspect_tir_rejects_unknown_alias() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix(&["inspect", "tir", "--tx", "ghost::transfer"]);
    assert!(!result.success());
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("ghost") || combined.contains("no protocol named"),
        "stderr should mention the unknown alias:\n{}",
        combined
    );
}

/// Tampered cache digest: `trix check` surfaces a digest-mismatch error and
/// tells the user to `trix use --force` to refresh.
#[test]
fn check_fails_on_digest_mismatch_after_tamper() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let digest = ctx.prime_dep_cache("acme", "widget", "0.1.0");
    // Declare the dep with a digest that does NOT match the metadata.json's.
    ctx.declare_dep(
        "widget",
        "acme",
        "widget",
        "0.1.0",
        "sha256:0000000000000000000000000000000000000000000000000000000000000bad",
    );
    let _ = digest;

    let result = ctx.run_trix(&["check"]);
    assert!(
        !result.success(),
        "expected digest mismatch failure but got success:\n{}",
        result.stdout
    );
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("digest"),
        "stderr should mention the digest mismatch:\n{}",
        combined
    );
}

/// Hand-edited trix.toml with an alias-only `ref` value: rejected on every
/// scoped command via the same diagnostic the CLI uses.
#[test]
fn trix_toml_rejects_alias_only_ref() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let mut content = ctx.read_file("trix.toml");
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(
        "\n[dependencies.widget]\nref = \"widget\"\ndigest = \"sha256:deadbeef\"\n",
    );
    ctx.write_file("trix.toml", &content);

    let result = ctx.run_trix(&["check"]);
    assert!(
        !result.success(),
        "alias-only ref in trix.toml should be rejected"
    );
}

/// Hand-edited trix.toml with `ref = "acme/widget:latest"`: rejected because
/// the file is a pinned lockfile, only concrete versions allowed.
#[test]
fn trix_toml_rejects_latest_ref() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let mut content = ctx.read_file("trix.toml");
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(
        "\n[dependencies.widget]\nref = \"acme/widget:latest\"\ndigest = \"sha256:deadbeef\"\n",
    );
    ctx.write_file("trix.toml", &content);

    let result = ctx.run_trix(&["check"]);
    assert!(
        !result.success(),
        "latest ref in trix.toml should be rejected"
    );
}

/// Projects without `[dependencies]` should be entirely unaffected.
#[test]
fn projects_without_dependencies_section_unchanged() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix(&["check"]);
    assert_success(&result);

    let config = ctx.load_trix_config();
    assert!(
        config.dependencies.is_empty(),
        "fresh init should have no dependencies declared"
    );
}
