---
title: "Story-to-Spec Coverage Audit"
type: audit
status: complete
author: "jkaloger"
date: 2026-03-25
tags: [coverage, specs, stories]
related: []
---


## Scope

Spec compliance audit across all 91 stories (STORY-001 through STORY-091) and all 22 specs (SPEC-001 through SPEC-022). The goal is to verify that every story's acceptance criteria have representation in at least one spec's `story.md` file.

## Criteria

For each story, its ACs must appear (verbatim or paraphrased) in one or more spec `story.md` files. Coverage is classified as:

- COVERED: all core ACs present in a spec
- PARTIAL: some ACs present, meaningful gaps remain
- MISSING: no spec coverage at all

## Findings

### Finding 1: Agent workflow skills have no spec

Severity: medium
Location: docs/stories/STORY-005-agent-workflow-skills.md
Description: SPEC-019 covers agent TUI integration (spawning `claude` processes), not skill files. None of the 22 specs contain ACs for the five skill files (write-rfc, create-story, create-iteration, resolve-context, review-iteration), their d2 diagrams, or their YAML format requirements.
Recommendation: Create a spec for the skill file format and discovery mechanism, or fold ACs into SPEC-019.

### Finding 2: Relations tab has no spec ACs

Severity: low
Location: docs/stories/STORY-010-read-only-relations-tab.md
Description: No spec has dedicated ACs for the relations tab visual treatment: cyan `>` indicator, j/k navigation within the tab, Enter to navigate to related doc, grouped-by-type display, or dimming of doc list.
Recommendation: Add relations tab ACs to SPEC-015 or SPEC-016.

### Finding 3: Border highlighting has no spec

Severity: low
Location: docs/stories/STORY-011-simplified-border-highlighting.md
Description: None of the specs contain ACs for the border highlighting rules: static border on Types panel, double/cyan border on doc list, dims when Relations tab is active, or cyan border on Relations panel.
Recommendation: Fold into SPEC-015 as visual state ACs.

### Finding 4: Metrics mode has no spec

Severity: low
Location: docs/stories/STORY-014-metrics-mode.md
Description: No spec covers sparklines, summary statistics, validation summary in metrics, or live metric updates. Note: STORY-036 (YAGNI) removed `ViewMode::Metrics`, so this story may be superseded.
Recommendation: If metrics mode is dead, mark STORY-014 as superseded. Otherwise create a spec.

### Finding 5: Validation indicators have no spec

Severity: medium
Location: docs/stories/STORY-018-validation-indicators.md
Description: No spec has ACs for the `!` prefix indicator, validation running on store load, per-document error display in the preview panel, or refresh on document change.
Recommendation: Add validation indicator ACs to SPEC-003 or SPEC-017.

### Finding 6: Tree view has no spec

Severity: medium
Location: docs/stories/STORY-025-tree-view.md
Description: No spec references `--tree`, nested tree output, orphaned documents section, `roots`/`orphaned` JSON structure, or mutual exclusion with `--summary`.
Recommendation: Add to SPEC-010 (document querying).

### Finding 7: Summary view has no spec

Severity: medium
Location: docs/stories/STORY-026-summary-view.md
Description: No spec references `--summary`, count-by-type-and-status output, health line, or mutual exclusion with `--tree`.
Recommendation: Add to SPEC-010 (document querying).

### Finding 8: Bulk update has no spec

Severity: medium
Location: docs/stories/STORY-027-bulk-update.md
Description: No spec references multiple-path `update`, continue-on-failure semantics, or the `updated`/`failed` JSON arrays. SPEC-008 only covers single-path update.
Recommendation: Add bulk update ACs to SPEC-008.

### Finding 9: Engine and CLI quality refactoring has no spec

Severity: info
Location: docs/stories/STORY-028-engine-and-cli-quality.md
Description: No spec covers shared `split_frontmatter`, removal of dead `validate()`, `FromStr` trait impls, dead dependency removal, shared main.rs setup, `handle_key` method extraction, `validation.rs` module, or `tests/common/mod.rs` helpers. These are internal refactoring concerns.
Recommendation: Refactoring stories may not need spec coverage. Consider whether these are correctly scoped as stories vs. chores.

### Finding 10: TUI test coverage has no spec

Severity: info
Location: docs/stories/STORY-029-tui-test-coverage.md
Description: No spec covers test requirements for `App` search/scroll/relation methods or `handle_key` integration tests.
Recommendation: Test stories typically don't need spec coverage. Consider reclassifying.

### Finding 11: Direct mode switching contradicts spec

Severity: high
Location: docs/stories/STORY-033-direct-mode-switching.md
Description: SPEC-015 actively specifies the old backtick cycling behaviour (`mode-cycle-order`, `mode-transition-side-effects`). No spec covers number-key switching, mode strip in title bar, or backtick removal. The story and spec are in conflict.
Recommendation: Either update SPEC-015 to reflect direct mode switching or mark STORY-033 as superseded.

### Finding 12: Four refactoring stories have no spec (034-036, 079-084)

Severity: info
Location: docs/stories/STORY-034, STORY-035, STORY-036, STORY-079 through STORY-084
Description: Nine stories covering internal refactoring (frontmatter utility extraction, TUI rendering consolidation, YAGNI dead code removal, engine safety, DRY consolidation, TUI nesting/naming/module splits, CLI fix module restructure, engine module splits, SOLID refactors) have zero spec representation.
Recommendation: Internal refactoring stories describe implementation reorganisation, not observable behaviour. These may be correctly out of scope for specs. Consider whether the project needs a policy on this.

### Finding 13: Table widget and scrollbar have no spec

Severity: low
Location: docs/stories/STORY-047, STORY-049
Description: The Table widget column layout and Scrollbar rendering details have no spec ACs. These are TUI implementation details.
Recommendation: Consider adding to SPEC-015 or SPEC-017 if the rendering contract matters.

### Finding 14: Tag editor with autocomplete has no spec

Severity: medium
Location: docs/stories/STORY-050-tag-editor-with-autocomplete.md
Description: No spec covers tag chips, tag autocomplete, `update_tags()`, or the `t` keybinding. All 8 ACs are unrepresented.
Recommendation: Add to SPEC-016 (document operations).

### Finding 15: Custom agent prompts partially covered

Severity: medium
Location: docs/stories/STORY-053-custom-agent-prompts.md
Description: SPEC-019 covers runtime invocation (`custom-prompt-text-input`, `custom-prompt-spawns-agent`) but the `.lazyspec/agents/` directory convention, YAML frontmatter format, prompt discovery, template variable interpolation, invalid-prompt handling, and the "no prompts" disabled state (ACs 1-7) have no spec ACs.
Recommendation: Add prompt discovery and format ACs to SPEC-019.

### Finding 16: Diagram rendering pipeline partially covered

Severity: medium
Location: docs/stories/STORY-063-diagram-rendering-pipeline.md
Description: SPEC-017 covers block detection, cache hits, and fallback, but terminal image protocol detection (`Sixel`/`KittyGraphics`/`None`), async loading indicator, inline image display, and cache invalidation on source change are absent.
Recommendation: Add missing ACs to SPEC-017.

### Finding 17: ID resolution for mixed formats partially covered

Severity: medium
Location: docs/stories/STORY-065-id-resolution-for-mixed-formats.md
Description: SPEC-007 has `resolve-filename-pre-computed-id` and `resolve-filename-no-number-placeholder`, but no ACs for `extract_id_from_name` recognizing alphanumeric sqids IDs, resolving mixed numeric+sqids in the same directory, or folder-based documents with sqids IDs.
Recommendation: Add mixed-format resolution ACs to SPEC-007.

### Finding 18: Fix numbering format conversion partially covered

Severity: medium
Location: docs/stories/STORY-066-fix-numbering-format-conversion.md
Description: SPEC-013 covers cascade and dry-run mechanics but is missing `--renumber sqids` and `--renumber incremental` command flag ACs, the `--type` scoping AC, and the "skip already-converted" AC.
Recommendation: Add renumber command ACs to SPEC-013.

### Finding 19: TUI link editor mostly uncovered

Severity: medium
Location: docs/stories/STORY-069-tui-link-editor.md
Description: SPEC-016 has `link-editor-live-search` and `link-editor-relation-type-cycling` but is missing 7 of 10 ACs: open overlay with `r`, document display format, Enter to confirm, Esc to cancel, `d` to delete, confirmed deletion, and no-document guard.
Recommendation: Add remaining link editor ACs to SPEC-016.

### Finding 20: Progress callback API has no spec

Severity: medium
Location: docs/stories/STORY-071-progress-aware-reservation-api.md
Description: No spec has ACs for `ReservationProgress`/`PruneProgress` enums, `on_progress` callback threading, or invocation order. SPEC-007's prose references `ReservationProgress` but the story.md has no corresponding ACs.
Recommendation: Add progress callback ACs to SPEC-007.

### Finding 21: CLI spinners have no spec

Severity: low
Location: docs/stories/STORY-072-cli-spinners-for-git-remote-operations.md
Description: No spec covers `indicatif` spinners on stderr, spinner suppression under `--json`, background thread for git ops, or progress bar for `reservations prune`.
Recommendation: Add to SPEC-008 or consider this an implementation detail.

### Finding 22: Seven future stories have no spec (085-091)

Severity: high
Location: docs/stories/STORY-085 through STORY-091
Description: Seven draft stories for upcoming features have zero or minimal spec coverage:
- STORY-085 (Blob Pinning): 0 of 13 ACs covered
- STORY-086 (Certification Workflow): 0 ACs covered
- STORY-087 (Drift Detection): 0 ACs covered
- STORY-088 (Spec Document Type): partial, missing directory structure and migration ACs
- STORY-089 (Relationship Model): 0 ACs covered
- STORY-090 (Skill and Workflow Updates): 0 ACs covered
- STORY-091 (Story-to-Spec Migration): partial, only `superseded-parent-warning` maps
Recommendation: These stories are draft and likely precede spec creation. Specs should be written before implementation begins.

## Summary

Of 91 stories audited:

| Coverage | Count | Percentage |
|----------|-------|------------|
| COVERED | 46 | 50.5% |
| PARTIAL | 20 | 22.0% |
| MISSING | 25 | 27.5% |

The MISSING stories fall into three categories:

1. Refactoring/quality stories (028, 029, 034-036, 079-084): 11 stories describing internal reorganisation. These may be correctly out of scope for specs, which describe observable behaviour.

2. TUI implementation details (011, 047, 049): 3 stories for visual polish that could be folded into existing specs.

3. Unspecified features (005, 014, 018, 025-027, 033, 050, 071-072, 085-091): 11 stories for features that lack corresponding specs. The most concerning are the 7 draft stories (085-091) for upcoming work, and the story/spec conflict in STORY-033.

The highest-priority gaps are Finding 11 (story/spec conflict) and Finding 22 (upcoming features without specs).
