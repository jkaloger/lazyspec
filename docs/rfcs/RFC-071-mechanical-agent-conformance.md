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

Move agent conformance out of skill prose and into mechanism. Five mechanisms: PreToolUse hooks that refuse direct edits to document files and inject the governing documents on code edits, derived routing fields on `show --json`, a per-document `validate --id`, an optional per-state `actions` map on lifecycles, and a `next <id>` command that returns the dispatch decision. Then collapse the skills to what only prose can carry: methodology, the human approval stops, and the rationalization table those stops need. This is the escape valve ADR-019 named. The create gate that reopens ADR-033 is ADR-036.

**Amended 2026-09-08 (RFC-068 landed):** the original draft proposed a `guard` command and a `Finding` struct. RFC-068 shipped structured findings first, and its `why` and `config --json` turn out to answer everything the hook needs, so the enforcement layer is now shell in the plugin and no new binary surface. See §Conformance hooks and §Per-document validate.

**Amended 2026-09-08 (review):** the derived fields are injected keys on the `show --json` object, not fields on a struct, and `context --json` is out of scope. See §Derived fields on show.

**Amended 2026-09-12 (review):** `role` and `lifecycle.roles` are both dropped in favour of one optional per-state `actions` map. lazyspec is a graph of context whose types, edges and lifecycles are the user's to name — a RAID register, an ADR-only repo, a type nobody here has imagined. A fixed role vocabulary asks every such user to translate their graph into four words this RFC chose, and three fixed state-role names ask the same in softer form. `next` now reads what the config says to do at a state, and otherwise reads the graph. See §Lifecycle actions and §Next.

## Motivation

1. Conformance is enforced by exhortation. Across the skill set roughly 70 `Do NOT` lines exist in HARD-GATE, NEVER and RED-FLAGS blocks; about 8 are backed by a refusal or a validate finding. The one real gate is `update --status` refusing a non-edge move (`src/engine/ops/update.rs:27`).
2. Every observed failure got a paragraph. Eight fixes were prose-only (ITERATION-023, 199, 202, 399, 400, 401, the `advance` hallucination fix in commit 3a0159d, and the confirm-before-mutate HARD-GATE). The lazy skill restates the boundary rule in its opening line, HARD-GATE, NEVER, Dispatch and Stop-at-Type-Boundary sections and is 219 lines / 20KB. Repetition lowers the salience of every rule it repeats.
3. The observed crossings are the sessions behind ITERATION-399, 400 and 401 (agents misreading the edge table and proposing the wrong side of a crossing) and the `advance` hallucination. The dogfood repo also carries 30 `iterations-need-stories` errors, nearly all authored before the verb skills existed (19 from March 2026, one since ADR-033). The after-the-fact finding has cleared none of them, which is what ADR-033 said would happen. ADR-036 takes that up.
4. Skills make agents compute what the engine already knows. The jq reverse lookup for child types (`traversal::child_types_for`), lifecycle successors (`Lifecycle::targets_from`), the ceiling-to-verb map, and "filter whole-repo validate to the touched doc" are each a paragraph of prose over an existing engine function. ADR-019 called this "the one real per-runtime drift vector".
5. The skills contradict each other. `lazy` gates the *first* graph-mutating dispatch of a turn behind explicit approval (HARD-GATE, `skills/lazy/SKILL.md:90`) and separately makes the draft-to-review advance automatic after authoring (`:169`) — the second is a mutation the first would stop. `lazy` claims that into-review advance; `generate` (`:56`) and `co-write` end at `/review` without it. Nobody commits on the single-unit path. `create-audit` hardcodes the type `audit` and the relation `related-to` (`skills/create-audit/SKILL.md:50-51`). The NEVER block appears in 10 skills in 6 textual variants; the GitHub-issues block appears in 4, one already missing a line.
6. RFC-068 built a reverse index nothing invokes. `why <path>` answers "which documents govern this file", which is RFC-068's own motivation 1 — an agent editing `src/engine/context/**` guessing at the spec that applies. No skill calls it and no event fires it, so the pins sit unread. The edit itself is the trigger.

## Goals

- An `Edit` or `Write` on a path under any type's `dir`, on `.lazyspec.toml`, or under `.lazyspec/cache/` is refused by a hook before it runs, with a message naming the CLI command to use instead.
- An `Edit` or `Write` on a source file some document governs injects those documents into context before the edit runs, with no skill prose asking for it.
- `show <id> --json` carries `next_statuses`, `child_types` (each with `authorship` and `verb`) and `unsatisfied_edges`, so no skill computes them.
- `validate --id <id>` filters findings to one document across every rule's document-path fields, so the post-mutation hook and the skills stop grepping whole-repo output by substring.
- `next <id> --json` returns one dispatch decision: the action, the verb, the target status or crossing, and whether the action needs human approval.
- Every `[[types.lifecycle]]` may declare `actions`, mapping a state to a verb string and an approval flag. `config --json` exposes it. `next` reports what it finds there and interprets none of it; where nothing is declared it reads the graph and never guesses meaning.
- A baseline eval exists before any skill is collapsed: `claude plugin eval` cases that reproduce the documented failures (cross a boundary unasked, edit a doc file directly, advance without approval) fail against the current skills where they should, and pass against the collapsed ones.
- `lazy` under 80 lines. No NEVER block appears in more than one skill. No skill restates a rule the binary refuses. Contradictions in Motivation 5 resolved with one owner each.
- Skill pinning tests in `src/cli/skills.rs` updated; a new test asserts no skill contains the jq reverse-lookup block.

## Non-goals

- Status-conditioned create gating. ADR-033 stands on that point; the structural gate is ADR-036.
- Enforcing authorship ceilings in the binary. Ceiling stays config data read by `next`.
- A `--ready` or dependency-ordered list for `/orchestrate`. Follow-up once `next` lands.
- Detecting subagent spawns, `sleep`, or commits. Runtime concerns; out of the binary's reach.
- Changing the `agents-md` runtime beyond regenerating from the collapsed skills.
- Derived fields on `context --json`. Its `target` is a path string, and widening it to an object is a breaking change to a key the TUI, web view and skills all read, for fields the caller can get from `show`. See §Derived fields on show.
- Removing `--body-file`. It stays. No skill mentions it today, so the collapse gains one line naming it as the file route — an addition, not a correction.
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

`show --json` is not a struct. It is assembled at `src/cli/show.rs:240`: `doc_to_json_with_family(doc, store)` serialises the document's frontmatter, then `body`, `comments` and `staleness` are injected as keys (`:247-249`). The four derived fields are injected the same way `staleness` is, not added to `DocMeta` — `DocMeta` is the frontmatter shape, and derived data does not belong on it.

- `next_statuses`: `lifecycle.targets_from(status)` (`src/engine/config.rs:689`), ordered as declared. Empty at a terminal status.
- `child_types`: `traversal::child_types_for(type)` (`src/engine/traversal.rs:119`), each entry `{type, authorship, verb}` where `verb` is the ceiling verb (`human` -> `scaffold`, `assisted` -> `co-write`, `generated` -> `generate`).
- `unsatisfied_edges`: the required edges this document sits on the `from` side of with no satisfying link. Shape is `ValidationIssue::UnsatisfiedEdge`'s (`src/engine/validation.rs:39`) — `edge_name`, `from_type`, `to`, `via` — serialised from the same variant validation already emits, not a parallel struct. Principle 6, and Motivation 4's own argument one rung further in. `["*"]` stays `["*"]`; never expanded.

RFC-068's `governs` and `reviewed` are `DocMeta` frontmatter fields carried by `doc_to_json_with_family`. No conflict, but no precedent either: `staleness` is the precedent these four follow.

`context <id> --json` does not carry them. Its `target` is a path string (`src/cli/context.rs:56`); see Non-goals.

TUI detail pane and web doc page show `next_statuses` and `child_types` as rows; they already render `related`.

### Per-document validate

**Amended 2026-09-08 (RFC-068 landed):** this section proposed a `Finding` struct emitted as a new `findings` array beside flat string `errors` and `warnings`. RFC-068 decision 7 took the breaking change instead: `errors` and `warnings` *are* arrays of objects, each with a `rule` slug, a `message` and rule-specific fields, in both `validate --json` and `status --json`. The struct, the slug and the TUI and web panel updates are done. What remains is the filter.

`validate --id <ID>` restricts output to the findings for one document. `--id` resolves the ID to a path through the store's existing lookup, then keeps a finding when that path appears in any of the finding's document-path fields. Those fields vary by rule, and the filter must name all of them:

| Field | Rules carrying it |
| --- | --- |
| `path` | `orphan-ref`, `rejected-parent`, `superseded-parent`, `stale`, `undeclared-attribute`, `unsatisfied-edge` |
| `parent` | `rejected-parent`, `superseded-parent` |
| `source`, `target` | `broken-link` |
| `paths` (a list) | `duplicate-id` |

Matching `path` alone would drop `broken-link` and `duplicate-id` entirely — the two rules most likely to fire immediately after a `link`, which is exactly what the post-mutation hook exists to catch. A rule that later adds a document-path field extends the filter; the test pinning this asserts every rule variant is reachable through `--id`.

It does not add a field, and it does not substring-match, which is the whole point of building it rather than leaving the jq in the hook.

Severity stays positional: a finding is an error or a warning by which array it lands in. No per-finding `severity` key.

### Lifecycle actions

**Amended 2026-09-12 (review):** this section replaces §Type role. The earlier draft added `[[types]] role = "planning" | "delivery" | "fix" | "record"` and `[[types.lifecycle]] roles = { review, work_ready, work_active }`. Both are dropped. A closed four-value role vocabulary is a guess about what a user's documents mean, and lazyspec does not know that — a RAID register's `risk`, a repo of nothing but ADRs, and a type nobody here has imagined are all valid graphs, and none of them translate into planning/delivery/fix/record. `roles` was the same imposition in softer form: three state names this RFC chose, which a register with no review state cannot fill in. What a document *is* stays the user's word; what `next` needs is what to do at a state, and the user can say that directly.

`[[types.lifecycle]] actions = { <state> = { verb = "<string>", approve = <bool> } }`. Optional, per state, keys validated against the declared `states`. `verb` is an opaque string — lazyspec never interprets it, it reports it. `approve` defaults to `false`. `config --json` exposes the map.

Shipped default, per type, in the vocabulary this repo already uses:

| Type | State | verb | approve |
| --- | --- | --- | --- |
| every type | `review` | `/review` | false |
| `iteration` | `accepted` | `/execute` | true |
| `iteration` | `in-progress` | `/review-work` | false |
| `bug` | `triaged` | `/execute` | true |
| `bug` | `in-progress` | `/review-work` | false |

Every other state is unannotated and falls to the graph rules below. A project that annotates nothing still gets useful answers; a project with a type this RFC has never heard of names its own verb and `next` reports it without knowing what it means.

### Next

`lazyspec next <id> --json` returns:

```
{ doc, type, status,
  action: "author" | "declared" | "boundary" | "advance" | "terminal",
  verb: string | null,
  next_status: string | null,
  crossing: { types: [{type, verb}], unsatisfied_edges: [...] } | null,
  requires_approval: bool }
```

`action` names where the answer came from, not what the answer means: `declared` is the config's `actions` entry, `boundary` and `advance` are read off the graph, `author` off the authorship ceiling. Skills dispatch `verb` and respect `requires_approval`; they do not branch on `action`.

Decision order, first match wins:

1. Body is unwritten (equal to the rendered template, or containing only headings and guidance comments) and an authoring verb is permitted: `author` at the ceiling verb, `requires_approval: true`. This is first because it is a fact about the document rather than a reading of the graph — a state annotated `/execute` should not execute an empty document.
2. The state has an `actions` entry: `declared`, carrying that entry's `verb` and `approve`.
3. The state has at least one explicitly-declared out-edge and the type has child types: `boundary`, `requires_approval: true`. `crossing.types` lists *every* child type with its ceiling verb and does not pick one — picking would need to know what the children mean. `crossing.unsatisfied_edges` reports which required edges are currently unmet. The out-edge condition is what keeps a terminal state from proposing children forever.
4. Exactly one explicitly-declared out-edge: `advance` into it, `requires_approval: false`. Wildcard edges (`from = "*"`, e.g. `-> superseded`) are reachable from everywhere and so are never "the" next move; they are excluded from this count. Without that exclusion the shipped config has almost no single-successor state at all.
5. Otherwise `terminal`, `verb: null`.

The binary names two verbs of its own and no more: the authorship ceiling map (`human -> /scaffold`, `assisted -> /co-write`, `generated -> /generate`) and `/advance`. Both are moves lazyspec genuinely performs. `/review`, `/execute` and `/review-work` are judgements about someone's documents and live in their config. If a second runtime ever wants its own authoring verbs, the ceiling map lifts to config then — one use does not need the indirection today (principle 6).

Human output is one line per field. The TUI gets a `next` row in the detail pane; the web doc page shows it beside status.

### Skill collapse

After the mechanisms land, each skill becomes methodology plus the stops that stay human:

- `lazy`: run `next`, present the decision, stop when `requires_approval`, dispatch the verb, run `validate --id`. The HARD-GATE keeps the two human stops and names the exception: approval before the first `create`, `link`, `/execute` or `/orchestrate` of a turn; never crossing a boundary unasked; the into-review advance runs without a stop. The RED-FLAGS table stays, trimmed to the four rows about approval ("pre-authorized", "said use /lazy", "the fix is obvious", "inline is exempt"). The "nothing refuses the create" row goes; the edit guard and ADR-036 make it moot. The jq block, the row-direction lecture, the multi-hop enumeration rules and the "common failure" paragraphs go: `next` computes what they explain.
- `lazy`'s frontmatter description becomes triggers only: "Use as the entry point for any work, including bugs, defects and unexpected behaviour." The workflow summary moves out of the description; agents that read only the description should learn when to fire it, not a stale outline of what it does.
- Authoring verbs: `generate` and `co-write` own the draft-to-review advance. `lazy` stops saying it does, and the HARD-GATE's first-mutation stop names that advance as its one exception rather than leaving the two rules to disagree.
- `execute` owns the work-open advance and says so once. `lazy` routes the single-unit path through `review-work` then `/advance` then one commit; that commit is named.
- `create-audit` reads type and relation from config like every other verb.
- The NEVER and BODY-CONTENT blocks go from every skill. The hook's refusal message names the replacement command; a paragraph repeating it is the drift vector the hook removes. `lazy` keeps one line: "Always `--json`."
- No skill gains a "run `why` before editing code" instruction. The edit guard injects it, which is the same argument one rung along: prose that duplicates a mechanism is the thing that drifts.
- One line names `--body-file` as the file route for a body too large for `--body`. No skill mentions it today.

`skills/README.md` drops the drift narrative and says: the binary answers, the hook delivers, the skill decides.

## Interfaces

```rust
@draft pub struct ChildType { pub r#type: String, pub authorship: Authorship, pub verb: &'static str }
@draft pub struct StateAction { pub verb: String, pub approve: bool }
@draft pub struct Next { /* fields per Design */ }
@draft pub fn next(store: &Store, config: &Config, id: &str) -> Result<Next>;
```

`unsatisfied_edges` reuses `ValidationIssue::UnsatisfiedEdge`; no new struct.

CLI: `next <ID> [--json]`, `validate [--id <ID>] [--json] [--warnings]`. `show` gains keys only. Config gains `[[types.lifecycle]] actions` and nothing else — `verb` is a `String` because it is the user's word, not an enum of this project's.

No new engine surface for the hooks. They compose `config --json`, `why --json` and `validate --json`, all shipped.

## Decisions (ADRs to emit)

- Routing moves into the binary. Amends ADR-019: the data contract stays, the "no brain command" clause is retired now the prose derivation has proved fragile.
- Enforcement lives in the plugin, not the binary. The hook scripts hold Claude Code's envelope format; lazyspec holds the questions. Rejected: a `guard` command classifying paths (the classification is three lines over `config --json`, and one implementation does not need one — principle 6), and a `guard --hook` mode reading the hook envelope on stdin (a vendor wire protocol compiled into a tool whose job is structured markdown — principle 1).
- Structural create gate. ADR-036, authored alongside this RFC, supersedes ADR-033: `create` for a type on the `from` side of a required chain edge needs `--parent` or `--orphan`.
- What to do at a state is declared per state, in the user's vocabulary, and lazyspec interprets none of it. Rejected: a `role` enum on types and fixed `review`/`work_ready`/`work_active` state names — both encode this repo's idea of a document lifecycle into a tool whose whole premise is that the graph is the user's. Also rejected: inferring either from `intent` prose or a state's position in the lifecycle, which `bug` (`reported/triaged/...`) falsifies outright.
- `next` never picks between child types. Where a crossing has several candidates it returns all of them for a human to choose, because choosing needs to know what the children mean.

## Stories

**Amended 2026-09-12 (review):** nine slices became five. Four of the nine had no consumer until another shipped — config keys nobody read, a filter with no caller, a baseline with no after-reading — so they were tasks wearing story clothes, and merging them made the real dependency visible instead of hiding it behind a link.

1. **STORY-285** — Route agent edits through the CLI and inject their specs. Edit guard, `why` injection and the Bash-write guard: one hook set, shell and jq in `hooks/`, no Rust. No dependencies.
2. **STORY-286** — Ask `next` what to do with a document. `lifecycle.actions` with shipped defaults, the derived keys on `show --json`, `engine::next` and the command, TUI and web rendering. No dependencies.
3. **STORY-287** — See findings for the document I just changed. `validate --id` across every rule's document-path fields, plus the post-mutation hook that calls it. No dependencies.
4. **STORY-288** — Collapse the skills to methodology and human stops. Carries the eval baseline as its first criterion: recorded against the current skills before anything is cut, re-run after. Depends on 285, 286, 287.
5. **STORY-289** — Require a parent when creating a document that needs one. Depends on ADR-036 accepted; independent of the rest.

## Risks and tradeoffs

- The hooks are Claude Code specific, and now there is no `guard` command for another runtime to fall back on. Accepted: the questions are all `--json` commands any runtime can call, and Claude Code is where every documented failure happened. A second runtime with a different event model is what would justify lifting classification into the engine.
- Injection fires per edit, so five edits to one file inject the same line five times. Accepted: it is one line, and a session cache is a hook change, not a design change.
- Injection reports governance, not currency. A document whose `reviewed` sha is ancient still reads as authoritative in the injected line. Accepted for this RFC; RFC-069 is the answer and the line gains a staleness marker when it lands.
- The Bash guard is a heuristic. A write it does not recognise gets through; a read it misreads as a write is refused and the agent reroutes through the CLI. Accepted: false refusals cost a retry, false passes cost what they cost today.
- `next` answers from `lifecycle.actions` and otherwise from the graph, and guesses meaning nowhere. A project that annotates nothing gets `author`, `boundary`, `advance` and `terminal` only — a missing route, never a wrong mutation. The cost is that the useful answers for a type nobody here imagined require the user to write one line per state; the shipped config carries those lines so the common case is already configured.
- Excluding wildcard edges from the single-successor count is a reading of intent, not a fact the config states. A project that declares `from = "*"` meaning it as the ordinary next move would get `terminal` where it wanted `advance`. Accepted: the shipped config's only wildcard is `-> superseded`, and the failure is a missing suggestion rather than a wrong one.
- Two earlier drafts of this section were wrong in the same direction — a role enum, then fixed state-role names — and both were argued from what this repo's types happen to look like. The graph is the user's; that is the constraint any future amendment to `next` answers to.
- Approval stops remain prose. A binary cannot know whether a human nodded. `requires_approval` makes the stop a data point the skill points at, and the trimmed RED-FLAGS table stays because the documented approval violations are the kind where the agent knows the rule and argues past it; data alone does not survive that.
