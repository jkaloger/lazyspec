---
name: scaffold
description: Use when creating a new document of a configured type at the most manual authorship level -- AI creates the file and frontmatter, surfaces intent and section guidance, then hands the body back to the human.
---

```
CREATE THE SHELL, HAND THE BODY BACK
```

Scaffold is the floor of the authorship order: AI makes the file and links it, then the human writes the body.

<HARD-GATE>
Do NOT write the document body. Scaffold creates the file, frontmatter, and links, then surfaces the type's intent and section guidance for the human to fill in.
Read the target type's config from `lazyspec config --json` before creating anything; the type is a parameter, never assumed.
</HARD-GATE>

<NEVER>
- Do NOT hand-edit document files to create or link them. Use `lazyspec create` (seed with `--body`/`--body-file`) and `lazyspec link`. To change body content, use `lazyspec update <id> --body`/`--body-file` -- for EVERY store, filesystem included. (Scaffold itself writes no body; it hands that back to the human.)
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured `parent_type` chain and `rules`.
</NEVER>

<BODY-CONTENT>
Set body at creation: `lazyspec create <type> "<title>" --body "content"` or `--body-file <path>` (`-` reads stdin). Change it later: `lazyspec update <ID> --body "content"` or `--body-file <path>`. Prefer `--body`/`--body-file` over any direct file edit, for ALL stores (filesystem and github-issues alike).
GitHub-issues docs additionally: never edit `.lazyspec/cache/` mirrors (read-only); always reference docs by shorthand ID (e.g. STORY-095), not cache paths.
</BODY-CONTENT>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Authorship Ceiling

The authorship order is `scaffold < co-write < generate`. A type's `authorship` value in config (`human`, `assisted`, `generated`) is the *ceiling* -- the highest verb permitted for that type.

Scaffold is the floor of that order, so it is permitted on **every** `authorship` value. **Scaffold never refuses on ceiling grounds.** Even a type whose ceiling is `human` can be scaffolded; that is exactly the manual case scaffold exists for.

## Preflight

1. `lazyspec config --json` -- read the target `<type>` entry: its `intent` (what the doc is for), its `authorship` ceiling (for confirmation only -- scaffold proceeds regardless), its `parent_type`, and the section guidance available from its template.
2. `lazyspec status --json` -- see what already exists and locate the parent document to link to.
3. `lazyspec context --json` -- understand the chain around the user's current position so the new document lands in the right place.

## Workflow

1. **Create the shell:** `lazyspec create <type> "<title>" --author <name>`, where `<type>` is the parameter read from config (e.g. in the shipped default config a type named `rfc`, but never assume that name -- read it).
2. **Link to the parent:** if config gives the type a `parent_type` and a parent exists, `lazyspec link <new-id> implements <parent-id>` -- using the configured relation name from `relationships` (the default config uses `implements`; read it, don't bake it).
3. **Surface intent + guidance:** show the human the type's `intent` from config and the per-section `<!-- guidance -->` comments from the scaffolded body. Tell the human these are the sections to fill in.
4. **Hand back:** stop. The human writes the body. Scaffold does not draft prose.

## Rules

- The `<type>` is always a parameter read from `config --json`. No type name is load-bearing in this prose.
- Scaffold never refuses on ceiling grounds -- it is the floor of `scaffold < co-write < generate`.
- Create and link only; never write the body.
- Read parent/relation/gate facts from config, never from `.lazyspec/` graph files directly.

---

---
name: co-write
description: Use when drafting a document of a configured type collaboratively -- AI proposes a draft body, the human edits, iterate -- up to the type's authorship ceiling.
---

```
PROPOSE A DRAFT, THE HUMAN EDITS, ITERATE
```

Co-write is the middle rung: AI scaffolds, then proposes a body toward the type's intent; the human revises and the loop repeats.

<HARD-GATE>
Do NOT proceed when the target type's `authorship` ceiling is `human` -- that type tops out at scaffold. Read the ceiling from `lazyspec config --json` and refuse, naming the ceiling.
Co-write proposes a draft for human editing; it does not finalise a body unilaterally.
</HARD-GATE>

<NEVER>
- Do NOT hand-edit document files. The CLI is the only writer: `lazyspec create` (seed with `--body`/`--body-file`), `lazyspec link`, and `lazyspec update <id> --body`/`--body-file` to change body content. This holds for EVERY store, filesystem included.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured `parent_type` chain and `rules`.
</NEVER>

<BODY-CONTENT>
Set body at creation: `lazyspec create <type> "<title>" --body "content"` or `--body-file <path>` (`-` reads stdin). Change it later: `lazyspec update <ID> --body "content"` or `--body-file <path>`. Prefer `--body`/`--body-file` over any direct file edit, for ALL stores (filesystem and github-issues alike).
GitHub-issues docs additionally: never edit `.lazyspec/cache/` mirrors (read-only); always reference docs by shorthand ID (e.g. STORY-095), not cache paths.
</BODY-CONTENT>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Authorship Ceiling

The authorship order is `scaffold < co-write < generate`. A type's `authorship` value in config is the ceiling.

Co-write is the middle rung. It is permitted when the type's `authorship` is `assisted` or `generated`.

**Refuse when the type's `authorship` is `human`.** Read the ceiling from `lazyspec config --json` and report it. Refusal text reads the ceiling out of config -- there is no hardcoded type-to-ceiling table:

> Type `<type>` is human-authored (ceiling = scaffold); drop to /scaffold.

where `<type>` and the ceiling are the actual values read from config for that run.

## Preflight

1. `lazyspec config --json` -- read the target `<type>`: its `intent`, its `authorship` ceiling (gate the verb on this), section guidance from its template, its `parent_type`, and the relation names in `relationships`.
2. `lazyspec status --json` -- locate the parent document to link to.
3. `lazyspec context --json` -- understand the chain around the user's position.

## Workflow

Scaffold, interview, then propose:

1. **Create + link** as in /scaffold: `lazyspec create <type> "<title>" --author <name>`, then `lazyspec link <new-id> <relation> <parent-id>` using the configured relation when a parent exists.
2. **Interview the human before drafting.** Co-write captures intent from the human, so grill before you write. Interview them relentlessly about every decision this document must resolve, walking each branch of the design tree and resolving dependencies between decisions one at a time. Ask ONE question at a time. For each question, give your recommended answer. If a question can be answered by exploring the codebase or reading `config --json` / parent docs / `@ref` targets, explore and answer it yourself instead of asking. Continue until every open branch the type's `intent` and section guidance imply is resolved -- do not start the draft with unresolved decisions.
3. **Propose a draft body** toward the type's `intent` and section guidance from config, incorporating the interview answers. Write the proposal to a file.
4. **Present for human edits.** The human revises; iterate the proposal with them.
5. **Apply the accepted draft:** `lazyspec update <id> --body-file <path>`.

## Rules

- The `<type>` is always a parameter read from `config --json`. No type name is load-bearing in this prose.
- Refuse when `authorship` is `human`; the refusal reports the config-read ceiling, not a baked table.
- Interview before drafting: resolve every open design decision with the human (one question at a time, recommended answer each, explore rather than ask when you can) before proposing the body.
- Always propose-for-edit; never finalise a body without the human in the loop.
- Read parent/relation/gate facts from config, never from `.lazyspec/` graph files directly.

---

---
name: generate
description: Use when authoring a full document body of a configured type from context -- AI writes the complete body, then asks for review -- only permitted when the type's authorship ceiling is `generated`.
---

```
WRITE THE WHOLE BODY, THEN ASK FOR REVIEW
```

Generate is the top rung: AI assembles context and writes the complete body, permitted only when the type's authorship ceiling is `generated`.

<HARD-GATE>
Do NOT proceed unless the target type's `authorship` ceiling is `generated`. For `human` and `assisted` types, refuse and name the permitted verb. Read the ceiling from `lazyspec config --json`.
Generate writes the full body, then routes to /review -- it does not self-approve.
</HARD-GATE>

<NEVER>
- Do NOT hand-edit document files. The CLI is the only writer: `lazyspec create` (seed with `--body`/`--body-file`), `lazyspec link`, and `lazyspec update <id> --body`/`--body-file` to change body content. This holds for EVERY store, filesystem included.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured `parent_type` chain and `rules`.
</NEVER>

<BODY-CONTENT>
Set body at creation: `lazyspec create <type> "<title>" --body "content"` or `--body-file <path>` (`-` reads stdin). Change it later: `lazyspec update <ID> --body "content"` or `--body-file <path>`. Prefer `--body`/`--body-file` over any direct file edit, for ALL stores (filesystem and github-issues alike).
GitHub-issues docs additionally: never edit `.lazyspec/cache/` mirrors (read-only); always reference docs by shorthand ID (e.g. STORY-095), not cache paths.
</BODY-CONTENT>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Authorship Ceiling

The authorship order is `scaffold < co-write < generate`. A type's `authorship` value in config is the ceiling.

Generate is the top rung. It is permitted **only** when the type's `authorship` is `generated`.

**Refuse for `human` and `assisted` types.** This is the headline ceiling-refusal case. Read the ceiling from `lazyspec config --json` and report it together with the permitted verb. The refusal text reads the ceiling string out of config; there is no baked type-to-ceiling table:

> Type `<type>` ceiling = co-write; drop to /co-write.

or, for a human-authored type:

> Type `<type>` ceiling = scaffold; drop to /scaffold.

where `<type>` and the ceiling are the actual values read from config for that run. Map ceiling to verb by the order itself: `human` -> scaffold, `assisted` -> co-write, `generated` -> generate.

## Preflight

1. `lazyspec config --json` -- read the target `<type>`: its `intent`, its `authorship` ceiling (gate the verb on this), section guidance, `parent_type`, and relation names.
2. `lazyspec context --json` -- assemble source material: parent docs, related docs, and referenced code. Expand `@ref` directives and pull referenced code with `lazyspec show -e <id>`.

## Workflow

1. **Create + link:** `lazyspec create <type> "<title>" --author <name>`, then `lazyspec link <new-id> <relation> <parent-id>` with the configured relation when a parent exists.
2. **Resolve residual gaps before writing.** Generate is context-first: it leans on parent docs, related docs, `@ref` targets, and the codebase, not on the human. So capture lightly -- resolve every decision you can from gathered context yourself, then surface ONLY the decisions the context cannot settle. Ask those as a short batch (one at a time, each with your recommended answer); skip the question entirely when the context already answers it. This is a lighter touch than /co-write's full interview -- you are filling gaps, not eliciting the whole design.
3. **Write the full body** from gathered context and resolved gaps toward the type's `intent` and section guidance. Write to a file.
4. **Apply:** `lazyspec update <id> --body-file <path>`.
5. **Request review:** route to /review. Generate never approves its own output.

## Rules

- The `<type>` is always a parameter read from `config --json`. No type name is load-bearing in this prose.
- Permitted only when `authorship` is `generated`; refuse for `human` and `assisted`, reporting the config-read ceiling and permitted verb -- never a baked table.
- Resolve from context first; ask the human only the decisions context cannot settle (lighter capture than /co-write). Never interrogate the whole design when the context already answers it.
- Always route to /review on completion.
- Read parent/relation/gate facts from config, never from `.lazyspec/` graph files directly.

---

---
name: advance
description: Use when moving a document to its next status along the type's lifecycle DAG, maintaining links and checking gates at the transition.
---

```
TRAVERSE ONE OUT-EDGE OF THE LIFECYCLE GRAPH
```

A type's lifecycle is a directed graph: the nodes are its statuses, the edges are the transitions config permits. A document sits on one status. Advance reads the out-edges from that status, picks the successor, confirms the gate on that edge holds, and writes the move. One document, one edge.

<HARD-GATE>
Propose only a successor: a status the current one has an out-edge to in `lifecycle.edges`. Read the edge set from config. The binary rejects any pair that is not an edge.
Advance writes status only. It never creates a child document, even when the move satisfies a gate that makes a child creatable.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured `parent_type` chain and `rules`.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"` or `--body-file <path>`.
- To modify after creation: `lazyspec update <ID> --body "new content"` or `--body-file <path>`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Preflight

1. `lazyspec config --json` gives the type's `lifecycle`: its `states` (the nodes) and `edges` (the transitions). The edge set decides which moves exist. Every status name comes from config; this skill names none.
2. `lazyspec show <id> --json` gives the document's current status.
3. `lazyspec context --json` gives the parent and child statuses a gate may depend on.

## Workflow

1. Find the successors. Keep the edges in `lifecycle.edges` whose `from` is the current status; their `to` values are the statuses you can move to. An edge with `from: "*"` applies from every status, so the default config's `* -> superseded` is always available.
2. Test the gate. A gate is a predicate on the target status, such as `require_parent_status`. Read the parent's status from `context --json` and check the predicate. If it fails, stop and report which status the parent must reach first.
3. Write the move. `lazyspec update <id> --status <next>`. The binary rejects any pair that is not an edge, so offer only successors.
4. Preserve the links across the move.

## Gates and the type boundary

A gate can make a child of another type creatable once the parent reaches a status. When that happens, advance writes the status move and stops. It does not create the child.

Two conditions separate a move within the lifecycle from crossing into a child type. The gate makes the child creatable; starting it is a second, human step, handled by /lazy's stop-at-boundary rule. Satisfying the gate is necessary, not sufficient.

## Rules

- The successor comes from `lifecycle.edges`. This skill names no status; config does.
- Write the move with `lazyspec update <id> --status <next>`; never go around the binary's edge check.
- Test the gate before moving; do not cross an unsatisfied gate.
- Never create a child document. Status only.
- Read lifecycle and gate facts from config, not from the `.lazyspec/` graph files.

---

---
name: execute
description: Use when carrying out the work a delivery document describes -- the build loop -- against its task breakdown and acceptance criteria.
---

```
DO THE WORK THE DOCUMENT DESCRIBES
```

Execute is the build loop: it orchestrates the task breakdown of a delivery document, dispatching a subagent per task and verifying each against its acceptance criteria.

<HARD-GATE>
Do NOT begin execution without a delivery document that carries a task breakdown. If the document lacks one, author it first (route to the appropriate authoring verb).
Confirm from `lazyspec config --json` that the document's type is a delivery type in this DAG before starting.
ALWAYS use subagents for the work. The orchestrator dispatches; it does not implement.
Each task must carry enough detail for a zero-context subagent. If it does not, fix the breakdown first.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured `parent_type` chain and `rules`.
- Do NOT implement tasks yourself. Dispatch a subagent per task. Do NOT dispatch parallel implementers.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"` or `--body-file <path>`.
- To modify after creation: `lazyspec update <ID> --body "new content"` or `--body-file <path>`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## No Ceiling

Execute is **work, not authoring.** The `scaffold < co-write < generate` ceiling does not apply here -- it governs who writes a document's prose, not who does the work the document describes. Do not confuse execute with the authoring trio.

## Preflight

1. `lazyspec config --json` -- confirm the document's `<type>` is a delivery type in this DAG (the type whose breakdown describes implementation work; in the shipped default config that is the `iteration` type, but read it -- do not assume the name).
2. `lazyspec show <id> --json` -- read the task breakdown and acceptance criteria.
3. `lazyspec context --json` -- pull the full chain (parent and grandparent docs) for intent and ACs.
4. `lazyspec convention --json` -- load codebase conventions. Include non-empty results in subagent prompts under `## Convention Context`.
5. Extract every task from the breakdown before dispatching any subagent.

## Subagent Dispatch

| Operation | Agent type | Model tier | Context to provide |
|-----------|-----------|-----------|--------------------|
| Implement task | general-purpose | most capable | Full task text, parent intent, acceptance criteria, prior task results, convention context |
| Review task (AC compliance) | general-purpose | most capable | Task text, acceptance criteria, implementer report |
| Review task (code quality) | general-purpose | lighter | Changed files, scoped test output, quality criteria |
| Final review | general-purpose | most capable | All acceptance criteria, full implementation summary |

Model tier is by capability, not product name: "most capable" for implementation and the correctness-bearing reviews, a "lighter" model for the code-quality pass. The orchestrator picks the concrete model.

## Per-Task Loop

Iterate the breakdown's tasks **sequentially**. For each task, dispatch an implementer subagent (general-purpose, most capable model) with:

- **Full task text** copied from the document (not a file reference).
- Scene-setting: parent intent (1-2 sentences), relevant acceptance criteria, prior task results.
- Lazyspec workflow rules: use the `lazyspec` CLI for doc ops, `--json` always, `--help` before unfamiliar commands, `lazyspec show -e <id>` to expand `@ref` directives.
- "Before you begin: ask questions about unclear requirements. Don't guess."
- TDD: failing test first, then implementation, then verify.
- Run **scoped** verification only -- just the tests/checks this task touches, never the full suite mid-loop. The full check runs once, at the orchestrator's Final Review.
- Self-review: completeness (ACs met?), YAGNI (only what was asked?), test quality (behavioral, isolated, deterministic, readable, specific).
- Report: what was implemented, verification results, files changed, concerns.

Handle implementer questions before letting them proceed: answer, then re-dispatch.

After the implementer reports, dispatch a **separate** reviewer subagent with task text, acceptance criteria, and the implementer report:

- **Stage 1 (AC compliance):** Run the scoped checks covering this task's ACs, verify each claimed AC has a passing test, check for missing requirements or scope creep. If any AC is unmet, report specifics. Do NOT run the full suite here -- that is the orchestrator's Final Review gate.
- **Stage 2 (code quality, only if Stage 1 passes):** Correctness, clarity, YAGNI, DRY, security. Evaluate test properties (behavioral, structure-insensitive, isolated, deterministic, readable, specific). Flag unjustified tradeoffs.

On failure: dispatch a fresh implementer with the specific issues, then re-review. Repeat until both stages pass. Mark the task complete.

## Context Refresh

Every 2 completed tasks, re-read the chain (`lazyspec context`, `lazyspec show` for the delivery document and its parent) to prevent drift.

## Final Review

The orchestrator runs this gate itself after all tasks complete. Subagents only ran scoped checks; this is the one place the full check runs.

1. Verify every task in the breakdown is complete, no out-of-scope work.
2. **Run the full check once.** It must pass -- required gate, no acceptance on failure. On failure, dispatch a targeted fix subagent, then re-run.
3. Run `lazyspec validate --json`.
4. Dispatch a final reviewer (most capable model) with all acceptance criteria and the implementation summary.
5. On pass, route to /review for critique, then to /advance for the status move. **/advance owns the status transition** -- execute does not move statuses itself.

## Rules

- The delivery `<type>` is read from `config --json`. No type name is load-bearing in this prose.
- No ceiling concept -- execute is work, not authoring.
- Fresh subagent per task (no context pollution). The reviewer is always a separate subagent from the implementer.
- Stage 1 (AC compliance) MUST pass before Stage 2 (code quality).
- Subagents receive full task text, not file references.
- One task, one review cycle. No batching. Sequential dispatch only -- no parallel implementers.
- Subagents run scoped checks only. The full check runs once, by the orchestrator, at Final Review.
- Route to /review then /advance on completion; advance owns the status move.
- Read type/chain facts from config and the CLI, never from `.lazyspec/` graph files directly.

---

---
name: review
description: Use when critiquing a document against its intent and acceptance criteria, or reviewing completed work, before advancing status.
---

```
CONFORMANCE FIRST, QUALITY SECOND
```

Review critiques a document against its declared intent and acceptance criteria, then its quality -- blocking on conformance failure before advancing.

<HARD-GATE>
Do NOT review quality before conformance. The document's acceptance criteria and declared intent come first; block on any conformance failure before looking at quality.
Do NOT approve without fresh verification evidence gathered in this session.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured `parent_type` chain and `rules`.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"` or `--body-file <path>`.
- To modify after creation: `lazyspec update <ID> --body "new content"` or `--body-file <path>`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Preflight

1. `lazyspec config --json` -- read the type's `intent` (the bar to critique against) and its `lifecycle` (to know which status review precedes, so the pass-route to /advance targets the right edge).
2. `lazyspec show <id> --json` -- read the document and its acceptance criteria.
3. `lazyspec context --json` -- read the chain (parent intent and ACs) so conformance is judged against the right spec.

## Workflow

Two-stage critique:

**Stage 1 -- Conformance.** Does the document satisfy its declared intent and its acceptance criteria? For work being reviewed, verify each acceptance criterion with fresh evidence run in this session. Block on any conformance failure.

**Stage 2 -- Quality.** Only after conformance passes: critique quality -- clarity, correctness, cohesion, and (for work) test quality. Flag unjustified tradeoffs.

Express targets generically: "the document's acceptance criteria", "its declared intent". No type name is baked in.

## Routing

- **On pass:** route to /advance to move status along the lifecycle edge that review precedes.
- **On fail:** route back to the appropriate authoring verb (one at or below the type's ceiling: /scaffold, /co-write, or /generate) for a document, or to /execute for work.

## Rules

- The type and its intent/lifecycle are read from `config --json`. No type name is load-bearing in this prose.
- Conformance (intent + ACs) before quality, always. Block on conformance failure.
- Approve only on fresh, in-session verification evidence.
- Route to /advance on pass; route back to the authoring verb or /execute on fail.
- Read type/lifecycle facts from config, never from `.lazyspec/` graph files directly.

---

---
name: systematic-debugging
description: Use when encountering any bug, test failure, or unexpected behavior, before proposing fixes
---

# Systematic Debugging

## Overview

Random fixes waste time and create new bugs. Quick patches mask underlying issues.

**Core principle:** ALWAYS find root cause before attempting fixes. Symptom fixes are failure.

**Violating the letter of this process is violating the spirit of debugging.**

## The Iron Law

```
NO FIXES WITHOUT ROOT CAUSE INVESTIGATION FIRST
```

If you haven't completed Phase 1, you cannot propose fixes.

## When to Use

Use for ANY technical issue:
- Test failures
- Bugs in production
- Unexpected behavior
- Performance problems
- Build failures
- Integration issues

**Use this ESPECIALLY when:**
- Under time pressure (emergencies make guessing tempting)
- "Just one quick fix" seems obvious
- You've already tried multiple fixes
- Previous fix didn't work
- You don't fully understand the issue

**Don't skip when:**
- Issue seems simple (simple bugs have root causes too)
- You're in a hurry (rushing guarantees rework)
- Manager wants it fixed NOW (systematic is faster than thrashing)

## The Four Phases

You MUST complete each phase before proceeding to the next.

### Phase 1: Root Cause Investigation

**BEFORE attempting ANY fix:**

1. **Read Error Messages Carefully**
   - Don't skip past errors or warnings
   - They often contain the exact solution
   - Read stack traces completely
   - Note line numbers, file paths, error codes

2. **Reproduce Consistently**
   - Can you trigger it reliably?
   - What are the exact steps?
   - Does it happen every time?
   - If not reproducible → gather more data, don't guess

3. **Check Recent Changes**
   - What changed that could cause this?
   - Git diff, recent commits
   - New dependencies, config changes
   - Environmental differences

4. **Gather Evidence in Multi-Component Systems**

   **WHEN system has multiple components (CI → build → signing, API → service → database):**

   **BEFORE proposing fixes, add diagnostic instrumentation:**
   ```
   For EACH component boundary:
     - Log what data enters component
     - Log what data exits component
     - Verify environment/config propagation
     - Check state at each layer

   Run once to gather evidence showing WHERE it breaks
   THEN analyze evidence to identify failing component
   THEN investigate that specific component
   ```

   **Example (multi-layer system):**
   ```bash
   # Layer 1: Workflow
   echo "=== Secrets available in workflow: ==="
   echo "IDENTITY: ${IDENTITY:+SET}${IDENTITY:-UNSET}"

   # Layer 2: Build script
   echo "=== Env vars in build script: ==="
   env | grep IDENTITY || echo "IDENTITY not in environment"

   # Layer 3: Signing script
   echo "=== Keychain state: ==="
   security list-keychains
   security find-identity -v

   # Layer 4: Actual signing
   codesign --sign "$IDENTITY" --verbose=4 "$APP"
   ```

   **This reveals:** Which layer fails (secrets → workflow ✓, workflow → build ✗)

5. **Trace Data Flow**

   **WHEN error is deep in call stack, trace it backward to the source:**
   - Where does bad value originate?
   - What called this with bad value?
   - Keep tracing up until you find the source
   - Fix at source, not at symptom

### Phase 2: Pattern Analysis

**Find the pattern before fixing:**

1. **Find Working Examples**
   - Locate similar working code in same codebase
   - What works that's similar to what's broken?

2. **Compare Against References**
   - If implementing pattern, read reference implementation COMPLETELY
   - Don't skim - read every line
   - Understand the pattern fully before applying

3. **Identify Differences**
   - What's different between working and broken?
   - List every difference, however small
   - Don't assume "that can't matter"

4. **Understand Dependencies**
   - What other components does this need?
   - What settings, config, environment?
   - What assumptions does it make?

### Phase 3: Hypothesis and Testing

**Scientific method:**

1. **Form Single Hypothesis**
   - State clearly: "I think X is the root cause because Y"
   - Write it down
   - Be specific, not vague

2. **Test Minimally**
   - Make the SMALLEST possible change to test hypothesis
   - One variable at a time
   - Don't fix multiple things at once

3. **Verify Before Continuing**
   - Did it work? Yes → Phase 4
   - Didn't work? Form NEW hypothesis
   - DON'T add more fixes on top

4. **When You Don't Know**
   - Say "I don't understand X"
   - Don't pretend to know
   - Ask for help
   - Research more

### Phase 4: Implementation

**Fix the root cause, not the symptom:**

1. **Create Failing Test Case**
   - Simplest possible reproduction
   - Automated test if possible
   - One-off test script if no framework
   - MUST have before fixing
   - Use a test-driven-development skill for writing proper failing tests, if one is available

2. **Implement Single Fix**
   - Address the root cause identified
   - ONE change at a time
   - No "while I'm here" improvements
   - No bundled refactoring

3. **Verify Fix**
   - Test passes now?
   - No other tests broken?
   - Issue actually resolved?

4. **If Fix Doesn't Work**
   - STOP
   - Count: How many fixes have you tried?
   - If < 3: Return to Phase 1, re-analyze with new information
   - **If ≥ 3: STOP and question the architecture (step 5 below)**
   - DON'T attempt Fix #4 without architectural discussion

5. **If 3+ Fixes Failed: Question Architecture**

   **Pattern indicating architectural problem:**
   - Each fix reveals new shared state/coupling/problem in different place
   - Fixes require "massive refactoring" to implement
   - Each fix creates new symptoms elsewhere

   **STOP and question fundamentals:**
   - Is this pattern fundamentally sound?
   - Are we "sticking with it through sheer inertia"?
   - Should we refactor architecture vs. continue fixing symptoms?

   **Discuss with your human partner before attempting more fixes**

   This is NOT a failed hypothesis - this is a wrong architecture.

## Red Flags - STOP and Follow Process

If you catch yourself thinking:
- "Quick fix for now, investigate later"
- "Just try changing X and see if it works"
- "Add multiple changes, run tests"
- "Skip the test, I'll manually verify"
- "It's probably X, let me fix that"
- "I don't fully understand but this might work"
- "Pattern says X but I'll adapt it differently"
- "Here are the main problems: [lists fixes without investigation]"
- Proposing solutions before tracing data flow
- **"One more fix attempt" (when already tried 2+)**
- **Each fix reveals new problem in different place**

**ALL of these mean: STOP. Return to Phase 1.**

**If 3+ fixes failed:** Question the architecture (see Phase 4.5)

## your human partner's Signals You're Doing It Wrong

**Watch for these redirections:**
- "Is that not happening?" - You assumed without verifying
- "Will it show us...?" - You should have added evidence gathering
- "Stop guessing" - You're proposing fixes without understanding
- "Ultrathink this" - Question fundamentals, not just symptoms
- "We're stuck?" (frustrated) - Your approach isn't working

**When you see these:** STOP. Return to Phase 1.

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "Issue is simple, don't need process" | Simple issues have root causes too. Process is fast for simple bugs. |
| "Emergency, no time for process" | Systematic debugging is FASTER than guess-and-check thrashing. |
| "Just try this first, then investigate" | First fix sets the pattern. Do it right from the start. |
| "I'll write test after confirming fix works" | Untested fixes don't stick. Test first proves it. |
| "Multiple fixes at once saves time" | Can't isolate what worked. Causes new bugs. |
| "Reference too long, I'll adapt the pattern" | Partial understanding guarantees bugs. Read it completely. |
| "I see the problem, let me fix it" | Seeing symptoms ≠ understanding root cause. |
| "One more fix attempt" (after 2+ failures) | 3+ failures = architectural problem. Question pattern, don't fix again. |

## Quick Reference

| Phase | Key Activities | Success Criteria |
|-------|---------------|------------------|
| **1. Root Cause** | Read errors, reproduce, check changes, gather evidence | Understand WHAT and WHY |
| **2. Pattern** | Find working examples, compare | Identify differences |
| **3. Hypothesis** | Form theory, test minimally | Confirmed or new hypothesis |
| **4. Implementation** | Create test, fix, verify | Bug resolved, tests pass |

## When Process Reveals "No Root Cause"

If systematic investigation reveals issue is truly environmental, timing-dependent, or external:

1. You've completed the process
2. Document what you investigated
3. Implement appropriate handling (retry, timeout, error message)
4. Add monitoring/logging for future investigation

**But:** 95% of "no root cause" cases are incomplete investigation.

## Supporting Techniques

Techniques that compose with the four phases:

- **Root-cause tracing** - trace a bad value backward through the call stack to its original trigger; fix at the source.
- **Defense in depth** - after finding the root cause, add validation at multiple layers so the class of bug cannot recur.
- **Condition-based waiting** - for timing-dependent bugs, replace arbitrary timeouts with polling on the actual condition.

Compose with a test-driven-development skill (Phase 4, Step 1: the failing test) and a verification-before-completion discipline (verify the fix before claiming success) where those are available.

## Real-World Impact

From debugging sessions:
- Systematic approach: 15-30 minutes to fix
- Random fixes approach: 2-3 hours of thrashing
- First-time fix rate: 95% vs 40%
- New bugs introduced: Near zero vs common

---

---
name: lazy
description: Use as the entry point for any work, including reported bugs, defects, and unexpected behaviour. Reads the configured DAG and the user's position, then dispatches the right verb -- advancing within the current document automatically but stopping at type boundaries.
---

```
ADVANCE WITHIN A DOCUMENT, STOP AT THE BOUNDARY
```

Lazy is the entry router: it reads the configured DAG and where the user is, then dispatches the right verb -- progressing within the current document automatically, but never crossing a type boundary on its own.

```d2
direction: down

preflight: Preflight {
  shape: rectangle
  config: "config --json (DAG)"
  status: "status --json (docs + statuses)"
  context: "context --json (chain)"
}

triage: Entry intent? {
  shape: diamond
  tooltip: "bug/defect reported, or positioned on a doc?"
}

debug: systematic-debugging {
  tooltip: "root cause FIRST -- no fix doc before Phase 1 done"
}

locate: Locate-in-DAG {
  shape: rectangle
  tooltip: "current type, status, outgoing edges, gates"
}

dispatch: Dispatch (computed from config) {
  shape: diamond
}

advance: /advance {tooltip: "status move, no authoring"}
author: authoring verb at ceiling {
  tooltip: "human -> /scaffold, assisted -> /co-write, generated -> /generate"
}
execute: /execute {tooltip: "work the document describes"}
review: /review {tooltip: "critique before next status"}

validate: validate touched doc {
  shape: rectangle
  tooltip: "validate --json scoped to the doc just mutated; fix introduced breakage"
}

boundary: STOP at type boundary {
  shape: hexagon
  tooltip: "child of a different type -- human-initiated only, even if gate is met"
}

preflight -> triage
triage -> debug: "bug / defect reported"
triage -> locate: "positioned on a doc"
debug -> boundary: "root cause found -- author config-driven fix doc, human-initiated"

locate -> dispatch

dispatch -> advance: "eligible status edge"
dispatch -> author: "authoring step due"
dispatch -> execute: "work pending"
dispatch -> review: "critique due"
dispatch -> boundary: "only next step crosses parent_type edge"

advance -> validate: "graph mutated"
author -> validate: "graph mutated"
validate -> locate: "loop within document"
execute -> locate
review -> locate
```

<HARD-GATE>
Do NOT auto-run `create <child-type>` across a `parent_type` edge. Crossing into a different type is always human-initiated -- even when a `require_parent_status` gate is already satisfied. Within-document progression is automatic; crossing a type boundary is not.
Compute the dispatch table from `lazyspec config --json` at runtime. There is no fixed chain in this prose.
A reported bug, defect, or unexpected behaviour is investigated to root cause FIRST -- via systematic-debugging -- before any fix document is authored. No fix doc before root cause.
After every graph-mutating dispatch (/advance and the authoring verbs), run `lazyspec validate --json` scoped to the touched document before looping.
</HARD-GATE>

<NEVER>
- Do NOT hand-edit document files. The CLI is the only writer: `lazyspec create` (seed with `--body`/`--body-file`), `lazyspec link`, and `lazyspec update <id> --body`/`--body-file` to change body content. This holds for EVERY store, filesystem included.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured `parent_type` chain and `rules`.
</NEVER>

<BODY-CONTENT>
Set body at creation: `lazyspec create <type> "<title>" --body "content"` or `--body-file <path>` (`-` reads stdin). Change it later: `lazyspec update <ID> --body "content"` or `--body-file <path>`. Prefer `--body`/`--body-file` over any direct file edit, for ALL stores (filesystem and github-issues alike).
GitHub-issues docs additionally: never edit `.lazyspec/cache/` mirrors (read-only); always reference docs by shorthand ID (e.g. STORY-095), not cache paths.
</BODY-CONTENT>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Preflight (the routing read)

This is the resolve-context fold-in: `/lazy` reads context from the CLI rather than calling a separate skill.

1. `lazyspec config --json` -- the full DAG: `types` (with `intent`, `authorship`, `lifecycle`, `parent_type`), `relationships`, and `rules` (including any `require_parent_status` gates).
2. `lazyspec status --json` -- what documents exist and each one's current status.
3. `lazyspec context --json` -- the chain around the user's current document.

## Entry triage: bug or defect

When the user arrives with a **bug, defect, test failure, or unexpected behaviour** rather than positioned on a document, handle it here before routing. The whole branch is DAG-agnostic: it reads the fix-doc type and its links from config, never assuming a type name.

1. **Root cause first.** REQUIRED SUB-SKILL: systematic-debugging. Complete its Phase 1 (root-cause investigation) BEFORE authoring any fix document. No fix doc before root cause -- that is the systematic-debugging Iron Law, and it gates this branch.
2. **Pick the fix-doc type from config.** Read `config --json`. If a type's `intent` describes defects/bugs/fixes (a user may have a dedicated `bug` type), use that type. Otherwise use the delivery type -- the type whose breakdown describes implementation work (in the shipped default config that is `iteration`, but read it; never hardcode the name).
3. **Find the document the bug touches.** `lazyspec search "<area>" --json` plus `context --json` to locate the story/spec/feature covering the buggy area.
4. **Propose a create+link that satisfies the type's relation rules.** The fix-doc type may carry a `parent_type` or a `relation-existence` rule (e.g. `iterations-need-stories`). Propose the `create` plus the `link` (using the configured relation) that satisfies those rules -- linking the fix doc to the doc it touches. If no document satisfies a required relation, report that the human must pick or create the parent first. NEVER create a standalone doc that bypasses a rule, and never invent a link the user did not confirm.
5. **Crossing into the fix-doc type is a type boundary -- human-initiated.** Lazy proposes the exact `create` + `link` commands and stops (see Stop-at-Type-Boundary). It does not auto-create the fix doc.

## Locate-in-DAG

From config + status + context, determine which document and type the user is on and where it sits in its lifecycle (current status, outgoing edges, gates).

## Dispatch (computed from config)

Build the dispatch table at runtime from config. No `parent_type` chain is hardcoded here. (The shipped default config happens to define a chain among types named `rfc`, `story`, and `iteration` -- treat that only as the shipped default, never as a routing assumption.)

**Within-document progression is automatic.** If the current document has an eligible outgoing `lifecycle` edge (the edge exists and its gate, if any, is met), dispatch the matching verb WITHOUT asking:

- a status move with no authoring/work needed -> /advance
- an authoring step appropriate to the type's `authorship` and current status -> the authoring verb at the type's ceiling (/scaffold, /co-write, or /generate)
- work described by the document -> /execute
- a critique step before the next status -> /review

**Authorship-aware dispatch.** When routing to an authoring action, pick the verb at or below the type's `authorship` ceiling. Default to the ceiling verb (`human` -> /scaffold, `assisted` -> /co-write, `generated` -> /generate) and allow the human to drop lower. Never dispatch an above-ceiling verb.

## Stop-at-Type-Boundary

When the only remaining next step would create a child of a **different type** -- crossing a `parent_type` edge -- `/lazy` **STOPS.** It reports the boundary and what the human can do next; it never auto-runs `create <child-type>`.

This holds **even when a `require_parent_status` gate is already satisfied.** Gate-clear makes the child _eligible_, not _automatic_. Crossing a type boundary is always human-initiated. Report it like:

> `<doc>` (type `<type>`) is at status `<status>`; its child type `<child-type>` is now eligible to create. Crossing types is human-initiated -- run /scaffold (or the ceiling verb for `<child-type>`) to start one.

with every value read from config + status for that run.

## Validate after each mutation

`/lazy` is the chokepoint for graph integrity. After every dispatched verb that **mutates the graph** -- `/advance` (status move plus relations) and the authoring verbs `/scaffold`, `/co-write`, `/generate` (create plus link) -- run `lazyspec validate --json` before looping back to locate.

- **Scope to the doc just touched.** `validate` is a whole-repo check and will report pre-existing findings across unrelated documents. Filter its output to findings naming the document this mutation created, linked, or advanced. Fix only the broken or dangling relation **this mutation introduced** before continuing. Do not block on pre-existing repo-wide findings.
- `/execute` and `/review` are not graph mutators in this loop, so they need no validate step here (`/execute` runs its own `validate` at Final Review).
- **Known limitation:** invoking a mutating verb standalone -- outside `/lazy` -- skips this check. `/lazy` is the canonical entry router; that is where graph integrity is enforced.

## Rules

- All routing reads from `config --json` / `status --json` / `context --json` at runtime. No type name and no chain are load-bearing in this prose.
- Within-document progression is automatic; crossing a type boundary is never automatic.
- Dispatch only verbs at or below the type's authorship ceiling.
- A reported bug/defect goes through root cause (systematic-debugging) before any fix doc is authored; the fix-doc type and its links are read from config and must satisfy the type's relation rules -- never create a standalone doc that bypasses a rule.
- After each mutating dispatch (`/advance`, `/scaffold`, `/co-write`, `/generate`), validate the touched doc and fix only the relation breakage this mutation introduced; standalone verb invocation outside `/lazy` skips this.
- Read DAG/gate/status facts from the CLI, never from `.lazyspec/` graph files directly.
