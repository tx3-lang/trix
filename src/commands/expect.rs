use std::path::Path;

use miette::Result;

use crate::spawn::cshell;

// Import Expect types from the `test` module
use crate::commands::test::ExpectUtxo;

/// Run all checks for a slice of `ExpectUtxo` expectations.
/// Returns `Ok(true)` when any expectation failed, `Ok(false)` when all passed.
///
///
pub fn expect_utxo(expects: &[ExpectUtxo], test_home: &Path) -> Result<bool> {
    let mut failed_any = false;

    for expect in expects.iter() {
        let mut failed = false;

        let utxos = cshell::wallet_utxos(&test_home, &expect.from)?;

        if expect.datum_equals.is_none() && expect.min_amount.is_empty() {
            if utxos.is_empty() {
                failed_any = true;
                eprintln!("Test Failed: No UTXOs found for wallet `{}`.", expect.from);
            }
            continue;
        }

        // Find UTXOs that match the datum if specified
        let matching_utxos: Vec<_> = if let Some(expected_datum) = &expect.datum_equals {
            utxos
                .iter()
                .filter(|utxo| {
                    if let Some(datum) = &utxo.datum {
                        match expected_datum {
                            serde_json::Value::String(s) => hex::encode(&datum.hash) == *s,
                            _ => false,
                        }
                    } else {
                        false
                    }
                })
                .collect()
        } else {
            // If no datum_equals specified, consider all UTXOs
            utxos.iter().collect()
        };

        for min_req in &expect.min_amount {
            let total_amount: u64 =
                if let (Some(policy), Some(name)) = (&min_req.policy, &min_req.name) {
                    // Check for specific asset
                    matching_utxos
                        .iter()
                        .flat_map(|utxo| utxo.assets.iter())
                        .map(|bal| {
                            let policy_hex = hex::encode(&bal.policy_id);
                            if policy_hex == *policy {
                                bal.assets
                                    .iter()
                                    .filter(|asset| String::from_utf8_lossy(&asset.name) == *name)
                                    .map(|asset| asset.output_coin.parse::<u64>().unwrap_or(0))
                                    .sum::<u64>()
                            } else {
                                0u64
                            }
                        })
                        .sum()
                } else {
                    // Check for lovelace
                    matching_utxos
                        .iter()
                        .map(|utxo| utxo.coin.parse::<u64>().unwrap_or(0))
                        .sum()
                };

            if total_amount < min_req.amount {
                failed = true;

                let asset_desc =
                    if let (Some(policy), Some(name)) = (&min_req.policy, &min_req.name) {
                        format!("asset {}.{}", policy, name)
                    } else {
                        "lovelace".to_string()
                    };

                eprintln!(
                    "Test Failed: wallet `{}` with insufficient {}.",
                    expect.from, asset_desc
                );
                eprintln!("Expected minimum: {}", min_req.amount);
                eprintln!("Found: {}", total_amount);
            }
        }

        if failed {
            failed_any = true;
        }
    }

    Ok(failed_any)
}
