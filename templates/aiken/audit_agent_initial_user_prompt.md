Analyze Aiken code for this single vulnerability skill. You are given file references only (no source code inline).

Workspace boundary:
- Workspace root: {{WORKSPACE_ROOT}}
- This is the only workspace you may operate in.
- Do not access or reason about files outside the allowed workspace scope.

Skill:
{{SKILL}}

Referenced Aiken files:
{{SOURCE_REFERENCES}}

Use the referenced files as your starting point. You may read additional files only if they are inside the allowed workspace scope and strictly required to validate the finding.

Allowed read commands: {{ALLOWED_COMMANDS}}
Scope rules:
- {{SCOPE_RULES}}

Return JSON action only.