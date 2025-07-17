use std::io::Read;
use std::{collections::HashMap, path::PathBuf};

use crate::config::{BindingOptions, Config, KnownChain, TrpConfig};
use clap::Args as ClapArgs;
use miette::IntoDiagnostic;
use serde::{Deserialize, Serialize, Serializer};
use tx3_lang::Protocol;

use convert_case::{Case, Casing};
use handlebars::{Context, Handlebars, Helper, Output, RenderContext, RenderErrorReason};
use reqwest::Client;
use tempfile::TempDir;
use zip::ZipArchive;

#[derive(ClapArgs)]
pub struct Args {}

/// Configuration structure for bindgen templates
#[derive(Debug, Deserialize)]
struct BindgenConfig {
    protocol_files: Option<Vec<String>>,
}

/// Structure returned by load_github_templates containing handlebars and optional config
struct TemplateBundle {
    handlebars: Handlebars<'static>,
    config: Option<BindgenConfig>,
}

fn make_helper<F>(name: &'static str, f: F) -> impl handlebars::HelperDef + Send + Sync + 'static
where
    F: Fn(&str) -> String + Send + Sync + 'static,
{
    move |h: &Helper, _: &Handlebars, _: &Context, _: &mut RenderContext, out: &mut dyn Output| {
        let param = h
            .param(0)
            .ok_or_else(|| RenderErrorReason::ParamNotFoundForIndex(name, 0))?;
        let input = param
            .value()
            .as_str()
            .ok_or_else(|| RenderErrorReason::InvalidParamType("Expected a string"))?;
        out.write(&f(input))?;
        Ok(())
    }
}

// Register any custom helpers here
/// An array of helper functions for converting strings to various case styles.
///
/// Each tuple in the array consists of:
/// - A string slice representing the name of the case style (e.g., "pascalCase").
/// - A function pointer that takes a string slice and returns a `String` converted to the corresponding case style.
///
/// These helpers are useful for dynamically applying different case transformations to strings,
/// such as converting identifiers to PascalCase, camelCase, CONSTANT_CASE, snake_case, or lower case.
fn register_handlebars_helpers(handlebars: &mut Handlebars<'_>) {
    #[allow(clippy::type_complexity)]
    let helpers: &[(&str, fn(&str) -> String)] = &[
        ("pascalCase", |s| s.to_case(Case::Pascal)),
        ("camelCase", |s| s.to_case(Case::Camel)),
        ("constantCase", |s| s.to_case(Case::Constant)),
        ("snakeCase", |s| s.to_case(Case::Snake)),
        ("lowerCase", |s| s.to_case(Case::Lower)),
    ];

    for (name, func) in helpers {
        handlebars.register_helper(name, Box::new(make_helper(name, func)));
    }
    // Add more helpers as needed
}

/// Loads Handlebars templates from a GitHub repository ZIP archive.
///
/// This function:
/// 1. Parses the GitHub URL in the format 'owner/repo' or 'owner/repo/branch'
/// 2. Downloads the repository as a ZIP file from GitHub
/// 3. Extracts the ZIP to a temporary directory
/// 4. Finds all `.hbs` files inside any `bindgen` directory in the archive
/// 5. Optionally loads a `trix-bindgen.toml` file from the `bindgen` directory
/// 6. Registers each found template with Handlebars, using its path relative to `bindgen/` (without the `.hbs` extension)
///
/// Returns a TemplateBundle containing the Handlebars registry and optional configuration.
async fn load_github_templates(github_url: &str) -> miette::Result<TemplateBundle> {
    // Parse GitHub URL
    let parts: Vec<&str> = github_url.split('/').collect();
    if parts.len() < 2 {
        return Err(miette::miette!(
            "Invalid GitHub URL format. Use 'owner/repo' or 'owner/repo/branch'"
        ));
    }

    let owner = parts[0];
    let repo = parts[1];
    let branch = if parts.len() > 2 { parts[2] } else { "main" };

    // Create a zip download URL
    let zip_url = format!(
        "https://github.com/{}/{}/archive/{}.zip",
        owner, repo, branch
    );

    println!("Reading template from https://github.com/{}", github_url);

    // Download the zip file
    let client = Client::new();
    let response = client.get(&zip_url).send().await.into_diagnostic()?;

    if !response.status().is_success() {
        return Err(miette::miette!(
            "Failed to download GitHub repository: HTTP {}",
            response.status()
        ));
    }

    // Create a temporary directory to extract files
    let temp_dir = TempDir::new().into_diagnostic()?;
    let zip_path = temp_dir.path().join("bindgen-template.zip");

    // Save the zip file
    let content = response.bytes().await.into_diagnostic()?;
    std::fs::write(&zip_path, &content).into_diagnostic()?;

    // Extract the zip file
    let file = std::fs::File::open(&zip_path).into_diagnostic()?;
    let mut archive = ZipArchive::new(file).into_diagnostic()?;

    // Register handlebars templates
    let mut handlebars = Handlebars::new();
    let mut config: Option<BindgenConfig> = None;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).into_diagnostic()?;
        let name = file.name().to_owned();

        // Check for trix-bindgen.toml in bindgen directory
        if name.contains("bindgen") && name.ends_with("trix-bindgen.toml") {
            let mut config_content = String::new();
            file.read_to_string(&mut config_content).into_diagnostic()?;

            config = toml::from_str::<BindgenConfig>(&config_content)
                .into_diagnostic()
                .ok();
            continue;
        }

        if name.contains("bindgen") && name.ends_with(".hbs") {
            // Remove everything before "bindgen/" and strip ".hbs" extension
            let template_name = name
                .split_once("bindgen/")
                .map(|x| x.1)
                .unwrap_or(&name)
                .strip_suffix(".hbs")
                .unwrap_or_else(|| name.split('/').next_back().unwrap_or(&name));

            let mut template_content = String::new();
            file.read_to_string(&mut template_content)
                .into_diagnostic()?;

            // Register handlebars template
            handlebars
                .register_template_string(template_name, template_content)
                .into_diagnostic()?;

            // println!("Registered template: {}", template_name);
        }
    }

    register_handlebars_helpers(&mut handlebars);

    Ok(TemplateBundle { handlebars, config })
}

struct BytesHex(Vec<u8>);

impl Serialize for BytesHex {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(&self.0))
    }
}

#[derive(Serialize)]
struct TxParameter {
    name: String,
    type_name: String,
}

#[derive(Serialize)]
struct Transaction {
    name: String,
    params_name: String,
    function_name: String,
    constant_name: String,
    ir_bytes: BytesHex,
    ir_version: String,
    parameters: Vec<TxParameter>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HandlebarsData {
    protocol_name: String,
    protocol_version: String,
    trp_endpoint: String,
    transactions: Vec<Transaction>,
    headers: HashMap<String, String>,
    env_vars: HashMap<String, String>,
}

struct Job {
    name: String,
    protocol: Protocol,
    dest_path: PathBuf,
    trp_endpoint: String,
    trp_headers: HashMap<String, String>,
    env_args: HashMap<String, String>,
}

fn generate_arguments(
    job: &Job,
    get_type_for_field: fn(&tx3_lang::ir::Type) -> String,
    version: &str,
) -> miette::Result<HandlebarsData> {
    let transactions = job
        .protocol
        .txs()
        .map(|tx_def| {
            let tx_name = tx_def.name.value.as_str();
            let proto_tx = job.protocol.new_tx(tx_name).unwrap();

            let parameters: Vec<TxParameter> = proto_tx
                .find_params()
                .iter()
                .map(|(key, type_)| TxParameter {
                    name: key.as_str().to_case(Case::Camel),
                    type_name: get_type_for_field(type_),
                })
                .collect();

            Transaction {
                name: tx_name.to_string(),
                params_name: format!("{}Params", tx_name).to_case(Case::Camel),
                function_name: format!("{}Tx", tx_name).to_case(Case::Camel),
                constant_name: format!("{}Ir", tx_name).to_case(Case::Camel),
                ir_bytes: BytesHex(proto_tx.ir_bytes()),
                ir_version: tx3_lang::ir::IR_VERSION.to_string(),
                parameters,
            }
        })
        .collect();

    let headers = job
        .trp_headers
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<HashMap<_, _>>();

    let env_vars = job
        .env_args
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<HashMap<_, _>>();

    Ok(HandlebarsData {
        protocol_name: job.name.clone(),
        protocol_version: version.to_string(),
        trp_endpoint: job.trp_endpoint.clone(),
        transactions,
        headers,
        env_vars,
    })
}

async fn execute_bindgen(
    job: &Job,
    github_url: &str,
    get_type_for_field: fn(&tx3_lang::ir::Type) -> String,
    version: &str,
    binding_options: &Option<BindingOptions>,
) -> miette::Result<()> {
    let template_bundle = load_github_templates(github_url).await?;

    // Create the destination directory if it doesn't exist
    std::fs::create_dir_all(&job.dest_path).into_diagnostic()?;

    let handlebars_params = generate_arguments(job, get_type_for_field, version)?;

    let standalone = binding_options.as_ref().and_then(|opts| opts.standalone).unwrap_or(false);

    let all_files = template_bundle
            .handlebars
            .get_templates()
            .keys()
            .cloned()
            .collect();

    let templates_to_process = if standalone {
        all_files
    } else {
        // If not standalone, use the config's protocol_files if available
        template_bundle
            .config
            .as_ref()
            .and_then(|c| c.protocol_files.clone())
            .unwrap_or_else(|| {
                all_files
            })
    };

    // Process only the selected templates
    for template_file in templates_to_process {
        let template_name = template_file.strip_suffix(".hbs").unwrap_or(&template_file);

        if template_bundle
            .handlebars
            .get_template(template_name)
            .is_some()
        {
            let template_content = template_bundle
                .handlebars
                .render(template_name, &handlebars_params)
                .unwrap();
            let output_path = job.dest_path.join(&template_name);
            std::fs::write(&output_path, template_content).unwrap();
            // println!("Generated file: {}", output_path.display());
        }
    }

    Ok(())
}

pub async fn run(_args: Args, config: &Config) -> miette::Result<()> {
    for bindgen in config.bindings.iter() {
        let protocol = Protocol::from_file(config.protocol.main.clone()).load()?;

        std::fs::create_dir_all(&bindgen.output_dir).into_diagnostic()?;

        let profile = config
            .profiles
            .clone()
            .unwrap_or_default()
            .devnet
            .trp
            .unwrap_or_else(|| TrpConfig::from(KnownChain::CardanoDevnet));

        let job = Job {
            name: config.protocol.name.clone(),
            protocol,
            dest_path: bindgen.output_dir.clone(),
            trp_endpoint: profile.url.clone(),
            trp_headers: profile.headers.clone(),
            env_args: HashMap::new(),
        };

        match bindgen.plugin.as_str() {
            "rust" => {
                execute_bindgen(
                    &job,
                    "tx3-lang/rust-sdk",
                    |_| "ArgValue".to_string(),
                    &config.protocol.version,
                    &bindgen.options,
                )
                .await?;
                println!("Rust bindgen successful");
            }
            "typescript" => {
                execute_bindgen(
                    &job,
                    "tx3-lang/web-sdk",
                    |ty| match ty {
                        tx3_lang::ir::Type::Int => "number".to_string(),
                        tx3_lang::ir::Type::Address => "string".to_string(),
                        tx3_lang::ir::Type::Bool => "boolean".to_string(),
                        tx3_lang::ir::Type::Bytes => "Uint8Array".to_string(),
                        tx3_lang::ir::Type::UtxoRef => "string".to_string(),
                        tx3_lang::ir::Type::List => "any[]".to_string(),
                        tx3_lang::ir::Type::Undefined => "any".to_string(),
                        tx3_lang::ir::Type::Unit => "void".to_string(),
                        tx3_lang::ir::Type::AnyAsset => "any".to_string(),
                        tx3_lang::ir::Type::Utxo => "any".to_string(),
                        tx3_lang::ir::Type::Custom(name) => name.clone(),
                    },
                    &config.protocol.version,
                    &bindgen.options,
                )
                .await?;
                println!("Typescript bindgen successful");
            }
            "python" => {
                execute_bindgen(
                    &job,
                    "tx3-lang/python-sdk",
                    |ty| match ty {
                        tx3_lang::ir::Type::Int => "int".to_string(),
                        tx3_lang::ir::Type::Bool => "bool".to_string(),
                        tx3_lang::ir::Type::Bytes => "bytes".to_string(),
                        tx3_lang::ir::Type::Unit => "None".to_string(),
                        tx3_lang::ir::Type::List => "list[Any]".to_string(),
                        tx3_lang::ir::Type::Address => "str".to_string(),
                        tx3_lang::ir::Type::UtxoRef => "str".to_string(),
                        tx3_lang::ir::Type::Custom(name) => name.clone(),
                        tx3_lang::ir::Type::AnyAsset => "str".to_string(),
                        tx3_lang::ir::Type::Undefined => "Any".to_string(),
                        tx3_lang::ir::Type::Utxo => "Any".to_string(),
                    },
                    &config.protocol.version,
                    &bindgen.options,
                )
                .await?;
                println!("Python bindgen successful");
            }
            "go" => {
                execute_bindgen(
                    &job,
                    "tx3-lang/go-sdk",
                    |ty| match ty {
                        tx3_lang::ir::Type::Int => "int64".to_string(),
                        tx3_lang::ir::Type::Bool => "bool".to_string(),
                        tx3_lang::ir::Type::Bytes => "Bytes".to_string(),
                        tx3_lang::ir::Type::Unit => "struct{}".to_string(),
                        tx3_lang::ir::Type::Address => "string".to_string(),
                        tx3_lang::ir::Type::UtxoRef => "string".to_string(),
                        tx3_lang::ir::Type::List => "[]interface{}".to_string(),
                        tx3_lang::ir::Type::Custom(name) => name.clone(),
                        tx3_lang::ir::Type::AnyAsset => "string".to_string(),
                        tx3_lang::ir::Type::Utxo => "interface{}".to_string(),
                        tx3_lang::ir::Type::Undefined => "interface{}".to_string(),
                    },
                    &config.protocol.version,
                    &bindgen.options,
                )
                .await?;
                println!("Go bindgen successful");
            }
            plugin => {
                execute_bindgen(
                    &job,
                    plugin,
                    |_| "ArgValue".to_string(), // Default type for unknown plugins
                    &config.protocol.version,
                    &bindgen.options,
                )
                .await?;
                println!("{} bindgen successful", &plugin);
            }
        };
    }

    Ok(())
}
