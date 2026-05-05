use std::process::Command;

use miette::{Context as _, IntoDiagnostic as _, bail};

use crate::commands::audit::Args;
use crate::config::RootConfig;

pub fn run(args: Args, config: &RootConfig) -> miette::Result<()> {
    let tool_path = crate::home::tool_path("preflight")?;

    let mut cmd = Command::new(tool_path);

    // Always-present flags with default values.
    cmd.args(["--state-out", &args.state_out]);
    cmd.args(["--report-out", &args.report_out]);
    cmd.args(["--skills-dir", &args.skills_dir]);
    cmd.args(["--ast-out", &args.ast_out]);
    cmd.args(["--provider", &args.provider]);
    cmd.args(["--read-scope", args.read_scope.as_str()]);

    // Optional string flags.
    if let Some(value) = &args.endpoint {
        cmd.args(["--endpoint", value]);
    }
    if let Some(value) = &args.model {
        cmd.args(["--model", value]);
    }
    if let Some(value) = &args.api_key_env {
        cmd.args(["--api-key-env", value]);
    }
    if let Some(value) = &args.reasoning_effort {
        cmd.args(["--reasoning-effort", value]);
    }

    // Boolean flags.
    if args.ai_logs {
        cmd.arg("--ai-logs");
    }
    if args.no_ast_cache {
        cmd.arg("--no-ast-cache");
    }
    if args.interactive_permissions {
        cmd.arg("--interactive-permissions");
    }

    // --main-source is injected from RootConfig.protocol.main; preflight
    // expects it as the fallback when its own .ak discovery returns empty.
    let main_source = config.protocol.main.to_string_lossy().to_string();
    cmd.args(["--main-source", &main_source]);

    let status = cmd
        .status()
        .into_diagnostic()
        .context("running preflight")?;

    if !status.success() {
        bail!("preflight exited with non-zero status");
    }

    Ok(())
}
