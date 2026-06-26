---
title: Surface milestone as forward targets relation on issue; drop derived targeted-by
type: iteration
status: accepted
author: jkaloger
date: 2026-06-26
tags: []
related:
- implements: STORY-168
- supersedes: ITERATION-223
---

## Goal

Issue doc carries forward `targets: MILESTONE-n` (github_native milestone), read from GitHub at fetch. Milestone `targeted-by` = virtual reverse link (`store.rs:98` `build_links`), never stored. Reverses ITERATION-223.

## Seams

- `src/engine/gh.rs:25` `GhIssue` — no `milestone` field. Add `milestone: Option<{number}>`. Fetch via issue_list JSON (`--json milestone`) / REST `milestone.number`.
- `src/engine/issue_cache.rs:347` `parse_issue` — inject `targets: MILESTONE-n` when milestone present + mapped.
- issue_map — number→shorthand for milestone kind (`shorthand_for_number` filters Issue-kind; need milestone-kind lookup).
- config — `targets` rel name from `github_native=="milestone"`, not hardcoded.
- DELETE: `meta_from_milestone` targeted-by block (`store_dispatch.rs:1097-1124`); `GhMilestoneApi::milestone_issues` (`gh.rs:311`) + real impl (`:958`) + 3 fakes + `NoopGh`; `parse_milestone_issue_numbers_json` (`gh.rs:182`).

## Approach

1. Add `milestone` to `GhIssue` + fetch field. Verify gh json key.
2. issue_map: lookup MILESTONE shorthand for a milestone number (kind=Milestone).
3. `parse_issue`: resolve native milestone → `targets: MILESTONE-n` relation (rel name from config github_native lookup, skip if unmapped).
4. Rip out targeted-by derivation + `milestone_issues` trait/impls/fakes/tests.
5. Milestone `related` stays `[]`; `targeted-by` renders from reverse_links.

## Task breakdown

- [ ] T1: `GhIssue.milestone` field + fetch wiring.
- [ ] T2: issue_map milestone-kind number→shorthand lookup.
- [ ] T3: `parse_issue` inject `targets`, rel name from config, skip unmapped.
- [ ] T4: delete `milestone_issues` (trait/real/fakes/NoopGh) + parse helper + meta_from_milestone block.
- [ ] T5: unit — issue w/ assigned milestone → `targets: MILESTONE-n`; unmapped milestone → no relation.
- [ ] T6: integration — fetch issues, `show STORY-n --json` has `targets`; `show MILESTONE-1 --json` reverse-links `targeted-by`; `validate` clean.
- [ ] T7: README/CLI doc.

## Acceptance criteria

- AC1: each issue w/ GitHub milestone gets forward `targets: MILESTONE-n` at fetch.
- AC2: milestone `targeted-by` derived virtually, never written to cache.
- AC3: unmapped milestone → relation skipped, no dangling finding.
- AC4: round-trip — `link X targets M` writes GitHub milestone, fetch reads it back as same relation.
- AC5: `milestone_issues` API + targeted-by derivation removed.

## Risks

- issue_list JSON may not expose milestone → REST fallback `GET issues/{n}`.
- reverse_links must resolve cross-store (issue→milestone) — verify build_links spans store types.

## Non-goals

Project membership. Writing relations back to GitHub (exists via link).

