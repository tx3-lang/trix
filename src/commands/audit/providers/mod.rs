mod anthropic;
mod openai;
mod scaffold;
mod shared;

use miette::{Context, IntoDiagnostic, Result};
use std::path::Path;

use super::model::{
    MiniPrompt, PermissionPromptSpec, ProviderSpec, SkillIterationResult, ValidatorContextMap,
    VulnerabilitySkill,
};
use super::Args;

use self::anthropic::AnthropicProvider;
use self::openai::OpenAiProvider;
use self::scaffold::ScaffoldProvider;

const DEFAULT_AI_ENDPOINT: &str = "https://api.openai.com/v1/responses";
const DEFAULT_AI_MODEL: &str = "gpt-4.1-mini";
const DEFAULT_AI_API_KEY_ENV: &str = "OPENAI_API_KEY";
const DEFAULT_ANTHROPIC_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_ANTHROPIC_MODEL: &str = "claude-3-5-haiku-latest";
const DEFAULT_ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_OLLAMA_ENDPOINT: &str = "http://localhost:11434/v1/chat/completions";
const DEFAULT_OLLAMA_MODEL: &str = "llama3.1";

pub trait AnalysisProvider {
    fn provider_spec(&self) -> ProviderSpec;

    fn analyze_skill(
        &self,
        skill: &VulnerabilitySkill,
        prompt: &MiniPrompt,
        source_references: &[String],
        validator_context: &ValidatorContextMap,
        project_root: &Path,
        permission_prompt: &PermissionPromptSpec,
    ) -> Result<SkillIterationResult>;
}

pub fn build_provider(args: &Args) -> Result<Box<dyn AnalysisProvider>> {
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
                ai_logs: args.ai_logs,
                reasoning_effort: args.reasoning_effort.clone(),
                ollama_compat: false,
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
                ai_logs: args.ai_logs,
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
            ai_logs: args.ai_logs,
            reasoning_effort: args.reasoning_effort.clone(),
            ollama_compat: true,
        })),
        value => Err(miette::miette!(
            "Unsupported provider '{}'. Expected one of: scaffold, openai, anthropic, ollama",
            value
        )),
    }
}
