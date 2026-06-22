---
title: Enriched templates and init materialization
type: iteration
status: draft
author: agent
date: 2026-06-21
tags: []
related:
- implements: STORY-148
---

## Changes

One iteration. Six ACs. PLAN ONLY — no code here.

Verified paths (read before touching):
- `src/engine/fs_ops.rs` — `load_template` (~14, on-disk-or-embedded), `default_template` (~49, embedded per-type defaults), `story_template` (~25, the spec `story.md` companion), `create_document` (~145; substitution + write at ~228-242).
- `src/engine/template.rs` — `render_template` (~8, naive `{key}` -> value `String::replace`).
- `src/cli/init.rs` — `run` (~39); empty templates dir made at ~50; `starter_config` (~13) holds the type list we iterate.
- `src/tui/content/gfm/render.rs` — `render_gfm_segments` (~210); `Markdown` segments go through `tui_markdown::from_str`.
- `src/tui/views/panels.rs` (~43-44) — TUI body view calls `extract_gfm_segments` + `render_gfm_segments`.
- `src/cli/show.rs` — `run` (~74) prints body raw with no markdown pass.

1. **Enrich embedded `default_template` per type** (fs_ops.rs ~49-140).
   - Each type's template gets an `<!-- intent: ... -->` header comment as the first body line (after frontmatter), stating the purpose of the type.
   - Each `## Section` gets an `<!-- guidance: ... -->` comment on the line under the heading, describing what belongs there. These REPLACE the bare `TODO:` / `- TODO` lines.
   - Cover every arm: `story`, `iteration`, `spec`, and the catch-all `_`. Keep frontmatter and `{title}`/`{author}`/`{date}`/`{type}` placeholders exactly as-is.
   - Leave `story_template` (~25, the spec companion `story.md`) consistent — it has its own AC structure; add an intent/guidance pass there too so spec subdirs match AC6.
   - Example shape:
     ```
     <!-- intent: fix a design decision and its alternatives before code -->

     ## Context
     <!-- guidance: problem, constraints, why now -->
     ```
   - AC1, AC3 (substitution leaves comments intact), AC6.
   - Verify: build; eyeball each arm carries one intent header + one guidance comment per section.

2. **Materialize default templates to disk in `init`** (init.rs ~50, inside `run`).
   - After `create_dir_all(templates.dir)`, iterate the config's default types and write `{type}.md` (lowercased) into the templates dir, content = `default_template(&type.name)`.
   - Use a write-if-absent guard (mirror `write_if_absent` at ~166) so re-running or pre-existing files are never clobbered.
   - Source the type list from `config.documents.types` (the same list `init` already iterates at ~47).
   - AC4, AC6.
   - Verify: `init` in a temp dir leaves a `.md` per default type in the templates dir, each enriched.

3. **Confirm comments are invisible in the renderer; strip in the one path that shows them raw.**
   - TUI path: `render_gfm_segments` -> `tui_markdown::from_str`. Verified `tui-markdown` 0.3.7 drops `Event::Html`/`Event::InlineHtml` (logs `warn!`, emits no span), so `<!-- ... -->` already renders invisibly. No change needed there — assert it.
   - CLI path: `show.rs` ~74 prints `body` RAW (no markdown pass), so comments WOULD show. Decide per AC2 scope: AC2 says "rendered for display". The TUI is the rendered surface; `show` is a plaintext dump. Plan: add an HTML-comment strip helper applied to the body before `println!` in `show.rs` (both `run` and the human-facing branch), so AC2 holds for the CLI display too. Do NOT strip in `run_json` (machine output keeps the raw file faithful) or in `get_body_raw`/`get_body_expanded` (those feed `@ref` expansion and must stay verbatim).
   - AC2.
   - Verify: TUI preview of a doc with intent/guidance shows no comment text; `show` (non-json) omits the comments; `show --json` body still contains them.

4. **Confirm on-disk override still wins** (fs_ops.rs `load_template` ~14).
   - No code change — `load_template` already prefers `{type}.md` on disk over the embedded default. With AC4 now writing those files, the on-disk file is the live source post-`init`.
   - AC5.
   - Verify: edit a materialized template, create a doc of that type, confirm the doc reflects the edit, not the embedded default.

5. **Confirm `{key}` substitution leaves comments intact** (template.rs `render_template` ~8).
   - No code change — `render_template` only replaces `{title}`/`{author}`/`{date}`/`{type}`; `<!-- ... -->` has no braces so it passes through untouched.
   - AC3.
   - Verify: render an enriched template with vars set, confirm placeholders resolved and every intent/guidance comment survives verbatim.

## Test Plan

No code in this doc — these are the checks the build phase must satisfy. One per AC.

- **AC1 — created doc contains comments.** Create a doc of a default type from the on-disk (materialized) template; assert the file body contains its `<!-- intent: ... -->` header and a `<!-- guidance: ... -->` comment for each section.
- **AC2 — rendered output excludes comments.** Feed a body carrying intent + guidance comments through the TUI render path (`extract_gfm_segments` + `render_gfm_segments`); assert no rendered line contains `intent:`/`guidance:` or `<!--`. Same assertion on `show` (non-json) output; and the inverse on `show --json` body (comments retained).
- **AC3 — substitution intact.** Render an enriched template via `render_template` with all four vars; assert placeholders are substituted AND every intent/guidance comment is present byte-for-byte.
- **AC4 — init writes files.** Run `init` in a temp root; assert the templates dir contains a `{type}.md` for each default type (dir no longer empty).
- **AC5 — on-disk override.** Write a materialized template, mutate it (e.g. add a marker section), create a doc of that type; assert the doc reflects the edited file, not the embedded default.
- **AC6 — defaults carry comments.** For each default type, materialize/inspect its template; assert presence of the `<!-- intent: ... -->` header and one `<!-- guidance: ... -->` per section.

## Notes

- **Two different "intent"s.** This story's `<!-- intent -->` is TEMPLATE CONTENT — an HTML comment living in the per-type markdown template. It is NOT the config `intent` field on `TypeDef` (STORY-145). Keep them separate: nothing here touches `TypeDef` or the config schema.
- **Renderer reality.** HTML comments are already invisible in the TUI because `tui-markdown` 0.3.7 ignores HTML events. The only display surface that leaks them is the raw `show` (non-json) print — that's the sole strip point. Machine paths (`--json`, `get_body_raw`/`expanded`) stay verbatim.
- **Downstream consumers.** The materialized, enriched templates are what the generic verb skills (STORY-147) read at authoring time and what the `/configure-type` meta-skill (STORY-149) edits. This story just makes the files exist on disk and carry methodology; consumption is out of scope.
- **No new format.** Still plain markdown + `{key}` substitution per RFC-048; the only upgrade is the comment payload and `init` materialization.
