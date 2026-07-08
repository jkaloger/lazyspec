---
title: ClickUp API client + reqwest transport + token validation
type: iteration
status: complete
author: unknown
date: 2026-07-05
tags: []
related:
- implements: STORY-197
- blocks: ITERATION-269
---

## Objective

Introduce lazyspec's first native reqwest HTTP client behind a `ClickupClient` trait whose token-validation call hits ClickUp's `/user` endpoint.

## Context

- Story + ACs: STORY-197 (auth). This slice delivers the transport + validation half; the credential file + `setup` command are a separate iteration.
- Design: RFC-056 §Transport, §Auth, and Interfaces (`ClickupClient` trait). Follow the reqwest-error-variant + HTTP-status classification posture; do NOT copy gh.rs `classify_gh_error`/`extract_http_status` stderr-substring scraping (RFC-056 names the x509->"HTTP 509" misparse).
- Mirror the existing `GhCli`/fake split for the real-impl + fake-impl pattern.
- Touch: new client module (e.g. `src/store/clickup/client.rs`), reqwest added to Cargo.toml (see installing-dependencies skill), a fake `ClickupClient` for tests.

## Satisfies

STORY-197 non-functional (native reqwest transport; error classification by real `reqwest::Error` variants + HTTP status codes) and the `/user`-validation logic underpinning AC1/AC2. File write + global read deferred (next iteration).

## Tasks

1. Add reqwest as a dependency (rustls/native-tls per project norms) via the installing-dependencies skill; no hand-editing lockfiles.
2. Define the `ClickupClient` trait with an `auth_status`/token-validation method taking a token and calling `GET /user`.
3. Implement the reqwest-backed real client against base URL `https://api.clickup.com/api/v2`; token in raw `Authorization` header (no `Bearer` prefix, `pk_`-prefixed personal token). Build a `ClickupError` classified off `reqwest::Error` variants (connect/timeout/decode) and HTTP status (401/403 -> invalid token, 429 -> rate limit, 5xx -> upstream).
4. Rate-limit handling: on 429, parse `X-RateLimit-Reset` (Unix epoch) / `X-RateLimit-Remaining` headers into the `ClickupError::RateLimited` variant carrying the reset instant; the client backs off to that instant rather than spinning. Per-token budget is 100 req/min on Free/Unlimited/Business.
5. Implement a fake `ClickupClient` returning scripted valid/invalid/error responses for downstream tests.
6. Unit-test classification: mapping from status codes and error variants to `ClickupError` cases, including 429 -> `RateLimited` with the reset instant taken from the header.

## Out of scope

- `lazyspec setup clickup` command, the `~/.lazyspec/credentials.toml` file, and the global credential read path -> next iteration.
- `ClickupTasksStore`, `StoreBackend::ClickupTasks`, dispatch registry, field/relation mapping -> later RFC-056 stories.

## Principles/conventions

- installing-dependencies skill for adding reqwest.
- type-driven-design skill: model `ClickupError` as a discriminated enum; avoid stringly-typed error paths.
- Test-first per project convention.

## Verification

- A valid token -> `auth_status` returns Ok with the ClickUp user identity.
- A 401 response -> a distinct invalid-token error variant, never a generic string scrape.
- A 429 response with `X-RateLimit-Reset` -> `RateLimited` variant carrying the parsed reset instant; no fake status invented from unrelated digits.
