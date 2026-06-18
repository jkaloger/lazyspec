---
title: Interactive agent run mode with terminal handover
type: iteration
status: draft
author: agent
date: 2026-06-18
tags: []
related:
- implements: STORY-136
---

## Context

Slice 5 (final) of RFC-046. Headless agents run `claude -p` in background; no way to open *live* interactive session on doc -- way TUI `e` hands terminal to `$EDITOR`, but for live agent (`claude`, `opencode`, `pi`, custom shell/tmux). Tool varies by machine/project -> configured once as global `[agents] interactive` shell cmd in toml, not hardcoded. This slice gives tmpl `mode: interactive` field its behaviour: select such tmpl -> suspend TUI -> hand terminal to configured cmd -> restore. Mirrors `run_editor`. See ADR-017.

Why interactive: headless = fire-and-forget background edit; interactive = pair/converse live on doc, full terminal. Different shape -> different dispatch.

Zero-defaults (ADR-015): engine ships NO built-in interactive cmd. `[agents] interactive` unset -> interactive actions unavailable: tmpls with `mode: interactive` do NOT run, are NOT offered. No fallback cmd, nothing for `init` to write.

Deps on siblings (anchors, not built here):
- Slice 2 (STORY-133): `enum RunMode { Headless, Interactive }` + `struct AgentPrompt { ..., mode: RunMode, allowed_tools: Option<String>, body_template: String }` in `src/engine/prompt.rs`. `mode` already parsed (default `Headless`). Tmpl load+render entrypoint produces rendered prompt string + metadata.
- Slice 4 (STORY-135): tmpl-driven action dialog (`src/tui/views/keys.rs`) lists resolved tmpls, dispatches HEADLESS entries thru `AgentSpawner`/`AgentRunner`. This slice ADDS interactive dispatch branch keyed on `mode`.

Two arch dicta (CONVENTION principles 3 + 6):
- dictum 3 (engine = no I/O assumptions; CLI/TUI never depend on each other): ENGINE builds the `Command` (program=`bash`, args=`["-lc", cmd]`, env `LAZYSPEC_PROMPT`/`LAZYSPEC_DOC_PATH`); it NEVER touches terminal state. Alternate-screen / raw-mode dance stays in TUI event loop. TUI suspends before run, restores + drains stdin after -- mirrors `run_editor`.
- dictum 6 (add indirection only at two concrete uses): interactive = single configured behaviour (run whatever project configured) -> earns NO trait (contrast headless `AgentRunner` trait, which admits multiple backends). Folding interactive onto `AgentRunner` would force claude-specific `ClaudeP` to run arbitrary opencode/tmux cmds. So interactive = plain engine fn building a `Command`, no trait.

Prompt passed by env (`$LAZYSPEC_PROMPT`), NOT interpolated into cmd line -> avoids shell-quoting rendered markdown (backticks, quotes, `$`). Cmd references the env vars: `claude "$LAZYSPEC_PROMPT"`, `tmux new-window claude "$LAZYSPEC_PROMPT"`, etc.

Interactive run = foreground + synchronous; like `e`, leaves NO `AgentRecord`. `allowed_tools` ignored for interactive (configured cmd owns its own tool policy).

## Test Plan

DICTUM-004 (testing): isolated (own `TempDir`/`Config`), behaviour-focused, fakes only at trait seams. Verify conventions before writing: `cargo run --quiet -- convention --tags iteration,testing --json`.

Engine tests in `src/engine/agent_interactive.rs` (`#[cfg(test)] mod tests`) unless noted:

- **AC1 -- interactive cmd parsed from `[agents]`.** `fn agents_config_parses_interactive` (`src/engine/config.rs` tests). Given toml `[agents]\ninteractive = 'claude "$LAZYSPEC_PROMPT"'` + valid `[[types]]`/`[[relationships]]` preamble; when `Config::parse`; then `config.agents.interactive == Some("claude \"$LAZYSPEC_PROMPT\"".into())`.
- **AC1 -- `[agents]` absent => None.** `fn agents_config_none_when_absent` (`config.rs` tests). Given preamble w/ no `[agents]`; when parse; then `config.agents.interactive.is_none()`.
- **AC3 -- interactive `Command` program/args/env.** `fn interactive_command_program_args_env` (`agent_interactive.rs`). Given cmd `claude "$LAZYSPEC_PROMPT"`, rendered prompt `"hello body"`, doc_path `/tmp/x.md`; when `build_interactive_command(cmd, prompt, doc_path)`; then program == `bash`, args == `["-lc", "claude \"$LAZYSPEC_PROMPT\""]`, env contains `LAZYSPEC_PROMPT=hello body` + `LAZYSPEC_DOC_PATH=/tmp/x.md`. Assert via `std::process::Command` getters (`get_program`, `get_args`, `get_envs`).
- **AC6 -- custom shell/tmux cmd.** `fn interactive_command_custom_tmux` (`agent_interactive.rs`). Given cmd `tmux new-window claude "$LAZYSPEC_PROMPT"`; when `build_interactive_command`; then program==`bash`, args==`["-lc", "tmux new-window claude \"$LAZYSPEC_PROMPT\""]`, env carries both vars. (Same builder; asserts arbitrary wrapper passes thru verbatim.)
- **AC7 -- `allowed_tools` ignored for interactive.** `fn interactive_command_ignores_allowed_tools` (`agent_interactive.rs`). Given `build_interactive_command` takes NO `allowed_tools` param; when built from a tmpl whose `allowed_tools` is `Some(...)`; then cmd args/env carry no `--allowedTools` / `allowed_tools` ref (builder signature physically cannot pass it).
- **AC5 -- unset => interactive unavailable (engine).** `fn interactive_unavailable_when_unset` (`agent_interactive.rs`). Given `config.agents.interactive == None`; when resolving dispatchable actions for a doc whose resolved tmpls include `mode: interactive` ones; then interactive tmpls excluded from offered set; headless tmpls unaffected.
- **AC2/AC4 -- TUI suspend/run/restore via seam.** `fn interactive_dispatch_triggers_suspend_run_restore` (`src/tui/views/keys.rs` or `src/tui/state/` tests). Selecting `mode: interactive` entry sets `app.interactive_request = Some(InteractiveRequest { cmd, prompt, doc_path })` (request field, drained by event loop -- same pattern as `editor_request`/`resume_request`). Assert request populated w/ correct cmd/prompt/doc_path; assert NO process spawned in handler (terminal handover happens in event loop). The real leave/restore is event-loop glue (mirrors `run_editor`), asserted by request-field seam not a live terminal.
- **AC5 -- interactive entries hidden when unset (TUI).** `fn interactive_entries_hidden_when_unset` (TUI tests). Given `config.agents.interactive == None` + doc whose resolved tmpls include interactive ones; when agent dialog opened (`a`); then `agent_dialog.actions` lists no interactive entries; headless entries present.
- **AC2 -- mode-distinction labelling.** `fn dialog_labels_interactive_entries` (TUI tests). Given `[agents] interactive` set + interactive tmpl resolved; when dialog built; then interactive entry visibly marked (hands-over-terminal indicator) distinct from headless entries.
- **AC7 -- no `AgentRecord` for interactive.** `fn interactive_run_leaves_no_record` (TUI tests). Given interactive entry dispatched (via request seam); when handled; then `app.agent_spawner.records` len unchanged + no `save_record` call (interactive path never touches `AgentSpawner`).

## Changes

### Task 1: `AgentsConfig` + `[agents]` config load

**ACs:** AC1, AC5 (engine half of zero-defaults).

**Files:**
- `src/engine/config.rs` -- add `AgentsConfig`, wire into `Config` + `RawConfig` + `parse_inner` + `Default`.

**Impl:**
1. Add struct after `GithubConfig`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentsConfig {
    #[serde(default)]
    pub interactive: Option<String>,
}
```
   `interactive` = ONLY global `[agents]` key (per-type allowlist + per-tmpl tool scope stay elsewhere). `Option` + zero-default: absent => `None` => interactive unavailable.
2. Add field to `Config` (after `coordination`):
```rust
    #[serde(default)]
    pub agents: AgentsConfig,
```
3. Add to `RawConfig`:
```rust
    #[serde(default)]
    agents: Option<AgentsConfig>,
```
4. In `parse_inner`, build into returned `Config`:
```rust
            agents: raw.agents.unwrap_or_default(),
```
5. In `#[cfg(any(test, feature = "test-support"))] Default for Config`, add `agents: AgentsConfig::default(),`.

**Verify:**
- `cargo build --features agent`
- `cargo test --features agent agents_config` -> AC1 parse + absent-None pass
- `cargo clippy --features agent -- -D warnings`

### Task 2: engine fn builds interactive `Command` (program/args/env, no terminal)

**ACs:** AC3, AC6, AC7.

**Files:**
- `src/engine/agent_interactive.rs` -- NEW module: `build_interactive_command`.
- `src/engine/mod.rs` (or wherever engine modules declared) -- add `pub mod agent_interactive;`.

**Impl:**
```rust
use std::path::Path;
use std::process::Command;

/// Build the foreground interactive Command from the configured `[agents] interactive`
/// shell string. Run via `bash -lc`; the rendered prompt + doc path are passed by
/// environment (LAZYSPEC_PROMPT / LAZYSPEC_DOC_PATH) so the command references them
/// without the engine shell-quoting rendered markdown. The engine never touches
/// terminal state (CONVENTION dictum 3); the caller (TUI) owns suspend/restore.
/// Single configured behaviour -> no trait (dictum 6).
pub fn build_interactive_command(cmd: &str, prompt: &str, doc_path: &Path) -> Command {
    let mut command = Command::new("bash");
    command
        .arg("-lc")
        .arg(cmd)
        .env("LAZYSPEC_PROMPT", prompt)
        .env("LAZYSPEC_DOC_PATH", doc_path);
    command
}
```
   Signature deliberately omits `allowed_tools` (AC7: interactive ignores it -- the configured cmd owns tool policy). Engine builds Command only; NO `.status()`/`.spawn()` here, NO crossterm.

**Verify:**
- `cargo build --features agent`
- `cargo test --features agent interactive_command` -> AC3 program/args/env, AC6 tmux, AC7 no-allowed-tools pass
- `cargo clippy --features agent -- -D warnings`

### Task 3: TUI suspend/run/restore dispatch (mirror `run_editor`)

**ACs:** AC2, AC4, AC7 (no record).

**Files:**
- `src/tui/state/app.rs` -- add `interactive_request: Option<InteractiveRequest>` field (init `None` in both `App::new` + test ctor near `resume_request`).
- `src/tui/state/forms.rs` (or `app.rs`) -- define `InteractiveRequest { cmd: String, prompt: String, doc_path: PathBuf }` (`#[cfg(feature = "agent")]`).
- `src/tui/infra/event_loop.rs` -- drain `interactive_request` after the `resume_request` block, run via suspend/restore.

**Impl:**
1. Request type (mirrors how `editor_request`/`resume_request` are drained in the loop):
```rust
#[cfg(feature = "agent")]
pub struct InteractiveRequest {
    pub cmd: String,
    pub prompt: String,
    pub doc_path: std::path::PathBuf,
}
```
2. Event-loop drain block, AFTER `resume_request` block (~line 524 of `event_loop.rs`), mirroring `run_editor`'s leave/run/restore + the resume block's stdin-lock + channel-drain discipline:
```rust
        #[cfg(feature = "agent")]
        if let Some(req) = app.interactive_request.take() {
            let _stdin_guard = stdin_lock.lock().unwrap();
            while rx.try_recv().is_ok() {}

            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            disable_raw_mode()?;
            let mut command = crate::engine::agent_interactive::build_interactive_command(
                &req.cmd,
                &req.prompt,
                &req.doc_path,
            );
            let _ = command.status();
            enable_raw_mode()?;
            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
            terminal.clear()?;

            drain_stdin();
            while rx.try_recv().is_ok() {}
            drop(_stdin_guard);
            let root = app.store.root().to_path_buf();
            app.store = Store::load(&root, config)?;
            app.refresh_validation(config);
        }
```
   Engine builds the Command; TUI does leave-alt-screen + disable-raw before, enable-raw + enter-alt + clear + drain after (AC2 suspend, AC4 restore). Foreground/synchronous: `.status()` blocks until exit. NO `AgentRecord` written here (AC7) -- interactive never touches `AgentSpawner`. Reload store after (agent may have edited the doc), same as resume block.

**Verify:**
- `cargo build --features agent`
- `cargo test --features agent interactive_dispatch` -> AC2/AC4 seam (request populated, no spawn in handler) pass; `cargo test --features agent interactive_run_leaves_no_record` -> AC7 pass
- `cargo clippy --features agent -- -D warnings`

### Task 4: dialog gating (offer interactive only when configured) + mode-distinction labels

**ACs:** AC2 (labelling), AC5 (gating).

**Files:**
- `src/tui/views/keys.rs` -- dispatch branch on `mode` in `handle_agent_dialog_key` (extends slice 4 wiring); set `interactive_request` for interactive entries.
- the dialog-build site (slice 4 populates `agent_dialog.actions` from resolved tmpls) -- gate + label interactive entries. Likely `src/tui/state/app.rs` open-dialog fn.

**Impl:**
1. **Gating (AC5):** when building `agent_dialog.actions` from resolved tmpls, SKIP entries whose `mode == RunMode::Interactive` if `config.agents.interactive.is_none()`. Unset => interactive tmpls neither listed nor runnable; headless unaffected.
2. **Labelling (AC2):** interactive entries that ARE offered get a visible mark distinct from headless (e.g. suffix indicating terminal handover), so user knows selection hands over the terminal. Each entry already carries its `name`/`description` (slice 4); add the mode indicator.
3. **Dispatch (AC2/AC4):** in the `Enter` arm of `handle_agent_dialog_key`, branch on the selected tmpl's `mode`:
   - `RunMode::Headless` -> existing slice-4 path (`AgentSpawner`/`AgentRunner`, records `AgentRecord`).
   - `RunMode::Interactive` -> render the tmpl body for the doc (slice-2 render entrypoint), read `config.agents.interactive` (`Some` guaranteed since gated), set:
```rust
self.agent_dialog.active = false;
self.interactive_request = Some(InteractiveRequest {
    cmd: interactive_cmd.clone(),
    prompt: rendered_body,
    doc_path: self.store.root.join(&doc_path),
});
```
   Sets request only -- event loop (Task 3) does the suspend/run/restore. NO `AgentSpawner` call -> no `AgentRecord` (AC7).

**Verify:**
- `cargo build --features agent`
- `cargo test --features agent interactive_entries_hidden_when_unset dialog_labels_interactive_entries` -> AC5 gating, AC2 labelling pass
- `cargo clippy --features agent -- -D warnings`

### Task 5: README -- `[agents] interactive` config + tmpl `mode: interactive`

**ACs:** docs for AC1/AC3/AC6 (per CLAUDE.md: update README on cli/config change).

**Files:**
- `README.md` -- Configuration section (`<details>` block, after `### Templates` ~line 442, still inside the details).

**Impl:** add `### Agents` subsection documenting the global `[agents]` block:
```toml
[agents]
interactive = 'claude "$LAZYSPEC_PROMPT"'
# or 'opencode -p "$LAZYSPEC_PROMPT"', 'pi', 'tmux new-window claude "$LAZYSPEC_PROMPT"'
```
   Prose: `interactive` = shell cmd run via `bash -lc` when an interactive-mode agent tmpl is selected in the TUI agent dialog; the rendered prompt is exported as `$LAZYSPEC_PROMPT` and the document path as `$LAZYSPEC_DOC_PATH`, which the command references. Zero-defaults: unset => interactive tmpls are not offered. Note tmpl frontmatter `mode: interactive` (vs default `headless`) marks a tmpl for terminal handover; `allowed_tools` is ignored for interactive (the configured command owns its tool policy).

**Verify:**
- `cargo run --quiet -- validate --json` (docs unaffected; README not validated, but confirm no breakage)
- visual: README renders `[agents]` block + `mode: interactive` note

## Notes

**Verified paths (file:symbol):**
- `src/tui/infra/event_loop.rs:40` `run_editor` -- suspend/run/restore template: `LeaveAlternateScreen` + `disable_raw_mode()` -> `Command::...status()` -> `enable_raw_mode()` + `EnterAlternateScreen` + `terminal.clear()`. The `resume_request` block (`event_loop.rs:505`) is the closer template: it adds the `stdin_lock` guard + `while rx.try_recv().is_ok() {}` channel drains + `drain_stdin()` + store reload. Task 3 mirrors the resume block exactly, swapping `Command::new("claude").args(["--resume", id])` for `build_interactive_command(...)`.
- `src/engine/config.rs` -- `Config` (`pub struct Config`, ~line 258), `RawConfig` (~line 344), `parse_inner` (~line 501), test-only `Default for Config` (~line 444). `AgentsConfig` slots beside `GithubConfig`/`CoordinationConfig` (same `Option`/`#[serde(default)]` pattern).
- `src/tui/views/keys.rs:169` `handle_agent_dialog_key` -- `Enter` arm (~line 194) is the dispatch point; slice 4 replaces the hardcoded `"Expand document"`/`"Create children"` string-match with tmpl-driven entries. Task 4 adds the `mode` branch on top of slice 4's structure.
- `src/tui/state/forms.rs:188` `AgentDialog { active, selected_index, actions, doc_path, doc_title, text_input }`; `src/tui/state/app.rs:244` `editor_request: Option<PathBuf>`, `:267` `resume_request: Option<String>` -- `interactive_request` follows these (request field drained by event loop).
- `src/tui/agent.rs:18` `AgentRecord`, `:139` `AgentSpawner::spawn` writes the record -- interactive path NEVER calls these (AC7).

**Decisions:**
- **Engine Command-builder signature:** `build_interactive_command(cmd: &str, prompt: &str, doc_path: &Path) -> Command`. Returns the built `Command`; caller runs it. No `allowed_tools` param (AC7: interactive ignores it). No terminal/crossterm types crossing the engine boundary (dictum 3).
- **TUI reuses run_editor's suspend/restore:** Task 3 copies the `resume_request` drain block (stdin lock + channel drains + leave/run/restore + `drain_stdin` + store reload), substituting the engine-built interactive Command. Keeps terminal-state ownership in the event loop, not the engine.
- **Env-var, not interpolation:** prompt passed as `LAZYSPEC_PROMPT` env (+ `LAZYSPEC_DOC_PATH`), cmd references `"$LAZYSPEC_PROMPT"`. Interpolating rendered markdown into the `bash -lc` string would require quoting backticks/quotes/`$`; env sidesteps it. `bash -lc` (not raw argv) so custom shell/tmux wrappers + pipes work (AC6).
- **Gating when unset (zero-defaults, ADR-015):** `config.agents.interactive == None` -> interactive tmpls skipped at dialog-build (AC5, Task 4) AND have no cmd to run. No engine default, nothing for `init` to write. Headless path fully independent of `[agents]`.
- **No trait for interactive (dictum 6):** single configured behaviour -> plain fn building a `Command`, not a second `AgentRunner` method. Headless keeps its trait (multiple backends admitted). Per ADR-017.
- **No AgentRecord (AC7):** interactive is foreground/synchronous like `e`; the dispatch sets `interactive_request` and never touches `AgentSpawner`, so no record is saved or polled.
