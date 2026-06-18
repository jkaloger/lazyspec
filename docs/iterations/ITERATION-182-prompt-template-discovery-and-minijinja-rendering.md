---
title: Prompt template discovery and minijinja rendering
type: iteration
status: draft
author: agent
date: 2026-06-18
tags: []
related:
- implements: STORY-133
---

## Context

STORY-133 = slice 2 of RFC-046 (supersedes RFC-016). Agent prompts baked into binary today: `build_expand_prompt` + `build_create_children_prompt` in `src/tui/agent.rs` → project can't change agent prompt w/o recompiling. This slice replaces hardcoded builders w/ user-authored templates discovered under `<repo>/.lazyspec/agents/*.md`, parsed → `AgentPrompt`, rendered w/ minijinja strict-undefined against doc context.

Zero-defaults principle (ADR-011/ADR-015): engine ships NO default prompts. `init` writes none (verified `src/cli/init.rs` — scaffolds types/templates/convention only, no agent prompt). No template files → agent mode has nothing to offer.

**What changes:**
- ADD `minijinja` dep (strict-undefined) → Cargo.toml.
- NEW engine module `src/engine/prompt.rs`: `AgentPrompt` + `RunMode`, discovery, fm parse, render-context assembly, strict render.
- DELETE `build_expand_prompt` + `build_create_children_prompt` from `src/tui/agent.rs` → engine ships zero prompts (AC9).

**Deps / coordination:**
- Deleting the 2 builders breaks their call sites in `src/tui/views/keys.rs` (line 215 `build_expand_prompt`, line 252 `build_create_children_prompt` via `spawn_create_children`). Both call sites + the fixed action dialog get fully rewired in **slice 4 / STORY-135** (`ITERATION-184`). This slice must keep build green → temporarily stub/remove the dead "Expand document"/"Create children" branches in `keys.rs` so the binary compiles w/o the builders. NOTE this as a coordination dependency: keys.rs dialog is authoritative in slice 4.
- OUT OF SCOPE (deps, do NOT implement here):
  - `AgentRunner`/`ClaudeP` spawning → slice 1 / STORY-132 (`ITERATION-181`).
  - Per-type `[[types]].agents` gating → slice 3 / STORY-134 (`ITERATION-183`). This slice LOADS + RENDERS; does NOT decide which types may use templates.
  - TUI action dialog rewire → slice 4 / STORY-135 (`ITERATION-184`).
  - ACTING on `mode: interactive` (terminal handover, `[agents]` interactive cmd) → slice 5 / STORY-136 (`ITERATION-185`). This slice only PARSES `mode` into `AgentPrompt`.

**Reuse (do NOT re-derive):** `resolve_chain(store, id, depth) -> Result<ResolvedContext>` (`src/engine/context.rs:29`) already does the DAG walk consumed by resolve-context CLI + TUI Relations/Graph views. REUSE it for the `context` render var. Do NOT re-walk `implements`/`related-to` edges.

**Verified shape gotcha:** `DocMeta` (`src/engine/document.rs:185`) has NO `body` field — body lives on disk. Read via `Store::get_body_raw(path, fs)` (`src/engine/store.rs:144`). `document.type` ← `DocMeta.doc_type` (Display). `document.status` ← `DocMeta.status` (Display). So render-context assembly needs `&Store` + `&dyn FileSystem`, not just a `&DocMeta`.

## Test Plan

All tests follow DICTUM-004 (`cargo run --quiet -- convention --tags iteration,testing --json`): isolated (own `TempDir`/`Store`), composable, fast (no sleeps/network/process-spawn — use `FileSystem` seam + `TempDir`), behavioral (assert on what render/parse produce, not internals), structure-insensitive (through public `prompt.rs` API), deterministic (fixed fixtures, no timestamps). Engine unit tests live inline in `#[cfg(test)] mod tests` at bottom of `src/engine/prompt.rs`; the no-builtin-prompts symbol check (AC9) is a grep-style guard in the same module's verify step + an inline assertion that `src/tui/agent.rs` exposes no such fns. Tests that need a chain/related graph build a real `Store` via `Store::load(tmp, &Config::default())` over in-memory files (mirror the `store_from` helper in `context.rs` tests).

- **AC1 — template discovery.**
  Test `discovers_md_templates_under_agents_dir` (inline, `src/engine/prompt.rs`).
  Given: `TempDir` w/ `.lazyspec/agents/expand.md` + `.lazyspec/agents/children.md`, each valid fm. When: `discover_prompts(repo_root, fs)`. Then: returns 2 `AgentPrompt`s (assert by `name` set). Also assert a non-`.md` file (`.lazyspec/agents/notes.txt`) is ignored.

- **AC2 — valid frontmatter parse.**
  Test `parses_full_frontmatter_into_agent_prompt` (inline).
  Given: a template w/ fm `name`, `description`, `mode: interactive`, `allowed_tools: "Read,Edit"` + a markdown body. When: parse. Then: `AgentPrompt { name, description, mode: RunMode::Interactive, allowed_tools: Some("Read,Edit"), body_template == <body> }` — assert each field incl. body retained verbatim as render template.

- **AC3 — mode defaults to headless.**
  Test `mode_defaults_to_headless_when_omitted` (inline).
  Given: fm w/ `name` + `description` only (no `mode`). When: parse. Then: `agent_prompt.mode == RunMode::Headless`. Companion: omitted `allowed_tools` → `None`.

- **AC4 — strict-undefined render success.**
  Test `renders_template_with_known_vars` (inline).
  Given: body `"Doc {{ document.id }} type {{ document.type }}"` + a selected doc. When: `render(&prompt, &ctx)`. Then: `Ok("Doc RFC-001 type rfc")` — fully substituted string, no leftover `{{ }}`.

- **AC5 — document context exposed.**
  Test `document_fields_resolve_from_selected_doc` (inline).
  Given: body referencing `document.id`, `.title`, `.type`, `.body`, `.status`, `.path`; a doc w/ known values + body read off disk. When: render. Then: each field equals that doc's value (`id`, `title`, `doc_type` Display, body raw, `status` Display, `path` display). Assert all six in one rendered string.

- **AC6 — child_types context exposed.**
  Test `child_types_resolve_from_parent_child_rules` (inline).
  Given: `Config` whose `[[rules]]` ParentChild declares `rfc -> story` (+ another parent rule for a different type to prove filtering); selected doc of type `rfc`; body `"{% for c in child_types %}{{ c }} {% endfor %}"`. When: render. Then: rendered contains `story` (the child type name for `rfc`) and NOT child types of unrelated parents. Companion: a type w/ no child rule → `child_types` empty list, loop renders empty (NOT an undefined error).

- **AC7 — unknown-variable render error.**
  Test `unknown_variable_is_render_error_not_empty` (inline).
  Given: body `"{{ document.bogus }}"` (or top-level `{{ nope }}`). When: render under strict-undefined. Then: returns `Err`; error string names the offending var. Assert it's an `Err`, NOT `Ok("")` — proves no silent empty substitution.

- **AC8 — malformed file skip-and-warn.**
  Test `malformed_frontmatter_file_is_skipped_with_warning` (inline).
  Given: dir w/ one valid template + one file missing `name` (and one w/ no fm at all). When: `discover_prompts`. Then: only the valid one returned; the malformed files are absent from the result. Warning behavior: discovery returns the loaded set + collects skipped-file warnings (assert the returned warnings/diagnostics contain the malformed paths) so the test stays behavioral w/o capturing stderr.

- **AC9 — no built-in prompts remain.**
  Test `no_builtin_prompt_builders_remain` (inline guard in `src/engine/prompt.rs` tests) + verify step.
  Then: `build_expand_prompt` / `build_create_children_prompt` symbols gone. Assert via `cargo build` (call sites removed) + grep guard: `rg "build_expand_prompt|build_create_children_prompt" src/` returns nothing. Confirm `init` writes no agent prompt (already true — `src/cli/init.rs` scaffolds no `.lazyspec/agents/` content; existing init tests unaffected).

- **AC10 — context lineage exposed.**
  Test `context_ancestors_and_related_resolve_from_resolve_chain` (inline).
  Given: a real `Store` over `RFC-001 <- STORY-001(implements RFC-001) <- ITERATION-001(implements STORY-001)` plus `ITERATION-001 related-to ADR-009`. Selected doc = `ITERATION-001`. Body iterates `{% for n in context.ancestors %}{{ n.type }} {{ n.id }}|{% endfor %}` + `{% for r in context.related %}{{ r.id }}{% endfor %}`. When: render (ctx built via `resolve_chain`). Then: `context.ancestors` = implements chain nearest-parent-first → `STORY-001` before `RFC-001` (target `ITERATION-001` EXCLUDED from ancestors); `context.related` contains `ADR-009`; each entry exposes same `document.*` fields (assert `n.body`/`r.title` resolve). Companion: forward/descendant direction OMITTED — assert a doc that `implements` the target does NOT appear in `context`.

## Changes

Tasks sequenced. Task 1 (dep) + Task 2 (types/module) are foundation. Task 9 (delete builders + fix call sites) lands last so the build stays green throughout. Each task is self-contained for a zero-context build subagent.

### Task 1 — add `minijinja` dep
**ACs:** 4, 5, 6, 7, 10 (render engine for all).
**Files:** `Cargo.toml`.
**Do:**
- Add `minijinja = "2"` to `[dependencies]` (alongside `serde_yaml = "0.9"`). minijinja is NOT yet a dependency — verify w/ `rg minijinja Cargo.toml Cargo.lock` (no hits before).
- No feature flags needed; the `prompt.rs` module is engine-layer and compiles in all builds.
**Verify:** `cargo build` resolves the new crate. `rg "^minijinja" Cargo.toml` shows the line.

### Task 2 — define `AgentPrompt` + `RunMode`, register module
**ACs:** 2, 3.
**Files:** NEW `src/engine/prompt.rs`; `src/engine.rs` (add `pub mod prompt;`).
**Do:**
- Register module: add `pub mod prompt;` to `src/engine.rs` (alongside `pub mod context;` etc.).
- In `prompt.rs` define:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Default)]
  pub enum RunMode {
      #[default]
      Headless,
      Interactive,
  }

  #[derive(Debug, Clone)]
  pub struct AgentPrompt {
      pub name: String,
      pub description: String,
      pub mode: RunMode,
      pub allowed_tools: Option<String>,
      pub body_template: String,
  }
  ```
- `RunMode` deserialize: derive/impl `Deserialize` w/ `#[serde(rename_all = "lowercase")]` so fm `mode: headless | interactive` parses; default `Headless` via `#[serde(default)]` on the fm field (NOT a serde default on the enum — required vs optional is enforced at the fm-struct level in Task 4).
- `name` + `description` REQUIRED; `mode` + `allowed_tools` OPTIONAL. (Enforcement lives in the raw-frontmatter struct in Task 4.)
**Verify:** `cargo build`. Inline unit tests `mode_defaults_to_headless_when_omitted` (AC3) + `parses_full_frontmatter_into_agent_prompt` (AC2) reference these types once Task 4 lands.

### Task 3 — template discovery under `.lazyspec/agents/`
**ACs:** 1, 8.
**Files:** `src/engine/prompt.rs`.
**Do:**
- Add `pub fn discover_prompts(repo_root: &Path, fs: &dyn FileSystem) -> (Vec<AgentPrompt>, Vec<PromptWarning>)` (or `-> Result<...>` + a warnings vec — keep skip-and-warn behavioral, NOT a hard error). Define a lightweight `PromptWarning { path: PathBuf, reason: String }` so AC8's test can assert on skipped files w/o capturing stderr; also emit `eprintln!`-style warning text for the human path.
- Read `<repo_root>/.lazyspec/agents/`. If dir absent → return empty (NOT an error; zero-defaults — no templates means nothing offered).
- For each `*.md` entry (filter on extension; ignore non-`.md`): read contents via `fs`, attempt parse (Task 4). On parse success → push `AgentPrompt`. On parse failure (missing/malformed fm) → push a `PromptWarning` and skip (AC8). Do NOT abort the whole discovery on one bad file.
- Use `FileSystem` seam (`src/engine/fs.rs`) for reads so tests run against `TempDir` w/o real-fs coupling per DICTUM-004.
**Verify:** inline `discovers_md_templates_under_agents_dir` (AC1) + `malformed_frontmatter_file_is_skipped_with_warning` (AC8). `cargo test -p lazyspec prompt`.

### Task 4 — frontmatter parse into `AgentPrompt`
**ACs:** 2, 3, 8.
**Files:** `src/engine/prompt.rs`.
**Do:**
- Add `pub fn parse_prompt(content: &str) -> Result<AgentPrompt>`.
- Reuse `crate::engine::document::split_frontmatter(content)` (`src/engine/document.rs:246`) → `(yaml, body)`. The `body` becomes `body_template` verbatim (retain as render template, AC2).
- Define a private raw fm struct: `#[derive(Deserialize)] struct RawPromptFm { name: String, description: String, #[serde(default)] mode: RunMode, #[serde(default)] allowed_tools: Option<String> }`. `serde_yaml::from_str` over the yaml. Missing `name`/`description` → serde error → propagated as `Err` (drives AC8 skip in Task 3); missing `mode` → `RunMode::default()` = `Headless` (AC3); missing `allowed_tools` → `None`.
- Map raw → `AgentPrompt { name, description, mode, allowed_tools, body_template: body }`.
**Verify:** inline `parses_full_frontmatter_into_agent_prompt` (AC2: all fields incl. `mode: interactive`, `allowed_tools: Some`, body verbatim), `mode_defaults_to_headless_when_omitted` (AC3), plus a malformed-fm case (missing `name`) returns `Err`. `cargo test -p lazyspec prompt`.

### Task 5 — render-context assembly: `document` + `child_types`
**ACs:** 5, 6.
**Files:** `src/engine/prompt.rs`.
**Do:**
- Build the minijinja context value. Define a builder, e.g. `pub fn build_render_context(store: &Store, config: &Config, doc: &DocMeta, fs: &dyn FileSystem) -> Result<minijinja::Value>` (or a serializable `RenderContext` struct minijinja serializes via `serde`).
- `document` map exposing: `id` (`doc.id`), `title` (`doc.title`), `type` (`doc.doc_type` Display/`as_str`), `body` (`store.get_body_raw(&doc.path, fs)?` — NOT a `DocMeta` field; read off disk per Context gotcha), `status` (`doc.status` Display/`as_str`), `path` (`doc.path` display string).
- `child_types`: list of child type NAMES for `doc.doc_type`. Derive from `config.rules` — for each `ValidationRule::ParentChild { parent, child, .. }` where `parent == doc.doc_type` Display, collect `child`. (This mirrors the existing derivation in `spawn_create_children`, `src/tui/views/keys.rs:235`.) Multiple matching rules → multiple child types. No matching rule → empty `Vec` (loop renders empty, must NOT be undefined — AC6 companion).
- Expose `child_types` as a sequence value so `{% for c in child_types %}` works.
**Verify:** inline `document_fields_resolve_from_selected_doc` (AC5), `child_types_resolve_from_parent_child_rules` (AC6 + empty-list companion). Build doc + config via `Config::default()` (carries the starter ParentChild rules `rfc->story`, `story->iteration`) over a `TempDir` `Store`. `cargo test -p lazyspec prompt`.

### Task 6 — render-context: `context` lineage via `resolve_chain`
**ACs:** 10.
**Files:** `src/engine/prompt.rs`.
**Do:**
- Extend `build_render_context` to add `context` w/ two sequences: `context.ancestors` + `context.related`.
- Source from `crate::engine::context::resolve_chain(store, &doc.id, depth)` (`src/engine/context.rs:29`). REUSE it; do NOT re-walk edges.
- `context.ancestors` = the `implements` chain. `resolve_chain` returns `nodes` root-first w/ the TARGET itself as the last node. For ancestors: take `nodes`, EXCLUDE the target (`n.doc.path != doc.path`), and present nearest-parent-first → reverse the remaining root-first node order so the immediate parent comes first (RFC: "nearest parent first"). Each entry = same `document.*` field map (id/title/type/body/status/path) — body read via `store.get_body_raw(&n.doc.path, fs)`.
- `context.related` = `resolved.related` (adjacent `related-to` `RelatedRef`s). Each entry = same `document.*` field map.
- OMIT `resolved.forward` (descendant/forward direction) entirely — RFC: a prompt's constraints come from what a doc implements, not what implements it.
- Pick a `depth` for the related ring (RFC examples use adjacent; depth `1` = directly adjacent related-to). Document the chosen depth in Notes.
- Each ancestor/related entry must expose the SAME `document.*` shape as the top-level `document`, so a template can do `{{ n.type }} {{ n.id }}: {{ n.body }}`. Factor a shared `doc_to_value(store, &DocMeta, fs)` helper so top-level `document` and each lineage entry are built identically.
**Verify:** inline `context_ancestors_and_related_resolve_from_resolve_chain` (AC10): chain `RFC-001 <- STORY-001 <- ITERATION-001` + `ITERATION-001 related-to ADR-009`; assert ancestors nearest-first (`STORY-001` then `RFC-001`, target excluded), related contains `ADR-009`, entry fields resolve, forward-direction doc excluded. `cargo test -p lazyspec prompt`.

### Task 7 — strict-undefined minijinja render
**ACs:** 4, 7.
**Files:** `src/engine/prompt.rs`.
**Do:**
- Add `pub fn render(prompt: &AgentPrompt, ctx: &minijinja::Value) -> Result<String>` (or take the `RenderContext`).
- Construct a `minijinja::Environment`, set `env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict)` so a reference to an unknown var is an ERROR, not a silent empty string (AC7).
- `env.render_str(&prompt.body_template, ctx)` (one-shot render of the body template against the context). Map minijinja errors → `anyhow::Error` whose message includes the underlying minijinja error (so the unknown-var name surfaces to the user, AC7).
**Verify:** inline `renders_template_with_known_vars` (AC4: `Ok` fully-substituted string) + `unknown_variable_is_render_error_not_empty` (AC7: `Err`, message names the var, NOT `Ok("")`). `cargo test -p lazyspec prompt`.

### Task 8 — public entrypoint wiring (load + render, no TUI/runner)
**ACs:** 1, 4, 5, 6, 10 (composition).
**Files:** `src/engine/prompt.rs`.
**Do:**
- Provide a cohesive public surface the later slices consume WITHOUT this slice touching the TUI: `discover_prompts` (Task 3) returns the loadable set; `build_render_context` (Tasks 5–6) + `render` (Task 7) turn one `AgentPrompt` + selected doc into a rendered string. Slice 4 (`ITERATION-184`) will call these from the dialog; slice 1's `AgentRunner::spawn` (`ITERATION-181`) consumes the rendered string. Do NOT wire any of that here — just expose the fns `pub`.
- Re-export nothing into TUI; keep all symbols under `crate::engine::prompt::*`. CLI/TUI never depend on each other (DICTUM principle 3).
**Verify:** `cargo build`. A small inline integration-style unit test composing `parse_prompt` → `build_render_context` → `render` end-to-end on a `TempDir` store (covers AC1/4/5/6/10 wiring). `cargo test -p lazyspec prompt`.

### Task 9 — DELETE builders + fix call sites
**ACs:** 9.
**Files:** `src/tui/agent.rs` (delete fns); `src/tui/views/keys.rs` (remove dead call sites).
**Do:**
- DELETE `build_expand_prompt` (`src/tui/agent.rs:108`) + `build_create_children_prompt` (`src/tui/agent.rs:97`) entirely. Engine ships ZERO default prompts (AC9).
- Fix call sites so the build stays green (both behind `#[cfg(feature = "agent")]`):
  - `src/tui/views/keys.rs:215` — the `action == "Expand document"` branch calls `build_expand_prompt`.
  - `src/tui/views/keys.rs:252` — `spawn_create_children` calls `build_create_children_prompt`.
  - These belong to the FIXED action dialog that slice 4 / STORY-135 (`ITERATION-184`) fully rewires to be template-driven. For THIS slice, remove/stub the two dead branches (drop the "Expand document" / "Create children" handling + `spawn_create_children`) so `cargo build --features agent` compiles. Leave a `// slice 4 / STORY-135: action dialog becomes template-driven` marker if a branch placeholder remains. Do NOT add a new template-dispatch path here — that's slice 4.
- Confirm `init` writes no agent prompt (no change needed — `src/cli/init.rs` already scaffolds none; AC9).
**Verify:** `cargo build && cargo build --features agent` green. Grep guard: `rg "build_expand_prompt|build_create_children_prompt" src/` returns nothing (AC9). Existing `src/tui/agent.rs` tests (`AgentRecord` roundtrip etc.) still pass — they don't touch the deleted fns. `cargo test --features agent`.

### README
Per CLAUDE.md: the CLI interface does NOT change in this slice (no new command/flag — `prompt.rs` is internal engine surface consumed by later slices). No README update required for this iteration. (Slice 4's dialog + slice 5's `[agents]` config are the README-affecting slices.)

## Notes

**Verified paths (file:symbol):**
- `src/tui/agent.rs:97` `build_create_children_prompt`, `:108` `build_expand_prompt` — the two builders to DELETE. Both `#[cfg(feature = "agent")]`-adjacent (TUI agent module). Call sites: `src/tui/views/keys.rs:215` (expand), `:252` (`spawn_create_children`, child type derived from `config.rules` ParentChild at `:235`).
- `src/engine/context.rs:29` `resolve_chain(store, id, depth) -> Result<ResolvedContext>`; `ResolvedContext` (`:22`) = `{ target, nodes: Vec<ContextNode>, forward: Vec<RelatedRef>, related: Vec<RelatedRef> }`. `ContextNode.doc: &DocMeta`; `RelatedRef.doc: &DocMeta`. `nodes` is root-first incl. the target as last node.
- `src/engine/document.rs:185` `struct DocMeta { path, title, doc_type, status, author, date, tags, provenance, related, validate_ignore, virtual_doc, id }` — NO `body` field. `split_frontmatter` at `:246`.
- `src/engine/store.rs:144` `Store::get_body_raw(path, fs) -> Result<String>` — reads body off disk; `:140` `get`, `:165` `resolve_shorthand`.
- `src/engine/config.rs:134` `TypeDef` (`parent_type` at `:149`); `ValidationRule::ParentChild { name, child, parent, link, severity }` (`:16`); `Config::default()` (test/test-support) carries starter ParentChild rules `rfc->story`, `story->iteration` (`default_rules()` `:419`).
- `src/cli/init.rs` `starter_config` + `scaffold_skeleton_files` — writes types/templates/convention/dictum skeletons only; NO agent prompt (AC9 already holds).
- `Cargo.toml:17` `serde_yaml = "0.9"` (reuse for fm parse); `:42` `agent = []` feature (TUI agent path gated by it).

**Decisions:**
- **Module placement:** new `src/engine/prompt.rs` (engine layer owns loading/parse/render per DICTUM principle 3). NOT in `src/tui/` — TUI/CLI consume it later; keep it I/O-trait-driven (`FileSystem` seam) so it's testable w/o real fs (DICTUM-004 fast/isolated). `src/engine/agent.rs` already exists (agent-id resolution / slice-1 trait home) — keep prompt concerns separate.
- **minijinja:** `Environment` + `UndefinedBehavior::Strict` (`env.set_undefined_behavior(...Strict)`) → unknown var = error surfaced to user, not silent empty string (AC7). Map minijinja `Error` → `anyhow` preserving the var name in the message.
- **`document.body` source:** `DocMeta` has no body; read via `Store::get_body_raw(path, fs)`. → render-context builder needs `&Store` + `&dyn FileSystem`, not just `&DocMeta`. Shared `doc_to_value(store, &DocMeta, fs)` helper builds the identical `document.*` shape for the top-level doc AND each ancestor/related entry.
- **`child_types` derivation:** from `config.rules` `ValidationRule::ParentChild` where `parent == doc.doc_type` → collect each `child`. Mirrors existing `spawn_create_children` logic (`keys.rs:235`). NOT from `TypeDef.parent_type` (that's the inverse, child→parent, and singleton-oriented). No matching rule → empty list (NOT undefined).
- **`ResolvedContext` → `context` var mapping:** `context.ancestors` = `resolved.nodes` MINUS the target, reversed to nearest-parent-first (resolve_chain emits root-first w/ target last). `context.related` = `resolved.related` (adjacent `related-to`). `resolved.forward` OMITTED (descendant/forward direction excluded per RFC — constraints come from what a doc implements). Each entry = same `document.*` field map via `doc_to_value`.
- **`resolve_chain` depth:** call w/ depth `1` for the `context.related` ring = directly-adjacent `related-to` docs (RFC describes "adjacent related-to documents"). The implements chain (`nodes`) is unbounded by depth (full ancestry) — depth only bounds the related-to BFS, which is the intended adjacent-only scope here.
- **Build-green ordering:** Task 9 (delete builders + strip dead `keys.rs` branches) lands last; the action dialog is authoritatively rewired in slice 4 / STORY-135 (`ITERATION-184`). This slice removes the dead path, does not add the new template-dispatch path.
