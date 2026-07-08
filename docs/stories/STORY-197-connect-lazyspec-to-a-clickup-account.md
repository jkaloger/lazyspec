---
title: Connect lazyspec to a ClickUp account
type: story
status: accepted
author: unknown
date: 2026-07-05
tags: []
related:
- implements: RFC-056
---

As a developer whose team tracks work in ClickUp, I authenticate lazyspec with a personal API token so it can read and write my ClickUp tasks.

Implements RFC-056 (ClickUp store). Journey step: act/auth.

## Acceptance criteria

- Given no stored credentials, when I run `lazyspec setup clickup` and enter a valid token, then the token is validated against ClickUp's `/user` endpoint and stored in the OS keychain (via the `keyring` crate).
- Given no reachable keychain backend (headless/CI), when setup succeeds, then the token is written to `~/.lazyspec/credentials.toml` under `[clickup] api_token` with dir mode `0700` and file mode `0600`, and the fallback is loudly logged — never a silent default.
- Given an invalid or revoked token, when I run `setup clickup`, then validation fails with a clear error and nothing is written to the keychain or the credential file.
- Given a stored token, when any ClickUp-backed command runs, then it reads the credential from the keychain first, falling back to the global file, never from the repo.

## Non-functional

- Transport is a native reqwest HTTP client (lazyspec's first). Errors are classified by real `reqwest::Error` variants and HTTP status codes, not stderr-substring scraping (do not repeat gh.rs's `classify_gh_error` wart).
- Token-only for v1: no OAuth, no env-var fallback. Credential store is global (keychain / ~/.lazyspec), never committed.
- Token wrapped in a newtype masking `Debug`/`Display` — never appears in logs, errors, or `--json`.
- On read, a credential file with perms looser than `0600` is refused (or loudly warned and repaired), not silently accepted.
