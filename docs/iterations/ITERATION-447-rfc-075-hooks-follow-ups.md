---
title: RFC-075 hooks follow-ups
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-29
tags: []
related:
- implements: STORY-296
- related-to: STORY-295
---

## Context

RFC-075 chunk review leftovers. Past fix budget. Each small, independent.

## Tasks

- T1: one `engine::user_state_dir()`. `hooks/trust.rs`, `cache.rs`, `credentials.rs` each do HOME fallback + `.lazyspec`. Only hooks honour `LAZYSPEC_STATE_DIR`. All three use it.
- T2: `validate_ignore` docs dropped from hook input (`hooks.rs`), untested, undocumented. Decide: drop filter, or test + README line.
- T3: `hooks.rs` `ReplyFinding` field copy of `HookFinding`. Derive `Deserialize` on `HookFinding`, delete copy.
- T4: `cli::hook::blocked_exit` returns `(String, i32)`, code always 1. Return `Option<String>`; one `exit_if_blocked` for both `main.rs` sites.
- T5: renames. `save_together` param `status` → `status_update`. `Cleared.warnings` / `TransitionOutcome.warnings` hold errors on blocked path → `findings`. `StaleFindingsRequest` carries hook work → `BackgroundFindingsRequest`.
- T6: README `[[hooks]]` key list lacks `context_types`. Add beside `from`/`to`.
- T7: README pre-transition: git-backed stores commit each update, status, rollback separately. All-or-nothing = content, not commit. Document.
- T8: `cli/hook.rs` duplicate `use` lines; `run_hook` `too_many_arguments` → args struct.
- T9: `cli_init_template_test.rs` one test asserts copy + JSON + exec bit + trust + hint. Split per behaviour.
- T10: `hooks.rs` protocol test sleeps 1s, spawns `sh`. Timeout covered in `subprocess.rs`; drop it, keep round-trip only.
- T11: `ships_hooks` (init) keys on `.lazyspec/hooks` prefix only. Hooks with `run` elsewhere get no trust hint. Key on `[[hooks]]` non-empty.
- T12: `ops::update::run` (config-less, public) skips gate + hooks. Test-only use → `pub(crate)` / `cfg(test)`, so no public bypass.
- T13: RFC-075 "Web view" bullet stale (STORY-290 removed web view). Drop.

## Acceptance

- AC1: every task done or explicitly declined with reason.
- AC2: gate green.
