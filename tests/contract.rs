//! Contract suite: trix's side of the `tx3c` process contract, exercised
//! against a fake `tx3c` (`tests/harness/fake_tx3c.rs`) — never the real
//! binary. What's asserted is what trix *sends* (argv, paths, ordering) and
//! how it *interprets* what comes back (stdout JSON, exit codes, stderr).
//!
//! The real cross-binary interop — does actual tx3c behave as the fake
//! pretends — is deliberately out of scope: that's the umbrella repo's DX
//! e2e journeys (`solution/e2e/`), which gate toolchain releases with real
//! released binaries. See `tests/README.md`.

#[path = "harness/mod.rs"]
mod harness;

#[path = "contract/check.rs"]
mod check;
#[path = "contract/codegen.rs"]
mod codegen;
#[path = "contract/compat.rs"]
mod compat;
#[path = "contract/inspect.rs"]
mod inspect;
