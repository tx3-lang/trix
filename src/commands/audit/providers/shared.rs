use miette::{Context, IntoDiagnostic, Result};
use serde::Deserialize;
use serde_json::Value;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::runtime::Handle;

use crate::commands::audit::model::{
    MiniPrompt, PermissionPromptSpec, SkillIterationResult, VulnerabilityFinding,
    VulnerabilitySkill,
};

pub(super) const MAX_AGENT_STEPS: usize = 25;
const MAX_COMMAND_OUTPUT_CHARS: usize = 30_000;
const AGENT_SYSTEM_PROMPT: &str =
    include_str!("../../../../templates/aiken/audit_agent_system_prompt.md");
const INITIAL_USER_PROMPT_TEMPLATE: &str =
    include_str!("../../../../templates/aiken/audit_agent_initial_user_prompt.md");
const PERMISSION_PROMPT_TEMPLATE: &str =
    include_str!("../../../../templates/aiken/permission_prompt.md");
const TOOL_RESULT_PROMPT_TEMPLATE: &str =
    include_str!("../../../../templates/aiken/audit_agent_tool_result_prompt.md");

#[derive(Debug)]
pub(super) enum AgentAction {
    Final(Value),
    ReadRequest(ReadRequest),
}

#[derive(Debug)]
pub(super) enum ReadRequest {
    ReadFile {
        path: String,
    },
    Grep {
        pattern: String,
        path: String,
        context_lines: usize,
    },
    ListDir {
        path: String,
    },
    FindFiles {
        path: String,
        glob: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct RawReadRequest {
    action: Option<String>,
    path: Option<String>,
    pattern: Option<String>,
    context_lines: Option<usize>,
    glob: Option<String>,
}

pub(super) fn build_agent_system_prompt() -> &'static str {
    AGENT_SYSTEM_PROMPT
}

fn parse_line_number(value: Option<&Value>) -> Option<usize> {
    value.and_then(|entry| {
        if let Some(number) = entry.as_u64() {
            return usize::try_from(number).ok();
        }

        entry
            .as_str()
            .and_then(|text| text.trim().parse::<usize>().ok())
    })
}

pub(super) fn build_initial_user_prompt(
    prompt: &MiniPrompt,
    source_references: &[String],
    permission_prompt: &PermissionPromptSpec,
) -> String {
    INITIAL_USER_PROMPT_TEMPLATE
        .replace("{{SKILL}}", &prompt.text)
        .replace("{{SOURCE_REFERENCES}}", &render_source_references(source_references))
        .replace(
            "{{PERMISSION_PROMPT}}",
            &render_permission_prompt(permission_prompt),
        )
}

pub(super) fn build_tool_result_user_prompt(request: &ReadRequest, output: &str) -> String {
    TOOL_RESULT_PROMPT_TEMPLATE
        .replace("{{REQUEST}}", &format!("{:?}", request))
        .replace("{{OUTPUT}}", output)
}

fn render_permission_prompt(permission_prompt: &PermissionPromptSpec) -> String {
    PERMISSION_PROMPT_TEMPLATE
        .replace("{{ workspace_root }}", &permission_prompt.workspace_root)
        .replace(
            "{{ allowed_commands }}",
            &permission_prompt.allowed_commands.join(", "),
        )
        .replace("{{ scope_rules }}", &permission_prompt.scope_rules.join("\n- "))
}

fn render_source_references(source_references: &[String]) -> String {
    if source_references.is_empty() {
        return "- (none)".to_string();
    }

    source_references
        .iter()
        .map(|path| format!("- {}", path))
        .collect::<Vec<String>>()
        .join("\n")
}

pub(super) fn parse_agent_action(content: &str) -> Result<AgentAction> {
    let parsed = parse_structured_content(content)?;

    let has_final_shape = parsed.get("findings").is_some() || parsed.get("status").is_some();
    let action_value = parsed
        .get("action")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase());

    if action_value.is_none() && has_final_shape {
        return Ok(AgentAction::Final(parsed));
    }

    let raw: RawReadRequest = serde_json::from_value(parsed.clone())
        .into_diagnostic()
        .context("Invalid agent action payload")?;

    match raw.action.unwrap_or_else(|| "final".to_string()).as_str() {
        "final" => Ok(AgentAction::Final(parsed)),
        "read_file" => Ok(AgentAction::ReadRequest(ReadRequest::ReadFile {
            path: raw.path.unwrap_or_else(|| ".".to_string()),
        })),
        "grep" => Ok(AgentAction::ReadRequest(ReadRequest::Grep {
            pattern: raw.pattern.unwrap_or_default(),
            path: raw.path.unwrap_or_else(|| ".".to_string()),
            context_lines: raw.context_lines.unwrap_or(2).min(20),
        })),
        "list_dir" => Ok(AgentAction::ReadRequest(ReadRequest::ListDir {
            path: raw.path.unwrap_or_else(|| ".".to_string()),
        })),
        "find_files" => Ok(AgentAction::ReadRequest(ReadRequest::FindFiles {
            path: raw.path.unwrap_or_else(|| ".".to_string()),
            glob: raw.glob,
        })),
        other => Err(miette::miette!("Unsupported agent action '{}'", other)),
    }
}

pub(super) fn execute_read_request(
    request: &ReadRequest,
    project_root: &Path,
    permission_prompt: &PermissionPromptSpec,
) -> Result<String> {
    match request {
        ReadRequest::ReadFile { path } => {
            ensure_allowed(permission_prompt, "cat")?;
            let scoped_path = resolve_scoped_path(project_root, path)?;
            enforce_read_scope(request, &scoped_path, project_root, permission_prompt)?;
            confirm_request_if_interactive(request, &scoped_path, project_root, permission_prompt)?;
            let args = vec![scoped_path.to_string_lossy().to_string()];
            run_command_capture("cat", &args, project_root)
        }
        ReadRequest::Grep {
            pattern,
            path,
            context_lines,
        } => {
            ensure_allowed(permission_prompt, "grep")?;
            let scoped_path = resolve_scoped_path(project_root, path)?;
            enforce_read_scope(request, &scoped_path, project_root, permission_prompt)?;
            confirm_request_if_interactive(request, &scoped_path, project_root, permission_prompt)?;
            let args = vec![
                "-n".to_string(),
                "-C".to_string(),
                context_lines.to_string(),
                "--".to_string(),
                pattern.clone(),
                scoped_path.to_string_lossy().to_string(),
            ];

            run_command_capture("grep", &args, project_root)
        }
        ReadRequest::ListDir { path } => {
            ensure_allowed(permission_prompt, "ls")?;
            let scoped_path = resolve_scoped_path(project_root, path)?;
            enforce_read_scope(request, &scoped_path, project_root, permission_prompt)?;
            confirm_request_if_interactive(request, &scoped_path, project_root, permission_prompt)?;
            let args = vec!["-la".to_string(), scoped_path.to_string_lossy().to_string()];
            run_command_capture("ls", &args, project_root)
        }
        ReadRequest::FindFiles { path, glob } => {
            ensure_allowed(permission_prompt, "find")?;
            let scoped_path = resolve_scoped_path(project_root, path)?;
            enforce_read_scope(request, &scoped_path, project_root, permission_prompt)?;
            confirm_request_if_interactive(request, &scoped_path, project_root, permission_prompt)?;
            let scoped = scoped_path.to_string_lossy().to_string();

            let args = if let Some(glob) = glob {
                vec![
                    scoped,
                    "-type".to_string(),
                    "f".to_string(),
                    "-name".to_string(),
                    glob.clone(),
                ]
            } else {
                vec![scoped, "-type".to_string(), "f".to_string()]
            };

            run_command_capture("find", &args, project_root)
        }
    }
}

fn enforce_read_scope(
    request: &ReadRequest,
    scoped_path: &Path,
    project_root: &Path,
    permission_prompt: &PermissionPromptSpec,
) -> Result<()> {
    if !permission_prompt.read_scope.eq_ignore_ascii_case("strict") {
        return Ok(());
    }

    if matches!(request, ReadRequest::ListDir { .. } | ReadRequest::FindFiles { .. }) {
        return Err(miette::miette!(
            "Request denied by strict read scope: directory listing and file discovery are not allowed"
        ));
    }

    if !scoped_path.is_file() {
        return Err(miette::miette!(
            "Request denied by strict read scope: only known source files can be accessed"
        ));
    }

    let allowed_paths = resolve_allowed_paths(project_root, permission_prompt)?;

    if allowed_paths.iter().any(|allowed| allowed == scoped_path) {
        return Ok(());
    }

    Err(miette::miette!(
        "Request denied by strict read scope: '{}' is not an allowed source file",
        display_relative_path(project_root, scoped_path)
    ))
}

fn resolve_allowed_paths(
    project_root: &Path,
    permission_prompt: &PermissionPromptSpec,
) -> Result<Vec<PathBuf>> {
    permission_prompt
        .allowed_paths
        .iter()
        .map(|path| resolve_scoped_path(project_root, path))
        .collect::<Result<Vec<PathBuf>>>()
}

fn confirm_request_if_interactive(
    request: &ReadRequest,
    scoped_path: &Path,
    project_root: &Path,
    permission_prompt: &PermissionPromptSpec,
) -> Result<()> {
    if !permission_prompt.interactive_permissions {
        return Ok(());
    }

    eprintln!(
        "[audit][permission] {} -> {}",
        summarize_read_request(request),
        display_relative_path(project_root, scoped_path)
    );
    eprint!("Allow this request? [y/N]: ");
    io::stderr().flush().into_diagnostic()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer).into_diagnostic()?;
    let accepted = matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes");

    if accepted {
        return Ok(());
    }

    Err(miette::miette!(
        "Request denied by user confirmation: {}",
        summarize_read_request(request)
    ))
}

fn display_relative_path(project_root: &Path, scoped_path: &Path) -> String {
    scoped_path
        .strip_prefix(project_root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| scoped_path.display().to_string())
}

fn ensure_allowed(permission_prompt: &PermissionPromptSpec, command: &str) -> Result<()> {
    if permission_prompt
        .allowed_commands
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(command))
    {
        return Ok(());
    }

    Err(miette::miette!(
        "Command '{}' is not permitted by permission prompt",
        command
    ))
}

fn resolve_scoped_path(project_root: &Path, requested_path: &str) -> Result<PathBuf> {
    let requested_path = requested_path.trim();
    let requested_path = if requested_path.is_empty() {
        "."
    } else {
        requested_path
    };

    let joined = if Path::new(requested_path).is_absolute() {
        PathBuf::from(requested_path)
    } else {
        project_root.join(requested_path)
    };

    let canonical = joined
        .canonicalize()
        .into_diagnostic()
        .with_context(|| format!("Path does not exist or is inaccessible: {}", requested_path))?;

    if !canonical.starts_with(project_root) {
        return Err(miette::miette!(
            "Path escapes project root and is not allowed: {}",
            requested_path
        ));
    }

    Ok(canonical)
}

fn run_command_capture(command: &str, args: &[String], cwd: &Path) -> Result<String> {
    let output = Command::new(command)
        .args(args)
        .current_dir(cwd)
        .output()
        .into_diagnostic()
        .with_context(|| format!("Failed to run command '{}'", command))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut combined = String::new();

    if !stdout.trim().is_empty() {
        combined.push_str(&stdout);
    }

    if !stderr.trim().is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&stderr);
    }

    if combined.trim().is_empty() {
        combined = format!(
            "(no output; command exited with status {})",
            output.status.code().unwrap_or_default()
        );
    }

    if !output.status.success() {
        combined.push_str(&format!(
            "\n(command exited with status {})",
            output.status.code().unwrap_or_default()
        ));
    }

    if combined.chars().count() > MAX_COMMAND_OUTPUT_CHARS {
        let truncated = combined
            .chars()
            .take(MAX_COMMAND_OUTPUT_CHARS)
            .collect::<String>();
        return Ok(format!(
            "{}\n...(truncated to {} chars)",
            truncated, MAX_COMMAND_OUTPUT_CHARS
        ));
    }

    Ok(combined)
}

fn parse_structured_content(content: &str) -> Result<Value> {
    if let Ok(parsed) = serde_json::from_str::<Value>(content) {
        return Ok(parsed);
    }

    let trimmed = content.trim();
    let fenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(str::trim);

    if let Some(fenced_content) = fenced {
        let fenced_content = fenced_content.strip_suffix("```").unwrap_or(fenced_content);
        if let Ok(parsed) = serde_json::from_str::<Value>(fenced_content.trim()) {
            return Ok(parsed);
        }
    }

    Err(miette::miette!(
        "AI provider response is not valid JSON for structured findings"
    ))
}

pub(super) fn block_on_runtime_aware<F, T>(future: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    match Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(future)),
        Err(_) => {
            let runtime = tokio::runtime::Runtime::new().into_diagnostic()?;
            runtime.block_on(future)
        }
    }
}

pub(super) fn summarize_read_request(request: &ReadRequest) -> String {
    match request {
        ReadRequest::ReadFile { path } => format!("read_file {}", path),
        ReadRequest::Grep {
            pattern,
            path,
            context_lines,
        } => format!(
            "grep pattern='{}' path={} context_lines={}",
            pattern, path, context_lines
        ),
        ReadRequest::ListDir { path } => format!("list_dir {}", path),
        ReadRequest::FindFiles { path, glob } => {
            format!("find_files path={} glob={}", path, glob.as_deref().unwrap_or("*"))
        }
    }
}

pub(super) fn describe_read_request_friendly(request: &ReadRequest) -> String {
    match request {
        ReadRequest::ReadFile { path } => {
            format!("read file '{}'", path)
        }
        ReadRequest::Grep {
            pattern,
            path,
            context_lines,
        } => format!(
            "search '{}' in '{}' ({} context lines)",
            pattern, path, context_lines
        ),
        ReadRequest::ListDir { path } => {
            format!("list directory '{}'", path)
        }
        ReadRequest::FindFiles { path, glob } => format!(
            "find files in '{}' with glob '{}'",
            path,
            glob.as_deref().unwrap_or("*")
        ),
    }
}

pub(super) fn render_tool_output_for_log(
    request: &ReadRequest,
    output: &str,
    max_chars: usize,
) -> String {
    match request {
        ReadRequest::ReadFile { path } => {
            format!(
                "📄 File '{}' read (content hidden in logs, {} chars)",
                path,
                output.chars().count()
            )
        }
        _ => truncate_for_log(output, max_chars),
    }
}

pub(super) fn render_model_output_for_log(output: &str, max_chars: usize) -> String {
    truncate_for_log(output, max_chars)
}

fn truncate_for_log(output: &str, max_chars: usize) -> String {
    let char_count = output.chars().count();
    if char_count <= max_chars {
        return output.to_string();
    }

    let preview = output.chars().take(max_chars).collect::<String>();
    format!("{}\n… (truncated, {} chars total)", preview, char_count)
}

pub(super) fn log_agent_progress(enabled: bool, message: impl AsRef<str>) {
    if enabled {
        eprintln!("🤖 {}", message.as_ref());
    }
}

pub(super) fn iteration_from_parsed(
    skill: &VulnerabilitySkill,
    parsed: Value,
) -> SkillIterationResult {
    let findings = parsed
        .get("findings")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let file = item
                        .get("file")
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                        .map(ToString::to_string)
                        .or_else(|| {
                            item.get("location")
                                .and_then(|value| value.get("file"))
                                .and_then(Value::as_str)
                                .filter(|value| !value.trim().is_empty())
                                .map(ToString::to_string)
                        });

                    let line = parse_line_number(item.get("line")).or_else(|| {
                        parse_line_number(item.get("location").and_then(|value| value.get("line")))
                    });

                    VulnerabilityFinding {
                        title: item
                            .get("title")
                            .and_then(Value::as_str)
                            .unwrap_or("Untitled finding")
                            .to_string(),
                        severity: item
                            .get("severity")
                            .and_then(Value::as_str)
                            .unwrap_or(&skill.severity)
                            .to_string(),
                        summary: item
                            .get("summary")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        evidence: item
                            .get("evidence")
                            .and_then(Value::as_array)
                            .map(|e| {
                                e.iter()
                                    .filter_map(Value::as_str)
                                    .map(ToString::to_string)
                                    .collect::<Vec<String>>()
                            })
                            .unwrap_or_default(),
                        recommendation: item
                            .get("recommendation")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        file,
                        line,
                    }
                })
                .collect::<Vec<VulnerabilityFinding>>()
        })
        .unwrap_or_default();

    let status = parsed
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("completed")
        .to_string();

    let next_prompt = parsed
        .get("next_prompt")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(|text| MiniPrompt {
            skill_id: skill.id.clone(),
            text: text.to_string(),
        });

    SkillIterationResult {
        skill_id: skill.id.clone(),
        status,
        findings,
        next_prompt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::audit::model::PermissionPromptSpec;

    #[test]
    fn execute_read_request_strict_allows_known_file() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        let file = root.join("validators/spend.ak");

        std::fs::create_dir_all(file.parent().expect("parent")).expect("create dir");
        std::fs::write(&file, "validator spend {}\n").expect("write file");

        let prompt = PermissionPromptSpec {
            shell: "bash".to_string(),
            allowed_commands: vec!["cat".to_string()],
            scope_rules: vec![],
            workspace_root: root.display().to_string(),
            read_scope: "strict".to_string(),
            interactive_permissions: false,
            allowed_paths: vec!["validators/spend.ak".to_string()],
        };

        let output = execute_read_request(
            &ReadRequest::ReadFile {
                path: "validators/spend.ak".to_string(),
            },
            &root.canonicalize().expect("canonical root"),
            &prompt,
        )
        .expect("request should be allowed");

        assert!(output.contains("validator spend"));
    }

    #[test]
    fn execute_read_request_strict_rejects_list_dir() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();

        let prompt = PermissionPromptSpec {
            shell: "bash".to_string(),
            allowed_commands: vec!["ls".to_string()],
            scope_rules: vec![],
            workspace_root: root.display().to_string(),
            read_scope: "strict".to_string(),
            interactive_permissions: false,
            allowed_paths: vec!["validators/spend.ak".to_string()],
        };

        let err = execute_read_request(
            &ReadRequest::ListDir {
                path: ".".to_string(),
            },
            &root.canonicalize().expect("canonical root"),
            &prompt,
        )
        .expect_err("strict scope should reject list_dir");

        assert!(err.to_string().contains("strict read scope"));
    }
}
