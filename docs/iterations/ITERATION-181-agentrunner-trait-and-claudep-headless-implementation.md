---
title: AgentRunner trait and ClaudeP headless implementation
type: iteration
status: complete
author: agent
date: 2026-06-18
tags: []
related:
- implements: STORY-132
---

## Context

Slice 1 of RFC-046 (supersedes RFC-016). Just the seam -> NO behaviour change to existing agent actions.

Today `AgentSpawner::spawn` (`src/tui/agent.rs:139`) builds process inline: `Command::new("claude").args(["-p", prompt])` + fixed `--allowedTools "Read,Edit,Write,Bash(lazyspec *)"`. No seam -> spawn exercised only by launching real `claude`, runtime locked to one binary. Dictum 4: subprocess spawn is I/O; I/O boundary -> trait so prod + test share one iface.

This slice:
- New engine trait `AgentRunner { fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle>; }` + `AgentContext` + `AgentHandle`. Lives in ENGINE (dictum 3: engine has no terminal/TUI assumptions; subprocess I/O behind trait per dictum 4).
- `ClaudeP` = sole v1 impl. Runs `claude -p <prompt> --session-id <id>`, stdio discarded, `--allowedTools` only when `allowed_tools` is `Some`. Hardcoded `Command::new("claude")` MOVES here.
- `AgentSpawner` (`src/tui/agent.rs`) refactored: OWNS records/polling/status/history, DELEGATES process creation to injected `AgentRunner`. Fake runner injectable -> tests assert on `AgentContext` w/o launching process (dictum 4: same trait prod + test).
- Relocate run-history dir: `$HOME/.lazyspec/agents/` -> `<repo>/.lazyspec/cache/agents/` (History location cleanup). `.lazyspec/agents/` reserved for user templates (slice 2).

**Dependencies (other slices, NOT planned here):** slice 2/STORY-133 (prompt templates + minijinja; DELETES `build_expand_prompt`/`build_create_children_prompt` -> here they stay UNCHANGED), slice 3/STORY-134 (per-type cfg), slice 4/STORY-135 (TUI dialog rewrite), slice 5/STORY-136 (interactive mode). minijinja NOT added here.

**Three-layer reminder (dictum 3):** trait + structs live on engine (`src/engine/agent.rs`); TUI (`src/tui/agent.rs`) depends on engine, never on CLI. `ClaudeP` (the real impl) ships in engine too. `AgentSpawner` (TUI) injects an `Arc<dyn AgentRunner>`; default wiring picks `ClaudeP`.

**Verified wrinkle — `AgentContext.allowed_tools` source:** pre-refactor every spawn used the SAME hardcoded `--allowedTools "Read,Edit,Write,Bash(lazyspec *)"`. AC7 (no behaviour change) -> `AgentSpawner::spawn` MUST keep passing that exact string as `Some("Read,Edit,Write,Bash(lazyspec *)".into())` into `AgentContext`. Per-template `allowed_tools` (incl. the `None`/Custom-prompt path) is slice 2/4 — NOT here. `None` branch in `ClaudeP` is implemented + tested now (AC4) because trait shape requires it, but no live caller passes `None` in slice 1.

**Verified wrinkle — history dir from repo root:** `AgentSpawner::new()` currently takes no args, reads `$HOME` via `dirs_home()` (`src/tui/agent.rs:37`). New dir is repo-relative -> spawner needs the repo root. `App` constructs spawner at `src/tui/state/app.rs:370` (+ test ctor `:1559`) where `store.root()` (`src/engine/store.rs:277`) is in scope. Thread root into `AgentSpawner::new(root)`; history fns derive `<root>/.lazyspec/cache/agents/`. `<root>/.lazyspec/cache/` is the existing cache convention (`src/engine/store.rs:57,371`).

## Test Plan

All tests follow DICTUM-004: isolated (own `TempDir`), fast (NO process spawn — the fake runner is the whole point), behavioral (assert on captured `AgentContext` + persisted record, not internal call order), deterministic. Engine unit tests for trait/`ClaudeP`/fake go inline in `#[cfg(test)] mod tests` at bottom of `src/engine/agent.rs`. `AgentSpawner` tests stay inline in `src/tui/agent.rs` (extend existing `mod tests`). All agent code is behind `#[cfg(feature = "agent")]` (`src/tui.rs:1`); engine module gated the same way — run tests with `--features agent`.

Fake runner (test double, defined in `src/engine/agent.rs` test module so both engine + TUI tests reuse it via a `pub(crate)` test helper, or duplicated minimally if cross-module reuse is awkward): `struct FakeRunner { captured: RefCell<Vec<AgentContext>>, fail: bool }` impl `AgentRunner` — records each `ctx`, returns an `AgentHandle` wrapping a trivially-finished child (e.g. spawn `true`/a no-op) or a synthesized handle; `fail=true` returns `Err`. Tradeoff: `AgentHandle.child: Child` means the fake still needs a real `Child` to return — spawn the platform `true` binary (fast, exits immediately) rather than `claude`. This is acceptable I/O-free-enough per dictum (no `claude`, deterministic exit). If even `true` is undesired, alternative is making `AgentHandle.child` hold something pollable behind a seam — OUT OF SCOPE; v1 uses real `Child` per RFC sketch, so the fake spawns `true`.

- **AC1 — `AgentRunner` is the spawn seam.**
  Test `spawner_builds_context_and_delegates_to_runner` (new, `src/tui/agent.rs` `mod tests`).
  Given: `AgentSpawner` wired with a `FakeRunner`. When: `spawn(prompt, doc_path, doc_title, action)`. Then: fake captured exactly one `AgentContext` whose `prompt`, `doc_path`, `session_id` (non-empty uuid) match the call; spawner did NOT construct a process itself (proven by fake being the only thing that could — no `claude` on PATH assumption). Assert on captured ctx fields.

- **AC2 — fake runner asserted on w/o launching a process.**
  Test `fake_runner_captures_context_without_subprocess` (new, `src/engine/agent.rs` `mod tests`).
  Given: a `FakeRunner`. When: `runner.spawn(ctx)` called directly with a hand-built `AgentContext`. Then: `runner.captured` contains that exact ctx (prompt/allowed_tools/doc_path/session_id all equal); returns `Ok(AgentHandle)`; no `claude` process launched. This is the seam-exists proof independent of `AgentSpawner`.

- **AC3 — `ClaudeP` runs headless `claude -p`.**
  Test `claudep_builds_claude_p_command` (new, `src/engine/agent.rs` `mod tests`).
  Tradeoff/seam: asserting the exact argv WITHOUT launching `claude` requires `ClaudeP` to expose its `Command` construction as a pure, testable fn. Plan: factor arg-building into `fn build_command(ctx: &AgentContext) -> Command` (or returns `(program, Vec<String> args)`) that `spawn` then `.spawn()`s. Test calls `build_command` and asserts (via `Command::get_program()` + `get_args()`) program == `"claude"`, args contain `-p`, `<prompt>`, `--session-id`, `<id>` in order. stdio-discard + actual `.spawn()` are NOT unit-asserted (can't without a process); covered by AC7's behavioural-parity reasoning + the fact `spawn` sets `Stdio::null()` on all three. Given: a `ClaudeP` + a ctx with `allowed_tools: None`. When: `build_command(&ctx)`. Then: argv == `["claude", "-p", <prompt>, "--session-id", <id>]`, no `--allowedTools`.

- **AC4 — `ClaudeP` passes `--allowedTools` only when present.**
  Test `claudep_includes_allowed_tools_only_when_some` (new, `src/engine/agent.rs` `mod tests`).
  Given: `ClaudeP`. When: `build_command` with `allowed_tools: Some("Read,Edit")`. Then: argv contains `--allowedTools` followed by `"Read,Edit"`. And: with `allowed_tools: None`, argv contains NO `--allowedTools` token. Two assertions, one test (the contrast is the behaviour).

- **AC5 — `AgentSpawner` retains record lifecycle ownership.**
  Test `spawner_creates_and_persists_record_with_fake_runner` (new, `src/tui/agent.rs` `mod tests`).
  Given: `AgentSpawner::new(<TempDir root>)` wired to `FakeRunner`. When: `spawn(...)`. Then: `self.records` gained one `AgentRecord` (status `Running`, matching `doc_title`/`action`/`doc_path`); `active_count() == 1`; a `<root>/.lazyspec/cache/agents/<session_id>.json` file was persisted (assert `load_all_records(Some(dir))` returns it). Proves the runner is NOT involved in record keeping (fake records nothing yet record exists).

- **AC6 — polling + status unchanged.**
  Test `poll_marks_complete_and_failed_via_fake_handles` (new, `src/tui/agent.rs` `mod tests`).
  Given: spawner wired to a `FakeRunner` returning handles whose children exit success / failure (fake spawns `true` for success, `false` for failure — both fast, deterministic). When: `spawn` twice then `poll_finished()`. Then: the success agent's record -> `Complete` with `finished_at` set; the failure agent's -> `Failed`; both persisted (`load_all_records` reflects it); `active_count() == 0`. Mirrors today's `poll_finished` semantics (`src/tui/agent.rs:173`). Tradeoff: relies on `true`/`false` binaries — acceptable, no network/sleep, exits are immediate; if a platform lacks them the fake can return a pre-exited child via the same `true` trick. Existing `agent_record_*` persistence tests stay and must still pass against the relocated dir API.

- **AC7 — no behaviour change to existing actions.**
  Test `expand_action_spawns_identical_context_as_before` (new, `src/tui/agent.rs` `mod tests`) + reuse of AC3/AC4 for argv.
  Given: spawner wired to `FakeRunner`. When: `spawn(prompt, full_path, title, "Expand document")` (the call shape `src/tui/views/keys.rs:218` makes). Then: captured `AgentContext.allowed_tools == Some("Read,Edit,Write,Bash(lazyspec *)")` (the exact pre-refactor string), `prompt` unchanged, `session_id` a uuid. Combined with AC3/AC4 proving `ClaudeP` turns that ctx into the identical `claude -p <prompt> --session-id <id> --allowedTools Read,Edit,Write,Bash(lazyspec *)` argv -> end-to-end the spawned command + allowed-tools + record persistence match pre-refactor. Tradeoff: no integration test launches real `claude` (dictum: no process spawn in tests); parity is argued via the captured ctx + the deterministic `build_command` mapping, which is the structure-insensitive way to prove "identical command" without a process.

## Changes

Sequenced. Task 1 (engine trait + `ClaudeP`) lands first and compiles standalone. Task 2 (spawner refactor) depends on it. Task 3 (history relocate + wiring) finishes the seam. Self-contained for a zero-context build subagent.

### Task 1 — `AgentRunner` trait + `AgentContext`/`AgentHandle` + `ClaudeP` in engine
**ACs:** 2, 3, 4 (foundation for 1, 7).
**Files:** NEW `src/engine/agent.rs`; `src/engine.rs` (add `#[cfg(feature = "agent")] pub mod agent;`).
**Do:**
- Create `src/engine/agent.rs`. Imports: `std::path::PathBuf`, `std::process::{Child, Command, Stdio}`, `anyhow::Result`.
- Define structs (match RFC @draft sketch verbatim):
  ```rust
  pub struct AgentContext {
      pub prompt: String,
      pub allowed_tools: Option<String>,
      pub doc_path: PathBuf,
      pub session_id: String,
  }
  pub struct AgentHandle {
      pub session_id: String,
      pub child: Child,
  }
  ```
  Derive `Debug, Clone, PartialEq, Eq` on `AgentContext` (needed for test assertions on captured ctx). Do NOT derive on `AgentHandle` (`Child` is not `Clone`/`PartialEq`); `Debug` only if it compiles (`Child: Debug`, so `#[derive(Debug)]` is fine).
- Define trait:
  ```rust
  pub trait AgentRunner {
      fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle>;
  }
  ```
- Define `pub struct ClaudeP;` (unit struct) and `impl AgentRunner for ClaudeP`.
- Factor arg-building into a pure helper so it is testable without launching (AC3/AC4 seam):
  ```rust
  impl ClaudeP {
      fn build_command(ctx: &AgentContext) -> Command {
          let mut cmd = Command::new("claude");
          cmd.args(["-p", &ctx.prompt]);
          cmd.args(["--session-id", &ctx.session_id]);
          if let Some(tools) = &ctx.allowed_tools {
              cmd.args(["--allowedTools", tools]);
          }
          cmd
      }
  }
  ```
  `spawn` then:
  ```rust
  fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle> {
      let mut cmd = ClaudeP::build_command(&ctx);
      let child = cmd
          .stdin(Stdio::null())
          .stdout(Stdio::null())
          .stderr(Stdio::null())
          .spawn()?;
      Ok(AgentHandle { session_id: ctx.session_id, child })
  }
  ```
- Inline `#[cfg(test)] mod tests`: define `FakeRunner` (see Test Plan), add AC2/AC3/AC4 tests using `Command::get_program()`/`get_args()` (no `.spawn()`). Make `FakeRunner` reachable from TUI tests — either `pub(crate)` behind `#[cfg(test)]`, or accept minimal duplication in `src/tui/agent.rs` (decide in Task 2; prefer a `#[cfg(test)] pub(crate)` export here to avoid dup).
**Verify:** `cargo build --features agent`; `cargo clippy --features agent --all-targets -- -D warnings`; `cargo test --features agent agent` (engine module tests AC2/3/4 green).

### Task 2 — refactor `AgentSpawner` to delegate to an injected `AgentRunner`
**ACs:** 1, 5, 6, 7.
**Files:** `src/tui/agent.rs`.
**Do:**
- Add `use crate::engine::agent::{AgentContext, AgentHandle, AgentRunner, ClaudeP};` and `use std::sync::Arc;`.
- Change `AgentSpawner` struct:
  ```rust
  pub struct AgentSpawner {
      running: Vec<(String, Child)>,
      pub records: Vec<AgentRecord>,
      runner: Arc<dyn AgentRunner>,
      history_dir: PathBuf,   // see Task 3
  }
  ```
  (`history_dir` is added in Task 3; if landing Task 2 before Task 3, keep the `None`-override history calls and add `history_dir` in Task 3. Prefer landing 2+3 together.)
- Replace the inline `Command::new("claude")...spawn()?` block (`src/tui/agent.rs:148-155`) with:
  ```rust
  let ctx = AgentContext {
      prompt: prompt.to_string(),
      allowed_tools: Some("Read,Edit,Write,Bash(lazyspec *)".to_string()),
      doc_path: doc_path.to_path_buf(),
      session_id: session_id.clone(),
  };
  let handle = self.runner.spawn(ctx)?;
  ```
  Then `self.running.push((handle.session_id, handle.child));` (preserve order: build record, `save_record`, push record, push running — same as today, AC5). The `allowed_tools` literal MUST equal the pre-refactor string verbatim (AC7).
- Constructors: keep a production default selecting `ClaudeP`, and add a runner-injecting ctor for tests:
  ```rust
  impl AgentSpawner {
      pub fn new(/* root: &Path — added in Task 3 */) -> Self {
          Self::with_runner(Arc::new(ClaudeP) /*, root */)
      }
      pub fn with_runner(runner: Arc<dyn AgentRunner> /*, root */) -> Self {
          let records = load_all_records(/* dir */).unwrap_or_default();
          AgentSpawner { running: Vec::new(), records, runner /*, history_dir */ }
      }
  }
  ```
  `Default for AgentSpawner` (`src/tui/agent.rs:124`) must keep compiling — once `new` takes a root, drop the `Default` impl (it has no root to supply) OR keep it only if a sensible root exists. Decision: REMOVE the `Default` impl; `new(root)` replaces it (no current caller relies on `Default` — verify with `grep -rn "AgentSpawner::default\|AgentSpawner>::default" src tests`; the `#[derive(Default)]`-free struct can't auto-derive anyway with `Arc<dyn ...>`).
- `poll_finished` (`src/tui/agent.rs:173`) is UNCHANGED in logic (try_wait per running child -> Complete/Failed) — it already operates on `self.running`'s `Child`, which now comes from `AgentHandle`. Only the history-write call signature changes in Task 3.
- Inline `mod tests`: add `FakeRunner` import/use, AC1/AC5/AC6/AC7 tests (see Test Plan). Keep existing `agent_record_*` tests.
**Verify:** `cargo build --features agent`; `cargo clippy --features agent --all-targets -- -D warnings`; `cargo test --features agent` (whole agent suite incl. AC1/5/6/7).

### Task 3 — relocate run-history dir to `<repo>/.lazyspec/cache/agents/` + thread root
**ACs:** 5, 6 (correct persistence location); supports all.
**Files:** `src/tui/agent.rs`, `src/tui/state/app.rs`.
**Do:**
- In `src/tui/agent.rs`: change the history-dir resolution. Today `agent_history_dir(override_path)` falls back to `dirs_home().join(".lazyspec").join("agents")` (`:28-35`). New: history is `<root>/.lazyspec/cache/agents/`. Approach: store the resolved `history_dir` on `AgentSpawner` (computed in `with_runner` from the passed `root`: `root.join(".lazyspec").join("cache").join("agents")`), and have `spawn`/`poll_finished`/`new` pass `Some(&self.history_dir)` to `save_record`/`update_record_status`/`load_all_records`. The free fns `save_record`/`load_all_records`/`update_record_status`/`agent_history_dir` KEEP their `override_path: Option<&Path>` signature (existing tests `:245,248,261,263,274` depend on `Some(dir)`), but the `None` fallback changes from `$HOME/.lazyspec/agents` to... — DECISION: the `None` branch is now only reached by callers that don't have a root. After this slice every real caller passes `Some(history_dir)`, so the `None` fallback is dead for the spawner; keep it as `dirs_home().join(".lazyspec").join("cache").join("agents")` (still moved out of the templates path) to avoid a footgun, and add a code comment that the spawner always passes an explicit dir. Remove `dirs_home`'s sole purpose only if it becomes unused (it stays used by the `None` fallback — keep it).
- Delete the now-removed `Default` impl; update `AgentSpawner::new` to `pub fn new(root: &Path) -> Self`.
- In `src/tui/state/app.rs`: the two construction sites `agent_spawner: AgentSpawner::new()` (`:370` production, `:1559` test ctor) -> `AgentSpawner::new(store.root())` / `AgentSpawner::new(&root)`. At `:370` `store` is in scope (`store.root()` -> `src/engine/store.rs:277`). At test ctor `:1559` use the test's root (`PathBuf::from(".")` per surrounding test, `:1524`). Also `load_all_records(None)` reload at `src/tui/state/app.rs:459` -> pass the spawner's history dir (expose a `pub fn history_dir(&self) -> &Path` getter on `AgentSpawner`, or reload via `self.agent_spawner` API).
- Confirm `<root>/.lazyspec/cache/agents/` does NOT collide with the templates path `<root>/.lazyspec/agents/` (slice 2). Different segment (`cache/`) — verified distinct.
**Verify:** `cargo build --features agent`; `cargo clippy --features agent --all-targets -- -D warnings`; `cargo test --features agent`; AC5/AC6 tests assert the persisted JSON lands under `<TempDir>/.lazyspec/cache/agents/`. `grep -rn "\.lazyspec/agents\|\.lazyspec\").join(\"agents\")" src` -> only the templates path (slice 2), no history writer.

### README
Per CLAUDE.md: no CLI interface changes in this slice (seam only, TUI-internal). If `README.md` documents the agent-history location (`~/.lazyspec/agents/`), update it to `<repo>/.lazyspec/cache/agents/`. Otherwise no README change. `grep -n "lazyspec/agents\|agent history\|\.lazyspec/cache" README.md` to confirm.

## Notes

**Verified real paths (file:symbol):**
- `src/tui/agent.rs:28` `agent_history_dir` (writes `$HOME/.lazyspec/agents/`), `:37` `dirs_home`, `:43` `save_record`, `:51` `load_all_records`, `:77` `update_record_status`, `:97` `build_create_children_prompt` (KEEP, deleted in slice 2), `:108` `build_expand_prompt` (KEEP), `:119` `pub struct AgentSpawner`, `:124` `impl Default`, `:130` `impl AgentSpawner`, `:139` `spawn` (hardcoded `Command::new("claude")` at `:148-155`), `:173` `poll_finished`, `:204` `active_count`.
- Spawn call sites (UNCHANGED behaviour, AC7): `src/tui/views/keys.rs:218` (Expand, allowed_tools=the fixed string), `:255` (Create children, via `spawn_create_children` `:229`), `:282` (Custom prompt).
- `App` spawner construction: `src/tui/state/app.rs:370` (prod, `store.root()` in scope), `:1559` (test ctor), records reload `:459`.
- Repo root: `src/engine/store.rs:277` `Store::root()`; cache convention `<root>/.lazyspec/cache/<...>` at `src/engine/store.rs:57,371`.
- Engine module decl: `src/engine.rs` (lib-style, NO `mod.rs`); TUI agent gating `#[cfg(feature = "agent")]` `src/tui.rs:1-2`.

**Design decisions:**
- **Trait module = `src/engine/agent.rs`** (NOT `src/tui/agent.rs`). Dictum 3: engine carries no terminal/TUI assumptions; subprocess spawn is pure I/O behind a trait (dictum 4). `src/tui/agent.rs` already exists for the SPAWNER (TUI-side lifecycle/records/poll) — leaving the trait + `ClaudeP` in the engine keeps the I/O seam in the inward layer and lets the TUI depend on it without a TUI->CLI/engine inversion. Both gated `#[cfg(feature = "agent")]`.
- **`ClaudeP` is the sole impl (dictum 6: trait only when ≥2 concrete uses).** Justification for introducing the trait with one impl now: the SECOND concrete use is the `FakeRunner` test double (dictum 4 explicitly: I/O boundaries get a trait so prod + test share one iface). So it is prod-impl + test-impl = the two uses dictum 6 wants. pi/opencode are admitted later, not shipped.
- **Fake runner wiring:** injected as `Arc<dyn AgentRunner>` via `AgentSpawner::with_runner(runner, root)`; prod `new(root)` selects `Arc::new(ClaudeP)`. Fake lives in `src/engine/agent.rs` test module, exported `#[cfg(test)] pub(crate)` so `src/tui/agent.rs` tests reuse it (avoid duplication). Fake captures each `AgentContext` in a `RefCell<Vec<_>>` and returns an `AgentHandle` wrapping a real-but-trivial `Child` (spawns `true`/`false` for deterministic exit) — no `claude`, no network, no sleep (DICTUM-004).
- **`AgentContext.allowed_tools` in slice 1 is always `Some("Read,Edit,Write,Bash(lazyspec *)")`** to preserve AC7 byte-identical behaviour. The `None` branch in `ClaudeP` is built + tested (AC4) because the trait/RFC shape requires it and slice 2 (per-template tools) + slice 4 (Custom prompt) will exercise it; no live `None` caller in this slice.
- **History dir derived from repo root, stored on the spawner.** `AgentSpawner::new(root)` computes `<root>/.lazyspec/cache/agents/` once and threads `Some(&history_dir)` into the free persistence fns (which keep their `Option<&Path>` override param for existing `TempDir` tests). `.lazyspec/agents/` is left untouched for slice 2 templates; `cache/agents/` sits alongside the existing `.lazyspec/cache/` (`src/engine/store.rs`). `Default for AgentSpawner` is REMOVED (no root to supply); `new(root)` is the sole ctor + `with_runner(runner, root)` for tests.
- **No process launched in any test** (DICTUM-004 fast/automated/deterministic): `ClaudeP` argv asserted via a pure `build_command` + `Command::get_program()`/`get_args()`; spawner behaviour asserted via the `FakeRunner`-captured `AgentContext` and persisted `AgentRecord`. End-to-end command parity (AC7) is argued from captured ctx + deterministic ctx->argv mapping, never by spawning `claude`.
