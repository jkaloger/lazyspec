---
title: Claude Code hook snippets and setup docs
type: iteration
status: accepted
author: agent
date: 2026-05-11
tags:
- hooks
- claude-code
- docs
related:
- implements: STORY-120
---



## Context

STORY-120 ACs 1,2,3,8,9. ITERATION-169 shipped throttle (ACs 4-7). Remaining: hook snippet wiring + docs. No Rust code. Pure doc + example.

Hook = inline shell one-liner in `.claude/settings.json`. Env-only `$ASSIGNED_TASK`. Unset → no-op silent. Post-tool-use + session-end non-zero must not abort session. Hardcoded `--min-interval 15m` (lease/4 of default 60m), doc note to tune.

Claude Code hook schema (verified `.claude/settings.json` in-repo):

```json
{ "hooks": { "<EventName>": [ { "hooks": [ { "type": "command", "command": "..." } ] } ] } }
```

Events: `SessionStart`, `PostToolUse`, `SessionEnd`.

## Changes

1. **README hook setup section** — `README.md`. New `### Claude Code Hooks` under `## Skills` (line 79) or new `## Coordination` section near end. Pick: insert after `## Skills` block, before `## Usage` (line 95). Content:
   - Heading + 2-line intro.
   - `.claude/settings.json` snippet (full 3 hooks, copy-pasteable).
   - `$ASSIGNED_TASK` contract: orchestrator sets it, unset → hooks no-op.
   - `--min-interval 15m` note: tune to `lease_duration / 4`.
   - Non-zero exit guard: `|| true` on PostToolUse/SessionEnd.

   Snippet shape:

   ```json
   {
     "hooks": {
       "SessionStart": [{
         "hooks": [{
           "type": "command",
           "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec claim \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --json || true"
         }]
       }],
       "PostToolUse": [{
         "hooks": [{
           "type": "command",
           "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec heartbeat \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --min-interval 15m --json || true"
         }]
       }],
       "SessionEnd": [{
         "hooks": [{
           "type": "command",
           "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec release \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --json || true"
         }]
       }]
     }
   }
   ```

   `[ -n "$VAR" ] && ... || true` covers ACs 1,2,3,8: env guard short-circuits when unset (AC2), `|| true` swallows non-zero from `lazyspec` so session-end never aborts (AC8). Same pattern for all three; SessionStart can technically drop `|| true` but keep it uniform.

   Cross-reference RFC-035 for design rationale.

2. **Example settings file** — `hooks/claude-code-settings.json` (new, top-level `hooks/` dir). Same JSON as snippet above. Linkable from README. `jq . hooks/claude-code-settings.json` must succeed. Future hook artefacts (other agents, daemon hook helpers) land alongside.

3. **RFC-035 hook section update** — `docs/rfcs/RFC-035-git-ref-document-storage-with-lease-based-claiming.md`. Existing snippet shows bare `lazyspec claim $ASSIGNED_TASK ...` with no env guard, no `--min-interval`, no `|| true`. Update to match README. Add 1-line note: see README + `examples/claude-code-hooks.json`. AC9 satisfied for `docs/` side.

4. **STORY-120 status** — promote `draft` → `accepted` once all ACs verified. Done after review.

## Test Plan

Doc-only iteration. No Rust unit tests. Verification = lint + manual smoke.

1. **AC2 (no-op when unset)** — copy snippet to throwaway shell, `unset ASSIGNED_TASK; bash -c '<command>'; echo $?`. Expect exit 0, no `lazyspec` invocation (verify by adding `set -x` or strace-equivalent; or: replace `lazyspec` in PATH with a fail-loud stub and confirm not called). Document expected output in iteration Notes.

2. **AC1 (SessionStart claim)** — `ASSIGNED_TASK=ITERATION-170 CLAUDE_SESSION_ID=test-1 bash -c '<SessionStart command>'`. Expect `lazyspec claim` runs. Repo must have coordination configured for full exercise; smoke = command resolves and lazyspec gets called.

3. **AC3 (PostToolUse heartbeat with throttle)** — same as AC1 but `lazyspec heartbeat ... --min-interval 15m`. Verifies throttle flag plumbed (already tested in ITERATION-169).

4. **AC8 (SessionEnd release, non-zero tolerated)** — `ASSIGNED_TASK=DOES-NOT-EXIST CLAUDE_SESSION_ID=test-1 bash -c '<SessionEnd command>'`. `lazyspec release` exits non-zero; hook line exits 0 due to `|| true`.

5. **AC9 (docs present)** — manual: grep README + RFC-035 for the snippet. `jq . hooks/claude-code-settings.json` parses.

6. **`lazyspec validate --json`** — passes on iteration + story + RFC-035 after edits.

No automated test seam exists for shell snippets in `.claude/settings.json`. Tradeoff: skip CI coverage; rely on manual smoke + future user-reported breakage. Alternative (rejected): a bats-style test runner — adds dep + CI surface for one-time snippet validation. Not worth it.

## Notes

- Hook command is shell one-liner, not script file. RFC-035 keeps it inline so it stays a copy-paste. No `*.sh` script shipped — `hooks/` holds JSON examples only.
- `|| true` chosen over `; exit 0` for terseness. Both work.
- `[ -n "$VAR" ]` chosen over `${VAR:?}` style. POSIX-compatible, no extra deps. Works in bash/zsh/dash.
- `init` left alone. User declined auto-writing `.claude/settings.json`. Reader copy-pastes from README.
- `--min-interval 15m` hardcoded. If lease_duration tuned, user edits snippet. Future: `lazyspec hooks emit` could template the snippet from config — out of scope.
