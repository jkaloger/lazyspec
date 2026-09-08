---
title: Show the staleness band on the TUI detail surface
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-275
reviewed: 1a7db94929dbfe5c9b1a0520e12cb9927ac6803d
---

## Objective

The selected document's TUI preview header carries its staleness band, computed in a background worker, so a cursor move never waits on git.

## Satisfies

STORY-275 AC1, AC3, AC4, AC5, and AC6's TUI clause. AC2 and AC6's web clause: dropped, not deferred — the web badge is not being built.

## Context

- Story + ACs: STORY-275. The governing decision is RFC-069 Decision 6 -- detail only, off the render path.
- **The pattern to copy is the search worker BUG-011 produced.** `event_loop.rs:668`-`:698` (one long-lived thread, drains the channel to the newest request, results carry a generation), `app.rs:2554` `update_search` and `:2575` `apply_search_results` (bump, dispatch, drop on mismatch), the `AppEvent::SearchResults` variant (`app.rs:331`) and its handler (`event_loop.rs:565`). Copy the shape, not the corpus: staleness has no snapshot to build.
- **`compute` does not need a `Store`.** `staleness.rs:96` takes `&Store` and reads exactly one thing off it -- `store.governs_root()`, at `:107` and `:119`. A worker thread cannot borrow `App`'s store, and cloning a `Store` per cursor move is absurd. Narrow the parameter to `governs_root: &Path`. Four production call sites (`cli/show.rs:159`, `:246`, `cli/why.rs:41`, `validation.rs:1216`) all hold a `&Store`; the `staleness.rs` fixtures are one mechanical replacement. This is the owned, thread-safe seam BUG-011 needed a whole new type (`SearchCorpus`) for -- here it is a parameter.
- **Dispatch site: the frame loop, beside `request_expansion`.** `event_loop.rs:788` already calls `app.request_expansion(&tx)` once per frame, and `:790` does per-selection diagram work. `request_expansion` (`expansion.rs:11`) is the shape: read the selection, return early if already satisfied or already in flight, else dispatch. One call site covers every way `selected_doc` moves. None of the eleven-odd mutators needs to know staleness exists.
- **The dedupe key is `(path, reviewed)`, not path.** A TUI status change stamps `reviewed` (STORY-274, `app.rs:3287` `confirm_status_change`) and reloads the document; keyed on path alone the pre-stamp band would sit on screen until the cursor left and came back. One tuple, no new plumbing. It does not notice HEAD moving under a running TUI. Accepted -- no watch.
- **One slot, not a cache.** `Option<Staleness>` for the current selection, discarded the moment the key changes. No map, no eviction, no `(reviewed, HEAD)` key. The worker draining to the newest request is what makes a held-down `j` cost one computation instead of N; the dedupe key is what makes a re-render cost none. Anything past that is STORY-276, which owns the measured cost and the caching non-goal RFC-069 named.
- **`App.git` stays the only git handle on `App`.** It cannot cross a thread boundary -- `Box<dyn GitRefOps>`, no `Send`. The worker owns a `GitCli` local to `event_loop::run`, exactly as the search worker owns its scoring. Tests never reach the worker: they drive `request_staleness` and `apply_staleness` directly, and a `#[cfg(test)] run_staleness_now` computes inline through `self.git`. `app.rs:2588` `run_search_now` is the shape.
- **The badge line is unconditional.** Governs and Reviewed (`panels.rs:1128`, `:1135`) are absent when their field is empty. Staleness is not: AC4 wants a placeholder *in the badge's place*, and a line that appears late reflows the header under the reader.
- Wording comes from `impl Display for Staleness` (`staleness.rs:77`) -- the same string `show` prints. Do not format band, driver and drift by hand in the TUI. `compute` is infallible, so there is no error state to render, only "not yet".
- Fullscreen (`panels.rs:1353`) is out. Its header is a chip strip carrying neither `governs` nor `reviewed`, by the call RFC-068 already made there.

## Tasks

1. Narrow `staleness::compute`'s first parameter from `&Store` to `governs_root: &Path`. Sweep the four production call sites and the `staleness.rs` fixtures. No behaviour change; green before anything else lands.
2. `AppEvent::StalenessComputed { generation, staleness }` beside `SearchResults`. `StalenessRequest { governs_root, config, doc, generation }` beside `SearchRequest` (`app.rs:305`) -- all owned, all `Send`.
3. `App` fields: `staleness: Option<Staleness>`, `staleness_generation: u64`, `staleness_key: Option<(PathBuf, Option<String>)>`, `staleness_tx: Sender<StalenessRequest>`. Three literals: `App::new` and the two test constructors (`app.rs:3847`, `:4337`).
4. Test-first: `request_staleness` on a new selection bumps the generation, clears the badge, and sends one request carrying that generation. Called again for the same `(path, reviewed)` it sends nothing. `app.rs:5821` is the fixture shape.
5. Test-first, AC5: `apply_staleness` with a superseded generation leaves the badge untouched; with the live one it sets it. Then the real case -- the selection moves off and back before the first result lands: the generation moved twice, so the first result is dropped and the badge is the second computation's, never the first document's.
6. Test-first, AC6: fifty documents in the tree, one frame's `request_staleness` puts exactly one request on the channel; one cursor move puts exactly one more. `doc_row_cells` (`panels.rs:663`) and `doc_row_for_node` (`:792`) take no git and no `Staleness`, and the assertion is that no row's cells carry a band word.
7. Implement `request_staleness` and `apply_staleness` in `src/tui/state/expansion.rs`, beside `request_expansion`.
8. The handler arm in `handle_app_event`; the worker thread in `event_loop::run` owning a `GitCli` and draining to the newest request; the one dispatch call in the frame loop.
9. Test-first, AC1 and AC4: `build_preview_header_lines` with `Some(&staleness)` renders one line carrying the `Display` string; with `None` it renders the placeholder in the same position; the line count is identical either way. Band picks the colour.
10. Add the parameter to `build_preview_header_lines` and render the line after Reviewed. Nine existing test call sites pass `None`.
11. README: the TUI section -- the preview header carries the band, computed in the background, placeholder until it lands.

## Out of scope

- **AC2 and the web detail band.** Dropped, not deferred -- the web badge is not being built, and AC6's web clause goes with it. STORY-275 closes with AC2 unmet.
- The fullscreen document view. See Context.
- List rows, list columns, list filters, and any list-side band. RFC-069 non-goals; AC6 is the fence, not a note.
- **A cache.** One slot for the current selection is the badge's value, not storage. No `(reviewed, HEAD)` key, no map, no eviction. STORY-276 owns the cost and the key.
- The `validate_full` staleness cost `refresh_validation` already pays from eleven call sites. Untouched here. STORY-276.
- Any watch on HEAD, or on `reviewed` beyond the store reload the TUI already does.

## Principles/conventions

`cargo run --quiet -- convention`. Principle 3: the band is engine work -- the TUI renders a `Staleness` and never decides one. Principle 6: no new type for the worker payload beyond the request struct the channel needs. DICTUM-007: the worker sends an `AppEvent` and never touches `App`. DICTUM-004: state-level tests, no terminal.

## Verification

In a repo whose documents carry `reviewed`, hold `j` down the document list. Keystrokes never queue behind git; the badge shows the placeholder for a beat and settles on the band of the document under the cursor, never the previous one.
