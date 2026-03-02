use miette::Result;
use std::path::Path;

use super::AnalysisProvider;
use crate::commands::audit::model::{
    MiniPrompt, PermissionPromptSpec, ProviderSpec, SkillIterationResult, ValidatorContextMap,
    VulnerabilitySkill,
};

#[path = "heuristic_detectors.rs"]
mod detectors;

#[derive(Debug, Default)]
pub struct HeuristicProvider;

impl AnalysisProvider for HeuristicProvider {
    fn provider_spec(&self) -> ProviderSpec {
        ProviderSpec {
            name: "heuristic".to_string(),
            model: None,
            notes: "Deterministic local heuristic provider. No external AI calls are performed."
                .to_string(),
        }
    }

    fn analyze_skill(
        &self,
        skill: &VulnerabilitySkill,
        _prompt: &MiniPrompt,
        source_references: &[String],
        validator_context: &ValidatorContextMap,
        project_root: &Path,
        _permission_prompt: &PermissionPromptSpec,
    ) -> Result<SkillIterationResult> {
        let findings = match detectors::collect_findings_for_skill(
            skill,
            source_references,
            validator_context,
            project_root,
        )? {
            Some(findings) => findings,
            None => {
                return Ok(SkillIterationResult {
                    skill_id: skill.id.clone(),
                    status: "unsupported-skill".to_string(),
                    findings: vec![],
                    next_prompt: None,
                });
            }
        };

        Ok(SkillIterationResult {
            skill_id: skill.id.clone(),
            status: "completed".to_string(),
            findings,
            next_prompt: None,
        })
    }
}
