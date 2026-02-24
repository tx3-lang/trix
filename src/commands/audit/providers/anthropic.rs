use miette::{Context, IntoDiagnostic, Result};
use serde_json::Value;
use std::path::Path;

use super::shared::{
    block_on_runtime_aware, build_agent_system_prompt, build_initial_user_prompt,
    describe_read_request, execute_read_request, iteration_from_parsed, log_agent_progress,
    parse_agent_action, preview_output_for_log, summarize_read_request, AgentAction,
    MAX_AGENT_STEPS,
};
use super::{
    AnalysisProvider,
};
use crate::commands::audit::model::{
    MiniPrompt, PermissionPromptSpec, ProviderSpec, SkillIterationResult, VulnerabilitySkill,
};

#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub version: String,
    pub ai_logs: bool,
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
        source_references: &[String],
        project_root: &Path,
        permission_prompt: &PermissionPromptSpec,
    ) -> Result<SkillIterationResult> {
        let canonical_root = project_root.canonicalize().into_diagnostic().with_context(|| {
            format!(
                "Failed to canonicalize project root {}",
                project_root.display()
            )
        })?;

        let system_prompt = build_agent_system_prompt();
        let initial_user_prompt = build_initial_user_prompt(prompt, source_references, permission_prompt);

        let mut messages = vec![serde_json::json!({
            "role": "user",
            "content": initial_user_prompt,
        })];

        for step_idx in 0..MAX_AGENT_STEPS {
            log_agent_progress(
                self.ai_logs,
                format!(
                    "[ ] skill={} step={}/{} ask model at {}",
                    skill.id,
                    step_idx + 1,
                    MAX_AGENT_STEPS,
                    self.endpoint
                ),
            );

            let payload = serde_json::json!({
                "model": self.model,
                "max_tokens": 1200,
                "system": system_prompt,
                "messages": messages.clone(),
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

            messages.push(serde_json::json!({
                "role": "assistant",
                "content": content,
            }));

            match parse_agent_action(content)? {
                AgentAction::Final(parsed) => {
                    let findings = parsed
                        .get("findings")
                        .and_then(Value::as_array)
                        .map(|items| items.len())
                        .unwrap_or(0);
                    let status = parsed
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("completed");

                    log_agent_progress(
                        self.ai_logs,
                        format!(
                            "[x] skill={} step={}/{} final status={} findings={}",
                            skill.id,
                            step_idx + 1,
                            MAX_AGENT_STEPS,
                            status,
                            findings
                        ),
                    );
                    return Ok(iteration_from_parsed(skill, parsed));
                }
                AgentAction::ReadRequest(request) => {
                    log_agent_progress(
                        self.ai_logs,
                        format!(
                            "[ ] skill={} step={}/{} {}",
                            skill.id,
                            step_idx + 1,
                            MAX_AGENT_STEPS,
                            describe_read_request(&request)
                        ),
                    );

                    log_agent_progress(
                        self.ai_logs,
                        format!(
                            "[ ] skill={} step={}/{} run local action: {}",
                            skill.id,
                            step_idx + 1,
                            MAX_AGENT_STEPS,
                            summarize_read_request(&request)
                        ),
                    );

                    let output = execute_read_request(&request, &canonical_root, permission_prompt)
                        .unwrap_or_else(|error| format!("Request failed: {}", error));

                    log_agent_progress(
                        self.ai_logs,
                        format!(
                            "[x] skill={} step={}/{} local action finished chars={}",
                            skill.id,
                            step_idx + 1,
                            MAX_AGENT_STEPS,
                            output.chars().count()
                        ),
                    );

                    log_agent_progress(
                        self.ai_logs,
                        format!(
                            "[i] skill={} step={}/{} output preview: {}",
                            skill.id,
                            step_idx + 1,
                            MAX_AGENT_STEPS,
                            preview_output_for_log(&output, 180)
                        ),
                    );

                    log_agent_progress(
                        self.ai_logs,
                        format!(
                            "[ ] skill={} step={}/{} send local output back to model",
                            skill.id,
                            step_idx + 1,
                            MAX_AGENT_STEPS
                        ),
                    );

                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": format!(
                            "Tool result for {:?}:\n{}\n\nContinue and return JSON.",
                            request,
                            output
                        ),
                    }));
                }
            }
        }

        Err(miette::miette!(
            "Anthropic provider exceeded max interactive read steps ({}) for skill '{}' (enable --ai-logs to inspect progress)",
            MAX_AGENT_STEPS,
            skill.id
        ))
    }
}
