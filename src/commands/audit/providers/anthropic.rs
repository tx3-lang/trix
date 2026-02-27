use miette::{Context, IntoDiagnostic, Result};
use serde_json::{json, Value};
use std::path::Path;

use super::shared::{
    block_on_runtime_aware, build_agent_system_prompt, build_initial_user_prompt,
    emit_reasoning_double_line_break, emit_reasoning_line_break, finalize_content_stdout,
    finalize_reasoning_stdout, log_agent_progress, run_agent_loop, stream_content_delta_to_stdout,
    stream_reasoning_delta_to_stdout, ContentStreamState, ReasoningStreamState,
};
use super::AnalysisProvider;
use crate::commands::audit::model::{
    MiniPrompt, PermissionPromptSpec, ProviderSpec, SkillIterationResult, ValidatorContextMap,
    VulnerabilitySkill,
};

const DEFAULT_MAX_TOKENS: u32 = 1200;
const THINKING_MAX_TOKENS: u32 = 1600;
const THINKING_BUDGET_TOKENS: u32 = 1024;
#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub version: String,
    pub ai_logs: bool,
}

fn build_anthropic_payload_variants(
    model: &str,
    system_prompt: &str,
    messages: &[Value],
    stream: bool,
) -> Vec<Value> {
    let normalized_messages = normalize_anthropic_messages(messages);

    let mut base = json!({
        "model": model,
        "max_tokens": DEFAULT_MAX_TOKENS,
        "system": system_prompt,
        "messages": normalized_messages,
    });

    if stream {
        base["stream"] = Value::Bool(true);
    }

    let mut with_thinking = base.clone();
    with_thinking["max_tokens"] = Value::from(THINKING_MAX_TOKENS);
    with_thinking["thinking"] = json!({
        "type": "enabled",
        "budget_tokens": THINKING_BUDGET_TOKENS
    });

    vec![with_thinking, base]
}

fn normalize_anthropic_messages(messages: &[Value]) -> Vec<Value> {
    messages
        .iter()
        .map(|message| {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user")
                .to_ascii_lowercase();

            let role = if role == "assistant" {
                "assistant"
            } else {
                "user"
            };

            let content = normalize_anthropic_message_content(message.get("content"));

            json!({
                "role": role,
                "content": content,
            })
        })
        .collect()
}

fn normalize_anthropic_message_content(content: Option<&Value>) -> Value {
    let Some(content) = content else {
        return json!([
            {
                "type": "text",
                "text": ""
            }
        ]);
    };

    if let Some(text) = content.as_str() {
        return json!([
            {
                "type": "text",
                "text": text
            }
        ]);
    }

    if let Some(items) = content.as_array() {
        let normalized_items = items
            .iter()
            .map(|item| {
                if item.get("type").and_then(Value::as_str).is_some() {
                    return item.clone();
                }

                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    return json!({
                        "type": "text",
                        "text": text,
                    });
                }

                json!({
                    "type": "text",
                    "text": item.to_string(),
                })
            })
            .collect::<Vec<Value>>();

        return Value::Array(normalized_items);
    }

    json!([
        {
            "type": "text",
            "text": content.to_string()
        }
    ])
}

fn maybe_emit_reasoning_line_break_on_summary_change(
    enabled: bool,
    state: &mut ReasoningStreamState,
    summary_index: Option<i64>,
) {
    let Some(current_index) = summary_index else {
        return;
    };

    if let Some(previous_index) = state.last_summary_index {
        if previous_index != current_index {
            emit_reasoning_double_line_break(enabled, state);
        }
    }

    state.last_summary_index = Some(current_index);
}

fn extract_anthropic_reasoning_delta(event: &Value) -> Option<String> {
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    match event_type.as_str() {
        "content_block_start" => {
            let block_type = event
                .pointer("/content_block/type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();

            if block_type.contains("thinking") || block_type.contains("reasoning") {
                return event
                    .pointer("/content_block/thinking")
                    .and_then(Value::as_str)
                    .or_else(|| event.pointer("/content_block/text").and_then(Value::as_str))
                    .map(ToString::to_string);
            }

            None
        }
        "content_block_delta" => {
            let delta_type = event
                .pointer("/delta/type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();

            if delta_type.contains("thinking") || delta_type.contains("reasoning") {
                return event
                    .pointer("/delta/thinking")
                    .and_then(Value::as_str)
                    .or_else(|| event.pointer("/delta/text").and_then(Value::as_str))
                    .map(ToString::to_string);
            }

            None
        }
        _ => None,
    }
}

fn extract_anthropic_reasoning_index(event: &Value) -> Option<i64> {
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    if event_type == "content_block_start" || event_type == "content_block_delta" {
        return event.get("index").and_then(Value::as_i64);
    }

    None
}

fn extract_anthropic_content_delta(event: &Value) -> Option<String> {
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    match event_type.as_str() {
        "content_block_start" => {
            let block_type = event
                .pointer("/content_block/type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();

            if block_type == "text" {
                return event
                    .pointer("/content_block/text")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
            }

            None
        }
        "content_block_delta" => {
            let delta_type = event
                .pointer("/delta/type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();

            if delta_type == "text_delta" {
                return event
                    .pointer("/delta/text")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
            }

            None
        }
        _ => None,
    }
}

fn extract_anthropic_non_stream_content(response_json: &Value) -> Option<String> {
    let mut text_chunks = Vec::new();

    if let Some(blocks) = response_json.get("content").and_then(Value::as_array) {
        for block in blocks {
            let block_type = block
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();

            if block_type == "text" {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        text_chunks.push(text.to_string());
                    }
                }
            }
        }
    }

    if text_chunks.is_empty() {
        response_json
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
    } else {
        Some(text_chunks.join(""))
    }
}

fn extract_anthropic_non_stream_reasoning(response_json: &Value) -> Option<String> {
    let mut reasoning_chunks = Vec::new();

    if let Some(blocks) = response_json.get("content").and_then(Value::as_array) {
        for block in blocks {
            let block_type = block
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();

            if block_type.contains("thinking") || block_type.contains("reasoning") {
                if let Some(text) = block
                    .get("thinking")
                    .and_then(Value::as_str)
                    .or_else(|| block.get("text").and_then(Value::as_str))
                {
                    if !text.trim().is_empty() {
                        reasoning_chunks.push(text.to_string());
                    }
                }
            }
        }
    }

    if reasoning_chunks.is_empty() {
        None
    } else {
        Some(reasoning_chunks.join("\n"))
    }
}

async fn stream_attempt(
    client: &reqwest::Client,
    endpoint: &str,
    api_key: &str,
    version: &str,
    payload: &Value,
    ai_logs: bool,
) -> Result<String> {
    let mut response = client
        .post(endpoint)
        .header("x-api-key", api_key)
        .header("anthropic-version", version)
        .json(payload)
        .send()
        .await
        .into_diagnostic()?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.into_diagnostic()?;
        return Err(miette::miette!(
            "Streaming request failed with status {}: {}",
            status,
            body
        ));
    }

    let mut pending = String::new();
    let mut model_output = String::new();
    let mut reasoning_stream_state = ReasoningStreamState::default();
    let mut content_stream_state = ContentStreamState::default();

    while let Some(chunk) = response.chunk().await.into_diagnostic()? {
        pending.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(newline_index) = pending.find('\n') {
            let line = pending[..newline_index].trim_end_matches('\r').to_string();
            pending.drain(..=newline_index);

            let line = line.trim();
            if line.is_empty() || !line.starts_with("data:") {
                continue;
            }

            let event_data = line[5..].trim();
            if event_data == "[DONE]" {
                break;
            }

            let event: Value = match serde_json::from_str(event_data) {
                Ok(parsed) => parsed,
                Err(_) => continue,
            };

            if let Some(reasoning_delta) = extract_anthropic_reasoning_delta(&event) {
                maybe_emit_reasoning_line_break_on_summary_change(
                    ai_logs,
                    &mut reasoning_stream_state,
                    extract_anthropic_reasoning_index(&event),
                );
                stream_reasoning_delta_to_stdout(
                    ai_logs,
                    &mut reasoning_stream_state,
                    &reasoning_delta,
                );
            }

            if let Some(content_delta) = extract_anthropic_content_delta(&event) {
                emit_reasoning_line_break(ai_logs, &mut reasoning_stream_state);
                model_output.push_str(&content_delta);
                stream_content_delta_to_stdout(ai_logs, &mut content_stream_state, &content_delta);
            }
        }
    }

    finalize_content_stdout(ai_logs, &mut content_stream_state);
    finalize_reasoning_stdout(ai_logs, &mut reasoning_stream_state);

    if model_output.is_empty() {
        return Err(miette::miette!(
            "Streaming response did not include output text deltas"
        ));
    }

    Ok(model_output)
}

async fn non_stream_attempt(
    client: &reqwest::Client,
    endpoint: &str,
    api_key: &str,
    version: &str,
    payload: &Value,
    ai_logs: bool,
) -> Result<String> {
    let response = client
        .post(endpoint)
        .header("x-api-key", api_key)
        .header("anthropic-version", version)
        .json(payload)
        .send()
        .await
        .into_diagnostic()?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.into_diagnostic()?;
        return Err(miette::miette!(
            "Request failed with status {}: {}",
            status,
            body
        ));
    }

    let response_json = response.json::<Value>().await.into_diagnostic()?;

    let content = extract_anthropic_non_stream_content(&response_json).ok_or_else(|| {
        miette::miette!("Anthropic provider returned an unexpected response payload")
    })?;

    if let Some(reasoning_text) = extract_anthropic_non_stream_reasoning(&response_json) {
        log_agent_progress(
            ai_logs,
            format!("🧠 Model reasoning output:\n{}", reasoning_text),
        );
    }

    Ok(content)
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
        validator_context: &ValidatorContextMap,
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
        let initial_user_prompt = build_initial_user_prompt(
            prompt,
            source_references,
            validator_context,
            permission_prompt,
        );

        let mut messages = vec![serde_json::json!({
            "role": "user",
            "content": initial_user_prompt,
        })];

        run_agent_loop(
            skill,
            &self.endpoint,
            self.ai_logs,
            &canonical_root,
            permission_prompt,
            &mut messages,
            "Anthropic provider",
            |messages| {
                block_on_runtime_aware(async {
                    let client = reqwest::Client::new();

                    if self.ai_logs {
                        let mut last_stream_error: Option<String> = None;
                        let stream_payloads = build_anthropic_payload_variants(
                            &self.model,
                            system_prompt,
                            messages,
                            true,
                        );

                        for (attempt_idx, payload) in stream_payloads.iter().enumerate() {
                            let stream_result = stream_attempt(
                                &client,
                                &self.endpoint,
                                &self.api_key,
                                &self.version,
                                payload,
                                self.ai_logs,
                            )
                            .await;

                            match stream_result {
                                Ok(content) => return Ok(content),
                                Err(error) => {
                                    last_stream_error = Some(error.to_string());
                                    log_agent_progress(
                                        self.ai_logs,
                                        format!(
                                            "⚠️ Streaming attempt {} failed: {}",
                                            attempt_idx + 1,
                                            error
                                        ),
                                    );
                                }
                            }
                        }

                        if let Some(error) = last_stream_error {
                            log_agent_progress(
                                self.ai_logs,
                                format!(
                                    "⚠️ Streaming unavailable, falling back to non-stream request: {}",
                                    error
                                ),
                            );
                        }
                    }

                    let non_stream_payloads =
                        build_anthropic_payload_variants(&self.model, system_prompt, messages, false);
                    let mut last_non_stream_error: Option<String> = None;

                    for (attempt_idx, payload) in non_stream_payloads.iter().enumerate() {
                        let request_result = non_stream_attempt(
                            &client,
                            &self.endpoint,
                            &self.api_key,
                            &self.version,
                            payload,
                            self.ai_logs,
                        )
                        .await;

                        match request_result {
                            Ok(content) => return Ok(content),
                            Err(error) => {
                                last_non_stream_error = Some(error.to_string());
                                log_agent_progress(
                                    self.ai_logs,
                                    format!(
                                        "⚠️ Non-stream attempt {} failed: {}",
                                        attempt_idx + 1,
                                        error
                                    ),
                                );
                            }
                        }
                    }

                    Err(miette::miette!(
                        "All non-stream model request attempts failed for model '{}': {}",
                        self.model,
                        last_non_stream_error.unwrap_or_else(|| "unknown error".to_string())
                    ))
                })
            },
        )
    }
}
