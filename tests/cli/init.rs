//! `trix init`: scaffolding and preservation. Purely filesystem behavior of
//! the trix binary — no helper tools involved.

use crate::harness::*;
use std::path::PathBuf;
use trix::config::KnownLedgerFamily;

#[test]
fn init_creates_valid_project_structure() {
    let ctx = TestContext::new();
    let result = ctx.run_trix(&["init", "--yes"]);

    assert_success(&result);

    // Verify all expected files exist
    ctx.assert_file_exists("trix.toml");
    ctx.assert_file_exists("main.tx3");
    ctx.assert_file_exists("tests/basic.toml");
    ctx.assert_file_exists(".gitignore");
    ctx.assert_file_exists("devnet.toml");

    // Verify trix.toml using struct deserialization
    let config = ctx.load_trix_config();
    assert!(
        !config.protocol.name.is_empty(),
        "protocol name should not be empty"
    );
    assert_eq!(
        config.protocol.version, "0.0.0",
        "version should be default 0.0.0"
    );
    assert_eq!(
        config.protocol.main,
        PathBuf::from("main.tx3"),
        "main file should be main.tx3"
    );
    assert!(
        matches!(config.ledger.family, KnownLedgerFamily::Cardano),
        "ledger family should be Cardano"
    );

    // Verify devnet.toml using struct deserialization
    let devnet = ctx.load_devnet_config();
    assert!(
        !devnet.utxos.is_empty(),
        "devnet.toml should contain utxo definitions"
    );

    // Verify tests/basic.toml using struct deserialization
    let test = ctx.load_test_config();
    assert!(
        !test.wallets.is_empty(),
        "test.toml should contain wallet definitions"
    );
    assert!(
        !test.transactions.is_empty(),
        "test.toml should contain transaction definitions"
    );
    assert!(
        !test.expect.is_empty(),
        "test.toml should contain expectations"
    );

    // Verify main.tx3 content
    let main_content = ctx.read_file("main.tx3");
    assert!(
        main_content.contains("party Sender"),
        "main.tx3 should contain Sender party"
    );
    assert!(
        main_content.contains("party Receiver"),
        "main.tx3 should contain Receiver party"
    );
    assert!(
        main_content.contains("tx transfer"),
        "main.tx3 should contain transfer transaction"
    );

    // Verify .gitignore content
    let gitignore_content = ctx.read_file(".gitignore");
    assert!(
        gitignore_content.contains(".tx3"),
        ".gitignore should contain .tx3 extension"
    );
}

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

/// First run against a pristine `TX3_HOME`: the global config is created
/// under it (not the real `~/.tx3` — the isolation seam every other test
/// relies on) and the one-time telemetry notice is printed.
#[test]
fn first_run_creates_global_config_under_tx3_home() {
    let ctx = TestContext::new_unseeded();
    let result = ctx.run_trix(&["init", "--yes"]);

    assert_success(&result);
    assert_output_contains(&result, "trix collects anonymous usage data");
    assert!(
        ctx.tx3_home().join("trix/config.toml").is_file(),
        "global config should be created under TX3_HOME"
    );
}
