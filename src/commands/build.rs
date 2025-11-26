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

pub fn run(args: Args, config: &Config, profile: &ProfileConfig) -> miette::Result<()> {
    let envs = if let Some(e) = &profile.env_file {
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

            let mut custom_types = serde_json::Map::new();
            // include normalized builtin type schemas so callers can reference them
            custom_types.insert(
                "utxo_ref".to_string(),
                Tx3Type::Primitive(ir::Type::UtxoRef).json_schema(),
            );
            custom_types.insert(
                "any_asset".to_string(),
                Tx3Type::Primitive(ir::Type::AnyAsset).json_schema(),
            );
            custom_types.insert(
                "utxo".to_string(),
                Tx3Type::Primitive(ir::Type::Utxo).json_schema(),
            );
            custom_types.insert(
                "bytes".to_string(),
                Tx3Type::Primitive(ir::Type::Bytes).json_schema(),
            );
            let mut args = serde_json::Map::new();
            for (key, kind) in prototx.find_params().iter() {
                let tx3_type = match kind {
                    tx3_lang::ir::Type::Custom(name) => {
                        let type_def = protocol
                            .ast()
                            .types
                            .iter()
                            .find(|x| x.name.value == *name)
                            .unwrap();
                        Tx3Type::Custom(CustomTx3Type {
                            r#type: kind.clone(),
                            ctx: type_def.clone(),
                        })
                    }
                    _ => Tx3Type::Primitive(kind.clone()),
                };

                if let Some(envs) = envs.as_ref() {
                    if let Some((_, value)) = envs.iter().find(|(k, _)| k.eq_ignore_ascii_case(key))
                    {
                        args.insert(key.clone(), tx3_type.env_to_value(value));
                        continue;
                    }
                }

                if let ir::Type::Custom(_) = kind {
                    custom_types.insert(tx3_type.to_string(), tx3_type.json_schema());
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
                "args": args_value,
                "types": custom_types,
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

struct CustomTx3Type {
    pub r#type: ir::Type,
    pub ctx: tx3_lang::ast::TypeDef,
}

enum Tx3Type {
    Primitive(ir::Type),
    Custom(CustomTx3Type),
}

impl Tx3Type {
    pub fn ir_type(&self) -> &ir::Type {
        match self {
            Tx3Type::Primitive(t) => t,
            Tx3Type::Custom(custom) => &custom.r#type,
        }
    }
    pub fn env_to_value(&self, value: &str) -> serde_json::Value {
        let ir_type = self.ir_type();

        match ir_type {
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

    pub fn json_schema(&self) -> serde_json::Value {
        fn ast_type_to_schema(ty: &tx3_lang::ast::Type) -> serde_json::Value {
            match ty {
                tx3_lang::ast::Type::Undefined => serde_json::json!({"type": "null"}),
                tx3_lang::ast::Type::Unit => serde_json::json!({}),
                tx3_lang::ast::Type::Int => serde_json::json!({"type": "integer"}),
                tx3_lang::ast::Type::Bool => serde_json::json!({"type": "boolean"}),
                tx3_lang::ast::Type::Bytes => {
                    serde_json::json!({"type": "string", "pattern": "^0x[0-9a-fA-F]+$"})
                }
                tx3_lang::ast::Type::Address => serde_json::json!({"type": "string"}),
                tx3_lang::ast::Type::UtxoRef
                | tx3_lang::ast::Type::AnyAsset
                | tx3_lang::ast::Type::Utxo => {
                    serde_json::json!({"type": "object"})
                }
                tx3_lang::ast::Type::List(inner) => {
                    serde_json::json!({"type": "array", "items": ast_type_to_schema(inner)})
                }
                tx3_lang::ast::Type::Map(_, value_ty) => {
                    serde_json::json!({"type": "object", "additionalProperties": ast_type_to_schema(value_ty)})
                }
                tx3_lang::ast::Type::Custom(id) => {
                    serde_json::json!({"$ref": format!("#/definitions/{}", id.value)})
                }
            }
        }

        match self {
            Tx3Type::Primitive(ir_type) => match ir_type {
                ir::Type::Undefined => serde_json::json!({"type": "null"}),
                ir::Type::Unit => serde_json::json!({}),
                ir::Type::Int => serde_json::json!({"type": "integer"}),
                ir::Type::Bool => serde_json::json!({"type": "boolean"}),
                ir::Type::Bytes => {
                    serde_json::json!({"type": "string", "pattern": "^0x[0-9a-fA-F]+$"})
                }
                ir::Type::Address => serde_json::json!({"type": "string"}),
                ir::Type::UtxoRef => {
                    // Represent utxo_ref as a single string with pattern: optional 0x hex (64 chars) # index
                    serde_json::json!({
                        "type": "string",
                        "pattern": "^(0x)?[0-9a-fA-F]{64}#[0-9]+$",
                        "description": "UTxO reference: <txid>#<index> (txid is 32-byte hex)"
                    })
                }
                ir::Type::AnyAsset => {
                    serde_json::json!({
                        "type": "object",
                        "properties": {
                            "amount": {"type": "integer"},
                            "policy": {"type": "string", "pattern": "^0x[0-9a-fA-F]+$"},
                            "asset_name": {"type": "string"}
                        },
                        "required": ["amount", "policy", "asset_name"]
                    })
                }
                // TODO: make sure this is factual
                ir::Type::Utxo => {
                    serde_json::json!({
                        "type": "object",
                        "properties": {
                            "tx_hash": {"type": "string", "pattern": "^(0x)?[0-9a-fA-F]{64}$"},
                            "output_index": {"type": "integer", "minimum": 0}
                        },
                        "required": ["tx_hash", "output_index"]
                    })
                }
                // TODO: add type parameters here if possible
                ir::Type::List => serde_json::json!({"type": "array", "items": {}}),
                ir::Type::Map => serde_json::json!({"type": "object", "additionalProperties": {}}),
                _ => panic!("Custom types are not primitive types"),
            },
            Tx3Type::Custom(custom) => {
                let def = &custom.ctx;
                let variants = def
                    .cases
                    .iter()
                    .enumerate()
                    .map(|(constructor, case)| {
                        let mut props = serde_json::Map::new();
                        let mut req = vec![];
                        for field in case.fields.iter() {
                            props.insert(
                                field.name.value.clone(),
                                ast_type_to_schema(&field.r#type),
                            );
                            req.push(field.name.value.clone());
                        }
                        let mut obj = serde_json::Map::new();
                        obj.insert(
                            "type".to_string(),
                            serde_json::Value::String("object".to_string()),
                        );
                        obj.insert(
                            "constructor".to_string(),
                            serde_json::Value::Number(constructor.into()),
                        );
                        obj.insert("properties".to_string(), serde_json::Value::Object(props));
                        obj.insert(
                            "required".to_string(),
                            serde_json::Value::Array(
                                req.into_iter().map(serde_json::Value::String).collect(),
                            ),
                        );
                        serde_json::Value::Object(obj)
                    })
                    .collect::<Vec<_>>();

                serde_json::json!({"oneOf": variants})
            }
        }
    }
}

impl Display for Tx3Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ir_type = self.ir_type();
        match ir_type {
            ir::Type::Undefined => write!(f, "undefined"),
            ir::Type::Unit => write!(f, "unit"),
            ir::Type::Int => write!(f, "int"),
            ir::Type::Bool => write!(f, "bool"),
            ir::Type::Bytes => write!(f, "bytes"),
            ir::Type::Address => write!(f, "address"),
            ir::Type::UtxoRef => write!(f, "utxo_ref"),
            ir::Type::AnyAsset => write!(f, "any_asset"),
            ir::Type::Utxo => write!(f, "utxo"),
            ir::Type::List => write!(f, "list"),
            ir::Type::Map => write!(f, "map"),
            ir::Type::Custom(name) => write!(f, "custom({})", name),
        }
    }
}
