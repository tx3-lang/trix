use clap::{Args as ClapArgs, Subcommand};

use crate::global::print_telemetry_info;

#[derive(ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Enable telemetry (anonymous usage data collection)
    On,
    /// Disable telemetry
    Off,
    /// Show the current telemetry status
    Status,
}

pub fn run(args: Args) -> miette::Result<()> {
    let mut global_config = crate::global::read_config()?;

    match args.command {
        Command::On => {
            global_config.telemetry.enabled = true;
            crate::global::save_config(&global_config)?;
            print_status(&global_config);
        }
        Command::Off => {
            global_config.telemetry.enabled = false;
            crate::global::save_config(&global_config)?;
            print_status(&global_config);
        }
        Command::Status => {
            print_status(&global_config);
        }
    }

    Ok(())
}

fn print_status(config: &crate::global::Config) {
    if config.telemetry.enabled {
        print_telemetry_info();
        println!("Telemetry: ON");
        
        // Shows user fingerprint if available
        if let Some(ref user_fingerprint) = config.telemetry.user_fingerprint {
            println!("User Fingerprint: {}", user_fingerprint);
        } else if let Ok(user_fingerprint) = crate::telemetry::get_user_fingerprint() {
            println!("User Fingerprint: {}", user_fingerprint);
        }
        return;
    }

    println!("Telemetry: OFF");
}
