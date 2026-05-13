---
title: Lifecycle hook runner
type: iteration
status: draft
author: agent
date: 2026-05-13
tags: []
related:
- implements: STORY-127
---

## In Scope

- `HookRunner` trait + `BashHookRunner` real impl
- 4 lifecycle pts: `after_create`, `before_run`, `after_run`, `before_remove`
- exec: `bash -lc <script>`, cwd=workspace
- env inject: `LAZYSPEC_DOC_ID`, `LAZYSPEC_DOC_TYPE`, `LAZYSPEC_AGENT_ID`, `LAZYSPEC_BRANCH`, `LAZYSPEC_WORKSPACE`
- per-hook `timeout_ms` (default 60000); timeout = hook fail
- fail semantics: `after_create`/`before_run` fatal (bubble Err); `after_run`/`before_remove` logged + Ok
- config `[orchestration.hooks]` w/ per-hook `script` + `timeout_ms`

## Out of Scope

- workspace creation (group A)
- agent spawn (group B)
- hook discovery beyond config
- multi-step pipelines (script author problem per RFC)

## Acceptance Criteria

- AC11: `after_create` runs after worktree creation, fatal on failure → aborts workspace creation, no agent spawn.
- AC12: `before_run` runs before each turn, fatal-to-attempt on failure.
- AC13: `after_run` runs after agent exits, failures logged not propagated.
- AC14: `before_remove` runs before teardown, failures logged not blocking.
- AC15: Hook env: `LAZYSPEC_DOC_ID`, `LAZYSPEC_DOC_TYPE`, `LAZYSPEC_AGENT_ID`, `LAZYSPEC_BRANCH`, `LAZYSPEC_WORKSPACE`.
- AC16: `timeout_ms` (default 60s) honored; timeout = hook failure per that hook's semantics.

## Test Plan

setup: TempDir = workspace. real `bash -lc`. assert via `HookOutcome` + caller `Result`.

- AC11 `after_create` fatal:
  - script `exit 0` → caller Ok, workspace kept
  - script `exit 1` → caller Err, no agent spawn (assert via mock/flag)
- AC12 `before_run` fatal:
  - `exit 0` → Ok
  - `exit 1` → Err (attempt aborted)
- AC13 `after_run` non-fatal:
  - `exit 1` → caller Ok; stderr/log captured (test log sink)
- AC14 `before_remove` non-fatal:
  - `exit 1` → caller Ok; teardown proceeds (assert teardown called after)
- AC15 env inject:
  - script: `printenv > $tmp/out`
  - read file; assert 5 vars present w/ expected values
- AC16 timeout:
  - script `sleep 999`, `timeout_ms=100`
  - assert `HookOutcome::Timeout` returned <500ms
  - for `before_run` → caller Err; for `after_run` → caller Ok
  - assert child killed (no zombie; SIGTERM then SIGKILL grace)

note DICTUM-004: no sleeps in test code itself. `sleep` inside hook script = test input, fine.

## Changes

1. **`src/engine/hooks.rs`** new mod. ACs 11-16.
   - types: `HookSpec { script: String, timeout: Duration }`
   - `HookEnv { doc_id, doc_type, agent_id, branch, workspace: PathBuf }`
   - `HookOutcome::{Ok, NonZero(i32), Timeout, SpawnFailed(String)}`
   - trait `HookRunner { fn run(&self, spec: &HookSpec, env: &HookEnv) -> HookOutcome; }`
   - impl `BashHookRunner` → `Command::new("bash").arg("-lc").arg(&spec.script).current_dir(&env.workspace).envs(env_map())`
   - verify: `cargo test engine::hooks`, `cargo clippy`

2. **`HookPoint` enum + caller-side wrapper** same file. ACs 11-14.
   - `enum HookPoint { AfterCreate, BeforeRun, AfterRun, BeforeRemove }`
   - `impl HookPoint { fn is_fatal(self) -> bool { matches!(self, Self::AfterCreate | Self::BeforeRun) } }`
   - `fn run_hook(point, runner, spec, env) -> Result<(), HookError>`:
     - match outcome; if Ok → Ok
     - else if fatal → Err
     - else log to stderr/tracing + Ok
   - verify: unit tests w/ fake `HookRunner` impl per point × outcome matrix

3. **Timeout impl in `BashHookRunner`**. AC16.
   - spawn child, deadline = now + timeout
   - loop: `child.try_wait()` → done?; else `thread::sleep(10ms)`; until deadline
   - on deadline: SIGTERM (`kill -15`), brief grace (e.g. 200ms poll), then SIGKILL if alive
   - on unix use `nix` or libc `kill(pid, SIGTERM/SIGKILL)`; check existing deps first
   - return `HookOutcome::Timeout`
   - check `src/daemon.rs` for accept_poll pattern; reuse style
   - verify: AC16 test passes <500ms; no zombie (assert `try_wait` Some after)

4. **Config `src/engine/config.rs`**. ACs 11-16.
   - `struct HookConfig { script: Option<String>, timeout_ms: Option<u64> }`
   - `struct OrchestrationHooks { after_create, before_run, after_run, before_remove: Option<HookConfig> }`
   - default `timeout_ms = 60_000`
   - nest under `[orchestration.hooks]`
   - verify: toml roundtrip test; missing section = all None = no-op hooks

## Notes

- depends on group A: workspace path exists before `after_create` fires
- `bash -lc` deliberate (RFC); multi-step = script author problem
- timeout kill = SIGTERM then SIGKILL grace (no zombies)
- trait seam = test impls w/o spawning bash (dictum 4 I/O at boundary)
- non-fatal hooks log via tracing/stderr; caller stays Ok
