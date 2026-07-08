---
title: setup clickup command + global credential file
type: iteration
status: draft
author: unknown
date: 2026-07-05
tags: []
related:
- implements: STORY-197
- blocks: ITERATION-274
---

## Objective

Add `lazyspec setup clickup` to capture a personal API token, validate it via the ClickUp client, and persist it keychain-first (`keyring` crate) with a loud plaintext-file fallback (`~/.lazyspec/credentials.toml`, `0600`), plus a global-only credential read path and a redacting token newtype.

## Context

- Story + ACs: STORY-197 (auth). This slice delivers the setup command + credential store; it consumes the `ClickupClient` from the prior iteration.
- Design: RFC-056 §Auth (OS keychain default via `keyring`; plaintext fallback file at `~/.lazyspec/credentials.toml` under `[clickup] api_token` only when no keychain backend is reachable, dir `0700`/file `0600`, loudly logged; global not per-repo, token-only, no env fallback; token redacted everywhere) and the `lazyspec setup clickup` interface.
- Touch: new CLI subcommand (`src/cli/setup.rs` or equivalent) wired into the command dispatch; a credentials module (keychain via `keyring` + fallback file read/write); token newtype; new `keyring` dependency; README CLI-interface update per project convention.

## Satisfies

STORY-197 AC1 (validate against `/user`, then store in keychain), AC2 (keychain unreachable -> file fallback with `0700`/`0600` perms, loudly logged), AC3 (invalid token -> clear error, nothing written to keychain or file), AC4 (ClickUp-backed commands read keychain first, then global file, never the repo).

## Tasks

1. Add the `setup clickup` subcommand: prompt for a token (masked where practical), non-interactive `--token` flag for scripting/tests.
2. Call the `ClickupClient` token validation before any write; on failure surface the classified error and leave keychain and credential file untouched.
3. Wrap the token in a newtype masking `Debug`/`Display` (fixed mask) so it can't leak into logs/errors/`--json`.
4. On success, store in the OS keychain via `keyring`. When no keychain backend is reachable, fall back to writing `[clickup] api_token` to `~/.lazyspec/credentials.toml` — dir `0700`, file `0600`, enforced on write; log the fallback loudly; merge rather than clobber an existing file.
5. Add a credential reader resolving keychain first, then the global file (explicitly not the repo / cwd); refuse (or warn loud and repair) a credential file with perms looser than `0600`; expose it for ClickUp-backed commands to consume.
6. Update the README CLI section for `lazyspec setup clickup`.

## Out of scope

- reqwest client + error classification (prior iteration).
- `ClickupTasksStore`, config `clickup_list_id`, dispatch registry, fetch/write paths -> later RFC-056 stories.
- OAuth, env-var fallback (RFC-056 non-goal).

## Principles/conventions

- Reuse the `ClickupClient` + `ClickupError` from the prior iteration; no second HTTP path.
- Credential file is global and never committed; do not read repo-local credentials.
- Test-first per project convention.

## Verification

- Valid token via `--token`: token lands in the keychain (or, with keychain unavailable, `[clickup] api_token` appears in `~/.lazyspec/credentials.toml` with `0600` perms and a loud fallback log); command exits 0.
- Invalid/revoked token: command errors clearly; keychain and credential file byte-for-byte unchanged (absent stays absent).
- With a token stored, the credential reader returns it while running inside a repo that contains no credential file.
- `{:?}` / `--json` output of anything holding the token shows the mask, never the token.
