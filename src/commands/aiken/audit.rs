use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::config::{ProfileConfig, RootConfig};

use super::model::{
    AnalysisStateJson, MiniPrompt, PermissionPromptSpec, SkillIterationResult,
    VulnerabilityFinding, VulnerabilityReportSpec, VulnerabilitySkill,
};
use super::provider::{AnalysisProvider, ScaffoldProvider};

const DEFAULT_SKILLS_DIR: &str = "skills/vulnerabilities";

#[derive(ClapArgs)]
pub struct Args {
    /// Path where the incremental analysis state JSON will be written.
    #[arg(long, default_value = ".tx3/aiken-audit/state.json")]
    pub state_out: String,

    /// Path where the final vulnerability report markdown will be written.
    #[arg(long, default_value = ".tx3/aiken-audit/vulnerabilities.md")]
    pub report_out: String,

    /// Path to vulnerability skill definitions.
    #[arg(long, default_value = "skills/vulnerabilities")]
    pub skills_dir: String,
}

pub fn run(args: Args, config: &RootConfig, _profile: &ProfileConfig) -> Result<()> {
    run_scaffold_analysis(args, config, &ScaffoldProvider)
}

fn run_scaffold_analysis(
    args: Args,
    config: &RootConfig,
    provider: &dyn AnalysisProvider,
) -> Result<()> {
    let skills_dir = PathBuf::from(&args.skills_dir);
    let state_out = PathBuf::from(&args.state_out);
    let report_out = PathBuf::from(&args.report_out);
    let target_path = config.protocol.main.display().to_string();

    let permission_prompt = build_permission_prompt_spec();
    let skills = load_skills(&skills_dir, &args.skills_dir)?;

    let mut state = AnalysisStateJson {
        version: "1".to_string(),
        target_path: target_path.clone(),
        provider: provider.provider_spec(),
        permission_prompt: permission_prompt.clone(),
        iterations: vec![],
    };

    write_state(&state_out, &state)?;

    run_skill_loop(&skills, provider, &mut state, &state_out)?;

    let report = build_report(&state);
    let report_markdown = render_report_markdown(&report);
    write_text_file(&report_out, &report_markdown)?;

    println!(
        "⚠️  EXPERIMENTAL: Aiken audit scaffold complete. Skills processed: {}",
        state.iterations.len()
    );
    println!("State written to: {}", state_out.display());
    println!("Report written to: {}", report_out.display());

    Ok(())
}

fn run_skill_loop(
    skills: &[VulnerabilitySkill],
    provider: &dyn AnalysisProvider,
    state: &mut AnalysisStateJson,
    state_out: &Path,
) -> Result<()> {
    for skill in skills {
        let prompt = build_mini_prompt(skill);
        let iteration = provider.analyze_skill(skill, &prompt)?;
        append_iteration(state, iteration);
        write_state(state_out, state)?;
    }

    Ok(())
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
        title: "Aiken Vulnerability Report".to_string(),
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
            "Aiken skills directory not found: {}",
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

    let parsed: SkillFrontmatter = serde_yaml_ng::from_str(&frontmatter)
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
