Analyze Aiken code for this single vulnerability skill. You are given file references only (no source code inline).

Task-priority rule:
- Treat the Skill block below as the authoritative, task-specific policy for this run.
- If generic wording elsewhere is broader, keep the Skill block as the source of truth for what to detect and what to ignore.

Skill (authoritative context):
--- SKILL START ---
{{SKILL}}
--- SKILL END ---

Referenced Aiken files:
{{SOURCE_REFERENCES}}

Validator context map:
--- CONTEXT MAP START ---
{{VALIDATOR_CONTEXT_MAP}}
--- CONTEXT MAP END ---

Use the referenced files as your starting point. You may read additional files only if they are inside the allowed workspace scope and strictly required to validate the finding.

Execution permissions:
--- PERMISSION PROMPT START ---
{{PERMISSION_PROMPT}}
--- PERMISSION PROMPT END ---

Return JSON action only.