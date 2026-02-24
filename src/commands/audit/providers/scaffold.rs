use miette::Result;
use std::path::Path;

use super::AnalysisProvider;
use crate::commands::audit::model::{
    MiniPrompt, PermissionPromptSpec, ProviderSpec, SkillIterationResult, VulnerabilitySkill,
};

#[derive(Debug, Default)]
pub struct ScaffoldProvider;

impl AnalysisProvider for ScaffoldProvider {
    fn provider_spec(&self) -> ProviderSpec {
        ProviderSpec {
            name: "scaffold".to_string(),
            model: None,
            notes: "Scaffolding-only provider. No external AI calls are performed.".to_string(),
        }
    }

    fn analyze_skill(
        &self,
        skill: &VulnerabilitySkill,
        prompt: &MiniPrompt,
        _source_references: &[String],
        _project_root: &Path,
        _permission_prompt: &PermissionPromptSpec,
    ) -> Result<SkillIterationResult> {
        Ok(SkillIterationResult {
            skill_id: skill.id.clone(),
            status: "scaffolded".to_string(),
            findings: vec![],
            next_prompt: Some(MiniPrompt {
                skill_id: skill.id.clone(),
                text: format!(
                    "Scaffold follow-up placeholder for skill '{}' based on prompt '{}'.",
                    skill.id, prompt.text
                ),
            }),
        })
    }
}
