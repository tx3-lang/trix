---
name: trix-cli-conventions
description: Standards and best practices for implementing CLI commands in the Trix project using the View-Model pattern with Askama templates, termimad rendering, and standardized function signatures. Apply when creating new CLI commands or refactoring existing ones.
license: MIT
metadata:
  version: "1.0"
---

# CLI Command Development Best Practices

This document defines the coding standards and architectural patterns for developing CLI commands in the Trix project.

## Module Structure

```
src/commands/<command_name>/
├── mod.rs          # Command enum, shared types, utilities, module exports
└── <subcommand>.rs # Individual subcommand implementations

templates/
└── <command_name>/ # External Askama templates only location
    ├── *.md        # Template files
    └── ...
```

## Standard Run Function Signature

Every command module must expose a `run` function with this exact signature:

```rust
pub fn run(
    args: <ArgsStruct>,
    config: &RootConfig,
    profile: &ProfileConfig,
) -> miette::Result<()>
```

Include all parameters even if unused (e.g., `ListArgs` as empty struct for commands without arguments).

## Template Location Rule

Templates **only** in root `templates/<command>/` directory. Never duplicate templates in `src/commands/<command>/`.

## Template Formatting Guidelines

### Header Hierarchy
- Use `##` for main sections
- Use `###` for subsections
- Maximum two levels of nesting

### Field Pattern
```markdown
- **Label:** `{{ view.field }}`
```

All template variables wrapped in backticks for consistency.

### Value Metadata
```markdown
- **Source:** ({{ value }})
```

Metadata in parentheses without backticks.

### Complete Section Example
```markdown
## Section Name
- **field:** `{{ view.field }}`
- **metadata:** ({{ view.source }})

### Subsection Name
- **field:** `{{ value }}`
{%- if !view.list.is_empty() %}
- **list:**
{%- for item in view.items %}
  - `{{ item.key }}`: `{{ item.value }}`
{%- endfor %}
{%- endif %}
```

### Empty States
```markdown
## Section Name
*(none)*
```

Use italic `*(none)*` for empty collections.

### Arrow Notation
Use `→` for "maps to" relationships:
```markdown
- `profile_name` (built-in) → `network_name` (built-in)
```

### Whitespace Control
- Always use `{%-` and `-%}` (with dash)
- No blank lines between sections
- Indent nested lists with 2 spaces

## Enum Display Pattern

Avoid verbose paths in templates. Implement `Display`:

```rust
impl std::fmt::Display for EnvFileStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Found => write!(f, "found"),
            Self::NotFound => write!(f, "not found"),
            Self::Error(msg) => write!(f, "error: {}", msg),
        }
    }
}
```

Then in template:
```jinja2
{%- if view.status.to_string() == "found" %}
- **Status:** `{{ view.status }}`
{%- else %}
- **Status:** `{{ view.status }}`
{%- endif %}
```

Or use `match` with simple patterns:
```jinja2
{%- match view.status %}}
{%- when EnvFileStatus::Found %}
- **Status:** `found`
{%- when EnvFileStatus::NotFound %}
- **Status:** `not found`
{%- endmatch %}
```

## View Model Pattern

**Materialization**: Build view structs from config
**Rendering**: Pass to template via Askama

```rust
// mod.rs - Shared types
pub struct CommandView {
    pub name: String,
    pub field: String,
}

// show.rs
fn build_view(config: &RootConfig) -> miette::Result<CommandView> {
    Ok(CommandView { ... })
}

fn render_view(view: &CommandView) {
    let markdown = Template::render_view(view);
    MadSkin::default().print_text(&markdown);
}
```

## Askama Template Definition

```rust
#[derive(Template)]
#[template(path = "<command>/<name>.md")]
struct CommandTemplate<'a> {
    view: &'a CommandView,
}

impl<'a> CommandTemplate<'a> {
    fn render_view(view: &'a CommandView) -> String {
        Self { view }
            .render()
            .expect("Template rendering failed")
    }
}
```

## Security: Mask Sensitive Values

```rust
pub(crate) fn mask_value(value: &str) -> String {
    if value.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}...{}", &value[..4], &value[value.len()-4..])
    }
}

pub(crate) fn should_mask_env_var(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("key") 
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("token")
        || lower.contains("private")
}
```

## File Organization

**Code** (`src/commands/<command>/`):
- `mod.rs`: Command enum, shared types, utilities, dispatcher
- `*.rs`: Subcommand implementations with `run()` functions

**Templates** (`templates/<command>/`):
- `*.md`: External Askama templates
- Single source of truth for templates

## Key Principles

1. **Consistency**: Same `run(args, config, profile)` signature everywhere
2. **Single Source**: Templates only in root `templates/` directory
3. **Uniform Formatting**: All values in backticks, metadata in parens
4. **Visual Hierarchy**: `##` → `###` → bullet lists
5. **Type Safety**: Compile-time template checking via Askama
6. **Security**: Mask sensitive values by default
7. **Maps To**: Use `→` notation for relationships
8. **Empty States**: Italic `*(none)*` for empty collections

## Complete Example

```rust
// src/commands/mycommand/mod.rs
use clap::{Args as ClapArgs, Subcommand};

pub mod show;

pub use show::run as run_show;

#[derive(Subcommand)]
pub enum Command {
    Show(ShowArgs),
}

#[derive(ClapArgs)]
pub struct ShowArgs {
    pub name: String,
}

pub fn run(args: Args, config: &RootConfig, profile: &ProfileConfig) -> miette::Result<()> {
    match args.command {
        Command::Show(args) => run_show(args, config, profile),
    }
}

#[derive(Debug, Clone)]
pub struct MyView {
    pub name: String,
    pub items: Vec<String>,
}

impl std::fmt::Display for MyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "status")
    }
}

// src/commands/mycommand/show.rs
pub fn run(args: super::ShowArgs, config: &RootConfig, _profile: &ProfileConfig) -> miette::Result<()> {
    let view = build_view(config, &args.name)?;
    render_view(&view);
    Ok(())
}

// templates/mycommand/show.md
## My Section
- **name:** `{{ view.name }}`
{%- if !view.items.is_empty() %}
- **items:**
{%- for item in view.items %}
  - `{{ item }}`
{%- endfor %}
{%- else %}
*(none)*
{%- endif %}
```

## References

- [Askama Documentation](https://djc.github.io/askama/)
- [Miette Error Handling](https://docs.rs/miette/latest/miette/)
- [Termimad Terminal Markdown](https://github.com/Canop/termimad)
- [Clap Derive Reference](https://docs.rs/clap/latest/clap/)
