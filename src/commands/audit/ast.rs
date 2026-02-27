use aiken_lang::{
    ast::{
        self, Annotation, ArgBy, ModuleKind, UntypedArg, UntypedDefinition, UntypedFunction,
        UntypedModule,
    },
    parser,
    version,
};
use chrono::Utc;
use cryptoxide::{digest::Digest as _, sha2::Sha256};
use miette::{Context, IntoDiagnostic, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::model::{
    AstMetadata, AstToolMetadata, SourceSpan, ValidatorContextEntry, ValidatorContextMap,
    ValidatorHandlerContext, ValidatorParameterContext,
};

#[derive(Debug, Clone)]
pub struct AstBuildOutput {
    pub metadata: AstMetadata,
    pub validator_context: ValidatorContextMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AstSnapshot {
    schema_version: u8,
    generated_at: String,
    tool: AstToolMetadata,
    source_fingerprint: String,
    files: Vec<AstFileSnapshot>,
    validator_context: ValidatorContextMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AstFileSnapshot {
    source_file: String,
    ast: String,
}

#[derive(Debug, Clone)]
struct ParsedModule {
    source_file: String,
    module: UntypedModule,
    ast_debug: String,
}

pub fn generate_ast_and_validator_context(
    project_root: &Path,
    source_files: &[PathBuf],
    ast_out_path: &Path,
    no_ast_cache: bool,
) -> Result<AstBuildOutput> {
    let source_fingerprint = fingerprint_sources(project_root, source_files)?;

    if ast_out_path.exists() && !no_ast_cache {
        let cached_text = std::fs::read_to_string(ast_out_path)
            .into_diagnostic()
            .with_context(|| format!("Failed to read cached AST JSON at {}", ast_out_path.display()))?;

        let cached_snapshot: AstSnapshot = serde_json::from_str(&cached_text)
            .into_diagnostic()
            .context("AST output unreadable/invalid JSON")?;

        if cached_snapshot.source_fingerprint == source_fingerprint {
            return Ok(AstBuildOutput {
                metadata: AstMetadata {
                    path: display_path_for_state(project_root, ast_out_path),
                    fingerprint: format!("sha256:{}", sha256_hex(cached_text.as_bytes())),
                    generated_at: cached_snapshot.generated_at,
                    tool: cached_snapshot.tool,
                },
                validator_context: cached_snapshot.validator_context,
            });
        }
    }

    let parsed_modules = source_files
        .iter()
        .map(|source_file| parse_module_snapshot(project_root, source_file))
        .collect::<Result<Vec<_>>>()?;

    let validator_context = build_validator_context_from_modules(&parsed_modules);

    let files = parsed_modules
        .iter()
        .map(|module| AstFileSnapshot {
            source_file: module.source_file.clone(),
            ast: module.ast_debug.clone(),
        })
        .collect::<Vec<_>>();

    let snapshot = AstSnapshot {
        schema_version: 1,
        generated_at: Utc::now().to_rfc3339(),
        tool: AstToolMetadata {
            name: "aiken".to_string(),
            version: version::compiler_version(false),
        },
        source_fingerprint,
        files,
        validator_context: validator_context.clone(),
    };

    let serialized_snapshot = serde_json::to_string_pretty(&snapshot).into_diagnostic()?;
    write_text_file(ast_out_path, &serialized_snapshot)?;

    Ok(AstBuildOutput {
        metadata: AstMetadata {
            path: display_path_for_state(project_root, ast_out_path),
            fingerprint: format!("sha256:{}", sha256_hex(serialized_snapshot.as_bytes())),
            generated_at: snapshot.generated_at,
            tool: snapshot.tool,
        },
        validator_context: snapshot.validator_context,
    })
}

    fn parse_module_snapshot(project_root: &Path, source_file: &Path) -> Result<ParsedModule> {
    let src = std::fs::read_to_string(source_file)
        .into_diagnostic()
        .with_context(|| format!("Failed to read source file {}", source_file.display()))?;

    let (module, _) = parser::module(&src, ModuleKind::Validator).map_err(|errors| {
        let rendered = errors
            .iter()
            .map(|error| format!("{error:?}"))
            .collect::<Vec<String>>()
            .join("\n");

        miette::miette!(
            "Aiken command failed: parser error(s) while generating AST for {}\n{}",
            display_path_for_state(project_root, source_file),
            rendered
        )
    })?;

    Ok(ParsedModule {
        source_file: display_path_for_state(project_root, source_file),
        ast_debug: format!("{:#?}", &module),
        module,
    })
}

fn build_validator_context_from_modules(modules: &[ParsedModule]) -> ValidatorContextMap {
    let mut validators = modules
        .iter()
        .flat_map(|module_snapshot| {
            module_snapshot
                .module
                .definitions
                .iter()
                .filter_map(|definition| {
                    let UntypedDefinition::Validator(validator) = definition else {
                        return None;
                    };

                    let module_id = module_name_from_source_file(&module_snapshot.source_file);
                    let id = format!("{}.{}", module_id, validator.name);

                    let mut handlers = validator
                        .handlers
                        .iter()
                        .map(function_to_handler_context)
                        .collect::<Vec<_>>();
                    handlers.push(function_to_handler_context(&validator.fallback));

                    Some(ValidatorContextEntry {
                        id,
                        module: module_snapshot.source_file.clone(),
                        source_file: module_snapshot.source_file.clone(),
                        source_span: resolve_source_span(&module_snapshot.module, validator.location),
                        handlers,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    validators.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.source_file.cmp(&right.source_file))
    });

    ValidatorContextMap { validators }
}

fn function_to_handler_context(function: &UntypedFunction) -> ValidatorHandlerContext {
    let parameters = function
        .arguments
        .iter()
        .enumerate()
        .map(|(index, argument)| ValidatorParameterContext {
            name: argument_name(argument, index),
            r#type: argument
                .annotation
                .as_ref()
                .map(annotation_to_string)
                .unwrap_or_else(|| "Unknown".to_string()),
        })
        .collect::<Vec<_>>();

    ValidatorHandlerContext {
        name: function.name.clone(),
        parameters,
    }
}

fn argument_name(argument: &UntypedArg, index: usize) -> String {
    match &argument.by {
        ArgBy::ByName(name) => name.get_name(),
        ArgBy::ByPattern(_) => argument.arg_name(index).get_name(),
    }
}

fn annotation_to_string(annotation: &Annotation) -> String {
    match annotation {
        Annotation::Constructor {
            module,
            name,
            arguments,
            ..
        } => {
            let qualified = module
                .as_ref()
                .map(|module_name| format!("{module_name}.{name}"))
                .unwrap_or_else(|| name.clone());

            if arguments.is_empty() {
                qualified
            } else {
                format!(
                    "{}<{}>",
                    qualified,
                    arguments
                        .iter()
                        .map(annotation_to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
        Annotation::Fn { arguments, ret, .. } => format!(
            "fn({}) -> {}",
            arguments
                .iter()
                .map(annotation_to_string)
                .collect::<Vec<_>>()
                .join(", "),
            annotation_to_string(ret)
        ),
        Annotation::Var { name, .. } | Annotation::Hole { name, .. } => name.clone(),
        Annotation::Tuple { elems, .. } => format!(
            "({})",
            elems
                .iter()
                .map(annotation_to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Annotation::Pair { fst, snd, .. } => {
            format!("Pair<{}, {}>", annotation_to_string(fst), annotation_to_string(snd))
        }
    }
}

fn resolve_source_span(module: &UntypedModule, span: ast::Span) -> Option<SourceSpan> {
    let start_line = module.lines.line_number(span.start)?;
    let end_byte = span.end.saturating_sub(1);
    let end_line = module.lines.line_number(end_byte).unwrap_or(start_line);

    Some(SourceSpan {
        start_line,
        end_line,
    })
}

fn module_name_from_source_file(source_file: &str) -> String {
    let without_extension = source_file.strip_suffix(".ak").unwrap_or(source_file);
    without_extension.replace('/', ".")
}

fn fingerprint_sources(project_root: &Path, source_files: &[PathBuf]) -> Result<String> {
    let mut hasher = Sha256::new();

    for source_file in source_files {
        let relative_path = display_path_for_state(project_root, source_file);
        let content = std::fs::read(source_file)
            .into_diagnostic()
            .with_context(|| format!("Failed to read source file {}", source_file.display()))?;

        hasher.input(relative_path.as_bytes());
        hasher.input(b"\0");
        hasher.input(&content);
        hasher.input(b"\0");
    }

    Ok(format!("sha256:{}", hasher.result_str()))
}

fn display_path_for_state(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn sha256_hex(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.input(value);
    hasher.result_str()
}

fn write_text_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .into_diagnostic()
            .with_context(|| format!("Failed to create output directory {}", parent.display()))?;
    }

    std::fs::write(path, content)
        .into_diagnostic()
        .with_context(|| format!("Failed to write file {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn validator_context_extracts_handlers_and_types() {
        let src = r#"
use cardano/transaction.{OutputReference, Transaction}

pub type Datum {
  owner: ByteArray,
}

pub type Redeemer {
  msg: ByteArray,
}

validator hello_world {
  spend(
    datum: Option<Datum>,
    redeemer: Redeemer,
    _own_ref: OutputReference,
    self: Transaction,
  ) {
    True
  }

  else(_) {
    fail
  }
}
"#;

        let (module, _) = parser::module(src, ModuleKind::Validator).expect("parse module");
        let context = build_validator_context_from_modules(&[ParsedModule {
            source_file: "onchain/validators/vesting.ak".to_string(),
            module,
            ast_debug: String::new(),
        }]);

        assert_eq!(context.validators.len(), 1);
        let validator = &context.validators[0];
        assert_eq!(validator.id, "onchain.validators.vesting.hello_world");
        assert_eq!(validator.handlers.len(), 2);
        assert_eq!(validator.handlers[0].name, "spend");
        assert_eq!(validator.handlers[0].parameters[0].name, "datum");
        assert_eq!(validator.handlers[0].parameters[0].r#type, "Option<Datum>");
        assert_eq!(validator.handlers[1].name, "else");
        assert_eq!(validator.handlers[1].parameters[0].r#type, "Unknown");
        assert!(validator.source_span.is_some());
    }

    #[test]
    fn validator_context_is_sorted_deterministically() {
        let src = r#"
validator zeta {
  else(_) { True }
}

validator alpha {
  else(_) { True }
}
"#;

        let (module, _) = parser::module(src, ModuleKind::Validator).expect("parse module");

        let context = build_validator_context_from_modules(&[ParsedModule {
            source_file: "validators/sample.ak".to_string(),
            module,
            ast_debug: String::new(),
        }]);

        let ids = context
            .validators
            .iter()
            .map(|validator| validator.id.clone())
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                "validators.sample.alpha".to_string(),
                "validators.sample.zeta".to_string()
            ]
        );
    }

    #[test]
    fn generate_ast_fails_when_cached_snapshot_is_invalid_json() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        let source = root.join("validators/broken.ak");
        let ast_out = root.join(".tx3/audit/aiken-ast.json");

        fs::create_dir_all(source.parent().expect("parent")).expect("create parent dir");
        fs::write(&source, "validator ok { else(_) { True } }").expect("write source file");
        fs::create_dir_all(ast_out.parent().expect("parent")).expect("create ast dir");
        fs::write(&ast_out, "{ this is invalid json }").expect("write invalid ast cache");

        let err = generate_ast_and_validator_context(root, &[source], &ast_out, false)
            .expect_err("expected invalid cached ast json failure");

        assert!(err.to_string().contains("AST output unreadable/invalid JSON"));
    }

    #[test]
    fn generate_ast_fails_when_aiken_source_cannot_be_parsed() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        let source = root.join("validators/invalid.ak");
        let ast_out = root.join(".tx3/audit/aiken-ast.json");

        fs::create_dir_all(source.parent().expect("parent")).expect("create parent dir");
        fs::write(&source, "validator broken { spend(").expect("write invalid source file");

        let err = generate_ast_and_validator_context(root, &[source], &ast_out, true)
            .expect_err("expected parser failure");

        assert!(err.to_string().contains("Aiken command failed"));
    }
}
