//! Integration tests for the `trix audit` spawn wrapper.
//!
//! Strategy: point `TX3_PREFLIGHT_PATH` at a small bash script that records
//! its argv to a file and exits 0. Then run `trix audit ...` and assert that
//! the recorded argv contains the flags we expect to forward.

#![cfg(all(unix, feature = "unstable"))]

use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn install_mock_preflight(ctx: &TestContext, log_path: &str) -> std::path::PathBuf {
    let mock_path = ctx.file_path("mock-preflight.sh");
    let log_full_path = ctx.file_path(log_path);

    let script = format!(
        "#!/usr/bin/env bash\nprintf '%s\\n' \"$@\" > {log}\nexit 0\n",
        log = log_full_path.display(),
    );
    fs::write(&mock_path, script).expect("write mock script");

    let mut perms = fs::metadata(&mock_path).expect("stat mock").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&mock_path, perms).expect("chmod mock");

    mock_path
}

fn run_audit_with_mock(ctx: &TestContext, audit_args: &[&str]) -> (CommandResult, Vec<String>) {
    let init_result = ctx.run_trix(&["init", "--yes"]);
    assert_success(&init_result);

    let mock_path = install_mock_preflight(ctx, "argv.log");
    let mock_path_str = mock_path.to_string_lossy().to_string();

    let result = ctx.run_trix_with_env(
        audit_args,
        &[("TX3_PREFLIGHT_PATH", mock_path_str.as_str())],
    );

    let recorded =
        fs::read_to_string(ctx.file_path("argv.log")).expect("mock should have written argv.log");
    let lines: Vec<String> = recorded.lines().map(str::to_string).collect();

    (result, lines)
}

fn flag_value<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    argv.iter()
        .position(|a| a == flag)
        .and_then(|i| argv.get(i + 1))
        .map(String::as_str)
}

#[test]
fn forwards_default_flags_and_injects_main_source() {
    let ctx = TestContext::new();
    let (result, argv) = run_audit_with_mock(&ctx, &["audit"]);

    assert_success(&result);

    // Default flag values from src/commands/audit.rs are forwarded.
    assert_eq!(
        flag_value(&argv, "--provider"),
        Some("scaffold"),
        "argv: {:?}",
        argv
    );
    assert_eq!(
        flag_value(&argv, "--state-out"),
        Some(".tx3/audit/state.json")
    );
    assert_eq!(
        flag_value(&argv, "--report-out"),
        Some(".tx3/audit/vulnerabilities.md")
    );
    assert_eq!(
        flag_value(&argv, "--skills-dir"),
        Some("skills/vulnerabilities")
    );
    assert_eq!(
        flag_value(&argv, "--ast-out"),
        Some(".tx3/audit/aiken-ast.json")
    );
    assert_eq!(flag_value(&argv, "--read-scope"), Some("workspace"));

    // --main-source is injected from RootConfig.protocol.main, not from the
    // user-facing CLI of `trix audit`. The init template uses "main.tx3".
    assert_eq!(flag_value(&argv, "--main-source"), Some("main.tx3"));

    // Boolean flags default to off → not present in argv.
    assert!(!argv.iter().any(|a| a == "--ai-logs"));
    assert!(!argv.iter().any(|a| a == "--no-ast-cache"));
    assert!(!argv.iter().any(|a| a == "--interactive-permissions"));
}

#[test]
fn forwards_provider_overrides_and_optional_flags() {
    let ctx = TestContext::new();
    let (result, argv) = run_audit_with_mock(
        &ctx,
        &[
            "audit",
            "--provider",
            "openai",
            "--model",
            "gpt-test",
            "--endpoint",
            "https://example/v1/responses",
            "--api-key-env",
            "MY_KEY",
            "--reasoning-effort",
            "high",
            "--ai-logs",
            "--no-ast-cache",
            "--read-scope",
            "strict",
            "--interactive-permissions",
        ],
    );

    assert_success(&result);

    assert_eq!(flag_value(&argv, "--provider"), Some("openai"));
    assert_eq!(flag_value(&argv, "--model"), Some("gpt-test"));
    assert_eq!(
        flag_value(&argv, "--endpoint"),
        Some("https://example/v1/responses")
    );
    assert_eq!(flag_value(&argv, "--api-key-env"), Some("MY_KEY"));
    assert_eq!(flag_value(&argv, "--reasoning-effort"), Some("high"));
    assert_eq!(flag_value(&argv, "--read-scope"), Some("strict"));

    assert!(argv.iter().any(|a| a == "--ai-logs"));
    assert!(argv.iter().any(|a| a == "--no-ast-cache"));
    assert!(argv.iter().any(|a| a == "--interactive-permissions"));
}

#[test]
fn propagates_non_zero_exit_from_preflight() {
    let ctx = TestContext::new();
    let init_result = ctx.run_trix(&["init", "--yes"]);
    assert_success(&init_result);

    // Mock that exits non-zero.
    let mock_path = ctx.file_path("mock-fail.sh");
    fs::write(&mock_path, "#!/usr/bin/env bash\nexit 7\n").expect("write");
    let mut perms = fs::metadata(&mock_path).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&mock_path, perms).expect("chmod");

    let result = ctx.run_trix_with_env(
        &["audit"],
        &[(
            "TX3_PREFLIGHT_PATH",
            mock_path.to_string_lossy().to_string().as_str(),
        )],
    );

    assert!(
        !result.success(),
        "trix audit should fail when preflight exits non-zero"
    );
}
