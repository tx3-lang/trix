use clap::Args as ClapArgs;
use miette::{Context as _, IntoDiagnostic as _};

use crate::config::RootConfig;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(short, long)]
    tx: String,

    #[arg(short, long)]
    pretty: bool,
}

pub fn run(args: Args, config: &RootConfig) -> miette::Result<()> {
    let main_path = config.protocol.main.clone();

    let content = std::fs::read_to_string(main_path).into_diagnostic()?;

    let mut ast = tx3_lang::parsing::parse_string(&content)?;

    tx3_lang::analyzing::analyze(&mut ast).ok()?;

    let ir = tx3_lang::lowering::lower(&ast, &args.tx)
        .into_diagnostic()
        .with_context(|| format!("lowering {}", args.tx))?;

    if args.pretty {
        println!("{}", serde_json::to_string_pretty(&ir).into_diagnostic()?);
    } else {
        println!("{}", serde_json::to_string(&ir).into_diagnostic()?);
    }

    Ok(())
}
