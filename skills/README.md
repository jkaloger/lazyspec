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
- **`/review`** -- two-stage critique of a **document** (conformance to intent + ACs first, quality second) before advancing.

When the next step is the work itself, `/lazy` presents the work plan -- which delivery documents are ready, the order their dependency edges imply, the route -- and stops for explicit approval. On approval it dispatches by count:

- **`/execute`** -- one ready unit. One agent does every task in the breakdown itself, TDD, runs the repo's gate command once, and reports. It opens the document's work-active status and stops there: it does not dispatch, review, commit, or close. (No authorship ceiling -- this is work, not authoring.)
- **`/orchestrate`** -- several ready units. Orders them by their dependency edges and drives the whole chunk: one build agent per unit, one review per unit, one commit per unit, the status transitions, then one comprehensive pass over the combined diff with a single cleanup commit.
- **`/review-work`** -- critique landed code against the document that specified it. Three stages: acceptance conformance, then **convention conformance** against `lazyspec convention --json` (each finding naming the principle or dictum it violates), then quality. Runs blocking-only per unit and comprehensive per chunk.

### Boundaries

Two lines keep the work verbs from overlapping:

- **Document vs code.** `/review` reads document bodies; `/review-work` reads diffs. Convention conformance lives only in `/review-work`.
- **Unit vs chunk.** `/execute` sees exactly one delivery document and never spawns an agent. `/orchestrate` sees a set, spans as many parent documents as the set does, and is the only agent that spawns agents. A unit that will not fit one `/execute` pass is a sizing defect: `/execute` stops at a task boundary and reports it rather than fanning out.

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
| `execute`     | Build one delivery document's breakdown in a single agent pass; terminal, reports and stops       |
| `orchestrate` | Drive a batch of delivery documents to done: order, dispatch, review, commit, close, chunk pass   |
| `review`      | Two-stage critique of a document: conformance to intent + ACs first, quality second               |
| `review-work` | Three-stage critique of landed code: acceptance conformance, convention conformance, quality      |
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
