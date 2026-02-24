use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::config::{ProfileConfig, RootConfig};

mod model;
mod provider;

use self::model::{
    AnalysisStateJson, MiniPrompt, PermissionPromptSpec, SkillIterationResult,
    VulnerabilityFinding, VulnerabilityReportSpec, VulnerabilitySkill,
};
use self::provider::{AnalysisProvider, AnthropicProvider, OpenAiProvider, ScaffoldProvider};

const DEFAULT_SKILLS_DIR: &str = "skills/vulnerabilities";
const DEFAULT_AI_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_AI_MODEL: &str = "gpt-4.1-mini";
const DEFAULT_AI_API_KEY_ENV: &str = "OPENAI_API_KEY";
const DEFAULT_ANTHROPIC_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_ANTHROPIC_MODEL: &str = "claude-3-5-haiku-latest";
const DEFAULT_ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_OLLAMA_ENDPOINT: &str = "http://localhost:11434/v1/chat/completions";
const DEFAULT_OLLAMA_MODEL: &str = "llama3.1";

#[derive(ClapArgs)]
pub struct Args {
    /// Path where the incremental analysis state JSON will be written.
    #[arg(long, default_value = ".tx3/audit/state.json")]
    pub state_out: String,

    /// Path where the final vulnerability report markdown will be written.
    #[arg(long, default_value = ".tx3/audit/vulnerabilities.md")]
    pub report_out: String,

    /// Path to vulnerability skill definitions.
    #[arg(long, default_value = "skills/vulnerabilities")]
    pub skills_dir: String,

    /// Analysis provider: scaffold | openai | anthropic | ollama
    #[arg(long, default_value = "scaffold")]
    pub provider: String,

    /// API endpoint override. Default depends on --provider.
    #[arg(long)]
    pub endpoint: Option<String>,

    /// Model override. Default depends on --provider.
    #[arg(long)]
    pub model: Option<String>,

    /// API key environment variable override. Default depends on --provider.
    #[arg(long)]
    pub api_key_env: Option<String>,
}

#[allow(unused_variables)]
pub fn run(args: Args, config: &RootConfig, profile: &ProfileConfig) -> Result<()> {
    #[cfg(feature = "unstable")]
    {
        _run(args, config, profile)
    }
    #[cfg(not(feature = "unstable"))]
    {
        let _ = args;
        let _ = config;
        let _ = profile;

        Err(miette::miette!(
            "The audit command is currently unstable and requires the `unstable` feature to be enabled."
        ))
    }
}

pub fn _run(args: Args, config: &RootConfig, _profile: &ProfileConfig) -> Result<()> {
    let provider = build_provider(&args)?;
    run_analysis(args, config, provider.as_ref())
}

fn run_analysis(
    args: Args,
    config: &RootConfig,
    provider: &dyn AnalysisProvider,
) -> Result<()> {
    let skills_dir = PathBuf::from(&args.skills_dir);
    let state_out = PathBuf::from(&args.state_out);
    let report_out = PathBuf::from(&args.report_out);
    let target_path = config.protocol.main.display().to_string();
    let project_root = std::env::current_dir().into_diagnostic()?;
    let source_files = discover_source_files(&project_root)?;
    let source_files = if source_files.is_empty() {
        vec![config.protocol.main.clone()]
    } else {
        source_files
    };

    let permission_prompt = build_permission_prompt_spec();
    let skills = load_skills(&skills_dir, &args.skills_dir)?;

    let mut state = AnalysisStateJson {
        version: "1".to_string(),
        target_path: target_path.clone(),
        source_files: source_files
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        provider: provider.provider_spec(),
        permission_prompt,
        iterations: vec![],
    };

    write_state(&state_out, &state)?;

    run_skill_loop(&skills, &source_files, provider, &mut state, &state_out)?;

    let report = build_report(&state);
    let report_markdown = render_report_markdown(&report);
    write_text_file(&report_out, &report_markdown)?;

    println!(
        "⚠️  EXPERIMENTAL: Audit complete. Iterations processed: {}",
        state.iterations.len()
    );
    println!("Source files analyzed: {}", state.source_files.len());
    println!("State written to: {}", state_out.display());
    println!("Report written to: {}", report_out.display());

    Ok(())
}

fn run_skill_loop(
    skills: &[VulnerabilitySkill],
    source_files: &[PathBuf],
    provider: &dyn AnalysisProvider,
    state: &mut AnalysisStateJson,
    state_out: &Path,
) -> Result<()> {
    for source_file in source_files {
        let source_code = std::fs::read_to_string(source_file)
            .into_diagnostic()
            .with_context(|| format!("Failed to read source file {}", source_file.display()))?;
        let target_path = source_file.display().to_string();

        for skill in skills {
            let prompt = build_mini_prompt(skill);
            let iteration = provider.analyze_skill(skill, &prompt, &target_path, &source_code)?;
            append_iteration(state, iteration);
            write_state(state_out, state)?;
        }
    }

    Ok(())
}

fn discover_source_files(project_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut to_visit = vec![project_root.to_path_buf()];

    while let Some(dir) = to_visit.pop() {
        let entries = std::fs::read_dir(&dir)
            .into_diagnostic()
            .with_context(|| format!("Failed to read directory {}", dir.display()))?;

        for entry in entries {
            let entry = entry.into_diagnostic()?;
            let path = entry.path();

            if path.is_dir() {
                let skip = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| matches!(name, ".git" | "target" | ".tx3" | "build"))
                    .unwrap_or(false);

                if !skip {
                    to_visit.push(path);
                }
                continue;
            }

            let is_source_file = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("ak"))
                .unwrap_or(false);

            if is_source_file {
                files.push(path);
            }
        }
    }

    files.sort();
    Ok(files)
}

fn build_provider(args: &Args) -> Result<Box<dyn AnalysisProvider>> {
    match args.provider.to_ascii_lowercase().as_str() {
        "scaffold" => Ok(Box::new(ScaffoldProvider)),
        "openai" => {
            let endpoint = args
                .endpoint
                .clone()
                .unwrap_or_else(|| DEFAULT_AI_ENDPOINT.to_string());
            let model = args
                .model
                .clone()
                .unwrap_or_else(|| DEFAULT_AI_MODEL.to_string());
            let api_key_env = args
                .api_key_env
                .as_deref()
                .unwrap_or(DEFAULT_AI_API_KEY_ENV);

            let api_key = std::env::var(api_key_env).into_diagnostic().with_context(|| {
                format!(
                    "Missing API key environment variable '{}'. Set it before running with --provider openai.",
                    api_key_env
                )
            })?;

            Ok(Box::new(OpenAiProvider {
                endpoint,
                api_key,
                model,
            }))
        }
        "anthropic" => {
            let endpoint = args
                .endpoint
                .clone()
                .unwrap_or_else(|| DEFAULT_ANTHROPIC_ENDPOINT.to_string());
            let model = args
                .model
                .clone()
                .unwrap_or_else(|| DEFAULT_ANTHROPIC_MODEL.to_string());
            let api_key_env = args
                .api_key_env
                .as_deref()
                .unwrap_or(DEFAULT_ANTHROPIC_API_KEY_ENV);

            let api_key = std::env::var(api_key_env)
                .into_diagnostic()
                .with_context(|| {
                    format!(
                        "Missing API key environment variable '{}'. Set it before running with --provider anthropic.",
                        api_key_env
                    )
                })?;

            Ok(Box::new(AnthropicProvider {
                endpoint,
                api_key,
                model,
                version: DEFAULT_ANTHROPIC_VERSION.to_string(),
            }))
        }
        "ollama" => Ok(Box::new(OpenAiProvider {
            endpoint: args
                .endpoint
                .clone()
                .unwrap_or_else(|| DEFAULT_OLLAMA_ENDPOINT.to_string()),
            api_key: "ollama".to_string(),
            model: args
                .model
                .clone()
                .unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.to_string()),
        })),
        value => Err(miette::miette!(
            "Unsupported provider '{}'. Expected one of: scaffold, openai, anthropic, ollama",
            value
        )),
    }
}

fn append_iteration(state: &mut AnalysisStateJson, iteration: SkillIterationResult) {
    state.iterations.push(iteration);
}

fn build_mini_prompt(skill: &VulnerabilitySkill) -> MiniPrompt {
    let text = compose_skill_prompt(skill);

    MiniPrompt {
        skill_id: skill.id.clone(),
        text,
    }
}

fn compose_skill_prompt(skill: &VulnerabilitySkill) -> String {
    let mut sections = vec![
        format!("Skill ID: {}", skill.id),
        format!("Name: {}", skill.name),
        format!("Severity: {}", skill.severity),
        format!("Description: {}", skill.description),
        format!("Prompt Fragment: {}", skill.prompt_fragment),
    ];

    if !skill.tags.is_empty() {
        sections.push(format!("Tags: {}", skill.tags.join(", ")));
    }

    if let Some(hint) = &skill.confidence_hint {
        sections.push(format!("Confidence Hint: {}", hint));
    }

    if !skill.examples.is_empty() {
        sections.push(format!("Examples:\n- {}", skill.examples.join("\n- ")));
    }

    if !skill.false_positives.is_empty() {
        sections.push(format!(
            "False Positives To Avoid:\n- {}",
            skill.false_positives.join("\n- ")
        ));
    }

    if !skill.references.is_empty() {
        sections.push(format!("References:\n- {}", skill.references.join("\n- ")));
    }

    if !skill.guidance_markdown.trim().is_empty() {
        sections.push(format!("Guidance:\n{}", skill.guidance_markdown.trim()));
    }

    sections.join("\n\n")
}

fn build_permission_prompt_spec() -> PermissionPromptSpec {
    PermissionPromptSpec {
        shell: "bash".to_string(),
        allowed_commands: vec![
            "grep".to_string(),
            "cat".to_string(),
            "find".to_string(),
            "ls".to_string(),
        ],
        scope_rules: vec![
            "Only execute commands within the current project root.".to_string(),
            "Do not write outside designated output artifacts.".to_string(),
        ],
    }
}

fn build_report(state: &AnalysisStateJson) -> VulnerabilityReportSpec {
    let findings = state
        .iterations
        .iter()
        .flat_map(|iteration| iteration.findings.iter().cloned())
        .collect::<Vec<VulnerabilityFinding>>();

    VulnerabilityReportSpec {
        title: "Vulnerability Report".to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        target: state.target_path.clone(),
        findings,
    }
}

fn load_skills(skills_dir: &Path, skills_dir_arg: &str) -> Result<Vec<VulnerabilitySkill>> {
    if !skills_dir.exists() {
        if skills_dir_arg == DEFAULT_SKILLS_DIR {
            return load_embedded_seed_skills();
        }

        return Err(miette::miette!(
            "Audit skills directory not found: {}",
            skills_dir.display()
        ));
    }

    let mut entries = std::fs::read_dir(skills_dir)
        .into_diagnostic()
        .context("Failed to read skills directory")?
        .filter_map(|entry| entry.ok().map(|value| value.path()))
        .filter(|path| path.is_file())
        .collect::<Vec<PathBuf>>();

    entries.sort();

    let skills = entries
        .iter()
        .map(|path| load_skill_from_file(path))
        .collect::<Result<Vec<VulnerabilitySkill>>>()?;

    if skills.is_empty() {
        return Err(miette::miette!(
            "No vulnerability skills found in {}",
            skills_dir.display()
        ));
    }

    Ok(skills)
}

fn load_embedded_seed_skills() -> Result<Vec<VulnerabilitySkill>> {
    let seed_files = [
        (
            Path::new("skills/vulnerabilities/001-state-transition.md"),
            include_str!("../../../skills/vulnerabilities/001-state-transition.md"),
        ),
        (
            Path::new("skills/vulnerabilities/002-authz-boundaries.md"),
            include_str!("../../../skills/vulnerabilities/002-authz-boundaries.md"),
        ),
        (
            Path::new("skills/vulnerabilities/003-strict-value-equality.md"),
            include_str!("../../../skills/vulnerabilities/003-strict-value-equality.md"),
        ),
    ];

    seed_files
        .iter()
        .map(|(path, content)| parse_skill_content(path, content))
        .collect::<Result<Vec<VulnerabilitySkill>>>()
}

fn load_skill_from_file(path: &Path) -> Result<VulnerabilitySkill> {
    let content = std::fs::read_to_string(path)
        .into_diagnostic()
        .with_context(|| format!("Failed to read vulnerability skill file {}", path.display()))?;

    parse_skill_content(path, &content)
}

fn parse_skill_content(path: &Path, content: &str) -> Result<VulnerabilitySkill> {
    let (frontmatter, body) = split_frontmatter(content).with_context(|| {
        format!(
            "Failed to parse frontmatter from vulnerability skill file {}",
            path.display()
        )
    })?;

    let normalized_frontmatter = normalize_yaml_indentation(&frontmatter);

    let parsed: SkillFrontmatter = serde_yaml_ng::from_str(&normalized_frontmatter)
        .into_diagnostic()
        .with_context(|| {
            format!(
                "Invalid YAML frontmatter in vulnerability skill file {}",
                path.display()
            )
        })?;

    let severity = parsed.severity.trim().to_ascii_lowercase();
    if !matches!(severity.as_str(), "low" | "medium" | "high" | "critical") {
        return Err(miette::miette!(
            "Invalid `severity` value '{}' in vulnerability skill file {}. Expected one of: low, medium, high, critical",
            parsed.severity,
            path.display()
        ));
    }

    Ok(VulnerabilitySkill {
        id: require_non_empty("id", path, parsed.id)?,
        name: require_non_empty("name", path, parsed.name)?,
        severity,
        description: require_non_empty("description", path, parsed.description)?,
        prompt_fragment: require_non_empty("prompt_fragment", path, parsed.prompt_fragment)?,
        examples: parsed.examples,
        false_positives: parsed.false_positives,
        references: parsed.references,
        tags: parsed.tags,
        confidence_hint: parsed.confidence_hint.filter(|value| !value.trim().is_empty()),
        guidance_markdown: body.trim().to_string(),
    })
}

fn normalize_yaml_indentation(input: &str) -> String {
    input.replace('\t', "  ")
}

fn split_frontmatter(content: &str) -> Result<(String, String)> {
    let content = content.trim_start_matches('\u{feff}');
    let mut lines = content.lines();

    let Some(first_line) = lines.next() else {
        return Err(miette::miette!("Skill file is empty"));
    };

    if first_line.trim() != "---" {
        return Err(miette::miette!(
            "Missing frontmatter start delimiter `---`"
        ));
    }

    let mut frontmatter_lines = Vec::new();
    let mut found_end = false;

    for line in lines.by_ref() {
        if line.trim() == "---" {
            found_end = true;
            break;
        }
        frontmatter_lines.push(line);
    }

    if !found_end {
        return Err(miette::miette!(
            "Missing frontmatter end delimiter `---`"
        ));
    }

    let body_lines = lines.collect::<Vec<_>>();

    Ok((frontmatter_lines.join("\n"), body_lines.join("\n")))
}

fn require_non_empty(field: &str, path: &Path, value: String) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(miette::miette!(
            "Field `{}` must be non-empty in vulnerability skill file {}",
            field,
            path.display()
        ));
    }

    Ok(trimmed.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillFrontmatter {
    id: String,
    name: String,
    severity: String,
    description: String,
    prompt_fragment: String,
    #[serde(default)]
    examples: Vec<String>,
    #[serde(default)]
    false_positives: Vec<String>,
    #[serde(default)]
    references: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    confidence_hint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_skill_content_reads_frontmatter_and_guidance() {
        let content = r#"---
id: strict-value-equality-003
name: Strict value equality
severity: high
description: Detect strict equality checks for ADA.
prompt_fragment: Find strict equality on ADA or full values.
examples:
  - output.value == expected
tags:
  - plutus-v2
confidence_hint: medium
---
# Instructions

Check validator outputs and avoid false positives for without_lovelace().
"#;

        let skill = parse_skill_content(Path::new("skill.md"), content).expect("should parse");

        assert_eq!(skill.id, "strict-value-equality-003");
        assert_eq!(skill.name, "Strict value equality");
        assert_eq!(skill.severity, "high");
        assert_eq!(skill.examples.len(), 1);
        assert!(skill.guidance_markdown.contains("# Instructions"));
    }

    #[test]
    fn parse_skill_content_requires_frontmatter() {
        let content = "id: foo";
        let error = parse_skill_content(Path::new("skill.md"), content).expect_err("should fail");
        assert!(error.to_string().contains("frontmatter"));
    }

    #[test]
    fn parse_skill_content_rejects_invalid_severity() {
        let content = r#"---
id: skill-1
name: Test skill
severity: urgent
description: desc
prompt_fragment: prompt
---
body
"#;

        let error = parse_skill_content(Path::new("skill.md"), content).expect_err("should fail");
        assert!(error.to_string().contains("Invalid `severity` value"));
    }

    #[test]
    fn discover_source_files_finds_ak_files_recursively() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        let validators = root.join("onchain/validators");

        fs::create_dir_all(&validators).expect("create validators dir");
        fs::write(validators.join("spend.ak"), "validator spend {}").expect("write ak file");
        fs::write(validators.join("readme.md"), "# ignore").expect("write non-ak file");

        let files = discover_source_files(root).expect("should discover files");

        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("onchain/validators/spend.ak"));
    }

    #[test]
    fn discover_source_files_skips_target_tx3_and_build_dirs() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();

        let normal_dir = root.join("contracts");
        let target_dir = root.join("target/generated");
        let tx3_dir = root.join(".tx3/tmp");
        let build_dir = root.join("build/output");

        fs::create_dir_all(&normal_dir).expect("create normal dir");
        fs::create_dir_all(&target_dir).expect("create target dir");
        fs::create_dir_all(&tx3_dir).expect("create tx3 dir");
        fs::create_dir_all(&build_dir).expect("create build dir");

        fs::write(normal_dir.join("ok.ak"), "validator ok {}").expect("write ak");
        fs::write(target_dir.join("skip.ak"), "validator skip {}").expect("write ak in target");
        fs::write(tx3_dir.join("skip2.ak"), "validator skip2 {}").expect("write ak in tx3");
        fs::write(build_dir.join("skip3.ak"), "validator skip3 {}").expect("write ak in build");

        let files = discover_source_files(root).expect("should discover files");

        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("contracts/ok.ak"));
    }
}

fn write_state(path: &Path, state: &AnalysisStateJson) -> Result<()> {
    let serialized = serde_json::to_string_pretty(state).into_diagnostic()?;
    write_text_file(path, &serialized)
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

fn render_report_markdown(report: &VulnerabilityReportSpec) -> String {
    let template = include_str!("../../../templates/aiken/report.md");
    let findings_markdown = render_findings_markdown(&report.findings);

    template
        .replace("{{ target }}", &report.target)
        .replace("{{ generated_at }}", &report.generated_at)
        .replace("{{ findings_markdown }}", &findings_markdown)
}

fn render_findings_markdown(findings: &[VulnerabilityFinding]) -> String {
    if findings.is_empty() {
        return "- *(none)*".to_string();
    }

    findings
        .iter()
        .map(|finding| {
            format!(
                "- **{}** (`{}`)\n  - Summary: {}\n  - Recommendation: {}",
                finding.title, finding.severity, finding.summary, finding.recommendation
            )
        })
        .collect::<Vec<String>>()
        .join("\n")
}
