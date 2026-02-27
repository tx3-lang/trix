//! CLI parsing for Trix

use clap::{Parser, Subcommand};

use crate::commands;

#[derive(Parser)]
#[command(name = "trix")]
#[command(about = "Package manager for the Tx3 language", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, short, default_value = "local", global = true)]
    pub profile: String,

    #[arg(long, short, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new Tx3 project
    Init(commands::init::Args),

    /// Invoke a transaction template
    Invoke(commands::invoke::Args),

    /// Start development network (powered by Dolos)
    Devnet(commands::devnet::Args),

    /// Explore a network (powered by CShell)
    Explore(commands::explore::Args),

    /// Generate bindings for smart contracts
    Codegen(commands::codegen::Args),

    /// Check a Tx3 package and all of its dependencies for errors
    Check(commands::check::Args),

    /// Inspect a Tx3 file
    Inspect(commands::inspect::Args),

    /// Run a Tx3 testing file
    Test(commands::test::Args),

    /// Build a Tx3 file
    Build(commands::build::Args),

    /// Manage crypographic identities
    Identities(commands::identities::Args),

    /// Inspect and manage profiles
    Profile(commands::profile::Args),

    /// Run vulnerability analysis scaffolding (UNSTABLE - This feature is experimental and may change)
    #[command(hide = true)]
    Audit(commands::audit::Args),

    /// Publish a Tx3 package into the registry (UNSTABLE - This feature is experimental and may change)
    #[command(hide = true)]
    Publish(commands::publish::Args),

    /// Telemetry configuration. Trix collects anonymous usage data to improve the tool.
    Telemetry(commands::telemetry::Args),
}
