use miette::{IntoDiagnostic, Result};
use serde_json::Value;
use tokio::runtime::Handle;

use super::model::{
    MiniPrompt, ProviderSpec, SkillIterationResult, VulnerabilityFinding, VulnerabilitySkill,
};

pub trait AnalysisProvider {
    fn provider_spec(&self) -> ProviderSpec;

    fn analyze_skill(
        &self,
        skill: &VulnerabilitySkill,
        prompt: &MiniPrompt,
        target_path: &str,
        source_code: &str,
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
        target_path: &str,
        _source_code: &str,
    ) -> Result<SkillIterationResult> {
        Ok(SkillIterationResult {
            skill_id: skill.id.clone(),
            target_path: target_path.to_string(),
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

#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
}

impl AnalysisProvider for OpenAiProvider {
    fn provider_spec(&self) -> ProviderSpec {
        ProviderSpec {
            name: "openai-compatible".to_string(),
            model: Some(self.model.clone()),
            notes: format!("Endpoint: {}", self.endpoint),
        }
    }

    fn analyze_skill(
        &self,
        skill: &VulnerabilitySkill,
        prompt: &MiniPrompt,
        target_path: &str,
        source_code: &str,
    ) -> Result<SkillIterationResult> {
        let system_prompt = "You are a security auditor specialized in Aiken smart contracts. Return JSON only with shape: {\"status\": string, \"findings\": [{\"title\": string, \"severity\": string, \"summary\": string, \"evidence\": [string], \"recommendation\": string}], \"next_prompt\": string|null}.";
        let user_prompt = format!(
            "Analyze the following Aiken source file for a single vulnerability skill.\n\nTarget path: {}\n\nSkill:\n{}\n\nSource code:\n{}",
            target_path, prompt.text, source_code
        );

        let payload = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt
                },
                {
                    "role": "user",
                    "content": user_prompt
                }
            ],
            "response_format": {
                "type": "json_object"
            }
        });

        let response_json = block_on_runtime_aware(async {
            let client = reqwest::Client::new();
            let response = client
                .post(&self.endpoint)
                .bearer_auth(&self.api_key)
                .json(&payload)
                .send()
                .await
                .into_diagnostic()?;

            let response = response.error_for_status().into_diagnostic()?;
            response.json::<Value>().await.into_diagnostic()
        })?;

        let content = response_json
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                miette::miette!("AI provider returned an unexpected response payload")
            })?;

        let parsed = parse_structured_content(content)?;

        Ok(iteration_from_parsed(skill, target_path, parsed))
    }
}

#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub version: String,
}

impl AnalysisProvider for AnthropicProvider {
    fn provider_spec(&self) -> ProviderSpec {
        ProviderSpec {
            name: "anthropic".to_string(),
            model: Some(self.model.clone()),
            notes: format!("Endpoint: {}", self.endpoint),
        }
    }

    fn analyze_skill(
        &self,
        skill: &VulnerabilitySkill,
        prompt: &MiniPrompt,
        target_path: &str,
        source_code: &str,
    ) -> Result<SkillIterationResult> {
        let system_prompt = "You are a security auditor specialized in Aiken smart contracts. Return JSON only with shape: {\"status\": string, \"findings\": [{\"title\": string, \"severity\": string, \"summary\": string, \"evidence\": [string], \"recommendation\": string}], \"next_prompt\": string|null}.";
        let user_prompt = format!(
            "Analyze the following Aiken source file for a single vulnerability skill.\n\nTarget path: {}\n\nSkill:\n{}\n\nSource code:\n{}",
            target_path, prompt.text, source_code
        );

        let payload = serde_json::json!({
            "model": self.model,
            "max_tokens": 1200,
            "system": system_prompt,
            "messages": [
                {
                    "role": "user",
                    "content": user_prompt
                }
            ]
        });

        let response_json = block_on_runtime_aware(async {
            let client = reqwest::Client::new();
            let response = client
                .post(&self.endpoint)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", &self.version)
                .json(&payload)
                .send()
                .await
                .into_diagnostic()?;

            let response = response.error_for_status().into_diagnostic()?;
            response.json::<Value>().await.into_diagnostic()
        })?;

        let content = response_json
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                miette::miette!("Anthropic provider returned an unexpected response payload")
            })?;

        let parsed = parse_structured_content(content)?;

        Ok(iteration_from_parsed(skill, target_path, parsed))
    }
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

fn block_on_runtime_aware<F, T>(future: F) -> Result<T>
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

fn iteration_from_parsed(
    skill: &VulnerabilitySkill,
    target_path: &str,
    parsed: Value,
) -> SkillIterationResult {
    let findings = parsed
        .get("findings")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| VulnerabilityFinding {
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
        target_path: target_path.to_string(),
        status,
        findings,
        next_prompt,
    }
}
