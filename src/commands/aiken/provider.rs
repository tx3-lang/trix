use miette::Result;

use super::model::{MiniPrompt, ProviderSpec, SkillIterationResult, VulnerabilitySkill};

pub trait AnalysisProvider {
    fn provider_spec(&self) -> ProviderSpec;

    fn analyze_skill(
        &self,
        skill: &VulnerabilitySkill,
        prompt: &MiniPrompt,
    ) -> Result<SkillIterationResult>;
}

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
