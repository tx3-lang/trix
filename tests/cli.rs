//! CLI suite: spawns only the `trix` binary — never a helper tool.
//!
//! Covers what genuinely needs the binary: arg parsing and dispatch, exit
//! codes, `init`'s filesystem behavior, and the error paths that fire
//! *before* any tool would be spawned (reference resolution, `[interfaces]`
//! validation, cache integrity). Rules themselves are unit-tested in-crate
//! (`src/refs.rs`, `src/interfaces/`); each test here is a wiring probe.
//!
//! See `tests/README.md` for the test-layering methodology.

#[path = "harness/mod.rs"]
mod harness;

#[path = "cli/init.rs"]
mod init;
#[path = "cli/interfaces.rs"]
mod interfaces;
#[path = "cli/refs.rs"]
mod refs;
