use super::*;

#[test]
fn init_preserves_existing_gitignore() {
    let ctx = TestContext::new();
    let existing_gitignore = "# My custom gitignore\n*.log\n";
    ctx.write_file(".gitignore", existing_gitignore);

    let result = ctx.run_trix(&["init", "--yes"]);

    assert_success(&result);
    ctx.assert_file_contains(".gitignore", "# My custom gitignore");
    ctx.assert_file_contains(".gitignore", "*.log");
}

#[test]
fn init_preserves_existing_main_tx3() {
    let ctx = TestContext::new();
    let existing_content = "// This is my existing main.tx3 file\nparty User;\n";
    ctx.write_file("main.tx3", existing_content);

    let result = ctx.run_trix(&["init", "--yes"]);

    assert_success(&result);
    ctx.assert_file_contains("main.tx3", "// This is my existing main.tx3 file");
    ctx.assert_file_contains("main.tx3", "party User");
}

#[test]
fn init_preserves_existing_test_file() {
    let ctx = TestContext::new();
    ctx.write_file(
        "tests/basic.toml",
        "# Custom test file\n[[wallets]]\nname = \"custom\"\n",
    );

    let result = ctx.run_trix(&["init", "--yes"]);

    assert_success(&result);
    ctx.assert_file_contains("tests/basic.toml", "# Custom test file");
    ctx.assert_file_contains("tests/basic.toml", "name = \"custom\"");
}

#[test]
#[cfg(feature = "unstable")]
fn aiken_audit_fails_without_trix_config() {
    let ctx = TestContext::new();
    let result = ctx.run_trix(&["audit"]);

    assert!(
        !result.success(),
        "audit should fail outside scoped project"
    );
    assert!(
        result
            .stderr
            .contains("No trix.toml found in current directory"),
        "Expected missing trix.toml error, got stderr: {}",
        result.stderr
    );
}

#[test]
#[cfg(feature = "unstable")]
fn aiken_audit_fails_with_missing_skills_dir() {
    let ctx = TestContext::new();
    let init_result = ctx.run_trix(&["init", "--yes"]);
    assert_success(&init_result);

    let result = ctx.run_trix(&["audit", "--skills-dir", "skills/does-not-exist"]);

    assert!(
        !result.success(),
        "audit should fail with invalid skills dir"
    );
    assert!(
        result.stderr.contains("Audit skills directory not found"),
        "Expected missing skills directory error, got stderr: {}",
        result.stderr
    );
}
