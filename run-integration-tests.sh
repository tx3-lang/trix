#!/bin/bash
# Run integration tests for Trix CLI
# Tests now run safely in parallel with isolated TestContext

cargo test --test integration_tests "$@"