---
name: build
description: Drive the selected iteration to completion via the build workflow
mode: interactive
---
Run the build workflow on iteration {{ document.id }} ({{ document.title }}).

Invoke the `/build` skill. It targets exactly this iteration:

- id: {{ document.id }}
- path: {{ document.path }}
- status: {{ document.status }}

Lineage (build skill preflight will re-read these in full):
{% for n in context.ancestors %}- {{ n.type }} {{ n.id }}: {{ n.title }}
{% endfor %}
Do not build any other iteration. Stop when this iteration's tasks are
complete and its final review passes.

## Iteration

{{ document.body }}
