# Trix CLI End-to-End Tests

This directory contains end-to-end (e2e) tests for the Trix CLI tool. These tests execute the actual `trix` binary and verify complete workflows and scenarios.

## Test Organization (Scenario-Based)

Tests are organized by **scenarios** rather than commands, making it easy to understand what aspect of the system is being tested:

```
tests/
├── README.md                # This file
├── e2e_tests.rs            # Test entry point
└── e2e/                    # E2E test modules
    ├── mod.rs              # TestContext + all utilities
    ├── smoke.rs            # "Does it run?" - basic sanity checks
    ├── happy_path.rs       # "Does it work correctly?" - ideal workflows
    └── edge_cases.rs       # "What about edge cases?" - error handling, preservation
```

### Scenario Definitions

| Scenario | Purpose | Example |
|----------|---------|---------|
| **smoke** | Basic functionality - does it run without crashing? | `init_runs_without_error` |
| **happy_path** | Ideal workflows - does it produce correct output? | `init_creates_valid_project_structure` |
| **edge_cases** | Edge cases, error handling, file preservation | `init_preserves_existing_gitignore` |

## Running Tests

**Run all e2e tests:**
```bash
cargo test --test e2e_tests
```

**Run specific scenario:**
```bash
# Smoke tests only
cargo test --test e2e_tests smoke

# Happy path tests only  
cargo test --test e2e_tests happy_path

# Edge case tests only
cargo test --test e2e_tests edge_cases
```

**Run specific test:**
```bash
cargo test --test e2e_tests init_creates_valid_project_structure
```

**Run all tests (including unit tests):**
```bash
cargo test
```

**Run with output visible:**
```bash
cargo test --test e2e_tests -- --nocapture
```

## Current Test Coverage

### Smoke Tests (1 test)

Basic sanity checks - ensure commands run without crashing:

- **`init_runs_without_error`** - Verifies `trix init --yes` executes successfully

### Happy Path Tests (1 test)

Ideal workflows - verify complete, correct behavior:

- **`init_creates_valid_project_structure`** - Comprehensive validation that init creates a fully valid project:
  - All expected files exist (trix.toml, main.tx3, tests/basic.toml, .gitignore, devnet.toml)
  - trix.toml: correct version (0.0.0), ledger (Cardano), main file
  - devnet.toml: has utxo definitions
  - tests/basic.toml: 2 wallets (bob, alice), 2 transactions, 2 expectations
  - main.tx3: Sender/Receiver parties, transfer transaction
  - .gitignore: contains .tx3 extension

### Edge Cases (3 tests)

Edge cases and preservation behavior:

- **`init_preserves_existing_gitignore`** - Verifies existing .gitignore is not overwritten
- **`init_preserves_existing_main_tx3`** - Verifies existing main.tx3 is not overwritten  
- **`init_preserves_existing_test_file`** - Verifies existing tests/basic.toml is not overwritten

**Total: 5 tests**

## Test Utilities (`e2e/mod.rs`)

All e2e-specific utilities are centralized in `e2e/mod.rs`:

### TestContext

Each test creates a `TestContext` which provides an isolated temporary directory:

```rust
let ctx = TestContext::new();
```

### Methods on TestContext

**Command Execution:**
- `ctx.run_trix(args: &[&str]) -> CommandResult` - Execute trix in the temp directory

**Config Loading (Type-Safe!):**
- `ctx.load_trix_config() -> RootConfig` - Load and parse trix.toml
- `ctx.load_devnet_config() -> DevnetConfig` - Load and parse devnet.toml
- `ctx.load_test_config() -> TestConfig` - Load and parse tests/basic.toml

**File Operations:**
- `ctx.write_file(path, content)` - Write file in temp directory
- `ctx.read_file(path) -> String` - Read file from temp directory
- `ctx.assert_file_exists(path)` - Assert file exists
- `ctx.assert_file_contains(path, pattern)` - Assert file contains text

### CommandResult

Returned by `run_trix()`:
- `result.success() -> bool` - Check if command succeeded
- `result.stdout` - Standard output
- `result.stderr` - Standard error

### Assertions

- `assert_success(result)` - Assert command succeeded
- `assert_failure(result)` - Assert command failed

## Writing New Tests

### Choose the Right Scenario

Ask yourself: "What am I testing?"

- **Does it run without crashing?** → `smoke.rs`
- **Does it work correctly in ideal conditions?** → `happy_path.rs`
- **What about edge cases/errors?** → `edge_cases.rs`

### Basic Test Template

```rust
use super::*;

#[test]
fn descriptive_test_name_without_prefix() {
    let ctx = TestContext::new();
    
    // Setup: create any pre-existing files
    ctx.write_file("existing.txt", "content");
    
    // Execute: run the trix command
    let result = ctx.run_trix(&["command", "--arg", "value"]);
    
    // Assert: verify results
    assert_success(&result);
    ctx.assert_file_exists("expected_file.txt");
    
    // Use struct assertions for TOML files!
    let config = ctx.load_trix_config();
    assert_eq!(config.protocol.name, "expected-name");
}
```

### Struct Assertion Examples

**For trix.toml (RootConfig):**
```rust
use std::path::PathBuf;
use trix::config::{RootConfig, KnownLedgerFamily};

let config = ctx.load_trix_config();
assert_eq!(config.protocol.name, "my-project");
assert_eq!(config.protocol.version, "0.0.0");
assert_eq!(config.protocol.main, PathBuf::from("main.tx3"));
assert!(matches!(config.ledger.family, KnownLedgerFamily::Cardano));
```

**For devnet.toml (DevnetConfig):**
```rust
use trix::devnet::Config as DevnetConfig;

let devnet = ctx.load_devnet_config();
assert!(!devnet.utxos.is_empty());
```

**For tests/basic.toml (TestConfig):**
```rust
use trix::commands::test::Test as TestConfig;

let test = ctx.load_test_config();
assert_eq!(test.wallets.len(), 2);
assert_eq!(test.wallets[0].name, "bob");
assert_eq!(test.wallets[0].balance, 10000000);
assert_eq!(test.transactions.len(), 2);
assert_eq!(test.expect.len(), 2);
assert_eq!(test.expect[0].from, "@bob");
```

### Best Practices

1. **Use scenario-based organization** - Put tests in the appropriate file based on what they test
2. **Don't repeat the scenario in function names** - Use `init_creates_valid_project` not `smoke_init_creates_valid_project`
3. **Always use `TestContext::new()`** - Every test should have its own isolated context
4. **Use struct assertions for TOML files** - Type-safe validation beats string matching
5. **Use string assertions only for non-structured files** (e.g., main.tx3, .gitignore)
6. **One test per file for smoke/edge cases, comprehensive tests for happy path**

## Architecture: Lib + Binary

To enable struct-based assertions, the project was refactored to a lib+binary pattern:

```
Cargo.toml
├── [lib] - trix crate (shared code)
└── [[bin]] - trix binary (CLI entry point)

src/
├── lib.rs          # Library exports
├── main.rs         # Binary entry (uses trix::*)
├── cli.rs          # CLI parsing
└── ...             # All modules
```

**Benefits:**
- E2E tests can import `trix::config::RootConfig` and other structs
- Code reuse between binary and tests
- Type-safe test assertions

## Adding New Test Scenarios

As the test suite grows, you may need new scenarios. To add one:

1. **Create new file** in `tests/e2e/` (e.g., `tests/e2e/performance.rs`)
2. **Add module declaration** to `tests/e2e/mod.rs`:
   ```rust
   pub mod performance;
   ```
3. **Write tests** in the new file following the scenario pattern
4. **Update this README** with the new scenario description

## Dependencies

E2E tests rely on:

- **assert_cmd** (2.0) - CLI testing framework
- **tempfile** (3.10) - Temporary directory management

Plus the trix library provides:
- `trix::config::RootConfig` - TOML config struct
- `trix::config::KnownLedgerFamily` - Ledger family enum
- `trix::devnet::Config` - Devnet config struct
- `trix::commands::test::Test` - Test config struct

## Troubleshooting

### "Failed to find trix binary"

Build first:
```bash
cargo build
```

### "Failed to load trix.toml config"

Usually means:
- File wasn't created (check `ctx.assert_file_exists("trix.toml")` first)
- Config format is invalid (rare - means trix has a bug!)
- File path is wrong

### Struct field errors

If you get compile errors about missing fields, the config struct changed. **This is good** - it caught a breaking change! Update the test to match the new structure.

### Type mismatches

Remember `PathBuf` for paths:
```rust
// Correct:
assert_eq!(config.protocol.main, PathBuf::from("main.tx3"));

// Wrong:
assert_eq!(config.protocol.main, "main.tx3");  // Type mismatch!
```

## Future Growth

This structure supports rapid test growth:

- **New commands**: Add tests to appropriate scenario files
- **New scenarios**: Create new files in `tests/e2e/`
- **Workflow tests**: Comprehensive multi-command tests go in `happy_path.rs`
- **Performance tests**: Could add `tests/e2e/performance.rs`
- **Regression tests**: Could add `tests/e2e/regression.rs` for bug reproductions
