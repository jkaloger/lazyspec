---
title: Stamp reviewed on a board-bound transition, never on a fetch
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-274
- related-to: STORY-276
reviewed: 31d96c41e3dc06d0fc103303fc29d5043d9599d1
---

## Objective

A human's `update <id> --status <next>` on a `status_authority` type moves the board card and stamps `reviewed` in the same issue write; a status arriving through `fetch` stamps nothing.

## Satisfies

STORY-274 AC4, AC5. Closes STORY-274.

## Context

- Story + ACs: STORY-274. The two-paths rule: RFC-069 §Design "Stamping" para 2, and Decision 4's rejected option "stamp on board-synced status".
- ITERATION-427 appended `("reviewed", sha)` to the update slice for `StoreBackend::Filesystem` only, and recorded the real seam name (`ops::update::run_with_config`, not `Store::update_status`). This slice adds `GithubIssues` to that gate and nothing else -- clickup, git-ref and milestone cache builders hardcode `reviewed: None`.
- **The distinction is the path, not the type.** One document, one type, two code paths that never meet:
  - Human: `ops::update::run_with_config` -> `registry.for_type(td).update(...)` (`src/engine/ops/update.rs:100`) -> `GithubIssuesStore::update` (`src/engine/store_dispatch.rs:1925`).
  - Board sync: `fetch` -> `sync::sync_all` (`src/engine/sync.rs:310`) -> `reconcile_project_fields_into_cache` (`:568`) -> `reconcile_project_fields_for_meta` -> `write_cache_file`.
  `sync_all` is handed no `GitRefOps` at all. The github-issues sync path has no way to read a HEAD, and never calls `ops::update`. So AC4 needs no gate, no flag, and no "did this come from fetch" boolean: it is true because the stamp is appended in a function fetch does not call. This slice's job is a regression test that says so, not a mechanism.
- AC5's stamp rides the **issue body**, not the cache file. `GithubIssuesStore::update` round-trips the remote body through `issue_body::deserialize` (`store_dispatch.rs:1946`) and `serialize` (`:2043`); `serialize` already writes a `reviewed:` line when `meta.reviewed` is set (`src/engine/issue_body.rs:102`) and `deserialize` already reads it back (`:178`). Stamping the cache file instead would be erased by the next fetch.
- Without a branch, `"reviewed"` falls through the key match into `attr_updates` (`store_dispatch.rs:1977`) and `apply_attrs` rejects it as an undeclared attribute -- the whole transition fails. One arm beside `"status"`, `"title"`, `"author"`: `"reviewed" => meta.reviewed = Some(value.to_string())`.
- The card move is untouched. `authority_status` resolves from the `status` key alone (`:1935`), and `write_authority_status` runs after the body push (`:2050`). One `issue_edit`, one field mutation, as today.
- AC4's other half is that fetch **preserves** an existing anchor. It does by construction: the cache round-trip parses `reviewed` into `DocMeta` and writes it back (`document.rs:896` `governs_and_reviewed_roundtrip_through_cache_serializer`). Test it anyway -- an anchor silently lost on fetch reads as a review that never happened.

## Tasks

1. Test-first, AC5 at the store: `GithubIssuesStore::update` with `[("status", <column>), ("reviewed", sha)]` on a `status_authority` type pushes an issue body carrying `reviewed: <sha>`, and still fires the board field mutation. `store_dispatch.rs:9308` `update_status_moves_the_card_on_the_authority_board` is the fixture.
2. Test-first: the same call on a type declaring no attributes returns `Ok`. It errors today; that is the regression ITERATION-427's gate is currently hiding.
3. Add the `"reviewed"` arm to `GithubIssuesStore::update`'s key match.
4. Lift the gate in `ops::update::run_with_config` to include `StoreBackend::GithubIssues`.
5. Test-first, AC5 end to end: `run_with_config` on a board-bound type stamps and moves the card, with exactly one `head:` call in the mock's `call_log()`.
6. Test-first, AC4: a fetch round that moves a doc's board status leaves `reviewed` byte-identical -- absent stays absent, a present sha survives. `sync.rs:1632` `nested_cache_docs_take_their_status_from_the_authority_board` is the fixture; extend it, do not write a parallel one.
7. README: the `status_authority` section -- a local `update --status` stamps `reviewed`, a status arriving through `fetch` does not.

## Out of scope

- clickup, git-ref and milestone backends. They cannot hold a `reviewed`; the gate keeps excluding them. Not deferred -- excluded.
- Any "was this local" flag threaded through the store trait. The paths already differ; see Context.
- Building a web status form. STORY-274 AC2 names one that does not exist; ITERATION-427 records it.
- `pin` (STORY-270). Unchanged, and still the way to stamp without a transition.
- **Caching, and the cost these anchors make real.** Board-bound documents now start carrying `reviewed`, so `StaleRule`'s per-document git cost on the `validate_full` path stops being hypothetical for them too -- one `git cat-file -p` per document plus one `git diff --numstat` per pinned document, synchronously, per `status --json` and per TUI validation refresh. STORY-276 owns it. Do not grow a cache here.
- TUI and web band badges (STORY-275).

## Principles/conventions

`cargo run --quiet -- convention`. Principle 6: no indirection for the local-versus-synced distinction -- two functions already are the distinction. Testing §Desiderata "Behavioral": assert on the pushed issue body and the cache file's bytes, not on which branch ran.

## Verification

On a repo with a `status_authority` type: `cargo run --quiet -- update <id> --status <next> --json | jq .reviewed` equals `git rev-parse HEAD` and the card has moved column; a following `cargo run --quiet -- fetch` then `show <id> --json | jq .reviewed` returns the same sha.
