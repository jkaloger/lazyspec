## Skills

These skills can complement AI driven workflows using lazyspec. They guide an AI agent through a structured documentation lifecycle: propose a design, slice it into stories, plan iterations, build, and review.

### The generic verb set

The verbs are **DAG-agnostic**: each one acts on a document *type* passed as a parameter and read from `lazyspec config --json` at runtime. None of them bake in a specific type name. The same prose works for any configured DAG -- the shipped default (a chain among types named `rfc`, `story`, `iteration`) is just one config among many.

`/lazy` is the entry router. It reads the configured DAG (`config --json`), what exists (`status --json`), and the chain around your position (`context --json`), then dispatches the right verb. It advances within the current document automatically, but **stops at type boundaries** -- it never auto-creates a child of a different type, even when a gate has cleared. Crossing a type boundary is always human-initiated.

From there `/lazy` dispatches:

- **Authoring** (ceiling-ordered `scaffold < co-write < generate`): a type's `authorship` value is the ceiling -- the highest authoring verb permitted.
  - `/scaffold` -- AI creates the file, frontmatter, and links; the human writes the body. Never refuses on ceiling grounds (it is the floor).
  - `/co-write` -- AI proposes a draft body, the human edits, iterate. Refuses when the type's ceiling is `human`.
  - `/generate` -- AI writes the full body from context, then asks for review. Permitted only when the type's ceiling is `generated`.
- **`/advance`** -- move a document to its next status along the type's `lifecycle` edges, checking gates. Status only; never spawns children.
- **`/execute`** -- carry out the work a delivery document describes, against its task breakdown and ACs. (No authorship ceiling -- this is work, not authoring.)
- **`/review`** -- two-stage critique (conformance to intent + ACs first, quality second) before advancing.

`resolve-context` folds into `context --json`: `/lazy` reads the chain from the CLI rather than calling a separate skill.

`create-audit` runs independently of the main pipeline. It produces findings that the user can triage.

`configure-type` also runs independently of the main pipeline -- it is a setup/meta action, not a lifecycle step. It interviews the user to co-author one custom document type's methodology, writes its enriched template, and records the `[[types]]` config via the config-write CLI. One type per run.

### Reference

| Skill         | Description                                                                                       |
| ------------- | ------------------------------------------------------------------------------------------------- |
| `lazy`        | Entry router. Reads the DAG and your position from config/status/context, dispatches the right verb, stops at type boundaries |
| `scaffold`    | Create a document's file, frontmatter, and links; hand the body back to the human (authorship floor) |
| `co-write`    | Propose a draft body, the human edits, iterate; refuses for `human`-ceiling types                 |
| `generate`    | Write the full body from context, then request review; permitted only for `generated`-ceiling types |
| `advance`     | Move a document to its next lifecycle status, checking gates; status only, no child spawning      |
| `execute`     | Carry out the work a delivery document describes against its task breakdown                        |
| `review`      | Two-stage critique: conformance to intent + ACs first, quality second                             |
| `create-audit`| Run a criteria-based review and document findings for user triage                                 |
| `configure-type`| Interview the user to co-author one custom document type; write its template and `[[types]]` config via the config-write CLI (runs independently of the pipeline) |

### Usage

Add the skills directory to your Claude Code settings or copy individual skills into your project's `.claude/skills/` directory. The `/lazy` skill will handle routing from there.

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
