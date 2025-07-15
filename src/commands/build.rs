use std::{
    collections::HashMap,
    fmt::Display,
    fs,
    io::{self, BufRead, Write},
    str::FromStr,
};

use crate::config::{Config, ProfileConfig};
use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, bail};
use serde_json::json;
use tx3_lang::{Protocol, ir};

#[derive(ClapArgs)]
pub struct Args {
    /// Path to save to a json file
    #[arg(short, long)]
    output: Option<String>,

    /// Select a profile to use the env_file for the args
    #[arg(short, long)]
    profile: Option<String>,
}

pub fn run(args: Args, config: &Config) -> miette::Result<()> {
    let profiles = config.profiles.as_ref();

    let profile: miette::Result<ProfileConfig> = match args.profile {
        Some(profile_arg) => match profiles {
            Some(p) => match profile_arg.as_str() {
                "devnet" => Ok(p.devnet.clone()),
                "preview" => Ok(p.preview.clone().unwrap_or_default()),
                "preprod" => Ok(p.preprod.clone().unwrap_or_default()),
                "mainnet" => Ok(p.mainnet.clone().unwrap_or_default()),
                _ => bail!("invalid profile"),
            },
            None => bail!("profile argument was provided but profiles are missing"),
        },
        None => Ok(profiles.map(|p| p.devnet.clone()).unwrap_or_default()),
    };

    let profile = profile?;

    let envs = if let Some(e) = profile.env_file {
        match fs::File::open(e) {
            Ok(file) => {
                let reader = io::BufReader::new(file);
                let mut map = HashMap::new();

                for line in reader.lines() {
                    let line = line.unwrap_or_default();
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }

                    if let Some((key, value)) = line.split_once('=') {
                        map.insert(
                            key.trim().to_string(),
                            value.trim_matches('"').trim().to_string(),
                        );
                    }
                }
                Some(map)
            }
            Err(error) => {
                eprintln!("failed to load env file: {}", error);
                None
            }
        }
    } else {
        None
    };

    let main_path = config.protocol.main.clone();

    let protocol = Protocol::from_file(main_path)
        .load()
        .into_diagnostic()
        .context("parsing tx3 file")?;

    let values = protocol
        .txs()
        .map(|tx| {
            let prototx = protocol.new_tx(&tx.name.value).unwrap();

            let hex = hex::encode(prototx.ir_bytes());

            let mut args = serde_json::Map::new();
            for (key, kind) in prototx.find_params().iter() {
                let tx3_type = Tx3Type(kind.clone());

                if let Some(envs) = envs.as_ref() {
                    if let Some((_, value)) = envs.iter().find(|(k, _)| k.eq_ignore_ascii_case(key))
                    {
                        args.insert(key.clone(), tx3_type.env_to_value(value));
                        continue;
                    }
                }

                args.insert(key.clone(), serde_json::Value::String(tx3_type.to_string()));
            }
            let args_value = serde_json::Value::Object(args);

            json!({
                "tir": {
                    "bytecode": hex,
                    "encoding": "hex",
                    "version": tx3_lang::ir::IR_VERSION
                },
                "args": args_value
            })
        })
        .collect::<Vec<_>>();

    let json = serde_json::to_string_pretty(&values).into_diagnostic()?;

    if let Some(output) = args.output {
        let mut file = fs::File::create(&output)
            .into_diagnostic()
            .context("invalid output path")?;

        file.write_all(json.as_bytes()).into_diagnostic()?;
    } else {
        println!("{json}");
    }

    Ok(())
}

struct Tx3Type(ir::Type);
impl Tx3Type {
    pub fn env_to_value(&self, value: &str) -> serde_json::Value {
        match &self.0 {
            ir::Type::Undefined => serde_json::Value::Null,
            ir::Type::Unit => serde_json::Value::String(String::from(value)),
            ir::Type::Int => match serde_json::Number::from_str(value) {
                Ok(number) => serde_json::Value::Number(number),
                Err(error) => {
                    eprintln!("failed to parse env to number: {} {}", value, error);
                    serde_json::Value::String(self.to_string())
                }
            },
            ir::Type::Bool => match bool::from_str(value) {
                Ok(bool) => serde_json::Value::Bool(bool),
                Err(error) => {
                    eprintln!("failed to parse env to bool: {} {}", value, error);
                    serde_json::Value::String(self.to_string())
                }
            },
            ir::Type::Bytes => match value.starts_with("0x") {
                true => serde_json::Value::String(String::from(value)),
                false => {
                    eprintln!(
                        "for bytes type, the env should be base16 and start with 0x: {}",
                        value
                    );
                    serde_json::Value::String(self.to_string())
                }
            },
            ir::Type::Address => serde_json::Value::String(String::from(value)),
            _ => serde_json::Value::String(self.to_string()),
        }
    }
}

impl Display for Tx3Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            ir::Type::Undefined => write!(f, "undefined"),
            ir::Type::Unit => write!(f, "unit"),
            ir::Type::Int => write!(f, "int"),
            ir::Type::Bool => write!(f, "bool"),
            ir::Type::Bytes => write!(f, "bytes"),
            ir::Type::Address => write!(f, "address"),
            ir::Type::UtxoRef => write!(f, "utxo_ref"),
            ir::Type::AnyAsset => write!(f, "any_asset"),
            ir::Type::List => write!(f, "list"),
            ir::Type::Custom(name) => write!(f, "custom({})", name),
        }
    }
}
