---
title: Template-driven TUI agent action dialog
type: iteration
status: accepted
author: agent
date: 2026-06-18
tags: []
related:
- implements: STORY-135
---

## Context

Slice 4 of RFC-046. TUI agent dlg (`a` on selected doc) currently fixed: built-in "Expand document" / "Create children" / "Custom prompt" baked in Rust (`build_expand_prompt`, `build_create_children_prompt` @ src/tui/agent.rs). RFC-046 → agent mode unopinionated → dlg must instead list prompt tmpls resolved for selected doc's TYPE, each shown by frontmatter `name` + `description`, + ONE freeform "Custom prompt". No engine-baked actions. Supersedes STORY-051 fixed set.

This slice wires HEADLESS dispatch + Custom only. Selecting headless entry → render tmpl → spawn bg via AgentRunner (through AgentSpawner) w/ rendered prompt + tmpl's `allowed_tools` → record AgentRecord → return immediately (TUI stays responsive). Empty resolved set → dlg offers Custom only (or nothing if type exposes no agents + no Custom path). Named-but-missing tmpls surfaced to user.

Deps (cross-slice anchors — DO NOT re-plan; consume as-is):
- Slice 1 (STORY-132): `AgentRunner` trait + `ClaudeP` impl + `AgentContext{prompt, allowed_tools, doc_path, session_id}` / `AgentHandle` @ src/engine/agent.rs. `AgentSpawner` (src/tui/agent.rs) owns records/polling, delegates process creation → injected `AgentRunner`.
- Slice 2 (STORY-133): `AgentPrompt{name, description, mode, allowed_tools, body_template}` + `enum RunMode{Headless, Interactive}` @ src/engine/prompt.rs; load+render entrypoint (discover `.lazyspec/agents/*.md`, minijinja strict render → rendered prompt string + metadata). `build_expand_prompt`/`build_create_children_prompt` GONE.
- Slice 3 (STORY-134): `resolve_agent_actions(type_agents: &[String], loaded: &[String]) -> ResolvedAgents{ actions, missing }` @ src/engine/agent.rs; `TypeDef.agents: Vec<String>` config. Use → resolve action set for doc's type.

Out of scope (deps, not this slice):
- AgentRunner trait / ClaudeP internals (slice 1) — consume runner.
- Tmpl load/render internals (slice 2) — consume rendered prompts + metadata.
- `resolve_agent_actions` internals + `TypeDef.agents` config shape (slice 3) — consume resolved set.
- INTERACTIVE dispatch: `mode: interactive`, terminal handover, `[agents] interactive` cmd, suspend/run/restore (slice 5 / STORY-136). Design dlg so slice 5 adds interactive-by-`mode`. Interactive entries must NOT crash dlg pre-slice-5 → DECISION: show-but-disabled (see Notes).

## Test Plan

One entry per AC. TUI tests exercise state transitions / dlg-list assembly thru public state API (DICTUM-004: behavioral, structure-insensitive, isolated `TempDir`, no spawned processes — assert via FAKE `AgentRunner` capturing `AgentContext`, never launch `claude`). All in `tests/integration/tui_agent_dialog_test.rs` (`#![cfg(feature = "agent")]`), reusing `TestFixture` + `App::new` harness already present.

- `test_dialog_lists_resolved_templates_by_name_desc` — AC1: given doc type resolves ≥1 tmpl → `a` → `agent_dialog.actions` lists one entry per resolved tmpl, each labelled frontmatter `name` + `description`.
- `test_custom_entry_present_when_agents_available` — AC2: dlg open for type exposing agents → "Custom prompt" entry present alongside resolved tmpls.
- `test_no_builtin_expand_or_create_children` — AC3: dlg open any doc → NO "Expand document"/"Create children" entries; only resolved tmpls + Custom.
- `test_headless_selection_builds_agent_context_via_fake_runner` — AC4: dlg open w/ resolved headless tmpl selected → confirm → fake `AgentRunner` captured one `AgentContext{prompt = rendered tmpl body, allowed_tools = tmpl's}` + `AgentRecord` created + dlg closed.
- `test_tui_responsive_after_headless_spawn` — AC5: after confirm headless → `handle_key` returns immediately, subsequent nav key (`j`) still moves selection (no block).
- `test_custom_prompt_spawns_with_runtime_default_tools` — AC6: select "Custom prompt", type text, submit → fake runner captured `AgentContext{prompt = typed text + doc context, allowed_tools = None}` (no restriction beyond runtime default).
- `test_empty_resolved_set_shows_only_custom` — AC7: doc type resolves no tmpls (but exposes agents) → `a` → actions == ["Custom prompt"] only; AND type exposes no agents + no Custom path → dlg not active / empty.
- `test_esc_cancels_no_spawn` — AC8: dlg open → `Esc` → `active == false` + fake runner captured zero contexts.
- `test_missing_template_report_surfaced` — (missing-report): type names tmpl w/ no loaded file → `resolve_agent_actions.missing` non-empty → surfaced via status-bar warning (see Notes); assert `app.status_bar_warnings` (or chosen field) carries missing name.

DICTUM-004 self-check: `cargo run --quiet -- convention --tags iteration,testing --json` — confirm isolated/fast/behavioral/structure-insensitive before writing tests.

## Changes

Implements STORY-135 (slice 4). Replaces fixed-action dlg w/ template-driven list + headless dispatch + freeform Custom. Assumes slices 1/2/3 merged (AgentRunner/AgentSpawner-delegation, prompt load+render, resolve_agent_actions + TypeDef.agents).

### Task 1: Delete fixed-action dlg code + builders

**ACs addressed:** AC3
**Files:**
- Modify: `src/tui/views/keys.rs` (handler ~169-256)
- Modify: `src/tui/agent.rs` (builders already deleted slice 2; verify gone)

**What to implement:**
Remove fixed-action wiring superseded by template-driven path:
- In `src/tui/views/keys.rs` `handle_agent_dialog_key` (~194-223 Enter arm): delete the `if action == "Expand document"` branch (~212-219, calls `build_expand_prompt` @ ~215) and the `else if action == "Create children"` branch (~220-222, calls `spawn_create_children`).
- Delete `fn spawn_create_children` entirely (~228-256).
- In `handle_normal_key` `a`-key arm (~651-677): delete the fixed `actions = vec!["Expand document", "Custom prompt"]` + `has_children → push("Create children")` logic; replaced wholesale in Task 2.
- Confirm `build_expand_prompt` / `build_create_children_prompt` already removed from `src/tui/agent.rs` by slice 2 (STORY-133). If lingering, delete + drop their `#[cfg(test)]` references.

Leave intact: dlg open/close, `Up`/`Down`/`Esc` nav (~176-193), `Custom prompt` branch (~203-206 → enters text_input), `handle_agent_text_input_key` (~258-299, rewired in Task 4).

**How to verify:** `cargo build --features agent` (compile error until Task 2 supplies new action build); `cargo clippy --features agent -- -D warnings`.

### Task 2: Build template-driven action list from resolve_agent_actions ∩ loaded tmpls

**ACs addressed:** AC1, AC2, AC3, AC7
**Files:**
- Modify: `src/tui/state/forms.rs` (`AgentDialog` struct ~188-216)
- Modify: `src/tui/state/app.rs` (App fields ~237-240; `App::new` ~302; populate loaded tmpls)
- Modify: `src/tui/views/keys.rs` (`a`-key arm in `handle_normal_key` ~651-677)

**What to implement:**
Dlg entries are no longer raw `Vec<String>` labels mapping to hardcoded behaviour. Introduce a structured entry so Enter can dispatch by `mode` + carry the tmpl's `allowed_tools` + `body_template`:

```rust
// src/tui/state/forms.rs
#[cfg(feature = "agent")]
#[derive(Clone)]
pub enum AgentAction {
    Template {
        name: String,
        description: String,
        mode: crate::engine::prompt::RunMode,
        allowed_tools: Option<String>,
        body_template: String,
    },
    Custom,
}

#[cfg(feature = "agent")]
pub struct AgentDialog {
    pub active: bool,
    pub selected_index: usize,
    pub actions: Vec<AgentAction>,
    pub doc_path: PathBuf,
    pub doc_title: String,
    pub text_input: Option<String>,
    pub missing: Vec<String>,
}
```
Update `AgentDialog::new()` (~206-215) to init `actions: Vec::new()`, `missing: Vec::new()`.

App holds the loaded tmpl set so dlg-open can resolve w/o re-discovering each keypress (DICTUM-004 fast; load once). Add field:
```rust
// src/tui/state/app.rs (~237-240, behind #[cfg(feature = "agent")])
#[cfg(feature = "agent")]
pub agent_prompts: Vec<crate::engine::prompt::AgentPrompt>,
```
In `App::new` (~302-...) load tmpls via slice-2 entrypoint (discover `.lazyspec/agents/*.md` → render-less metadata load; body rendered later on selection). Skip-and-warn malformed handled inside slice-2 loader. Init `agent_prompts: <loaded>` (empty Vec on load error — agents simply absent, ADR-015 zero-defaults). Test ctor (~1559) inits `agent_prompts: Vec::new()`.

Rewrite `a`-key arm (`handle_normal_key`, ~651-677):
```rust
#[cfg(feature = "agent")]
(KeyCode::Char('a'), _) => {
    if let Some(doc) = self.selected_doc_meta() {
        let doc_type_str = doc.doc_type.to_string();
        let doc_path = doc.path.clone();
        let doc_title = doc.title.clone();

        // type's declared agents (slice 3: TypeDef.agents)
        let type_agents: Vec<String> = config
            .documents
            .types
            .iter()
            .find(|t| t.name == doc_type_str)
            .map(|t| t.agents.clone())
            .unwrap_or_default();

        let loaded_names: Vec<String> =
            self.agent_prompts.iter().map(|p| p.name.clone()).collect();
        let resolved = crate::engine::agent::resolve_agent_actions(&type_agents, &loaded_names);

        let mut actions: Vec<crate::tui::state::forms::AgentAction> = resolved
            .actions
            .iter()
            .filter_map(|name| self.agent_prompts.iter().find(|p| &p.name == name))
            .map(|p| crate::tui::state::forms::AgentAction::Template {
                name: p.name.clone(),
                description: p.description.clone(),
                mode: p.mode.clone(),
                allowed_tools: p.allowed_tools.clone(),
                body_template: p.body_template.clone(),
            })
            .collect();

        // Custom always offered when type exposes agents (AC2);
        // when type exposes none AND nothing resolved → dlg not opened (AC7 nothing path).
        let type_exposes_agents = !type_agents.is_empty();
        if type_exposes_agents {
            actions.push(crate::tui::state::forms::AgentAction::Custom);
        }

        if actions.is_empty() {
            return; // AC7: nothing — no dlg
        }

        self.agent_dialog = crate::tui::state::forms::AgentDialog {
            active: true,
            selected_index: 0,
            actions,
            doc_path,
            doc_title,
            text_input: None,
            missing: resolved.missing.clone(),
        };
    }
}
```
DECISION (AC7 "Custom only"): when `type_exposes_agents` but `resolved.actions` empty → list == `[Custom]`. When type exposes NO agents → no dlg (return). This matches RFC "Custom only, or nothing if type exposes no agents and project authors no Custom path" — Custom is gated on the type declaring agents at all. (See Notes.)

Update dlg list nav arms (`handle_agent_dialog_key` ~179-193) — already index/len based, unaffected by entry type change.

**How to verify:** `cargo build --features agent`; `cargo clippy --features agent -- -D warnings`; `cargo test --features agent test_dialog_lists_resolved test_custom_entry_present test_no_builtin test_empty_resolved`.

### Task 3: Headless dispatch on selection (render → AgentSpawner/AgentRunner → AgentRecord)

**ACs addressed:** AC4, AC5
**Files:**
- Modify: `src/tui/views/keys.rs` (`handle_agent_dialog_key` Enter arm ~194-223)

**What to implement:**
Enter on a selected entry dispatches by entry kind + `mode`:
```rust
KeyCode::Enter => {
    let entry = self
        .agent_dialog
        .actions
        .get(self.agent_dialog.selected_index)
        .cloned();
    let Some(entry) = entry else { return; };

    match entry {
        AgentAction::Custom => {
            self.agent_dialog.text_input = Some(String::new()); // Task 4
        }
        AgentAction::Template { mode, name, allowed_tools, body_template, .. } => {
            match mode {
                RunMode::Headless => {
                    self.agent_dialog.active = false;
                    let doc_path = self.agent_dialog.doc_path.clone();
                    let doc_title = self.agent_dialog.doc_title.clone();
                    let full_path = self.store.root.join(&doc_path);
                    // render via slice-2 entrypoint: body_template + render ctx (document.*, child_types, context)
                    if let Ok(rendered) =
                        crate::engine::prompt::render_body(&body_template, &full_path, &self.store, config)
                    {
                        let _ = self.agent_spawner.spawn_with_tools(
                            &rendered,
                            allowed_tools.as_deref(),
                            &full_path,
                            &doc_title,
                            &name, // action label = tmpl name
                        );
                    }
                }
                RunMode::Interactive => {
                    // slice 5 / STORY-136: terminal handover not wired yet.
                    // show-but-disabled — Enter is a no-op pre-slice-5 (see Notes).
                }
            }
        }
    }
}
```
NOTE — `render_body` signature is slice-2 owned; treat as: `(body_template, doc_full_path, &Store, &Config) -> Result<String>` producing the rendered prompt (strict minijinja over `document.*`/`child_types`/`context`). Adapt to slice-2's actual entrypoint name/sig at impl time — do NOT re-implement rendering here (out of scope).

NOTE — `AgentSpawner::spawn` (src/tui/agent.rs ~139) currently takes no `allowed_tools` (it hardcoded `--allowedTools`). Slice 1 refactors spawning behind `AgentRunner` building `AgentContext{allowed_tools}`. This slice needs a per-call `allowed_tools`: use the `AgentSpawner` method that forwards `allowed_tools` into the injected runner's `AgentContext`. If slice-1's `AgentSpawner` already threads `Option<&str>` allowed_tools, call that; otherwise the call shown (`spawn_with_tools`) is the minimal forwarding wrapper — verify against slice-1's landed `AgentSpawner` API + use ITS signature (do NOT reshape the runner). AgentRecord still written by `AgentSpawner` (existing flow), action label = tmpl `name`.

AC5: headless `spawn*` returns immediately (bg child, existing non-blocking flow) → `handle_key` unblocked → TUI responsive. No code beyond not blocking.

**How to verify:** `cargo build --features agent`; `cargo clippy --features agent -- -D warnings`; `cargo test --features agent test_headless_selection_builds_agent_context test_tui_responsive_after_headless_spawn`.

### Task 4: Custom prompt text input → headless spawn, runtime-default tools

**ACs addressed:** AC6
**Files:**
- Modify: `src/tui/views/keys.rs` (`handle_agent_text_input_key` ~258-299)
- Modify: `src/tui/views/overlays.rs` (Custom Prompt render ~418-453 — label only, mostly unchanged)

**What to implement:**
Rewire `handle_agent_text_input_key` Enter (~269-290): spawn headless w/ typed text + selected doc as context, `allowed_tools = None` (no restriction beyond runtime default, AC6):
```rust
KeyCode::Enter => {
    let prompt = buffer.clone();
    let full_path = self.store.root.join(&self.agent_dialog.doc_path);
    self.agent_dialog.active = false;
    self.agent_dialog.text_input = None;

    if !prompt.is_empty() {
        let doc_title = self.agent_dialog.doc_title.clone();
        if let Ok(content) = self.fs.read_to_string(&full_path) {
            let full_prompt =
                format!("Here is the document:\n\n{content}\n\nUser request: {prompt}");
            let _ = self.agent_spawner.spawn_with_tools(
                &full_prompt,
                None, // AC6: no allowed_tools restriction beyond runtime default
                &full_path,
                &doc_title,
                "Custom prompt",
            );
        }
    }
}
```
Keep `Esc`/`Backspace`/`Char(c)` arms (~266-298). Custom dispatch is HEADLESS only this slice (no interactive Custom). Render (`draw_agent_dialog` text_input branch ~418-453) unchanged — already shows "Custom Prompt — {doc_title}" + buffer.

**How to verify:** `cargo build --features agent`; `cargo test --features agent test_custom_prompt_spawns_with_runtime_default_tools`.

### Task 5: Surface named-but-missing-template report

**ACs addressed:** (missing-report, per scope)
**Files:**
- Modify: `src/tui/views/keys.rs` (`a`-key arm — already captures `resolved.missing` into `agent_dialog.missing` in Task 2)
- Modify: `src/tui/views/overlays.rs` (`draw_agent_dialog` list branch ~456-487)

**What to implement:**
DECISION: surface `resolved.missing` (type named a tmpl w/ no loaded file → user authored a reference but not the file) inline in the dlg as a dimmed footer line, so it's visible at the moment of action (no separate screen). In `draw_agent_dialog` (list branch ~456-487), when `!dialog.missing.is_empty()`, append a footer line below the list:
```rust
// after building `items`, before render:
if !dialog.missing.is_empty() {
    items.push(ListItem::new(Line::from(Span::styled(
        format!("  ! missing templates: {}", dialog.missing.join(", ")),
        Style::default().fg(Color::DarkGray),
    ))));
}
```
Footer is non-selectable: `handle_agent_dialog_key` nav clamps to `actions.len()` (the missing line is render-only, NOT in `actions`), so `Up`/`Down`/Enter never land on it. Adjust `content_height` (~457) to `+1` when `missing` non-empty so popup grows to fit.

**How to verify:** `cargo build --features agent`; `cargo test --features agent test_missing_template_report_surfaced`; manual `cargo run` visual check (automated test asserts `agent_dialog.missing` populated).

### Task 6: Tests

**ACs addressed:** AC1, AC2, AC3, AC4, AC5, AC6, AC7, AC8, missing-report
**Files:**
- Modify: `tests/integration/tui_agent_dialog_test.rs` (delete obsolete fixed-action tests, add slice-4 tests)

**What to implement:**
Delete obsolete tests asserting fixed actions: `test_no_create_children_for_iteration` (~101-133), `test_create_children_for_rfc` (~137-160) — those built-in actions are GONE (AC3). Keep `test_esc_closes_dialog` (→ rename/extend `test_esc_cancels_no_spawn`), `test_unhandled_key_ignored`, `test_a_key_empty_list`.

Add fake `AgentRunner` capturing `AgentContext` (DICTUM-004: trait seam, no process spawn). Inject into `App.agent_spawner` via slice-1's `AgentSpawner` ctor that accepts an injected `AgentRunner` (use that ctor in tests; verify slice-1 API). Fake records each `AgentContext{prompt, allowed_tools, doc_path, session_id}` into a shared `Vec`.

Add per-AC tests (see Test Plan for full list + assertions): `test_dialog_lists_resolved_templates_by_name_desc`, `test_custom_entry_present_when_agents_available`, `test_no_builtin_expand_or_create_children`, `test_headless_selection_builds_agent_context_via_fake_runner`, `test_tui_responsive_after_headless_spawn`, `test_custom_prompt_spawns_with_runtime_default_tools`, `test_empty_resolved_set_shows_only_custom`, `test_esc_cancels_no_spawn`, `test_missing_template_report_surfaced`.

Fixtures: write `.lazyspec/agents/<name>.md` tmpl files into `TestFixture` root + set type's `agents = [...]` in fixture config so resolution has input. For empty-set: type w/ `agents = []` (or absent). For missing: type names a tmpl w/ no file → assert `agent_dialog.missing` carries the name.

**How to verify:** `cargo test --features agent tui_agent_dialog`; full `cargo test --features agent`.

### Task 7: README

**ACs addressed:** n/a (docs)
**Files:**
- Modify: `README.md`

**What to implement:**
README currently documents the TUI only as a general dashboard (line ~111) w/ NO per-key agent-dlg table → fixed Expand/Create-children were never documented, so removing them needs no edit. If a TUI keybinding section is added elsewhere in this slice's scope, note `a` opens the template-driven agent dlg (lists user-authored `.lazyspec/agents/*.md` tmpls + Custom prompt). DECISION: minimal/no README change required this slice (no fixed-action docs to remove, no new CLI surface). Verify w/ grep: `grep -n "Expand document\|Create children\|agent dialog" README.md` → expect no stale references.

**How to verify:** `grep -n "Expand document\|Create children" README.md` returns nothing stale.

## Notes

Verified paths (file:symbol):
- `src/tui/views/keys.rs#handle_normal_key` — `a`-key arm at ~651-677 (fixed actions built here; rewired Task 2). `#[cfg(feature = "agent")]`.
- `src/tui/views/keys.rs#handle_agent_dialog_key` — ~169-226; Enter arm ~194-223 (Expand @ ~212-219 calls `build_expand_prompt` @ ~215; Create-children @ ~220-222). `spawn_create_children` ~228-256. `handle_agent_text_input_key` ~258-299 (Custom spawn @ ~269-290).
- `src/tui/agent.rs#AgentSpawner` — ~119-207; `spawn` @ ~139-171 (currently hardcodes `Command::new("claude")` + `--allowedTools "Read,Edit,Write,Bash(lazyspec *)"` @ ~148-151; slice 1 moves into `ClaudeP` behind `AgentRunner`). `AgentRecord` ~17-26. `build_expand_prompt`/`build_create_children_prompt` ~97-117 (deleted slice 2).
- `src/tui/state/forms.rs#AgentDialog` — ~188-216 (struct + `new`). Extended w/ `AgentAction` enum + `missing` field Task 2.
- `src/tui/state/app.rs` — `App` agent fields ~237-240; `App::new` ~302; test ctor ~1559. Add `agent_prompts` field.
- `src/tui/views/overlays.rs#draw_agent_dialog` — ~414-488; text_input branch ~418-453, list branch ~456-487. `src/tui/views.rs` dispatch ~228-229.
- `src/tui/infra/event_loop.rs` — `handle_app_event` @ ~121 calls `app.handle_key(...)` w/ `&Config` @ ~124; `poll_finished` reaping happens in loop (slice-1 owned).
- `tests/integration/tui_agent_dialog_test.rs` — `TestFixture` + `App::new` harness; `press()` helper ~7-9.

Cross-slice anchors (consume, NOT re-plan):
- `AgentRunner`/`ClaudeP`/`AgentContext`/`AgentHandle` @ src/engine/agent.rs (slice 1). `AgentSpawner` delegates spawn → injected runner.
- `AgentPrompt{name, description, mode, allowed_tools, body_template}` + `RunMode{Headless, Interactive}` @ src/engine/prompt.rs (slice 2) + load+render entrypoint. `render_body`-style call name/sig is slice-2 owned — adapt at impl.
- `resolve_agent_actions(type_agents, loaded) -> ResolvedAgents{actions, missing}` @ src/engine/agent.rs + `TypeDef.agents: Vec<String>` (slice 3).

Decisions:
- **Loaded tmpl set:** loaded ONCE in `App::new` into `App.agent_prompts` (discover `.lazyspec/agents/*.md` via slice-2 entrypoint). Dlg-open resolves against this in-memory set → no per-keypress disk I/O (DICTUM-004 fast). Empty/error → empty Vec → agents absent (ADR-015 zero-defaults).
- **Resolved actions:** `a`-key → `resolve_agent_actions(type.agents, loaded_names)` → map `resolved.actions` → loaded `AgentPrompt` → `AgentAction::Template{...}`. `resolved.missing` → `agent_dialog.missing`.
- **Where rendering happens:** ON SELECTION (Enter), not at dlg-open. Dlg holds `body_template` (raw); render only the chosen tmpl → avoids rendering N tmpls every open + surfaces strict-undefined errors only for the picked one.
- **Custom → no allowed_tools:** Custom maps to a headless spawn w/ `allowed_tools = None` → runtime default (slice-1 `ClaudeP` omits `--allowedTools` when None). Prompt = "Here is the document:\n\n{content}\n\nUser request: {typed}".
- **AC7 Custom-only vs nothing:** Custom entry gated on type DECLARING agents (`!type_agents.is_empty()`). Type exposes agents but resolves zero tmpls → `[Custom]`. Type exposes NO agents → no dlg opened (return). Matches RFC "Custom only, or nothing if the type exposes no agents and the project authors no Custom path".
- **Interactive pre-slice-5:** SHOW-BUT-DISABLED. Interactive `AgentPrompt` entries appear in list (so user sees them) but Enter is a no-op `RunMode::Interactive => {}` — does NOT crash, does NOT spawn. Slice 5 fills that arm w/ suspend/run/restore terminal handover. (Alt rejected: hiding them → user can't tell tmpl exists; show-disabled keeps discoverability + non-crashing per scope.)
- **Missing-report surface:** dimmed non-selectable footer line in the dlg ("! missing templates: <names>") — visible at action time, no extra screen. Nav clamps to `actions.len()` (footer render-only).
- **README:** no fixed-action docs existed → nothing stale to remove; no new CLI surface → minimal/no edit (grep-verify).
