use clap::{Args as ClapArgs, ValueEnum};
use miette::Result;

use crate::config::{ProfileConfig, RootConfig};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ReadScopeArg {
    Workspace,
    Strict,
}

impl ReadScopeArg {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Strict => "strict",
        }
    }
}

#[derive(ClapArgs)]
pub struct Args {
    /// Path where the incremental analysis state JSON will be written.
    #[arg(long, default_value = ".tx3/audit/state.json")]
    pub state_out: String,

    /// Path where the final vulnerability report markdown will be written.
    #[arg(long, default_value = ".tx3/audit/vulnerabilities.md")]
    pub report_out: String,

    /// Path to vulnerability skill definitions.
    #[arg(long, default_value = "skills/vulnerabilities")]
    pub skills_dir: String,

    /// Path where the Aiken AST snapshot JSON will be written.
    #[arg(long, default_value = ".tx3/audit/aiken-ast.json")]
    pub ast_out: String,

    /// Analysis provider: scaffold | heuristic | openai | anthropic | ollama
    #[arg(long, default_value = "scaffold")]
    pub provider: String,

    /// API endpoint override. Default depends on --provider.
    #[arg(long)]
    pub endpoint: Option<String>,

    /// Model override. Default depends on --provider.
    #[arg(long)]
    pub model: Option<String>,

    /// API key environment variable override. Default depends on --provider.
    #[arg(long)]
    pub api_key_env: Option<String>,

    /// Optional reasoning effort hint for OpenAI-compatible providers (e.g. low|medium|high).
    #[arg(long)]
    pub reasoning_effort: Option<String>,

    /// Print chat-style progress of model requests and local tool actions while auditing.
    #[arg(long, default_value_t = false)]
    pub ai_logs: bool,

    /// Regenerate AST even if an up-to-date snapshot is already available.
    #[arg(long, default_value_t = false)]
    pub no_ast_cache: bool,

    /// File read scope for AI-assisted local tool requests: workspace | strict.
    #[arg(long, value_enum, default_value_t = ReadScopeArg::Workspace)]
    pub read_scope: ReadScopeArg,

    /// Ask confirmation before executing each AI-requested local read action.
    #[arg(long, default_value_t = false)]
    pub interactive_permissions: bool,
}

#[allow(unused_variables)]
pub fn run(args: Args, config: &RootConfig, profile: &ProfileConfig) -> Result<()> {
    #[cfg(feature = "unstable")]
    {
        let _ = profile;
        crate::spawn::preflight::run(args, config)
    }
    #[cfg(not(feature = "unstable"))]
    {
        let _ = (args, config, profile);
        Err(miette::miette!(
            "The audit command is currently unstable and requires the `unstable` feature to be enabled."
        ))
    }
}
