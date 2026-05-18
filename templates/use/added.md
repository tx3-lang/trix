## Interface {% if view.replaced %}replaced{% else %}added{% endif %}{% if view.dry_run %} (dry run — trix.toml not modified){% endif %}
- **alias:** `{{ view.alias }}`
- **ref:** `{{ view.reference }}`
- **digest:** `{{ view.digest }}`
- **cache:** `{{ view.cache_path }}`

## Transactions
{%- if view.transactions.is_empty() %}
*(none)*
{%- else %}
{%- for tx in view.transactions %}
- `{{ tx }}`
{%- endfor %}
{%- endif %}
