use clap::Args as ClapArgs;
use miette::IntoDiagnostic as _;
use serde::Serialize;

use crate::config::Config;
use tx3_lang::ast::Symbol;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(short, long)]
    pretty: bool,
}

#[derive(Serialize, Debug)]
struct SymbolInfo {
    name: String,
    r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cases: Option<String>,
}

pub fn run(config: &Config) -> miette::Result<()> {
    let main_path = config.protocol.main.clone();

    let content = std::fs::read_to_string(main_path).into_diagnostic()?;

    let mut ast = tx3_lang::parsing::parse_string(&content)?;

    tx3_lang::analyzing::analyze(&mut ast).ok()?;

    // Extract symbols from the global scope
    let symbols = if let Some(scope) = ast.scope() {
        let mut symbol_list = Vec::new();

        for (name, symbol) in scope.symbols() {
            let r#type = match symbol {
                Symbol::EnvVar(_, ty) => format!("EnvVar({})", ty),
                Symbol::ParamVar(_, ty) => format!("ParamVar({})", ty),
                Symbol::LocalExpr(_) => "LocalExpr".to_string(),
                Symbol::Output(idx) => format!("Output({})", idx),
                Symbol::Input(_) => "Input".to_string(),
                Symbol::PartyDef(_) => "PartyDef".to_string(),
                Symbol::PolicyDef(_) => "PolicyDef".to_string(),
                Symbol::AssetDef(_) => "AssetDef".to_string(),
                Symbol::TypeDef(_) => "TypeDef".to_string(),
                Symbol::AliasDef(_) => "AliasDef".to_string(),
                Symbol::RecordField(_) => "RecordField".to_string(),
                Symbol::VariantCase(_) => "VariantCase".to_string(),
                Symbol::Fees => "Fees".to_string(),
            };

            let cases = match symbol {
                Symbol::EnvVar(env_name, ty) => Some(format!("env: {}, type: {}", env_name, ty)),
                Symbol::ParamVar(param_name, ty) => {
                    Some(format!("param: {}, type: {}", param_name, ty))
                }
                Symbol::TypeDef(typedef) => {
                    let cases: Vec<String> = typedef
                        .cases
                        .iter()
                        .map(|c| {
                            if c.fields.is_empty() {
                                c.name.value.clone()
                            } else {
                                let fields: Vec<String> = c
                                    .fields
                                    .iter()
                                    .map(|f| format!("{}: {}", f.name.value, f.r#type))
                                    .collect();
                                format!("{} {{ {} }}", c.name.value, fields.join(", "))
                            }
                        })
                        .collect();
                    Some(format!("{}", cases.join(", ")))
                }
                Symbol::AliasDef(aliasdef) => Some(format!("alias: {}", aliasdef.alias_type)),
                _ => None,
            };

            symbol_list.push(SymbolInfo {
                name: name.clone(),
                r#type,
                cases,
            });
        }

        symbol_list.sort_by(|a, b| a.r#type.cmp(&b.r#type));
        symbol_list
    } else {
        vec![]
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&symbols).into_diagnostic()?
    );

    Ok(())
}
