//! Trix - The Tx3 package manager
//!
//! This library provides the core functionality of the Trix CLI tool,
//! including configuration management, command execution, and blockchain
//! integration for the Tx3 language.

pub mod builder;
pub mod cli;
pub mod commands;
pub mod config;
pub mod devnet;
pub mod dirs;
pub mod global;
pub mod home;
pub mod spawn;
pub mod telemetry;
pub mod updates;
pub mod wallet;
