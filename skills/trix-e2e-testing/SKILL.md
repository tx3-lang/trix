---
name: trix-e2e-testing
description: Standards and best practices for writing end-to-end tests for the Trix CLI. Apply when adding new e2e tests or modifying existing test infrastructure.
license: MIT
metadata:
  version: "1.0"
---

# E2E Testing Guide for Trix CLI

This document defines the architectural patterns and conventions for writing integration tests that verify CLI behavior through actual command execution.

## Test Structure

```
tests/
├── e2e_tests.rs          # Entry point that includes all e2e modules
├── e2e/
│   ├── mod.rs            # TestContext, assertions, utilities
│   ├── smoke.rs          # Basic "does it run?" tests
│   ├── happy_path.rs     # Comprehensive workflow validation
│   └── edge_cases.rs     # Error cases, edge cases, preservation
└── README.md             # Testing documentation
```

## Core Principles

### 1. Scenario-Based Organization

Organize by **what you're testing**, not by command:

- **Smoke** (`smoke.rs`): One-liners that verify commands don't crash
  - Focus: "Does it run?"
  - Pattern: Individual tests per behavior
  
- **Happy Path** (`happy_path.rs`): Full workflow validation
  - Focus: "Does it work correctly?"
  - Pattern: Single comprehensive test per workflow
  
- **Edge Cases** (`edge_cases.rs`): Error handling and special scenarios
  - Focus: "What happens when things go wrong?"
  - Pattern: Individual tests per edge case

### 2. Struct-Based Assertions

**Prefer deserializing config files into structs over string matching:**

```rust
// ✅ Good - Type-safe, refactoring-friendly
let config = ctx.load_trix_config();
assert!(!config.wallets.is_empty());
assert_eq!(config.protocol.version, "0.0.0");

// ❌ Bad - Brittle, breaks on format changes
let content = ctx.read_file("trix.toml");
assert!(content.contains("version = \"0.0.0\""));
```

### 3. Test Function Naming

**Don't repeat scenario context in function names:**

```rust
// ✅ Good - Context is the file (happy_path.rs)
fn init_creates_valid_project() { }
fn check_validates_valid_project() { }
fn devnet_starts_in_background() { }

// ❌ Bad - Redundant with file location
fn happy_path_init_creates_valid_project() { }
```

### 4. Test Organization Patterns

**Smoke Tests:**
```rust
#[test]
fn init_runs_without_error() {
    let ctx = TestContext::new();
    let result = ctx.run_trix(&["init", "--yes"]);
    assert_success(&result);
}
```

**Happy Path:**
```rust
#[test]
fn init_creates_valid_project_structure() {
    let ctx = TestContext::new();
    ctx.run_trix(&["init", "--yes"]);
    
    // Verify all files exist
    ctx.assert_file_exists("trix.toml");
    ctx.assert_file_exists("main.tx3");
    
    // Verify using struct deserialization
    let config = ctx.load_trix_config();
    assert!(!config.wallets.is_empty());
}
```

**Edge Cases:**
```rust
#[test]
fn init_preserves_existing_gitignore() {
    let ctx = TestContext::new();
    ctx.write_file(".gitignore", "existing");
    ctx.run_trix(&["init", "--yes"]);
    ctx.assert_file_contains(".gitignore", "existing");
}
```

## TestContext API

### Basic Operations

```rust
let ctx = TestContext::new();           // Create isolated temp directory

ctx.run_trix(&["init", "--yes"]);       // Execute trix command
ctx.path();                              // Get temp directory path
ctx.file_path("trix.toml");             // Get full path to file
ctx.read_file("main.tx3");              // Read file as string
ctx.write_file("test.txt", "content");  // Write file (creates dirs)
```

### Assertions

```rust
// File existence
ctx.assert_file_exists("trix.toml");

// File content (string matching)
ctx.assert_file_contains(".gitignore", ".tx3");

// Struct deserialization
let config = ctx.load_trix_config();      // Returns RootConfig
let devnet = ctx.load_devnet_config();    // Returns DevnetConfig  
let test = ctx.load_test_config();        // Returns Test

// Command result
assert_success(&result);
assert_output_contains(&result, "success message");
```

### Background Process Testing

```rust
#[test]
fn devnet_starts_in_background() {
    let ctx = TestContext::new();
    ctx.run_trix(&["init", "--yes"]);
    
    // Start in background mode
    let result = ctx.run_trix(&["devnet", "--background"]);
    assert_success(&result);
    assert_output_contains(&result, "devnet started in background");
    
    // Verify port is open (Dolos gRPC on 5164)
    let port_open = wait_for_port(5164, 30);
    assert!(port_open, "Port should be open within 30s");
    
    // Cleanup
    let _ = std::process::Command::new("pkill")
        .args(["-f", "dolos"])
        .output();
}
```

### Port Availability

```rust
// Wait for port with timeout
if wait_for_port(5164, 30) {
    // Port is open, service is ready
}
```

## Adding New Tests

### 1. Choose the Right File

- **Smoke**: Quick sanity check - does the command exit 0?
- **Happy Path**: Full validation of a feature - files created, content correct
- **Edge Cases**: Specific scenarios like missing files, invalid inputs, preservation

### 2. Use TestContext

Always create a fresh `TestContext` for isolation:

```rust
#[test]
fn my_new_test() {
    let ctx = TestContext::new();
    // ... test code
}
```

### 3. Chain Commands for Workflows

```rust
// Happy path spanning multiple commands
ctx.run_trix(&["init", "--yes"]);
ctx.run_trix(&["check"]);
ctx.run_trix(&["build"]);
```

### 4. Assert with Structs

```rust
// Load config and assert on struct fields
let config = ctx.load_trix_config();
assert!(!config.wallets.is_empty());
assert_eq!(config.protocol.version, "0.0.0");
```

## Key Patterns

### Parallel Safety
Each test gets its own `TempDir`, so tests run in parallel safely. Never use `std::env::set_current_dir()` - use `Command::current_dir()` instead.

### Lib + Binary Pattern
The project is structured as both a library and binary:
- `src/lib.rs` exports all modules publicly
- Tests import structs: `use trix::config::RootConfig;`
- This enables type-safe config assertions

### File Preservation Tests
When testing file preservation (e.g., existing `.gitignore`):
1. Write the existing content
2. Run the command
3. Assert both old and new content exist

### Background Processes
For commands that spawn long-running processes:
1. Use `--background` flag if available
2. Wait for port to confirm process is up
3. Always cleanup in test (even if test fails)

## Complete Example

```rust
// tests/e2e/happy_path.rs
use super::*;

#[test]
fn my_feature_works() {
    let ctx = TestContext::new();
    
    // Setup: Initialize project
    let init_result = ctx.run_trix(&["init", "--yes"]);
    assert_success(&init_result);
    
    // Action: Run my command
    let result = ctx.run_trix(&["my-command"]);
    
    // Assert: Command succeeded
    assert_success(&result);
    assert_output_contains(&result, "expected output");
    
    // Assert: Files created correctly
    ctx.assert_file_exists("output.txt");
    
    // Assert: Config valid
    let config = ctx.load_trix_config();
    assert!(!config.some_field.is_empty());
}
```

## References

- `tests/e2e/mod.rs` - TestContext implementation
- `tests/e2e/smoke.rs` - Smoke test examples
- `tests/e2e/happy_path.rs` - Happy path examples
- `tests/e2e/edge_cases.rs` - Edge case examples
