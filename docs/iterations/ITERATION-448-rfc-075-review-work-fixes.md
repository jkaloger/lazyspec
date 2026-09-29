---
title: RFC-075 review-work fixes
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-29
tags: []
related:
- implements: STORY-296
---

## Context

RFC-075 review-work RED. Fix pass. Each small, independent.

## Tasks

- T1: finish 447 T5 rename. `StaleFindingsComputed` → `BackgroundFindingsComputed`, `stale_findings_tx` → `background_findings_tx`, `request_stale_findings` / `apply_stale_findings` / `run_stale_findings_now` → `background_` equivalents. Engine `validation::stale_findings` keeps name (still staleness only).
- T2: `main.rs` update + `Hook` arms: logic (hook_findings JSON, finding print, arg build, exit) → `cli::update` / `cli::hook::run`. main = wiring only.
- T3: `ScriptedRunner` fake beside `HookRunner` (`hooks.rs` test_support). Move into cfg(test) of consumer. Two consumers → decline w/ reason if move = dup.
- T4: `pub` → `pub(crate)`: `pre_transition::updates_are_current`, `store::read_body`, `store::read_part_body`, where only engine calls.
- T5: `subprocess.rs` timeout test sleeps. Short/no sleep, or drop if covered elsewhere.
- T6: `cli_init_template_test` no-hooks test: set `LAZYSPEC_STATE_DIR` temp like siblings. Drop test comments restating name.
- T7: `user_state_dir` HOME unset → `./.lazyspec` = in repo, repo can ship trust. No HOME + no override → no state dir: hooks untrusted, cache off, credentials error.
- T8: `validation::validate_full` silent `HookEnv::process(false)` ignores `--no-hooks`, hits real trust store. Callers pass env explicit; drop default.
- T9: hook findings dup worker (`event_loop.rs`) vs test `run_background_findings_now`. One shared fn both call, so AC6 test covers real path.
- T10: `check_status_gate` runs twice (`run_with_config` + `write_doc`). Once.
- T11: status-write fail + rollback ok → error lacks "nothing was saved" context. Add.
- T12: `hooks::content_hash` parts no delimiter → collision. Delimit/length-prefix.
- T13: `hook run` JSON `id` = raw arg. Use resolved `doc.id`.
- T14: `app.rs` `hook_env` doc comment wrong (no cache). Fix. Unused `hook_config()` / `picker_on_rfc_001()` test helpers: use or delete.

## Acceptance

- AC1: every task done or declined w/ reason.
- AC2: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` green.


## Outcome

- T3 declined: `ScriptedRunner` 3 consumers (`hooks`, `pre_transition`, `tui::state::app`). Move = triplicate.
- T14 partial: comment fixed. `hook_config()` / `picker_on_rfc_001()` used, kept.
- Extra: `HookEnv::disabled()`. `process(true)` read backwards at ~115 test sites.
