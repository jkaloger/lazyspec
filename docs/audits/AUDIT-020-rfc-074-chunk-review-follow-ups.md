---
title: RFC-074 chunk review follow-ups
type: audit
status: draft
author: Claude Sonnet 5
date: 2026-09-28
tags: []
related:
- related-to: RFC-074
---

## Summary

Chunk review fix pass landed findings 1-8 (HIGH/MED). Findings 9, 11-25 below still open. Follow-up for next chunk or backlog.

## Context

RFC-074 bundles (STORY-291), missing-part validation (STORY-292), workflow packs (STORY-293). Base cecfba1, chunk HEAD 5162bcf, fix commits on top.

Note: fixes 1-8 touched fs_ops.rs, validation.rs, cli/validate.rs, store.rs, cli/init.rs, git_store.rs, store_dispatch.rs, git_ref_store.rs, ops/create.rs, ops/update.rs. Line numbers below (from original review) may drift a few lines from that; still find-able by symbol/comment name.

## Residual gaps from findings 1-8 (not fully closed)

- Finding 6 (init.rs pack clone reuse): fixed narrowly in `resolve_pack_source` via a lightweight `.git/config`/`.git/HEAD` origin+branch check, re-clone on mismatch. `Config::load_extended_url` (config.rs:2205) still does blind `!exists` reuse, unfixed — same bug class, `extends` side. Tracks into item 12 (shared `ensure_config_clone` helper).
- Finding 5 (bundle-aware `reload_file`): `Store::reload_bundle` has no `Config`/declared-parts access at incremental-reload time, so part order falls back to "previous known order, then alphabetical" rather than true template order. Correct again after next full `Store::load`. Minor, but not full AC5 compliance mid-session.
- Finding 8, STORY-293 AC4 "`--template` skips the wizard": added `template_starter_is_excluded_from_the_pack_path` (proves `pack_template(Some("starter"))` is `None`, so main.rs's `if let Some(pack) = pack_template(...)` branch is never taken for `starter`) and confirmed via existing code read that a real pack short-circuits before any TTY/interactive check in `main.rs`. Did not add a true end-to-end TTY-level integration test (no pty harness in this suite) — residual gap, low value to chase further without one.

## Open findings (verbatim-ish, file:line as reviewed)

9. LOW. `--force` works on plain `init`/wizard too, but help text says "With --template" (src/cli.rs:98). Fix: `requires = "template"` clap attr, or fix the help/README wording.

10. Note, not a defect: openspec pack lives in-repo only (examples/openspec) by user decision. Publishing as a standalone repo is a future follow-up, not a bug.

11. MED. Principle 4 (seams over globals) violation: template.rs:26, :62, :87 call `std::fs` directly. Called from `Store::load_with_fs` (which threads a `&dyn FileSystem`) and from `MissingPartRule` (a validator, should be fs-agnostic for testing). Fix: take `&dyn FileSystem` through these fns.

12. LOW. DICTUM-003 (no speculative pub): `pub(crate) mod extends` (config.rs:1) wider than needed. Fix: keep private; factor one `ensure_config_clone` helper shared by init.rs's `resolve_pack_source` and config.rs:2205 `load_extended_url` — this is also where finding 6's config.rs-side gap gets closed properly.

13. LOW. DICTUM-001 (no speculative derive): fs_ops.rs:20 (`CreatedDocument`) and ops/create.rs:26 (`CreateOutcome`) derive more than used; construction sites should prefer `..Default::default()` over listing every field.

14. LOW. DICTUM-004 (no real git in unit tests): cli_init_template_test.rs:63 (`init_template_url_clones_under_the_local_cache_then_copies_the_pack`) spawns a real `git init`/`commit` subprocess. Fix: `MockGitRefClient` where feasible; this one may need to stay real since it asserts an actual clone lands (documents that trade-off if kept).

15. MED. `validate` silently skips a misconfigured directory template (validation.rs, `MissingPartRule`, around line 1480: `let Ok(TemplateKind::Directory) = resolve_template_kind(...) else { continue }`). A type with no `index.md` or `subdirectory=false` mismatch should surface as a type-level error somewhere in `validate`, not just on `create`/`config --json`. Currently: nothing for that type in `validate`, matching design comment's stated intent, but means `validate` alone won't catch a template misconfig introduced after the fact.

16. MED. `install_pack --force` (init.rs, `install_pack`, ~199-222) never clears a project's old `.lazyspec/templates/` before copying the new pack's over it: a file the old templates dir had that the new pack doesn't ship survives, silently mixing packs.

17. LOW. init.rs: existing-config refusal happens inside `install_pack` (line ~207, `ensure_no_config`) which runs AFTER the network clone in `resolve_pack_source` (line ~179) — a wasted clone before the refusal. Also a redundant `fs::create_dir_all(root)` at ~211 (root already exists once we're inside an existing project).

18. LOW. Dead `skip_index` param on `load_child_markdown_files` (loader.rs:96-99, called at :286) — check whether both call sites still need the branch; drop the param if not.

19. LOW. `## <name>` part-concatenation format duplicated: cli/show.rs:206 and tui/state/expansion.rs:78 both format a part heading the same way. Fix: one engine fn (e.g. on `Part` or a small formatter in `engine::document`), both callers use it.

20. LOW. Near-identical directory-scan logic at template.rs:62 and :87 (declared-parts order scan, sidecar/child scan) — candidate to merge into one parametrized scan, mirroring `scan_document_folder` in loader.rs.

21. LOW. `CreateOutcome` (ops/create.rs) duplicates `CreatedDocument` (fs_ops.rs) field-for-field; `run_with_body_full`'s wrapper (`run_with_body`, `run`) is thin enough to question whether three layers are earning their keep. Consider collapsing.

22. LOW. `write_part` (fs_ops.rs, ~422) re-resolves the document from `store` even though most callers already hold a resolved `DocMeta` — avoidable double lookup.

23. LOW. loader.rs:142 (`looks_like_frontmatter`): a part file that happens to start with `---` (e.g. a markdown horizontal rule as the very first line) is classified as a child document and parsed as one, erroring if it isn't valid frontmatter. Edge case, but worth a documented rule or explicit rejection message pointing at the cause.

24. LOW. ~50 comments across the touched files cite "RFC-074 ACn" — the RFC itself has no numbered ACs (only STORY-291/292/293 do). Should read "STORY-29x ACn". Sweep candidates: template.rs:48-61, :74-86; validation.rs:162-170, :1453-1464 (note: MissingPartRule's own doc comment already reads "STORY-292 AC1" after my STORY-292 AC1/AC3 fix in this pass, so re-check before sweeping — some may already be right); loader.rs:136-141; ops/create.rs:21-24 (already fixed to STORY-291 AC2 in this pass, re-check); validate.rs:39-45 (still "RFC-074 AC2" as of this pass, not touched). Also: comments citing a file path in prose (e.g. "src/cli/show.rs") — flagged by user's own comment-style rule as restating what an IDE already shows.

25. LOW. examples/openspec/.lazyspec.toml:10 comment text wrongly claims the loader refuses the pack in some case — verify current loader behaviour and correct or delete the comment.
