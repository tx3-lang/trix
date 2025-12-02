use std::io::Read;
use std::{collections::HashMap, path::PathBuf};

use crate::config::{BindingsTemplateConfig, Config, ProfileConfig};
use clap::Args as ClapArgs;
use miette::IntoDiagnostic;
use serde::{Serialize, Serializer};
use tx3_lang::Protocol;

use convert_case::{Case, Casing};
use handlebars::{Context, Handlebars, Helper, Output, RenderContext, RenderErrorReason};
use reqwest::Client;
use tempfile::TempDir;
use zip::ZipArchive;

#[derive(ClapArgs)]
pub struct Args {}

/// Structure returned by load_github_templates containing handlebars and optional config
struct TemplateBundle {
    handlebars: Handlebars<'static>,
    static_files: Vec<(String, String)>,
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

fn parse_type_from_string(type_str: &str) -> Result<tx3_lang::ir::Type, String> {
    match type_str {
        "Int" => Ok(tx3_lang::ir::Type::Int),
        "Bool" => Ok(tx3_lang::ir::Type::Bool),
        "Bytes" => Ok(tx3_lang::ir::Type::Bytes),
        "Unit" => Ok(tx3_lang::ir::Type::Unit),
        "Address" => Ok(tx3_lang::ir::Type::Address),
        "UtxoRef" => Ok(tx3_lang::ir::Type::UtxoRef),
        "AnyAsset" => Ok(tx3_lang::ir::Type::AnyAsset),
        "Utxo" => Ok(tx3_lang::ir::Type::Utxo),
        "Undefined" => Ok(tx3_lang::ir::Type::Undefined),
        "List" => Ok(tx3_lang::ir::Type::List),
        _ => Ok(tx3_lang::ir::Type::Custom(type_str.to_string())),
    }
}

fn get_argvalue_type_for_language(type_: &tx3_lang::ir::Type, language: &str) -> String {
    match language {
        "typescript" => match &type_ {
            tx3_lang::ir::Type::Int => "ArgValueInt | number | bigint".to_string(),
            tx3_lang::ir::Type::Bool => "ArgValueBool | bool".to_string(),
            tx3_lang::ir::Type::Bytes => "ArgValueBytes | Uint8Array".to_string(),
            tx3_lang::ir::Type::Unit => "string".to_string(),
            tx3_lang::ir::Type::Address => "ArgValueAddress | Uint8Array | string".to_string(),
            tx3_lang::ir::Type::UtxoRef => "ArgValueUtxoRef".to_string(),
            tx3_lang::ir::Type::List => "any[]".to_string(),
            tx3_lang::ir::Type::Custom(name) => name.clone(),
            tx3_lang::ir::Type::AnyAsset => "any".to_string(),
            tx3_lang::ir::Type::Utxo => "any".to_string(),
            tx3_lang::ir::Type::Undefined => "any".to_string(),
            tx3_lang::ir::Type::Map => "any".to_string(),
        },
        _ => "ArgValue".to_string(),
    }
}

fn get_type_for_language(type_: &tx3_lang::ir::Type, language: &str) -> String {
    match language {
        "rust" => "ArgValue".to_string(),
        "typescript" => match &type_ {
            tx3_lang::ir::Type::Int => "bigint | number".to_string(),
            tx3_lang::ir::Type::Bool => "boolean".to_string(),
            tx3_lang::ir::Type::Bytes => "Uint8Array".to_string(),
            tx3_lang::ir::Type::Unit => "void".to_string(),
            tx3_lang::ir::Type::Address => "string".to_string(),
            tx3_lang::ir::Type::UtxoRef => "string".to_string(),
            tx3_lang::ir::Type::List => "any[]".to_string(),
            tx3_lang::ir::Type::Custom(name) => name.clone(),
            tx3_lang::ir::Type::AnyAsset => "string".to_string(),
            tx3_lang::ir::Type::Utxo => "any".to_string(),
            tx3_lang::ir::Type::Undefined => "any".to_string(),
            tx3_lang::ir::Type::Map => "any".to_string(),
        },
        "python" => match &type_ {
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
            tx3_lang::ir::Type::Map => "Any".to_string(),
        },
        "go" => match &type_ {
            tx3_lang::ir::Type::Int => "int64".to_string(),
            tx3_lang::ir::Type::Bool => "bool".to_string(),
            tx3_lang::ir::Type::Bytes => "[]byte".to_string(),
            tx3_lang::ir::Type::Unit => "struct{}".to_string(),
            tx3_lang::ir::Type::Address => "string".to_string(),
            tx3_lang::ir::Type::UtxoRef => "string".to_string(),
            tx3_lang::ir::Type::List => "[]interface{}".to_string(),
            tx3_lang::ir::Type::Custom(name) => name.clone(),
            tx3_lang::ir::Type::AnyAsset => "string".to_string(),
            tx3_lang::ir::Type::Utxo => "interface{}".to_string(),
            tx3_lang::ir::Type::Undefined => "interface{}".to_string(),
            tx3_lang::ir::Type::Map => "interface{}".to_string(),
        },
        _ => "ArgValue".to_string(), // Default fallback
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

    // helper to convert ir types to language types
    handlebars.register_helper(
        "typeFor",
        Box::new(
            |h: &Helper,
             _: &Handlebars,
             _: &Context,
             _: &mut RenderContext,
             out: &mut dyn Output| {
                let type_param = h
                    .param(0)
                    .ok_or_else(|| RenderErrorReason::ParamNotFoundForIndex("typeFor", 0))?;
                let lang_param = h
                    .param(1)
                    .ok_or_else(|| RenderErrorReason::ParamNotFoundForIndex("typeFor", 1))?;

                let type_ = if let Some(type_str) = type_param.value().as_str() {
                    parse_type_from_string(type_str)
                        .map_err(|_| RenderErrorReason::InvalidParamType("Failed to parse type"))?
                } else if let Some(obj) = type_param.value().as_object() {
                    // Handle {"Custom": "TypeName"} format
                    if let Some(custom_name) = obj.get("Custom").and_then(|v| v.as_str()) {
                        tx3_lang::ir::Type::Custom(custom_name.to_string())
                    } else {
                        return Err(RenderErrorReason::InvalidParamType(
                            "Unknown type object format",
                        )
                        .into());
                    }
                } else {
                    return Err(RenderErrorReason::InvalidParamType(
                        "Expected type as string or object",
                    )
                    .into());
                };

                let language = lang_param.value().as_str().ok_or_else(|| {
                    RenderErrorReason::InvalidParamType("Expected language as string")
                })?;

                let output_type = get_type_for_language(&type_, language);

                out.write(&output_type)?;
                Ok(())
            },
        ),
    );

    // helper to convert types to ArgValue type names (Int -> ArgValueInt)
    handlebars.register_helper(
        "argValueType",
        Box::new(
            |h: &Helper,
             _: &Handlebars,
             _: &Context,
             _: &mut RenderContext,
             out: &mut dyn Output| {
                let type_param = h
                    .param(0)
                    .ok_or_else(|| RenderErrorReason::ParamNotFoundForIndex("argValueType", 0))?;
                let lang_param = h
                    .param(1)
                    .ok_or_else(|| RenderErrorReason::ParamNotFoundForIndex("argValueType", 1))?;

                let type_ = if let Some(type_str) = type_param.value().as_str() {
                    parse_type_from_string(type_str)
                        .map_err(|_| RenderErrorReason::InvalidParamType("Failed to parse type"))?
                } else if let Some(obj) = type_param.value().as_object() {
                    if let Some(custom_name) = obj.get("Custom").and_then(|v| v.as_str()) {
                        tx3_lang::ir::Type::Custom(custom_name.to_string())
                    } else {
                        return Err(RenderErrorReason::InvalidParamType(
                            "Unknown type object format",
                        )
                        .into());
                    }
                } else {
                    return Err(RenderErrorReason::InvalidParamType(
                        "Expected type as string or object",
                    )
                    .into());
                };

                let language = lang_param.value().as_str().ok_or_else(|| {
                    RenderErrorReason::InvalidParamType("Expected language as string")
                })?;

                let output_type = get_argvalue_type_for_language(&type_, language);

                out.write(&output_type)?;
                Ok(())
            },
        ),
    );

    // helper to check if a type is a custom type
    handlebars.register_helper(
        "isCustomType",
        Box::new(
            |h: &Helper,
             _: &Handlebars,
             _: &Context,
             _: &mut RenderContext,
             out: &mut dyn Output| {
                let type_param = h
                    .param(0)
                    .ok_or_else(|| RenderErrorReason::ParamNotFoundForIndex("isCustomType", 0))?;

                let is_custom = if type_param.value().as_str().is_some() {
                    false // Simple string types are not custom
                } else if let Some(obj) = type_param.value().as_object() {
                    obj.contains_key("Custom")
                } else {
                    false
                };

                out.write(if is_custom { "true" } else { "false" })?;
                Ok(())
            },
        ),
    );
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
async fn load_github_templates(
    github_url: &str,
    temp_dir: &TempDir,
    path: &str,
) -> miette::Result<TemplateBundle> {
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

    println!(
        "Reading template from https://github.com/{}/{} (ref: {})",
        owner, repo, branch
    );

    // Download the zip file
    let client = Client::new();
    let response = client.get(&zip_url).send().await.into_diagnostic()?;

    if !response.status().is_success() {
        return Err(miette::miette!(
            "Failed to download GitHub repository: HTTP {}",
            response.status()
        ));
    }

    let zip_path = temp_dir.path().join("bindgen-template.zip");

    // Save the zip file
    let content = response.bytes().await.into_diagnostic()?;
    std::fs::write(&zip_path, &content).into_diagnostic()?;

    // Extract the zip file
    let file = std::fs::File::open(&zip_path).into_diagnostic()?;
    let mut archive = ZipArchive::new(file).into_diagnostic()?;

    let mut bindgen_path = PathBuf::new();

    // Get root_dir
    let root_dir_name = archive.name_for_index(0).unwrap_or("");

    bindgen_path.push(root_dir_name);
    bindgen_path.push(path);
    // Ensure the bindgen path ends with a separator
    bindgen_path.push("");

    // let mut config: Option<BindgenConfig> = None;
    // Check for trix-bindgen.toml in the directory
    // let toml_name = bindgen_path.join("trix-bindgen.toml").to_string_lossy().to_string();

    // if let Ok(mut config_file) = archive.by_name(&toml_name) {
    //     let mut config_content = String::new();
    //     config_file.read_to_string(&mut config_content).into_diagnostic()?;

    //     config = toml::from_str::<BindgenConfig>(&config_content)
    //         .into_diagnostic()
    //         .ok();
    // }

    // Register handlebars templates
    let mut handlebars = Handlebars::new();
    let mut static_files = Vec::new();

    let bindgen_path_string = bindgen_path.to_string_lossy().to_string();
    let archive_bindgen_index = archive.index_for_name(&bindgen_path_string).unwrap_or(0);

    // Skip files that are not in the bindgen_path or are the bindgen_path itself
    for i in archive_bindgen_index..archive.len() {
        let mut file = archive.by_index(i).into_diagnostic()?;
        let name = file.name().to_owned();

        if !name.starts_with(&bindgen_path_string) {
            break; // Stop processing if we reach a file outside the bindgen path
        }

        // If the file is a directory or its the trix-bindgen.toml, skip it
        if file.is_dir() || name.ends_with("trix-bindgen.toml") {
            continue;
        }

        // Remove everything before "bindgen/" and strip ".hbs" extension
        let template_name = name.strip_prefix(&bindgen_path_string).unwrap_or(&name);

        if name.ends_with(".hbs") {
            let template_name = template_name.strip_suffix(".hbs").unwrap_or(&name);

            let mut template_content = String::new();
            file.read_to_string(&mut template_content)
                .into_diagnostic()?;

            // Register handlebars template
            handlebars
                .register_template_string(template_name, template_content)
                .into_diagnostic()?;

            // println!("Registered template: {}", template_name);
            continue;
        }

        if file.is_file() {
            let dest_path = temp_dir.path().join(template_name);
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent).into_diagnostic()?;
            }
            let mut out_file = std::fs::File::create(&dest_path).into_diagnostic()?;
            std::io::copy(&mut file, &mut out_file).into_diagnostic()?;
            static_files.push((dest_path.display().to_string(), template_name.to_string()));
        }
    }

    register_handlebars_helpers(&mut handlebars);

    Ok(TemplateBundle {
        handlebars,
        static_files,
    })
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
#[serde(rename_all = "camelCase")]
struct TxParameter {
    name: String,
    type_name: tx3_lang::ir::Type,
    is_custom: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TypeField {
    name: String,
    type_name: String,
    is_custom: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TypeVariant {
    name: String,
    index: usize,
    fields: Vec<TypeField>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CustomTypeDef {
    name: String,
    variants: Vec<TypeVariant>,
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
    options: HashMap<String, serde_json::Value>,
    custom_types: Vec<CustomTypeDef>,
}

struct Job {
    name: String,
    protocol: Protocol,
    dest_path: PathBuf,
    trp_endpoint: String,
    trp_headers: HashMap<String, String>,
    env_args: HashMap<String, String>,
    options: HashMap<String, serde_json::Value>,
}

fn ast_type_to_string(ty: &tx3_lang::ast::Type) -> String {
    match ty {
        tx3_lang::ast::Type::Undefined => "Undefined".to_string(),
        tx3_lang::ast::Type::Unit => "Unit".to_string(),
        tx3_lang::ast::Type::Int => "Int".to_string(),
        tx3_lang::ast::Type::Bool => "Bool".to_string(),
        tx3_lang::ast::Type::Bytes => "Bytes".to_string(),
        tx3_lang::ast::Type::Address => "Address".to_string(),
        tx3_lang::ast::Type::UtxoRef => "UtxoRef".to_string(),
        tx3_lang::ast::Type::AnyAsset => "AnyAsset".to_string(),
        tx3_lang::ast::Type::Utxo => "Utxo".to_string(),
        tx3_lang::ast::Type::List(inner) => format!("List<{}>", ast_type_to_string(inner)),
        tx3_lang::ast::Type::Map(k, v) => {
            format!("Map<{}, {}>", ast_type_to_string(k), ast_type_to_string(v))
        }
        tx3_lang::ast::Type::Custom(id) => id.value.clone(),
    }
}

fn is_custom_type(ty: &tx3_lang::ir::Type) -> bool {
    matches!(ty, tx3_lang::ir::Type::Custom(_))
}

fn generate_arguments(job: &Job, version: &str) -> miette::Result<HandlebarsData> {
    let custom_types: Vec<CustomTypeDef> = job
        .protocol
        .ast()
        .types
        .iter()
        .map(|type_def| {
            let variants: Vec<TypeVariant> = type_def
                .cases
                .iter()
                .enumerate()
                .map(|(index, variant)| {
                    let fields: Vec<TypeField> = variant
                        .fields
                        .iter()
                        .map(|field| {
                            let type_name = ast_type_to_string(&field.r#type);
                            let is_custom = matches!(&field.r#type, tx3_lang::ast::Type::Custom(_));
                            TypeField {
                                name: field.name.value.clone(),
                                type_name,
                                is_custom,
                            }
                        })
                        .collect();
                    TypeVariant {
                        name: variant.name.value.clone(),
                        index,
                        fields,
                    }
                })
                .collect();
            CustomTypeDef {
                name: type_def.name.value.clone(),
                variants,
            }
        })
        .collect();

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
                    is_custom: is_custom_type(type_),
                    type_name: type_.clone(),
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
        options: job.options.clone(),
        custom_types,
    })
}

async fn execute_bindgen(
    job: &Job,
    template_config: &BindingsTemplateConfig,
    version: &str,
) -> miette::Result<()> {
    // Create a temporary directory to extract files
    let temp_dir = TempDir::new().into_diagnostic()?;
    let github_url = format!(
        "{}/{}",
        &template_config.repo,
        template_config.r#ref.as_deref().unwrap_or("main")
    );

    let template_bundle =
        load_github_templates(&github_url, &temp_dir, &template_config.path).await?;

    // Create the destination directory if it doesn't exist
    std::fs::create_dir_all(&job.dest_path).into_diagnostic()?;

    let handlebars_params = generate_arguments(job, version)?;

    let handlebars_template_iter = template_bundle.handlebars.get_templates().iter();

    for (name, _) in handlebars_template_iter {
        let template_content = template_bundle
            .handlebars
            .render(name, &handlebars_params)
            .unwrap();
        if template_content.is_empty() {
            // Skip empty templates
            continue;
        }
        let output_path = job.dest_path.join(name);
        if let Some(parent) = output_path.parent() {
            // Create parent directories if they don't exist
            std::fs::create_dir_all(parent).into_diagnostic()?;
        }
        std::fs::write(&output_path, template_content).into_diagnostic()?;
        // println!("Generated file: {}", output_path.display());
    }

    // Copy static files to the destination directory
    for (src_path, file_destination) in &template_bundle.static_files {
        let dest_path = job.dest_path.join(file_destination);
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent).into_diagnostic()?;
        }
        std::fs::copy(src_path, dest_path).into_diagnostic()?;
        // println!("Copied static file: {}", dest_path.display());
    }

    Ok(())
}

pub async fn run(_args: Args, config: &Config, profile: &ProfileConfig) -> miette::Result<()> {
    for bindgen in config.bindings.iter() {
        let protocol = Protocol::from_file(config.protocol.main.clone()).load()?;

        std::fs::create_dir_all(&bindgen.output_dir).into_diagnostic()?;

        let trp_config = profile
            .trp
            .as_ref()
            .ok_or_else(|| miette::miette!("TRP config not found"))?;

        let job = Job {
            name: config.protocol.name.clone(),
            protocol,
            dest_path: bindgen.output_dir.clone(),
            trp_endpoint: trp_config.url.clone(),
            trp_headers: trp_config.headers.clone(),
            env_args: HashMap::new(),
            options: bindgen.options.clone().unwrap_or_default(),
        };

        execute_bindgen(&job, &bindgen.template, &config.protocol.version).await?;
        println!("Bindgen successful");
    }

    Ok(())
}
