## Skills

These skills define a spec-driven workflow using lazyspec. They guide an AI agent through a structured documentation lifecycle: propose a design, lock down contracts in specs, plan implementation, build, and review.

### Workflow

`lazy` is the entry point. It inspects existing documents and routes to the right skill.

A typical flow looks like:

```
/lazy -> create-spec -> create-plan -> build -> review-plan
```

For heavy or cross-cutting work, an RFC precedes the spec:

```
/lazy -> write-rfc -> create-spec -> create-plan -> build -> review-plan
```

`/resolve-context` can be used at any point to gather the full document chain (plan -> spec -> RFC if linked) before starting work. When continuing in the same session (e.g. after `/create-spec`), you already have context and can skip directly to `/create-plan`.

`create-audit` runs independently of the main pipeline. It produces findings that the user can triage into plans.

### Hierarchy

| Document | Purpose |
| -------- | ------- |
| RFC | Design intent and motivation (optional, for heavy/cross-cutting work) |
| Spec | Technical contracts: data models, API surface, validation, error handling, edge cases |
| Plan | Task breakdown and test plan for implementing a spec |
| ADR | Architectural decisions, linked to any document |

### Reference

| Skill | Description |
| ----------------- | ---------------------------------------------------------------------------- |
| `lazy` | Detect existing RFCs, Specs, and Plans to determine the right starting point |
| `write-rfc` | Create an RFC with design intent, interface sketches, and derived Specs |
| `create-spec` | Create a Spec with contract sections, standalone or linked to an RFC |
| `create-plan` | Create a Plan with task breakdown and test plan against a Spec |
| `build` | Execute a Plan's task breakdown, dispatching per-task with review gates |
| `review-plan` | Two-stage review: spec contract compliance first, code quality second |
| `resolve-context` | Gather the full document chain for an agent before it begins work |
| `create-audit` | Run a criteria-based review and document findings for user triage |

### Usage

Add the skills directory to your Claude Code settings or copy individual skills into your project's `.claude/skills/` directory. The `lazy` skill will handle routing from there.

## License

Some skills adapted from [obra/superpowers](https://github.com/obra/superpowers).

MIT License

Copyright (c) 2025 Jesse Vincent

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
