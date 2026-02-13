use super::*;
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
    assert_eq!(
        test.wallets.len(),
        2,
        "test.toml should contain 2 wallet definitions"
    );
    assert_eq!(
        test.transactions.len(),
        2,
        "test.toml should contain 2 transaction definitions"
    );
    assert_eq!(
        test.expect.len(),
        2,
        "test.toml should contain 2 expectations"
    );
    assert_eq!(test.wallets[0].name, "bob", "first wallet should be bob");
    assert_eq!(
        test.wallets[0].balance, 10000000,
        "bob should have correct balance"
    );
    assert_eq!(
        test.wallets[1].name, "alice",
        "second wallet should be alice"
    );
    assert_eq!(
        test.wallets[1].balance, 5000000,
        "alice should have correct balance"
    );
    assert_eq!(
        test.expect[0].from, "@bob",
        "first expect should be from @bob"
    );
    assert!(
        !test.expect[0].min_amount.is_empty(),
        "first expect should have min_amount"
    );
    assert_eq!(
        test.expect[1].from, "@alice",
        "second expect should be from @alice"
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
