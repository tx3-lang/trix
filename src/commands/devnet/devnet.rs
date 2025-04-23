use std::process::{Command, Stdio};

use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, bail};

use crate::config::Config;

use super::{get_home_path, handle_devnet};

#[derive(ClapArgs)]
pub struct Args {
    /// run devnet as a background process
    #[arg(short, long, default_value_t = false)]
    background: bool,
}

pub fn run(args: Args, config: &Config) -> miette::Result<()> {
    let home_path = get_home_path()?;
    let tmp_path = handle_devnet(&home_path, config)?;

    let mut dolos_config_path = tmp_path.clone();
    dolos_config_path.push("dolos.toml");

    let mut dolos_path = home_path.clone();

    if cfg!(target_os = "windows") {
        dolos_path.push(".tx3/default/bin/dolos.exe");
    } else {
        dolos_path.push(".tx3/default/bin/dolos");
    };

    let mut cmd = Command::new(dolos_path.to_str().unwrap_or_default());

    cmd.args([
        "-c",
        dolos_config_path.to_str().unwrap_or_default(),
        "daemon",
    ]);

    if args.background {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());

        cmd.spawn()
            .into_diagnostic()
            .context("failed to spawn dolos devnet in background")?;

        println!("devnet started in background");
        return Ok(());
    }

    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

    let mut child = cmd
        .spawn()
        .into_diagnostic()
        .context("failed to spawn dolos devnet")?;

    let status = child
        .wait()
        .into_diagnostic()
        .context("failed to wait for dolos devnet")?;

    if !status.success() {
        bail!("dolos devnet exited with code: {}", status);
    }

    Ok(())
}
