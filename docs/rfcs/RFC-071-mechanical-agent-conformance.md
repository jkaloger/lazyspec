---
title: "Mechanical agent conformance"
type: rfc
status: review
author: "Jack Kaloger"
date: 2026-09-07
tags: []
related: []
---

## Summary

Move agent conformance out of skill prose and into mechanism. Five mechanisms: PreToolUse hooks that refuse direct edits to document files and inject the governing documents on code edits, derived routing fields on `show --json`, a per-document `validate --id`, a `role` key on types, and a `next <id>` command that returns the dispatch decision. Then collapse the skills to what only prose can carry: methodology, the human approval stops, and the rationalization table those stops need. This is the escape valve ADR-019 named. The create gate that reopens ADR-033 is ADR-036.

**Amended 2026-09-08 (RFC-068 landed):** the original draft proposed a `guard` command and a `Finding` struct. RFC-068 shipped structured findings first, and its `why` and `config --json` turn out to answer everything the hook needs, so the enforcement layer is now shell in the plugin and no new binary surface. See §Conformance hooks and §Per-document validate.

## Motivation

1. Conformance is enforced by exhortation. Across the skill set roughly 70 `Do NOT` lines exist in HARD-GATE, NEVER and RED-FLAGS blocks; about 8 are backed by a refusal or a validate finding. The one real gate is `update --status` refusing a non-edge move.
2. Every observed failure got a paragraph. Eight fixes were prose-only (ITERATION-023, 199, 202, 399, 400, 401, the `advance` hallucination fix in commit 3a0159d, and the confirm-before-mutate HARD-GATE). The lazy skill restates the boundary rule in its opening line, HARD-GATE, NEVER, Dispatch and Stop-at-Type-Boundary sections and is 218 lines / 20KB. Repetition lowers the salience of every rule it repeats.
3. The observed crossings are the sessions behind ITERATION-399, 400 and 401 (agents misreading the edge table and proposing the wrong side of a crossing) and the `advance` hallucination. The dogfood repo also carries 30 `iterations-need-stories` errors, nearly all authored before the verb skills existed (19 from March 2026, one since ADR-033). The after-the-fact finding has cleared none of them, which is what ADR-033 said would happen. ADR-036 takes that up.
4. Skills make agents compute what the engine already knows. The jq reverse lookup for child types (`traversal::child_types_for`), lifecycle successors (`Lifecycle::targets_from`), the ceiling-to-verb map, and "filter whole-repo validate to the touched doc" are each a paragraph of prose over an existing engine function. ADR-019 called this "the one real per-runtime drift vector".
5. The skills contradict each other. `lazy` says advance without asking (Dispatch) and stop before any advance (HARD-GATE). `lazy` says it owns the draft-to-review advance after authoring; `generate` and `co-write` end without it. Nobody commits on the single-unit path. `create-audit` hardcodes the type `audit` and the relation `related-to`. The NEVER block appears in 10 skills in 6 textual variants; the GitHub-issues block appears in 4, one already missing a line.
6. RFC-068 built a reverse index nothing invokes. `why <path>` answers "which documents govern this file", which is RFC-068's own motivation 1 — an agent editing `src/engine/context/**` guessing at the spec that applies. No skill calls it and no event fires it, so the pins sit unread. The edit itself is the trigger.

## Goals

- An `Edit` or `Write` on a path under any type's `dir`, on `.lazyspec.toml`, or under `.lazyspec/cache/` is refused by a hook before it runs, with a message naming the CLI command to use instead.
- An `Edit` or `Write` on a source file some document governs injects those documents into context before the edit runs, with no skill prose asking for it.
- `show <id> --json` carries `next_statuses`, `child_types` (each with `authorship` and `verb`), `unsatisfied_edges` and `role`, so no skill computes them.
- `validate --id <id>` filters findings to one document, so the post-mutation hook and the skills stop grepping whole-repo output by substring.
- `next <id> --json` returns one dispatch decision: the action, the verb, the target status or crossing, and whether the action needs human approval.
- Every `[[types]]` entry may declare `role`; `config --json` and `show --json` expose it; `next` uses it to pick the work route and the fix-doc type.
- A baseline eval exists before any skill is collapsed: `claude plugin eval` cases that reproduce the documented failures (cross a boundary unasked, edit a doc file directly, advance without approval) fail against the current skills where they should, and pass against the collapsed ones.
- `lazy` under 80 lines. No NEVER block appears in more than one skill. No skill restates a rule the binary refuses. Contradictions in Motivation 5 resolved with one owner each.
- Skill pinning tests in `src/cli/skills.rs` updated; a new test asserts no skill contains the jq reverse-lookup block.

## Non-goals

- Status-conditioned create gating. ADR-033 stands on that point; the structural gate is ADR-036.
- Enforcing authorship ceilings in the binary. Ceiling stays config data read by `next`.
- A `--ready` or dependency-ordered list for `/orchestrate`. Follow-up once `next` lands.
- Detecting subagent spawns, `sleep`, or commits. Runtime concerns; out of the binary's reach.
- Changing the `agents-md` runtime beyond regenerating from the collapsed skills.
- Removing `--body-file`. It stays; skills stop pretending it was removed.
- A `guard` command. See §Conformance hooks: the classification is three lines of shell over `config --json`, and a command earns its place when a second runtime asks for it.
- Judging pin staleness on edit. The hook reports which documents govern a file, not whether they still describe it. That is RFC-069.

## Design

### Conformance hooks

Claude Code hooks expose five mechanisms. Four are useful here:

| Pattern | Mechanism | Effect |
| --- | --- | --- |
| Refuse | `permissionDecision: "deny"` + reason | Tool never runs; the reason reaches the agent |
| Inject | `hookSpecificOutput.additionalContext` | Text enters model context; the tool proceeds normally |
| Escalate | `permissionDecision: "ask"` + reason | Forces the human prompt even for a pre-approved tool |
| Silence | exit 0, no output | The default and the majority case |

`permissionDecision: "allow"` is the fifth and is never emitted. It *bypasses* the permission prompt, so attaching context through it would silently auto-approve edits the user would otherwise be asked about. Injection omits the decision key entirely and leaves the permission flow untouched.

**Edit guard.** One `PreToolUse` entry matching `Edit|Write|MultiEdit`, one script, one decision per path. It reads `tool_input.file_path`, relativises against `$CLAUDE_PROJECT_DIR`, and takes the first branch that matches:

- under `.lazyspec/cache/` -> refuse, "cache mirrors are read-only; regenerated by `lazyspec fetch`"
- `.lazyspec.toml` -> refuse, "use `lazyspec config set <key> <value>`"
- `*.md` under any dir in `lazyspec config --json | jq -r '.types[].dir'` -> refuse, "`<ID>` is a lazyspec document. Use `lazyspec update <ID> --body`"
- matched by `lazyspec why <path> --json` -> inject `Governed by: SPEC-002 Document Store (draft) via src/engine/store/**`
- otherwise -> silent

Type dirs come from config rather than a hardcoded `docs/`, so the hook holds in any lazyspec repo. The ID for the refusal message comes from the filename. No allowlist; a human who wants to hand-edit uses their editor, not the agent.

**Post-mutation validate.** One `PostToolUse` entry matching `Bash`. When the command was a lazyspec mutation naming an ID, inject that document's findings. This is the caller that makes `validate --id` worth building: without it the hook filters whole-repo output on a path substring, and `STORY-23` matches `STORY-237`.

**Commit gate.** A `PreToolUse` entry matching `Bash` that escalates `git commit` when `validate` reports errors. Documented as an opt-in pattern, not shipped enabled: this repo carries 56 standing errors, so a gate here fires on every commit and trains the agent to dismiss it. It is honest only against a clean baseline.

**Bash writes.** The edit guard covers `Edit|Write|MultiEdit` and nothing else. A `PreToolUse` entry on `Bash` refusing a command that names a path under a typed `dir` alongside a write (`>`, `>>`, `sed -i`, `tee`, `mv`, `cp`) is the one piece with no existing command behind it. Heuristic, not a parser; it catches the routes harness modes that prefer Bash for edits actually use.

All of this ships in `hooks/hooks.json` and `hooks/` beside it, distributed by the existing `.claude-plugin` manifest that already carries the `convention --preamble` hook. The envelope format stays in the plugin: `tool_name`, `tool_input` and `hookSpecificOutput` are Claude Code's wire protocol, and a binary that parsed them would ship a release every time Anthropic renamed a field, while every other runtime paid for a schema it does not speak. Principle 1. Other runtimes get `config --json` and `why --json` and wire their own event.

### Derived fields on show

`DocDetail` (the `show --json` shape) gains four fields, all computed, none stored:

- `role`: the type's `role` value or `null`.
- `next_statuses`: `lifecycle.targets_from(status)`, ordered as declared. Empty at a terminal status.
- `child_types`: `traversal::child_types_for(type)`, each entry `{type, authorship, verb}` where `verb` is the ceiling verb (`human` -> `scaffold`, `assisted` -> `co-write`, `generated` -> `generate`).
- `unsatisfied_edges`: the required edges this document sits on the `from` side of with no satisfying link, each `{edge, required, via, candidates}` where `candidates` is the `to` set. `["*"]` stays `["*"]`; never expanded.

`context <id> --json` `target` reuses the same struct, so it carries them too. TUI detail pane and web doc page show `next_statuses` and `child_types` as rows; they already render `related`.

RFC-068 added `governs` and `reviewed` to the same struct. No conflict; these are computed, those are frontmatter.

### Per-document validate

**Amended 2026-09-08 (RFC-068 landed):** this section proposed a `Finding` struct emitted as a new `findings` array beside flat string `errors` and `warnings`. RFC-068 decision 7 took the breaking change instead: `errors` and `warnings` *are* arrays of objects, each with a `rule` slug, a `message` and rule-specific fields, in both `validate --json` and `status --json`. The struct, the slug and the TUI and web panel updates are done. What remains is the filter.

`validate --id <ID>` restricts output to the findings for one document. Findings key on `path`, a document path, not on an ID — `--id` resolves the ID through the store's existing lookup and compares paths. It does not add a field, and it does not substring-match, which is the whole point of building it rather than leaving the jq in the hook.

Severity stays positional: a finding is an error or a warning by which array it lands in. No per-finding `severity` key.

### Type role

`[[types]] role = "planning" | "delivery" | "fix" | "record"`. Optional. `config --json` exposes it. `next` uses `delivery` to route to work and `fix` (falling back to `delivery`) for the bug-triage branch. `config add-type` prompts for it. Shipped default sets `rfc`/`spec`/`story` planning, `iteration` delivery, `bug` fix, `adr`/`audit`/`convention`/`dictum` record.

### Next

`lazyspec next <id> --json` returns:

```
{ doc, type, status, role,
  action: "advance" | "author" | "review" | "review-work" | "work" | "boundary" | "terminal",
  verb: "/advance" | "/scaffold" | "/co-write" | "/generate" | "/review" | "/review-work" | "/execute" | null,
  next_status: string | null,
  crossing: { edge, types: [{type, verb}] } | null,
  requires_approval: bool }
```

Decision order, first match wins:

1. Body is unwritten (equal to the rendered template, or containing only headings and guidance comments) and an authoring verb is permitted: `author` at the ceiling verb. `requires_approval: true`.
2. Status has a successor whose name is the type's review state: `advance` into it. `requires_approval: false`. This is the one mutation that runs without a stop; the HARD-GATE lists it as such.
3. Status is the review state: `review`. `requires_approval: false`.
4. `role = delivery` and status is the work-ready state: `work`, verb `/execute`, `requires_approval: true`.
5. `role = delivery` and status is the work-active state: `review-work`, `requires_approval: false`.
6. `child_types` non-empty: `boundary`, with the crossing filled from `child_types` and the first unsatisfied edge. `requires_approval: true`. A planning document at its work-ready state reaches this rule because rule 4 needs `role = delivery`.
7. Otherwise `terminal`.

Review, work-ready and work-active states are the lifecycle's second, third and fourth states unless the type names them under `lifecycle.roles`; a config extension deferred until a lifecycle needs it.

Human output is one line per field. The TUI gets a `next` row in the detail pane; the web doc page shows it beside status.

### Skill collapse

After the mechanisms land, each skill becomes methodology plus the stops that stay human:

- `lazy`: run `next`, present the decision, stop when `requires_approval`, dispatch the verb, run `validate --id`. The HARD-GATE keeps the two human stops and names the exception: approval before the first `create`, `link`, `/execute` or `/orchestrate` of a turn; never crossing a boundary unasked; the into-review advance runs without a stop. The RED-FLAGS table stays, trimmed to the four rows about approval ("pre-authorized", "said use /lazy", "the fix is obvious", "inline is exempt"). The "nothing refuses the create" row goes; the edit guard and ADR-036 make it moot. The jq block, the row-direction lecture, the multi-hop enumeration rules and the "common failure" paragraphs go: `next` computes what they explain.
- `lazy`'s frontmatter description becomes triggers only: "Use as the entry point for any work, including bugs, defects and unexpected behaviour." The workflow summary moves out of the description; agents that read only the description should learn when to fire it, not a stale outline of what it does.
- Authoring verbs: `generate` and `co-write` own the draft-to-review advance. `lazy` stops saying it does.
- `execute` owns the work-open advance and says so once. `lazy` routes the single-unit path through `review-work` then `/advance` then one commit; that commit is named.
- `create-audit` reads type and relation from config like every other verb.
- The NEVER and BODY-CONTENT blocks go from every skill. The hook's refusal message names the replacement command; a paragraph repeating it is the drift vector the hook removes. `lazy` keeps one line: "Always `--json`."
- No skill gains a "run `why` before editing code" instruction. The edit guard injects it, which is the same argument one rung along: prose that duplicates a mechanism is the thing that drifts.
- `--body-file` is mentioned as the file route it is.

`skills/README.md` drops the drift narrative and says: the binary answers, the hook delivers, the skill decides.

## Interfaces

```rust
@draft pub struct ChildType { pub r#type: String, pub authorship: Authorship, pub verb: &'static str }
@draft pub struct UnsatisfiedEdge { pub edge: String, pub required: Severity, pub via: Vec<String>, pub candidates: Vec<String> }
@draft pub enum Role { Planning, Delivery, Fix, Record }
@draft pub struct Next { /* fields per Design */ }
@draft pub fn next(store: &Store, config: &Config, id: &str) -> Result<Next>;
```

CLI: `next <ID> [--json]`, `validate [--id <ID>] [--json] [--warnings]`. `show` and `context` gain fields only. `[[types]] role`.

No new engine surface for the hooks. They compose `config --json`, `why --json` and `validate --json`, all shipped.

## Decisions (ADRs to emit)

- Routing moves into the binary. Amends ADR-019: the data contract stays, the "no brain command" clause is retired now the prose derivation has proved fragile.
- Enforcement lives in the plugin, not the binary. The hook scripts hold Claude Code's envelope format; lazyspec holds the questions. Rejected: a `guard` command classifying paths (the classification is three lines over `config --json`, and one implementation does not need one — principle 6), and a `guard --hook` mode reading the hook envelope on stdin (a vendor wire protocol compiled into a tool whose job is structured markdown — principle 1).
- Structural create gate. ADR-036, authored alongside this RFC, supersedes ADR-033: `create` for a type on the `from` side of a required chain edge needs `--parent` or `--orphan`.
- Type role is a first-class config key, not inferred from `intent` prose.

## Stories

1. Baseline eval: `claude plugin eval` cases for boundary crossing, direct doc edit, advance without approval, recorded against current skills. No dependencies. Ships first.
2. Edit guard hook: refuse document, config and cache paths; inject governing documents on code paths. Shell and jq in `hooks/`, no Rust. No dependencies.
3. Derived fields on `show`/`context`. No dependencies.
4. `validate --id`. No dependencies.
5. `role` on types, default config, `add-type` prompt. No dependencies.
6. `next`. Depends on 3 and 5.
7. Post-mutation validate hook and the Bash-write guard. Depends on 4.
8. Skill collapse, pinning tests, eval re-run. Depends on 1, 2, 6, 7.
9. Structural create gate. Depends on ADR-036 accepted; independent of the rest.

## Risks and tradeoffs

- The hooks are Claude Code specific, and now there is no `guard` command for another runtime to fall back on. Accepted: the questions are all `--json` commands any runtime can call, and Claude Code is where every documented failure happened. A second runtime with a different event model is what would justify lifting classification into the engine.
- Injection fires per edit, so five edits to one file inject the same line five times. Accepted: it is one line, and a session cache is a hook change, not a design change.
- Injection reports governance, not currency. A document whose `reviewed` sha is ancient still reads as authoritative in the injected line. Accepted for this RFC; RFC-069 is the answer and the line gains a staleness marker when it lands.
- The Bash guard is a heuristic. A write it does not recognise gets through; a read it misreads as a write is refused and the agent reroutes through the CLI. Accepted: false refusals cost a retry, false passes cost what they cost today.
- `next` hardcodes a lifecycle shape (review state second, work states third and fourth) until `lifecycle.roles` exists. Types with other shapes fall to `advance`/`terminal`, never to a wrong mutation. Accepted; the extension is additive.
- Approval stops remain prose. A binary cannot know whether a human nodded. `requires_approval` makes the stop a data point the skill points at, and the trimmed RED-FLAGS table stays because the documented approval violations are the kind where the agent knows the rule and argues past it; data alone does not survive that.
