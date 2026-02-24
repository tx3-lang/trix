use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilitySkill {
    pub id: String,
    pub name: String,
    pub severity: String,
    pub description: String,
    pub prompt_fragment: String,
    pub examples: Vec<String>,
    pub false_positives: Vec<String>,
    pub references: Vec<String>,
    pub tags: Vec<String>,
    pub confidence_hint: Option<String>,
    pub guidance_markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiniPrompt {
    pub skill_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillIterationResult {
    pub skill_id: String,
    pub status: String,
    pub findings: Vec<VulnerabilityFinding>,
    pub next_prompt: Option<MiniPrompt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityFinding {
    pub title: String,
    pub severity: String,
    pub summary: String,
    pub evidence: Vec<String>,
    pub recommendation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisStateJson {
    pub version: String,
    pub source_files: Vec<String>,
    pub provider: ProviderSpec,
    pub permission_prompt: PermissionPromptSpec,
    pub iterations: Vec<SkillIterationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSpec {
    pub name: String,
    pub model: Option<String>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionPromptSpec {
    pub shell: String,
    pub allowed_commands: Vec<String>,
    pub scope_rules: Vec<String>,
    #[serde(default = "default_read_scope")]
    pub read_scope: String,
    #[serde(default)]
    pub interactive_permissions: bool,
    #[serde(default)]
    pub allowed_paths: Vec<String>,
}

fn default_read_scope() -> String {
    "workspace".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityReportSpec {
    pub title: String,
    pub generated_at: String,
    pub findings: Vec<VulnerabilityFinding>,
}
