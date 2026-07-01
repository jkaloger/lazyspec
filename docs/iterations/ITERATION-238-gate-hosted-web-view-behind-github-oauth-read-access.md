---
title: Gate hosted web view behind GitHub OAuth read access
type: iteration
status: accepted
author: unknown
date: 2026-06-30
tags: []
related:
  - implements: STORY-181
---

## Objective

Gate the hosted web view behind GitHub OAuth: authenticate the reviewer, admit only repo collaborators with `read`-or-higher permission via a signed-cookie session, and refuse to start on any non-loopback bind unless OAuth is configured.

## Satisfies

STORY-181, all ACs (bind policy, OAuth handshake + `state`/CSRF, membership admit, 403 unauthorized, 503 fail-closed, signed-cookie verify, TTL caching). The ACs are one tightly coupled auth subsystem and ship as a single slice; see Out of scope for what is explicitly not in this iteration.

## Context

- Story + ACs (the authoritative behavior list): STORY-181.
- RFC sections: RFC-052 §Authentication (session-establishment check, fail-closed, bind-not-independent), §Interfaces (`serve --bind`, `[web]` table).
- Repo coordinate resolution to reuse: `src/engine/github.rs` (`resolve_repo`, `infer_github_repo`) — the owner/repo the collaborator check runs against.
- Existing GitHub access pattern: `src/engine/gh.rs` shells out to the `gh` CLI; there is **no** in-process HTTP client, token store, or OAuth today. This iteration adds the first in-process HTTP/auth path (it cannot reuse the `gh`-CLI shell-out, which authenticates as the operator, not the reviewer).
- Prior web stories this builds on: STORY-176 (serve skeleton, `Arc<Store>`, `--bind`), STORY-180 (`engine::github_url`, `[web]` coordinates).
- Touch: `src/web/auth.rs` (new — OAuth handlers, session, membership check), `src/web/server.rs` / `src/web/routes.rs` (mount `/auth/github`, `/auth/callback`; insert the session-guard middleware), the `serve` CLI command (bind-policy gate), `src/engine/config.rs` (`[web]` `session_ttl`, OAuth client id/secret read from env not config file), `Cargo.toml` (`web` feature deps: an HTTP client + cookie/signing crates).

## Auth flow and threat model (slice-specific, not in the linked docs)

Flow: unauthenticated request to a content route under a hosted bind -> 302 to `GET /auth/github`, which mints a random `state`, stores it server-side keyed to the nascent session, and redirects to GitHub's authorize URL (read scope only). GitHub redirects to `GET /auth/callback?code&state`; the handler verifies `state` matches (else 400, no session), exchanges `code` for a user access token, reads the login, then calls `GET /repos/{owner}/{repo}/collaborators/{username}/permission`. Permission `read`+ -> set signed session cookie carrying the admit result and an establishment timestamp; no permission -> 403, no cookie; GitHub API error/unreachable -> 503, no cookie (fail closed). Subsequent requests present the cookie; the guard verifies the signature (bad signature -> treat as unauthenticated, re-enter the flow) and, if the cached result is within `session_ttl`, admits with no GitHub call; the first request past TTL re-runs the collaborator check.

Threat model (what the gate defends and what it does not): `state` defends the OAuth redirect against CSRF/login-fixation; the cookie signature (server-held key) defends against forged/tampered admit claims — the cookie is a bearer credential, so it must be signed, `HttpOnly`, `Secure`, and `SameSite=Lax`. The membership cache means revocation is **eventually consistent**, bounded by `session_ttl` (default 15 min): a collaborator removed mid-session retains access until TTL expiry — accepted per RFC-052. Fail-closed on API error is deliberate: never admit an unverified user. The OAuth client secret and session signing key are secrets read from env, never committed and never logged. Loopback (`127.0.0.1`) bypasses the entire gate (single-user local dev only); the bind-policy check is what prevents that bypass from ever being exposed on a hosted address.

## Tasks

1. Add `web`-feature deps to `Cargo.toml` (in-process HTTP client for the token exchange + collaborator call; cookie + HMAC signing) and the config surface: `[web] session_ttl` (default 15 min) in `src/engine/config.rs`, OAuth client id/secret + signing key read from env.
2. Implement the bind-policy gate in the `serve` command: non-loopback bind with OAuth unconfigured exits non-zero with a "hosted bind requires OAuth" message; loopback without OAuth keeps STORY-176 no-auth mode. Test both branches.
3. Implement `GET /auth/github` (mint + store `state`, authorize redirect, read scope) and `GET /auth/callback` (verify `state` -> 400 on mismatch; code->token exchange; read login). Test the `state` mismatch rejection.
4. Implement the collaborator-permission check against the resolved repo coords (reuse `src/engine/github.rs`); admit on `read`+, 403 on none, 503 on API error/unreachable. Cover all three outcomes test-first.
5. Implement signed-cookie sessions: set on admit with establishment timestamp; a session-guard middleware that verifies the signature (tampered -> unauthenticated, redirect to flow) and enforces the `session_ttl` cache (within TTL -> no API call; past TTL -> re-check). Mount the guard over content routes. Test signature-reject and the within-TTL no-call path.
6. Update the README `serve` section for the OAuth/bind config and required env vars.

## Out of scope

- Write scopes / write-back of any kind (RFC non-goal).
- Per-request membership re-checks — TTL-bounded caching only, per RFC-052.
- Differentiated in-app permissions: admission is binary (admit/deny).
- Org-team membership as a distinct admission path: repo collaborator permission only.
- Multi-repo / multi-project hosting: one instance, one project.
- Any STORY-176..180 functionality (skeleton, doc page, search, graph, deep-links) — those are already landed and only mounted-behind by this iteration's guard.

## Principles / conventions

- `lazyspec` conventions: dev binary via `cargo run`; update the README when the CLI surface changes (per project CLAUDE.md).
- RFC-052 layering principle 3: the web layer depends only on `engine`, never `tui`/`cli`. New code lives under `src/web/` and is gated behind the `web` cargo feature so default builds stay async/HTTP-free.
- Type-driven-design (skill): model admit/deny and the session outcomes as enums so unverified/unauthorized/admitted states are not confusable; secrets are typed values that never derive `Debug`/`Display` that prints them.

## Verification

- Hosted bind (e.g. `0.0.0.0`) with no OAuth env set: `serve` exits non-zero, prints the hosted-bind message; loopback with no OAuth still serves.
- `state` mismatch on `/auth/callback` returns 400 with no `Set-Cookie`.
- Collaborator with `read`+ gets a `Set-Cookie`; non-collaborator gets 403; simulated GitHub API error yields 503 — all with no session leaked.
- A cookie with a flipped signature byte is rejected and redirects into `/auth/github`.
