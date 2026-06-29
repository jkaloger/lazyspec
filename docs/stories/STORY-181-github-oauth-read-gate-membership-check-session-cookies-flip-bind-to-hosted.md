---
title: GitHub OAuth read gate, membership check, session cookies; flip bind to hosted
type: story
status: accepted
author: jkaloger
date: 2026-06-30
tags: []
related:
- implements: RFC-052
---## Context

RFC-052 gates the hosted view behind GitHub OAuth: a reviewer authenticates with their existing GitHub account and is admitted iff they are a collaborator/member on the configured repo. This story adds the OAuth web-application flow (`GET /auth/github`, `GET /auth/callback`), the membership check, signed-cookie sessions, and the bind policy that ties auth to hosting. It is the last story and gates hosted deployment; STORY-176 through 180 are developable and demoable on loopback without it.

Two RFC constraints are load-bearing. First, **auth and bind are not independent**: any non-loopback bind requires OAuth configured, and the server refuses to start hosted without it; the no-auth path exists only for `127.0.0.1` single-user local dev. Second, the membership check runs once at session establishment and is cached for a bounded TTL (per-request checks would couple every page load to GitHub latency and rate limits); when the GitHub API is unavailable at establishment, the server **fails closed** (denies the session). Revocation is therefore eventually consistent, bounded by the TTL.

The authoritative admission check is GitHub's collaborator-permission endpoint for the configured repo (`GET /repos/{owner}/{repo}/collaborators/{username}/permission`); a non-error permission of `read` or higher admits. OAuth client id/secret, repo coordinates, and session TTL are configured out-of-band (env / `.lazyspec.toml` `[web]`), never committed. Read scope only.

## Acceptance Criteria

- **Given** a hosted bind (any non-loopback address) and no OAuth configuration
  **When** `lazyspec serve --bind <non-loopback>` is invoked
  **Then** the server refuses to start, exits non-zero, and prints that hosted bind requires OAuth.

- **Given** a loopback bind (`127.0.0.1`)
  **When** `serve` starts without OAuth
  **Then** it runs in single-user no-auth mode (the STORY-176 behavior is preserved).

- **Given** a hosted, OAuth-configured instance and a request with no valid session cookie
  **When** the client hits any content route
  **Then** it is redirected through `GET /auth/github` into the GitHub web-application OAuth flow (read scope only).

- **Given** the OAuth handshake
  **When** `GET /auth/github` issues the authorize redirect and `GET /auth/callback` receives the response
  **Then** a `state` parameter is generated, stored, and verified on callback; a callback with a missing or mismatched `state` is rejected with HTTP 400 and no session is created.

- **Given** a valid OAuth callback for a user whose collaborator permission on the configured repo is `read` or higher
  **When** `GET /auth/callback` completes
  **Then** a signed session cookie is set and the user is admitted.

- **Given** a valid OAuth callback for a user with no collaborator permission on the repo
  **When** the membership check runs
  **Then** no session cookie is set and the response is HTTP 403.

- **Given** the GitHub API returns an error or is unreachable at session establishment
  **When** the membership check is attempted
  **Then** the server fails closed: no session cookie is set and the response is HTTP 503 (the user is not admitted).

- **Given** a request carrying a session cookie whose signature does not verify
  **When** the request is processed
  **Then** the cookie is rejected (treated as unauthenticated) and the user is sent back through the OAuth flow.

- **Given** an established session and a configured TTL of `T`
  **When** requests arrive within `T` of establishment
  **Then** the cached membership result is used with no GitHub API call; the first request after `T` re-runs the membership check.

## Scope

### In Scope

- `GET /auth/github` and `GET /auth/callback` OAuth web-application flow (read scope), including `state`/CSRF generation and verification.
- Repo collaborator-permission check (`read`-or-higher admits) at session establishment, cached for a bounded, configurable TTL.
- Fail-closed behavior (HTTP 503, no session) when the GitHub API errors or is unreachable; HTTP 403 for authenticated-but-unauthorized users.
- Signed-cookie sessions with signature verification (tampered cookies rejected).
- Bind policy: hosted (non-loopback) requires OAuth; loopback allows no-auth.
- Out-of-band config: OAuth client id/secret, repo coordinates, and `[web] session_ttl` (default 15 minutes), all uncommitted.
- `--bind <addr>` flag wired to the policy.

### Out of Scope

- Write scopes or any write-back (RFC non-goal).
- Per-request membership re-checks (explicitly rejected for latency/rate-limit reasons; TTL-bounded caching only).
- Differentiated in-app permissions: the view is read-only single-tier, so admission is binary (admitted / denied); "permissions mirror the repo" reduces to the admit/deny gate.
- A separate identity system; GitHub accounts are the only identity.
- Org-team membership as a distinct path: admission is by repo collaborator permission, not org-team resolution.
- Multi-repo / multi-project hosting (one instance serves one project).
