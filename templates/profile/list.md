## Available Profiles

{%- for item in view.profiles %}
- `{{ item.name }}` ({{ item.source }}) → {{ item.network }} ({{ item.network_source }})
{%- endfor %}

## Available Networks

{%- for item in view.networks %}
- {{ item.name }} ({{ item.source }})
{%- endfor %}
