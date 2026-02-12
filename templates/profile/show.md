## Profile
- **name**: `{{ view.name }}`
- **Source:** ({{ view.source }})

## Network
- **name:** `{{ view.network.name }}`
- **Source:** ({{ view.network.source }})
- **Is Testnet:** {{ view.network.is_testnet }}

### TRP Configuration
- **Source:** ({{ view.network.trp.url_source }})
- **URL:** {{ view.network.trp.url }}
{%- if !view.network.trp.headers.is_empty() %}
- **Headers:**
{%- for (key, value) in view.network.trp.headers %}
  - `{{ key }}`: {{ value }}
{%- endfor %}
{%- endif %}

### U5C Configuration
- **Source:** ({{ view.network.u5c.url_source }})
- **URL:** {{ view.network.u5c.url }}
{%- if !view.network.u5c.headers.is_empty() %}
- **Headers:**
{%- for (key, value) in view.network.u5c.headers %}
  - `{{ key }}`: {{ value }}
{%- endfor %}
{%- endif %}

## Identities
{%- if view.identities.is_empty() %}
*(none)*
{%- else %}
{%- for identity in view.identities %}
- {{ identity.name }} ({{ identity.kind }})
{%- endfor %}
{%- endif %}

## Environment File:
- **location**: {{ view.env_file.file_name }}
{%- match view.env_file.status %}
{%- when crate::commands::profile::EnvFileStatus::Found %}
{%- if view.env_file.variables.is_empty() %}
- **Status:** found (empty)
{%- else %}
- **Status:** found
- **Variables:**
{%- for (key, value) in view.env_file.variables %}
  - `{{ key }}`: {{ value }}
{%- endfor %}
{%- endif %}
{%- when crate::commands::profile::EnvFileStatus::NotFound %}
- **Status:** not found
{%- when crate::commands::profile::EnvFileStatus::Error with (msg) %}
- **Status:** error - {{ msg }}
{%- endmatch %}
