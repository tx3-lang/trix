use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, Result};
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
    MiniPrompt {
        skill_id: skill.id.clone(),
        text: format!(
            "[{}:{}] {}",
            skill.severity, skill.title, skill.prompt_fragment
        ),
    }
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
    let mut id = None;
    let mut title = None;
    let mut severity = None;
    let mut description = None;
    let mut prompt_fragment = None;

    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim().to_string();

        match key {
            "id" => id = Some(value),
            "title" => title = Some(value),
            "severity" => severity = Some(value),
            "description" => description = Some(value),
            "prompt_fragment" => prompt_fragment = Some(value),
            _ => {}
        }
    }

    Ok(VulnerabilitySkill {
        id: id.ok_or_else(|| {
            miette::miette!(
                "Missing `id` field in vulnerability skill file {}",
                path.display()
            )
        })?,
        title: title.ok_or_else(|| {
            miette::miette!(
                "Missing `title` field in vulnerability skill file {}",
                path.display()
            )
        })?,
        severity: severity.ok_or_else(|| {
            miette::miette!(
                "Missing `severity` field in vulnerability skill file {}",
                path.display()
            )
        })?,
        description: description.ok_or_else(|| {
            miette::miette!(
                "Missing `description` field in vulnerability skill file {}",
                path.display()
            )
        })?,
        prompt_fragment: prompt_fragment.ok_or_else(|| {
            miette::miette!(
                "Missing `prompt_fragment` field in vulnerability skill file {}",
                path.display()
            )
        })?,
    })
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
