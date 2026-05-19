//! Spawning the external CLIs `trix` orchestrates.
//!
//! `trix` links no `tx3-*` crate; it drives the toolchain binaries (`tx3c`,
//! `cshell`, `dolos`) as subprocesses. Version compatibility for every
//! integration lives in [`compat`]; each spawn path calls
//! [`ensure_supported`] at its command chokepoint before invoking the tool.

pub mod compat;
pub mod cshell;
pub mod dolos;
pub mod tx3c;

pub use compat::ensure_supported;
