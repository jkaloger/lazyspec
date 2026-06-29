---
title: 'Store-aware relation vocabulary: constrain github-milestones relations'
type: iteration
status: accepted
author: jkaloger
date: 2026-06-29
tags: []
related:
- implements: STORY-171
---

## Goal

Store-aware relation vocab. Milestone docs (`store="github-milestones"`, `.lazyspec.toml:106-112`) constrained:
- milestone-store doc = TARGET only, only of `github_native="milestone"` rel (`targets`, `.lazyspec.toml:131-135`).
- milestone-store doc = NEVER source of any rel.
- `targets` target MUST be milestone-store doc.
Defect E. Enforce once in core (`link.rs`) -> CLI + TUI both honour. Scoped to github-milestones ONLY -> github-projects/membership untouched.

## Seams

- `src/cli/link.rs:60` `link_inner` / `:299` `unlink_inner` -- both resolve `from_id`/`to_id` + `rel_str` BEFORE frontmatter write (`:71`,`:310`). New guard slots here, pre-write.
- `src/engine/config.rs:907` `type_by_name` -> `:251` `TypeDef.store` (`:127` `StoreBackend`, `:134` `GithubMilestones`). `:912` `relationship_by_name` -> `:282` `github_native`.
- `src/engine/store.rs` `DocMeta.doc_type` (`document.rs:314` `DocType`, `:69` `as_str`) -> endpoint type name -> `type_by_name(..).store`. Resolve via `store.resolve_relation_target` / existing id->meta lookup.
- `src/tui/state/app.rs:836` `rel_types = config.relationship_keywords()` (populated on doc open). `:3042` `update_link_search` (target candidate filter). `:3072` `confirm_link` -> `link_with_config` (`Result` currently `let _`-dropped at `keys.rs:179`).
- `src/tui/state/forms.rs:282` `LinkEditor` -- has NO `error` field yet (sibling iter adds `error: Option<String>`); guard error surfaces through it.
- `src/tui/views/keys.rs:165-176` rel-type cycle over `self.rel_types`.

## Approach

1. Core: one fn validates `(source_store, rel.github_native, target_store)` triple vs rule. Resolve each endpoint store via `DocMeta.doc_type` + `type_by_name`.
2. Call early in `link_inner` AND `unlink_inner`, post-resolve / pre-`rewrite_frontmatter`. Violation -> `anyhow` err, clear msg.
3. TUI: filter rel-type list + target search by viewed doc store + selected rel `github_native`. Milestone-store viewed doc -> empty rel list + msg.
4. Surface core err in link-editor `error` field (no panic, no half-write).

## Task breakdown

- [ ] T1: `fn validate_milestone_relation(config, source_store, rel_def, target_store) -> Result<()>` in `link.rs` (or `config.rs`). Rule:
  - source_store == `GithubMilestones` -> Err "milestone docs cannot be the source of a relation".
  - rel `github_native == "milestone"` (i.e. `targets`) + target_store != `GithubMilestones` -> Err "`targets` requires a milestone target".
  - rel `github_native != "milestone"` + target_store == `GithubMilestones` -> Err "milestone docs can only be targeted by `targets`".
  - else Ok. Resolve store: endpoint id -> `DocMeta.doc_type.as_str()` -> `config.type_by_name(..).store`.
- [ ] T2: call in `link_inner` (`:60`) after `resolve_to_id` (`:68`), before `rewrite_frontmatter` (`:71`). Same in `unlink_inner` (`:299`) pre-`:310`. NB direction already flipped (`:65`,`:304`) -> validate post-flip `from_id`/`to_id`.
- [ ] T3: NO touch to `apply_native_membership` / projects / github-projects store paths. Guard keys on `github_native=="milestone"` + `StoreBackend::GithubMilestones` ONLY.
- [ ] T4: TUI rel-type filter. On open (`app.rs:836` region): if viewed doc store == `GithubMilestones` -> `rel_types = []` (+ flag for empty-state msg in editor). Else drop `github_native=="milestone"` rels whose milestone-only target set would be empty? No -> keep all non-source-violating rels; targets handled by search filter T5.
- [ ] T5: `update_link_search` (`app.rs:3042`): selected rel `github_native=="milestone"` -> candidates = milestone-store docs only; else EXCLUDE milestone-store docs. Resolve candidate store via its `doc_type` + config. Keep existing query `contains` + sort (`:3055-3064`).
- [ ] T6: surface guard err. `confirm_link` (`:3072`) already returns `Result`; set `link_editor.error = Some(e..)` on Err instead of dropping at `keys.rs:179`. Milestone-store viewed doc -> editor shows "milestone docs cannot start a relation", no candidates.
- [ ] T7: tests (see ACs). Extend `mod tests` (`link.rs:476+`); reuse `milestone_assoc_config` (`:497`) + add an ordinary `implements` rel + a non-milestone target type.
- [ ] T8: README/CLI doc note -- `link`/`unlink` reject store-illegal relations.

## Acceptance criteria

- AC1: `link STORY-7 --targets--> MILESTONE-3` -> Ok; frontmatter `targets: MILESTONE-3` written; native PATCH recorded (regression: `link_native_milestone_sets_and_clears_association` `:546` still green).
- AC2: `link STORY-7 --implements--> MILESTONE-3` -> Err containing "milestone docs can only be targeted by `targets`". NO frontmatter write (guard before `rewrite_frontmatter` `:71`), NO native call.
- AC3: `link MILESTONE-3 --<any rel>--> STORY-7` -> Err containing "milestone docs cannot be the source". Holds for `targets` too (milestone never source). No write.
- AC4: `targets` to a NON-milestone target (`link STORY-7 --targets--> STORY-9`, both github-issues) -> Err "`targets` requires a milestone target". No write.
- AC5: `unlink_inner` honours the SAME guard symmetrically (illegal triple rejected pre-`retain` `:310`); legal unlink (`STORY-7 targets MILESTONE-3`) still clears native assoc.
- AC6: core-shared -> CLI (`link_with_config`) and TUI (`confirm_link` -> same `link_inner`) both reject identically; no second copy of the rule.
- AC7: TUI search -- selected rel `github_native=="milestone"` -> `update_link_search` results contain ONLY milestone-store docs; any other selected rel -> results EXCLUDE all milestone-store docs.
- AC8: TUI viewed doc is milestone-store -> `rel_types` empty + editor shows clear empty-state msg; no candidate offered (milestone never source).
- AC9: guard `anyhow` msg propagates to `link_editor.error` (T6) -> visible in TUI, no panic, no partial frontmatter.

## Tests

- `link_milestone_target_via_targets_ok` -- `milestone_assoc_config`; `link_inner STORY-7 targets MILESTONE-3` -> Ok + `targets: MILESTONE-3` in cache (mirror of `:546`).
- `link_milestone_via_ordinary_rel_rejected` -- add `implements` rel to config; `link_inner STORY-7 implements MILESTONE-3` -> `Err` msg "can only be targeted by `targets`"; assert MILESTONE/STORY cache `.md` UNCHANGED (no `related`).
- `link_from_milestone_rejected` -- `link_inner MILESTONE-3 targets STORY-7` (and one ordinary rel) -> `Err` "cannot be the source"; no write.
- `targets_to_non_milestone_rejected` -- add `story2`/`STORY` github-issues type; `link_inner STORY-7 targets STORY-9` -> `Err` "requires a milestone target".
- `unlink_honours_store_guard` -- illegal `unlink_inner MILESTONE-3 ...` rejected; legal `unlink STORY-7 targets MILESTONE-3` still records `(7, None)` (`:608` shape).
- TUI: `update_link_search` w/ `rel_type_index` on a `github_native=="milestone"` rel -> results all milestone-store; on ordinary rel -> zero milestone-store; milestone-store viewed doc -> `rel_types.is_empty()`.

## Non-goals

- A/B/C/D link-path reconcile (separate iteration).
- github-projects / `membership` constraints -- untouched.
- New store kinds beyond github-milestones; generalised per-store relation vocab schema (rule hard-coded to milestone for now).
- Validate command sweep of existing illegal links (guard is write-time only).

