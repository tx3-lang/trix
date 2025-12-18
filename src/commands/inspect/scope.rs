use crate::config::Config;
use clap::Args as ClapArgs;
use std::collections::HashSet;
use tx3_lang::ast::Symbol;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(
        short = 't',
        long = "type",
        value_name = "TYPE",
        num_args = 1..,
        action = clap::ArgAction::Append,
        value_delimiter = ' ',
        help = "Filter by symbol type (e.g. typedef, assetdef, envvar, paramvar, localexpr, output, input,
        partydef, policydef, aliasdef, recordfield, variantcase, fees).
        Accepts multiple values."
    )]
    types: Vec<String>,
}

#[derive(Debug)]
struct SymbolInfo {
    name: String,
    r#type: String,
    cases: Option<String>,
}

fn build_symbol_info(name: &str, symbol: &Symbol) -> SymbolInfo {
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
        Symbol::EnvVar(env, ty) => Some(format!("env: {}, type: {}", env, ty)),
        Symbol::ParamVar(param, ty) => Some(format!("param: {}, type: {}", param, ty)),
        Symbol::AliasDef(alias) => Some(format!("alias: {}", alias.alias_type)),
        Symbol::TypeDef(typedef) => {
            let cases = typedef
                .cases
                .iter()
                .map(|case| {
                    if case.fields.is_empty() {
                        format!("{}", case.name.value)
                    } else {
                        let fields = case
                            .fields
                            .iter()
                            .map(|f| format!("  + {}: {}", f.name.value, f.r#type))
                            .collect::<Vec<_>>()
                            .join("\n");

                        format!("- {}\n{}", case.name.value, fields)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            Some(cases)
        }
        _ => None,
    };

    SymbolInfo {
        name: name.to_string(),
        r#type,
        cases,
    }
}

pub fn run(args: Args, config: &Config) -> miette::Result<()> {
    let loader = tx3_lang::loading::ProtocolLoader::from_file(&config.protocol.main);
    let protocol = loader.load()?;

    let mut symbols = Vec::new();
    let mut current_scope = protocol.ast().scope();

    while let Some(scope) = current_scope {
        for (name, symbol) in scope.symbols().iter() {
            symbols.push(build_symbol_info(name, symbol));
        }
        current_scope = scope.parent();
    }

    symbols.sort_by(|a, b| a.r#type.cmp(&b.r#type).then(a.name.cmp(&b.name)));

    if !args.types.is_empty() {
        let filters: HashSet<String> = args.types.iter().map(|t| t.to_ascii_lowercase()).collect();

        symbols.retain(|s| {
            s.r#type
                .split('(')
                .next()
                .map(|k| filters.contains(&k.to_ascii_lowercase()))
                .unwrap_or(false)
        });
    }

    if symbols.is_empty() {
        if args.types.is_empty() {
            println!("No symbols found in scope.");
        } else {
            println!("No symbols found for types: {}", args.types.join(", "));
        }
        return Ok(());
    }

    let mut last_type: Option<&str> = None;

    for sym in &symbols {
        if last_type != Some(sym.r#type.as_str()) {
            println!("\n\x1b[1m{}\x1b[0m", sym.r#type);
            last_type = Some(&sym.r#type);
        }

        match &sym.cases {
            Some(cases) if cases.contains('\n') => {
                println!("  {}", sym.name);
                for line in cases.lines() {
                    println!("    {}", line);
                }
            }
            Some(cases) => {
                println!("  {} — {}", sym.name, cases);
            }
            None => {
                println!("  {}", sym.name);
            }
        }
    }

    Ok(())
}
