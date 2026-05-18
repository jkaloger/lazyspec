---
title: Prompt rendering pipeline
type: iteration
status: accepted
author: agent
date: 2026-05-16
tags: []
related:
- implements: STORY-125
---

## In Scope

STORY-125 slice 5 of RFC-041. All 8 ACs.

- Dispatch-time prompt renderer (`render_prompt`) reusing existing preflight minijinja w/ strict-undefined.
- `AgentContext.prompt: String` carrying rendered string to `AgentRunner`.
- `ClaudeP` pipes rendered prompt to claude subprocess stdin.
- Session-start iteration snapshot captured at fresh dispatch, persisted into `AgentMetadata.session_start_iteration_ids` (field already lands in ITER-179).
- Snapshot reload at retry/continuation from `read_agent_metadata` (free fn already lands in ITER-179).
- `prior_iterations` computed per turn = current store iterations w/ `implements: <doc_id>` MINUS snapshot.
- `attempt` render var: `None` on fresh dispatch, `Some(obs.attempt)` on continuation/retry.
- Verify hot-reload + in-flight immutability (ITER-178 already wires notify watcher + preflight gate; this iter only confirms in-flight sessions are not re-rendered on template change).

## Out of Scope

- Workflow file structure migration to `.lazyspec/workflows/<role>.md` (RFC-041 trajectory note, deferred to multi-role).
- The instructive content of `.lazyspec/prompts/builder.md` (deliverable artifact, not code).
- IPC events around dispatch + turn boundaries (slice 6).
- Metadata-ref remote push cadence (slice 8 Group B, already separately tracked).
- Status mutation by daemon (still forbidden).
- Tool allow-list enforcement (slice 6 / runtime config consumer).

## Acceptance Criteria

**AC1: builder role loads from documented path**

Given `.lazyspec/prompts/builder.md` exists w/ valid template
When daemon resolves builder template at dispatch time
Then renderer reads body from `config.orchestration.prompt_path` (default `.lazyspec/prompts/builder.md`)
And rendered output is non-empty

**AC2: rendering exposes doc, attempt, prior_iterations**

Given builder template references `doc.id`, `doc.title`, `doc.body`, `attempt`, `prior_iterations`
When `render_prompt(template, doc, attempt, prior_iterations)` runs
Then minijinja substitutes all three vars
And `doc` exposes id/title/body/status/assignees (matches RFC-041 §Prompt rendering)

**AC3: unknown variables fail at config load, not at dispatch**

Given builder template references variable not in render context
When daemon runs preflight on startup (ITER-178 path)
Then preflight fails with strict-undefined error
And dispatch is gated on `last_preflight.is_ok()` (already wired ITER-178)

**AC4: attempt distinguishes first turn from continuation**

Given fresh dispatch site (tick.rs:790)
When `render_prompt` is called
Then `attempt` arg is `None`
And given retry-drain site (tick.rs:1100), `attempt` arg is `Some(retry.attempt)` where `retry.attempt` increments per turn

**AC5: prior_iterations reflects store-diff against session-start snapshot**

Given session-start snapshot `S = {iter_ids w/ implements: doc_id at fresh-dispatch time}`
When current iteration `ITER-X` is added to store post-dispatch w/ `implements: <doc_id>`
Then `prior_iterations` arg passed to `render_prompt` on next turn = `current_iters \ S`
And `ITER-X ∈ prior_iterations`
And iterations in `S` are absent from `prior_iterations`

**AC6: snapshot survives daemon restart**

Given session A dispatched at T0 captures snapshot S0 into `refs/lazyspec/agents/<sid>` metadata
When daemon is killed and restarted mid-session
Then retry/continuation path reads `AgentMetadata.session_start_iteration_ids` via `read_agent_metadata`
And subsequent render exposes `prior_iterations = current \ S0`
And no second snapshot is captured (snapshot is write-once per session)

**AC7: notify event triggers preflight; failure invalidates new dispatches**

(Inherited from ITER-178. This iter verifies the renderer participates: preflight uses same minijinja API as live render.)

Given running daemon, preflight passing
When `.lazyspec/prompts/builder.md` modified to introduce unknown var
Then notify fires, preflight reruns, fails
And next tick's fresh-dispatch path does not call `render_prompt` for new candidates (gated by `last_preflight.is_ok()`)

**AC8: in-flight sessions are not restarted on template change**

Given session A dispatched against template body `B_A`
When template file changes to `B_B` post-dispatch
Then session A's next turn (retry/continuation) re-renders against the **current on-disk template** at the time of re-spawn
And session A is NOT killed/restarted by the template change itself

> NOTE: AC8 wording in STORY-125 says in-flight sessions "continue running against template version A". Read literally that requires capturing rendered prompt or template body per session. Discuss tradeoff in Test Plan + Notes; recommend re-reading current template at each turn (RFC-041 explicit: "fresh `claude -p` invocation" per turn; no live process keeps the template). Confirm before implementation.

## Test Plan

Per DICTUM-004: real types over mocks, trait seams at I/O, deterministic, behavioral.

### Unit tests — `src/engine/prompt.rs` (new)

**AC1, AC2**
- `render_substitutes_doc_fields` — input doc w/ fixed id/title/body/status/assignees, template referencing each → output contains each literal.
- `render_substitutes_attempt_some` — `attempt = Some(3)`, template `{{ attempt }}` → output `3`.
- `render_substitutes_attempt_none_branch` — template `{% if attempt is none %}FIRST{% else %}CONT{% endif %}`, `attempt = None` → `FIRST`. Other case → `CONT`.
- `render_substitutes_prior_iterations_loop` — `prior_iterations = vec!["ITER-1","ITER-2"]`, template loops → both ids in output.
- `render_substitutes_prior_iterations_empty` — empty vec, template `{% if prior_iterations %}HAS{% else %}NONE{% endif %}` → `NONE`.

**AC3**
- `render_fails_on_undefined_variable` — template `{{ ghost }}`, strict-undefined → `Err`. Error msg mentions `ghost`.
- `render_fails_on_syntax_error` — `{{ unterminated` → `Err`.

### Unit tests — `src/engine/prompt.rs::prior_iterations`

**AC5**
- `prior_iterations_excludes_snapshot` — current set `{A,B,C}`, snapshot `{A}` → output `{B,C}` (order-insensitive assertion via sorted vec).
- `prior_iterations_empty_when_no_new` — current = snapshot → output empty.
- `prior_iterations_all_when_snapshot_empty` — current `{A,B}`, snapshot empty → output `{A,B}`.

### Unit tests — `src/engine/tick.rs`

**AC4 fresh dispatch path**
- `fresh_dispatch_renders_with_attempt_none` — fake `PromptRenderer` (recording seam) captures call; assert recorded `attempt == None`.

**AC4 retry/continuation path**
- `retry_dispatch_renders_with_attempt_some_carried_from_pending` — seed `PendingRetry { attempt: 3, .. }`; drain queue; assert recorded `attempt == Some(3)`.

**AC5 + AC6 snapshot capture + reload**
- `fresh_dispatch_captures_session_start_snapshot` — store seeded w/ two iterations implementing the candidate doc; fresh dispatch → `AgentMetadataWriter::write` recorded w/ `session_start_iteration_ids = ["ITER-A","ITER-B"]` (sorted).
- `fresh_dispatch_writes_snapshot_exactly_once` — second tick on same running session does NOT re-write snapshot (idempotency via `session_start_iteration_ids.is_empty()` check on prev metadata).
- `retry_reloads_snapshot_from_metadata` — pre-seed `AgentMetadata` w/ `session_start_iteration_ids = ["ITER-A"]`; seed store w/ `{ITER-A,ITER-B}` implementing doc; queue retry; assert renderer call captures `prior_iterations = ["ITER-B"]`.

**AC7 (verify gate, not duplicate ITER-178)**
- `dispatch_skipped_when_preflight_fails` — seed preflight report `{ prompt_renders: false }`; tick → `PromptRenderer` NOT invoked, no spawn.

**AC8**
- `in_flight_session_not_killed_on_template_change` — running session A; flip preflight to fail (simulates notify); tick → session A's cancel sender NOT triggered; reader handle still live. (Behavioral assertion against `running.lock().contains_key`).

### Integration tests — `tests/prompt_rendering.rs` (new)

**AC1 end-to-end**
- Real `TempDir` w/ `.lazyspec/prompts/builder.md`; call `render_prompt_from_path(...)` (thin wrapper); assert output contains substituted values.

**AC6 cross-restart**
- Real temp repo; write `AgentMetadata` w/ snapshot `{ITER-A}`; new `GitRefAgentMetadata` instance + `read_agent_metadata` resolves it; pass through renderer; `prior_iterations` correct.

### Trait seam — `PromptRenderer`

DICTUM-006: introduce trait only w/ 2 concrete uses. We have:
1. Real `MinijinjaPromptRenderer` reading template from disk per turn.
2. `RecordingPromptRenderer` in tick.rs tests (records calls without rendering).

Two uses ⇒ trait justified. Trait:

```rust
pub trait PromptRenderer: Send + Sync {
    fn render(
        &self,
        doc: &DocSummary,
        attempt: Option<u32>,
        prior_iterations: &[String],
    ) -> Result<String>;
}
```

Production impl reads `prompt_path` lazily on each `render` call (AC8 default: current template, not cached). Discuss alternative in Notes.

### Test tradeoffs

- **Renderer pure fn vs trait via disk**: pure `render_prompt(template_str, ...)` is unit-testable with no I/O. Trait wraps it w/ disk read for production. Trait seam used at tick boundary so tick tests don't touch disk.
- **`ClaudeP` stdin not unit-tested**: subprocess + libc, no good seam. Extract `fn write_prompt_stdin(w: &mut impl Write, prompt: &str) -> io::Result<()>` and test against `Cursor<Vec<u8>>`. Live subprocess covered in manual smoke.

### Manual test plan

**MT1 — fresh dispatch, prompt visible to claude**
1. `cargo build`. Replace claude binary in test config w/ a wrapper script `tee /tmp/claude-input.txt` reading stdin.
2. Start daemon, assign STORY-X w/ `claude-bot`.
3. After dispatch: `cat /tmp/claude-input.txt` shows rendered prompt w/ `doc.id == STORY-X`, contains `This is the first turn`.

**MT2 — continuation increments attempt**
1. Wrapper script sleeps then exits 0 to trigger continuation.
2. After ≥2 turns: stdin captures show `turn 1` then `turn 2` (or whatever obs.attempt produces).

**MT3 — snapshot survives restart**
1. Start daemon, dispatch STORY-X w/ no iterations.
2. Mid-session, manually create `ITER-CREATED` w/ `implements: STORY-X`.
3. `kill -9` daemon. Restart.
4. Continuation turn after restart: prompt contains `ITER-CREATED` under "Prior iterations created in this session". (Pre-existing iterations created BEFORE session start are NOT in `prior_iterations`.)

**MT4 — hot reload preflight (ITER-178 regression check)**
1. Edit `.lazyspec/prompts/builder.md` to add `{{ undefined_thing }}`. Save.
2. Within seconds: daemon log shows preflight failure. New assignments NOT dispatched.
3. Revert. New assignments resume.

**MT5 — in-flight session survives template change**
1. Dispatch STORY-X w/ slow wrapper.
2. Edit prompt file (valid change).
3. Assert STORY-X session continues; no kill/respawn.
4. Next turn of STORY-X re-renders w/ new template body (current behavior; document in MT and Notes).

## Changes

Numbered tasks. Each is self-contained for a zero-context build subagent.

### Task 1: `render_prompt` pure function + `DocSummary` shape

**ACs**: AC1, AC2, AC3
**Files**: new `src/engine/prompt.rs`, `src/engine/mod.rs`

Define:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct DocSummary {
    pub id: String,
    pub title: String,
    pub body: String,
    pub status: String,
    pub assignees: Vec<String>,
}

pub fn render_prompt(
    template: &str,
    doc: &DocSummary,
    attempt: Option<u32>,
    prior_iterations: &[String],
) -> Result<String> {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    env.add_template("prompt", template)?;
    let tmpl = env.get_template("prompt")?;
    tmpl.render(minijinja::context! {
        doc => doc,
        attempt => attempt,
        prior_iterations => prior_iterations,
    }).map_err(Into::into)
}
```

Re-export from `src/engine/mod.rs`. Add `pub mod prompt;`.

Replace `prompt_renders_ok` in `src/engine/preflight.rs` to call `render_prompt` w/ stub `DocSummary` (eliminates duplicate minijinja config). Stub doc: `{ id: "DUMMY", title: "x", body: "", status: "draft", assignees: vec![] }`, `attempt = None`, `prior_iterations = &[]`.

Verify: `cargo test engine::prompt`, `cargo test engine::preflight`, `cargo clippy --all-targets -- -D warnings`.

### Task 2: `PromptRenderer` trait + `MinijinjaPromptRenderer` impl

**ACs**: AC1, AC8
**Files**: `src/engine/prompt.rs`

```rust
pub trait PromptRenderer: Send + Sync {
    fn render(
        &self,
        doc: &DocSummary,
        attempt: Option<u32>,
        prior_iterations: &[String],
    ) -> Result<String>;
}

pub struct MinijinjaPromptRenderer {
    pub prompt_path: PathBuf,
}

impl PromptRenderer for MinijinjaPromptRenderer {
    fn render(&self, doc, attempt, prior_iterations) -> Result<String> {
        let template = std::fs::read_to_string(&self.prompt_path)
            .with_context(|| format!("read prompt {}", self.prompt_path.display()))?;
        render_prompt(&template, doc, attempt, prior_iterations)
    }
}
```

AC8 design call: per-turn disk read. Each retry/continuation re-reads the file. No caching. RFC-041 says "in-flight sessions continue running against the template they were dispatched with" — strict reading would require capturing the template body in `AgentMetadata` or `PendingRetry`. Tradeoff documented in Notes; recommend the simpler per-turn read for v1. **Flag for user review during build.**

Verify: `cargo test engine::prompt::tests`.

### Task 3: Carry rendered prompt through `AgentContext`

**ACs**: AC1, AC2
**Files**: `src/engine/runner.rs`

Add field:

```rust
pub struct AgentContext {
    pub workspace: PathBuf,
    pub doc_id: String,
    pub agent_id: String,
    pub branch: String,
    pub prompt: String,   // NEW
}
```

Update test fixture in `runner.rs::tests::agent_event_variants_are_exhaustive` to include `prompt: String::new()`.

Update all `AgentContext { ... }` constructions: `tick.rs:790`, `tick.rs:1100`, and all test sites (search `AgentContext {`). Pass empty string from tests that don't exercise the renderer; pass rendered prompt from tick.

Verify: `cargo build`, `cargo test`.

### Task 4: `ClaudeP` writes prompt to subprocess stdin

**ACs**: AC1
**Files**: `src/engine/runner/claudep.rs`

Change `Stdio::null()` → `Stdio::piped()` for stdin. After `spawn`, take stdin and write `ctx.prompt` then drop (closes pipe → claude reads EOF).

```rust
use std::io::Write;
// ...
let mut child = Command::new(&self.binary)
    .arg("-p")
    .arg("--output-format")
    .arg("stream-json")
    .arg("--allowedTools")
    .arg(&self.allowed_tools)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .with_context(|| format!("spawn {}", self.binary))?;

if let Some(mut stdin) = child.stdin.take() {
    let prompt = ctx.prompt.clone();
    std::thread::spawn(move || {
        let _ = stdin.write_all(prompt.as_bytes());
        // stdin dropped here closes the pipe
    });
}
```

Threaded write avoids blocking on huge prompts. Test seam: extract `fn write_prompt(mut w: impl Write, prompt: &str) -> io::Result<()> { w.write_all(prompt.as_bytes()) }` w/ a unit test against `Vec<u8>`.

Verify: `cargo test engine::runner`, manual MT1.

### Task 5: Session-start snapshot computation

**ACs**: AC5, AC6
**Files**: `src/engine/prompt.rs` (free fns), `src/engine/store.rs` (verify list API exists)

```rust
/// Iterations currently in the store that implement `doc_id`.
pub fn iterations_implementing(store: &Store, doc_id: &str) -> Vec<String> {
    store
        .documents()
        .iter()
        .filter(|d| d.doc_type() == DocType::Iteration)
        .filter(|d| d.related.iter().any(|r|
            r.relation_type == RelationType::Implements && r.target == doc_id
        ))
        .map(|d| d.id().to_string())
        .collect::<Vec<_>>()
        .sorted()  // deterministic; use `.sort()` after `.collect()`
}

/// Set diff: current iterations not in the session-start snapshot.
pub fn prior_iterations(current: &[String], snapshot: &[String]) -> Vec<String> {
    let snap: std::collections::HashSet<&str> = snapshot.iter().map(String::as_str).collect();
    current.iter().filter(|id| !snap.contains(id.as_str())).cloned().collect()
}
```

Verify exact `Store` API w/ `grep -n "fn documents\|pub fn list" src/engine/store.rs` during build; adjust accessor if needed.

Verify: `cargo test engine::prompt::prior_iterations`.

### Task 6: Tick fresh-dispatch wires renderer + snapshot capture

**ACs**: AC1, AC2, AC4, AC5, AC6
**Files**: `src/engine/tick.rs`, `src/engine/daemon.rs`

Add field on `TickLoop`:

```rust
pub prompt_renderer: Arc<dyn PromptRenderer>,
```

At fresh dispatch site (~line 790, BEFORE `runner.spawn(ctx)`):

1. Build `DocSummary` from `cand` (the loaded candidate doc). Map `doc_id`, title, body, status, assignees from `cand`.
2. Compute snapshot: `let snapshot = iterations_implementing(&store, &cand.doc_id);` — store must be accessible here; check existing dispatch flow (likely re-uses `Store::load(&root, &config)` from candidates pass — reuse same handle).
3. Compute `prior_iterations`: at fresh dispatch always empty (`snapshot == current` at this instant). Pass `&[]`.
4. Render: `let prompt = self.prompt_renderer.render(&doc, None, &[])?;` — handle err via `publish_dispatch_error(&cand.doc_id, "prompt_render", &e); release; continue;`.
5. Set `ctx.prompt = prompt`.
6. After successful spawn, write initial `AgentMetadata` to ref w/ `session_start_iteration_ids = snapshot` via `metadata.write(...)`. (`metadata` writer accessor on `TickLoop`; check ITER-179 wiring; if not present yet, plumb it through.)

At retry/continuation site (~line 1100):

1. Read prev metadata: `let prev = read_agent_metadata(&git, &root, &retry.session_id)?;`
2. `let snapshot = prev.map(|m| m.session_start_iteration_ids).unwrap_or_default();`
3. `let current = iterations_implementing(&store, &retry.doc_id);`
4. `let prior = prior_iterations(&current, &snapshot);`
5. Load candidate doc for `DocSummary` (re-resolve from store by id).
6. Render: `self.prompt_renderer.render(&doc, Some(retry.attempt), &prior)?` → set `ctx.prompt`.
7. Render failure: emit failed, release lease, abandon (same shape as `respawn_failed`).

Update `Daemon::run` to construct `Arc::new(MinijinjaPromptRenderer { prompt_path: config.orchestration.prompt_path })` and pass into `TickLoop`.

Verify: `cargo test engine::tick`, `cargo clippy --all-targets -- -D warnings`.

### Task 7: Recording test seam for renderer + update all tick tests

**ACs**: AC4, AC5, AC6, AC7, AC8
**Files**: `src/engine/tick.rs` test module

```rust
struct RecordingPromptRenderer {
    calls: Mutex<Vec<(String, Option<u32>, Vec<String>)>>,
}

impl PromptRenderer for RecordingPromptRenderer {
    fn render(&self, doc, attempt, prior) -> Result<String> {
        self.calls.lock().unwrap().push((doc.id.clone(), attempt, prior.to_vec()));
        Ok(format!("RENDERED:{}:{:?}:{:?}", doc.id, attempt, prior))
    }
}
```

Update all `TickLoop::new(...)` test constructions to pass `Arc::new(RecordingPromptRenderer::default())`. Each new test (per Test Plan) reaches into `.calls` for assertions.

Verify: `cargo test engine::tick`.

### Task 8: `AgentMetadataWriter` first-write w/ snapshot

**ACs**: AC5, AC6
**Files**: `src/engine/agent_metadata.rs`, `src/engine/tick.rs`

Confirm `GitRefAgentMetadata::write` (added ITER-179) accepts a full `AgentMetadata` including `session_start_iteration_ids`. Fresh-dispatch tick path calls `metadata.write(&AgentMetadata { session_id, doc_id, doc_type, status: Running, started_at: now, last_event_at: now, tokens_in: 0, tokens_out: 0, turn_count: 0, error: None, session_start_iteration_ids: snapshot, ..agent_id })` — verify field set against current `AgentMetadata` definition during build.

Idempotency: if `read_agent_metadata` returns `Some` with non-empty snapshot, do NOT re-write at fresh dispatch (defensive against double-claim edge cases). Behavior: skip metadata write if prev exists; renderer still uses prev snapshot.

Verify: `cargo test engine::agent_metadata`, `cargo test engine::tick::tests::fresh_dispatch_writes_snapshot_exactly_once`.

### Task 9: README + CLI update if surface changes

**ACs**: housekeeping
**Files**: `README.md`

No new CLI flags. Document under "Daemon" section: prompt path config + render var contract (doc/attempt/prior_iterations). Reference `.lazyspec/prompts/builder.md` shipped template.

Verify: visual diff.

### Task 10: Clippy + integration smoke

- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo fmt --check`
- `cargo test`
- `cargo test --test prompt_rendering` (integration)

## Notes

- **AC8 literal reading**: STORY-125 says in-flight sessions continue against the template "they were dispatched with". RFC-041 §Prompt rendering does NOT explicitly require captured template body; it says "in-flight sessions are not interrupted by template changes". Per-turn disk read satisfies the "not interrupted" requirement w/o adding a new persistence concern. Capturing the template body in `AgentMetadata` is the strictest interpretation but adds bytes to every ref commit. Recommend per-turn read; flag during build for user confirmation before locking in.
- **`DocSummary` vs full `Document`**: pass a slimmed shape so the template surface is stable. Full document has frontmatter fields not relevant to the agent prompt (paths, provenance, validate_ignore). Mirrors RFC-041 §Prompt rendering: "full normalized document (id, title, body, status, assignees, context_chain)". `context_chain` deferred; add when a template references it (DICTUM-006).
- **Snapshot idempotency**: AC5 says "iteration_ids that implemented the story at session start". Single write-on-fresh-dispatch is the simplest invariant. Reads on retry use whatever the first write captured; never overwritten. Document this in code comment on the metadata write site.
- **`prior_iterations` ordering**: sort lexically for determinism (test fixtures + agent diff readability). `iterations_implementing` sorts before return.
- **Renderer trait justified by DICTUM-006**: 2 concrete uses (`MinijinjaPromptRenderer`, `RecordingPromptRenderer`). Pure `render_prompt` fn is the inner reusable core; trait wraps disk I/O at the seam.
- **`ClaudeP` stdin pipe**: prior `Stdio::null` meant claude saw EOF immediately → likely silent no-op turns. This iter ships the first wire that actually delivers a prompt. Manual MT1 is the smoke test that proves the wire is live.
- **Preflight dedup**: Task 1 replaces preflight's inline minijinja call w/ `render_prompt`. Single source of truth for strict-undefined behavior. Eliminates risk of preflight + dispatch diverging.
- **Test seam shape**: `PromptRenderer` trait declared in engine. Tick tests inject `RecordingPromptRenderer`. `ClaudeP` does NOT depend on `PromptRenderer` — it receives an already-rendered `String` via `AgentContext.prompt`. Layering: tick orchestrates renderer; runner consumes string. DICTUM-003 satisfied.
- **Test count budget**: ~20 unit tests + 2 integration. AC5 + AC6 carry the most weight (snapshot semantics); AC1-3 are straightforward; AC7-8 verify gates, not duplicate ITER-178 coverage.
