---
title: "Document status consistency audit"
type: audit
status: draft
author: "jkaloger"
date: 2026-03-15
tags: []
related: []
---

## Scope

Health check audit across all RFCs, Stories, and Iterations. Checking that
document statuses are internally consistent, relationships are valid, and
the project's document graph reflects reality.

## Criteria

1. Every iteration must link to a parent story (unless standalone bug fix)
2. Accepted children should not reference non-accepted parents
3. No documents should implement superseded RFCs without a replacement link
4. Draft documents with accepted children are inconsistent
5. Documents in "review" status should be actively in progress or resolved
6. No duplicate document IDs
7. Orphaned iterations (no parent link) should be intentional

## Findings

### Finding 1: STORY-063 in "review" with 6 accepted iterations

**Severity:** high
**Location:** docs/stories/STORY-063-diagram-rendering-pipeline.md
**Description:** STORY-063 (Diagram rendering pipeline) is in "review" status, but all 6 of its child iterations (ITERATION-066 through ITERATION-071) are accepted. Codebase verification confirms all ACs are satisfied: diagram block extraction, async rendering, caching with content-hash keying, terminal protocol detection (Sixel/Kitty/iTerm2/Halfblocks), and fallback hints. 18 tests in `tui_diagram_test.rs` cover all criteria. Merged as `4a00991`.
**Recommendation:** Accept STORY-063.

### Finding 2: RFC-016 in "draft" with 2 accepted stories

**Severity:** high
**Location:** docs/rfcs/RFC-016-init-agents-from-tui.md
**Description:** RFC-016 (Init agents from TUI) is still "draft", but STORY-051 and STORY-052 are both accepted with full implementations behind `#[cfg(feature = "agent")]`. Agent dialog with keybindings, non-blocking spawning, and a dedicated Agents view mode with status polling all exist and are tested. The remaining RFC intent (custom prompt file discovery) is captured separately in STORY-053.
**Recommendation:** Accept RFC-016.

### Finding 3: RFC-020 superseded status is incorrect

**Severity:** high
**Location:** docs/rfcs/RFC-020-fix-command-numbering-conflict-resolution.md
**Description:** RFC-020 is marked "superseded" but all four areas it proposed are implemented: conflict detection and renumbering (`collect_conflict_fixes` in `fix.rs`), reference cascade (`cascade_references` in `fix.rs`), graceful degradation (`ResolveError::Ambiguous` in `store.rs`, `!` prefix in TUI), and validation diagnostic (`DuplicateId` variant in `validation.rs`). No successor RFC exists. RFC-027 (Sqids) is complementary, not a replacement.
**Recommendation:** Revert RFC-020 from "superseded" to "accepted".

### Finding 4: STORY-053 not implemented; STORY-060 fully implemented

**Severity:** medium
**Location:** docs/stories/STORY-053-custom-agent-prompts.md, docs/stories/STORY-060-reference-cascade.md
**Description:** Both stories are in "review" but their actual state differs. STORY-053 (Custom agent prompts) ACs are not met: no `.lazyspec/agents/` directory scanning, no YAML prompt file format, no discovery logic. Only a freeform text input exists. STORY-060 (Reference cascade) is fully implemented: `cascade_references()` in `fix.rs` handles both `related` frontmatter and `@ref` body directives, with 5 tests in `cli_fix_cascade_test.rs`. ITERATION-062 (its child) is also implemented but marked draft.
**Recommendation:** Move STORY-053 back to "draft". Accept STORY-060 and ITERATION-062.

### Finding 5: Duplicate document ID "2026"

**Severity:** medium
**Location:** docs/iterations/2026-03-04-lazyspec-design.md, 2026-03-04-lazyspec-implementation.md, 2026-03-05-ai-workflow-design.md, 2026-03-05-ai-workflow-implementation.md
**Description:** Four legacy iterations use date-based naming that produces the same extracted ID "2026". This causes a validation error and could confuse tooling that relies on unique IDs.
**Recommendation:** Renumber these to use the ITERATION-NNN scheme, or mark them with `validate_ignore` if they're historical artifacts.

### Finding 6: Orphaned iterations need parent links

**Severity:** medium
**Location:** ITERATION-048, ITERATION-056, ITERATION-023
**Description:** Three accepted iterations have no parent story. Codebase investigation identified appropriate parents: ITERATION-048 (agent feature flag) should link to STORY-052 (it gates the agent TUI feature). ITERATION-056 (SHA-pinned @ref migration) should link to STORY-058 (ref expansion hardening, same-day work). ITERATION-023 (CLI discovery in skills) is genuinely standalone documentation work with no natural parent.
**Recommendation:** Link ITERATION-048 to STORY-052, ITERATION-056 to STORY-058, and `validate_ignore` ITERATION-023.

### Finding 7: 10 draft stories, 8 confirmed unstarted

**Severity:** low
**Location:** STORY-014, STORY-018, STORY-025, STORY-026, STORY-027, STORY-032, STORY-033, STORY-035, STORY-043, STORY-050
**Description:** Codebase verification found 8 of 10 are genuinely not started. Two have partial overlap with delivered work: STORY-018 (Validation Indicators) may be superseded by STORY-046 which delivered `⚠` and `!` indicators plus a warnings panel. STORY-033 (Direct Mode Switching) has cycle-based switching via backtick but not the direct-jump keybindings the title implies. Both need AC review to confirm.
**Recommendation:** Triage the 8 unstarted stories. Review STORY-018 and STORY-033 ACs against existing functionality to determine if they should be accepted, partially credited, or remain draft.

### Finding 8: 7 draft RFCs confirmed not started

**Severity:** low
**Location:** RFC-017, RFC-022, RFC-023, RFC-024, RFC-025, RFC-026, RFC-027
**Description:** All 7 draft RFCs have zero implementation in the codebase. No `.github/workflows` directory (RFC-024), no status bar component (RFC-022), no settings screen (RFC-023), no sqids dependency (RFC-027), etc.
**Recommendation:** Triage the backlog. Reject RFCs that are no longer relevant.

### Finding 9: 2 of 4 draft iterations are actually complete

**Severity:** low
**Location:** ITERATION-030, ITERATION-061, ITERATION-062, ITERATION-063
**Description:** ITERATION-062 (Reference cascade) is fully implemented with `cascade_references()` and 5 tests. ITERATION-063 (Validation diagnostic for duplicate IDs) is implemented with `DuplicateId` variant and grouping logic in `validate_full`. Both should be accepted. ITERATION-030 (deduplicate validate_full) is not done: `validate_full` is still called 2-3x per CLI invocation. ITERATION-061 (Graceful degradation) is not done: detection exists but no consumer-facing degradation logic.
**Recommendation:** Accept ITERATION-062 and ITERATION-063. Leave ITERATION-030 and ITERATION-061 as draft.

### Finding 10: ITERATION-058 rejected but story accepted

**Severity:** info
**Location:** docs/iterations/ITERATION-058-ref-line-numbers-captions-and-max-length-truncation.md
**Description:** ITERATION-058 is rejected, but its parent STORY-058 is accepted. This is valid: the iteration approach was rejected and the story was completed via other iterations (ITERATION-057, etc.).
**Recommendation:** No action needed. Status is consistent.

## Summary

The project has 27 RFCs, 63 stories, and 75 iterations. The document graph is
mostly healthy, with the majority of documents correctly linked and statused.

Codebase verification across all findings identified 8 concrete status fixes
needed:

| Document | Current | Should be | Why |
|----------|---------|-----------|-----|
| STORY-063 | review | accepted | All ACs verified in code, 18 tests pass |
| RFC-016 | draft | accepted | Both child stories implemented and tested |
| RFC-020 | superseded | accepted | All 4 areas implemented, no successor RFC |
| STORY-060 | review | accepted | `cascade_references()` implemented and tested |
| STORY-053 | review | draft | ACs not met, only freeform input exists |
| ITERATION-062 | draft | accepted | Implementation complete in `fix.rs` |
| ITERATION-063 | draft | accepted | `DuplicateId` diagnostic implemented |
| ITERATION-048 | accepted (orphan) | accepted + link to STORY-052 | Gates agent feature |
| ITERATION-056 | accepted (orphan) | accepted + link to STORY-058 | Ref migration work |
| ITERATION-023 | accepted (orphan) | validate_ignore | Standalone skill docs |

The remaining findings (duplicate "2026" IDs, backlog triage of 8 unstarted
stories and 7 draft RFCs, 2 genuinely draft iterations) are housekeeping items
that don't indicate incorrect status, just accumulated backlog.
