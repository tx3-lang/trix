use super::*;
use std::path::PathBuf;
#[cfg(feature = "unstable")]
use trix::commands::audit::model::AnalysisStateJson;
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
    // Just check basic root structures exist, not every field
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
fn check_validates_valid_project() {
    let ctx = TestContext::new();

    // First init a project with valid Tx3 files
    ctx.run_trix(&["init", "--yes"]);

    // Then run check on the valid project
    let result = ctx.run_trix(&["check"]);

    assert_success(&result);
    assert_output_contains(&result, "check passed, no errors found");
}

#[test]
fn devnet_starts_and_cshell_connects() {
    let ctx = TestContext::new();

    // First init a project
    let init_result = ctx.run_trix(&["init", "--yes"]);
    assert_success(&init_result);

    // Start devnet in background
    let result = ctx.run_trix(&["devnet", "--background"]);

    assert_success(&result);
    assert_output_contains(&result, "devnet started in background");

    // Wait for gRPC port to be open (Dolos uses port 5164 for gRPC)
    let port_open = wait_for_port(5164, 30);
    assert!(
        port_open,
        "Devnet gRPC port 5164 should be open within 30 seconds"
    );

    // Setup cshell environment using the project's wallet setup function
    // Change to temp directory so wallet::setup can find trix.toml via protocol_root()
    let original_dir = std::env::current_dir().expect("should get current dir");
    std::env::set_current_dir(ctx.path()).expect("should change to temp dir");

    let config = ctx.load_trix_config();
    let profile = config
        .resolve_profile("local")
        .expect("should resolve local profile");
    let wallet = trix::wallet::setup(&config, &profile).expect("should setup cshell environment");

    // Restore original directory
    std::env::set_current_dir(original_dir).expect("should restore original dir");

    // Run cshell provider test using the spawn mechanism
    let test_result = trix::spawn::cshell::provider_test(&wallet.target_dir, "trix-local");
    assert!(
        test_result.is_ok(),
        "cshell provider test should succeed: {:?}",
        test_result.err()
    );

    // Cleanup: kill dolos process
    let _ = std::process::Command::new("pkill")
        .args(["-f", "dolos"])
        .output();
}

#[test]
#[cfg(feature = "unstable")]
fn aiken_audit_runs_in_initialized_project() {
    let ctx = TestContext::new();
    let init_result = ctx.run_trix(&["init", "--yes"]);
    assert_success(&init_result);

    let result = ctx.run_trix(&["audit"]);

    assert_success(&result);
    assert_output_contains(&result, "EXPERIMENTAL");

    ctx.assert_file_exists(".tx3/audit/state.json");
    ctx.assert_file_exists(".tx3/audit/vulnerabilities.md");

    let state_content = ctx.read_file(".tx3/audit/state.json");
    let state: AnalysisStateJson =
        serde_json::from_str(&state_content).expect("state.json should be valid AnalysisStateJson");

    assert_eq!(state.version, "1");
    assert_eq!(
        state.iterations.len(),
        3,
        "expected one iteration per seed skill"
    );
}
