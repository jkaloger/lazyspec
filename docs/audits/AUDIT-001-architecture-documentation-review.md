---
title: Architecture Documentation Review
type: audit
status: draft
author: jkaloger
date: 2026-03-15
tags: []
related:
- related-to: docs/rfcs/RFC-002-ai-driven-workflow.md
- related-to: docs/rfcs/RFC-005-tui-flat-navigation-model.md
- related-to: docs/rfcs/RFC-007-agent-native-cli.md
- related-to: docs/rfcs/RFC-009-codebase-quality-baseline.md
- related-to: docs/rfcs/RFC-012-architecture-review-yagni-dry-cleanup.md
- related-to: docs/rfcs/RFC-016-init-agents-from-tui.md
- related-to: docs/rfcs/RFC-019-inline-type-references-with-ref.md
---








## Scope

Spec compliance audit of `docs/architecture/` (ARCH-001 through ARCH-005) against three sources: the Rust source code in `src/`, all accepted RFCs, and internal diagram consistency. Triggered by the initial architecture documentation commit on the `docs/arch` branch.

## Criteria

1. **Accuracy**: Do the architecture docs correctly describe what the code does and what the RFCs decided?
2. **Completeness**: Are significant modules, features, or design decisions missing from the docs?
3. **Diagram accuracy**: Do diagrams reflect actual struct fields, method signatures, and control flow?
4. **Diagram readability**: Are diagrams well-structured, renderable, and visually clear?

## Findings

### Finding 1: `list` command filter described as `--type` flag

**Severity:** medium
**Location:** `docs/architecture/ARCH-004-cli/commands.md`
**Description:** The doc body says "Filters: `--type`, `--status`" but `doc_type` is a positional argument in the CLI definition (`#[arg()]`), not a `--type` flag. The CLI interface reference at the bottom of the same file correctly shows `lazyspec list [TYPE] [--status STATUS]`, contradicting the body text.
**Recommendation:** Change "Filters: `--type`, `--status`" to reflect the positional argument.

### Finding 2: False template-to-store dependency in engine diagram

**Severity:** medium
**Location:** `docs/architecture/ARCH-003-engine/index.md` (d2 diagram, line ~85)
**Description:** The engine component diagram has an arrow `template -> store: "next_number() from dir"`. In code, `next_number()` is a standalone function in `template.rs` that reads the filesystem directly via `fs::read_dir(dir)`. It does not call into the Store.
**Recommendation:** Remove the `template -> store` arrow or replace with `template -> filesystem`.

### Finding 3: Mode switching diagram shows "Tab" instead of number keys

**Severity:** medium
**Location:** `docs/architecture/ARCH-005-tui/app-state.md` (d2 diagram, lines ~36-61)
**Description:** The view mode diagram shows cycling via "Tab". RFC-011 replaced backtick cycling with direct number keys `1-4`. The architecture doc predates this change.
**Recommendation:** Update diagram transitions to show number key switching (`1`-`4`).

### Finding 4: Metrics view mode still documented

**Severity:** medium
**Location:** `docs/architecture/ARCH-005-tui/app-state.md`
**Description:** The app-state doc and its d2 diagram list Metrics as a view mode. RFC-012 explicitly identified `ViewMode::Metrics` as a YAGNI violation and proposed removal. The docs should reflect the post-RFC-012 state.
**Recommendation:** Remove Metrics from the view mode diagram and text, or note its YAGNI status if it still exists in code.

### Finding 5: `validate-ignore` contradicts RFC-002 without acknowledgment

**Severity:** medium
**Location:** `docs/architecture/ARCH-003-engine/validation.md`
**Description:** RFC-002 states "All validation is strict. No flags to weaken it." The architecture doc documents `validate-ignore: true` as a per-document flag to skip validation entirely. This was added by a later story, but the contradiction with the founding RFC is not acknowledged.
**Recommendation:** Add a note explaining that `validate-ignore` was introduced post-RFC-002 via STORY-030.

### Finding 6: Validation flow diagram implies sequential phases

**Severity:** medium
**Location:** `docs/architecture/ARCH-003-engine/validation.md` (d2 diagram, lines ~26-62)
**Description:** The diagram shows broken link checking, status consistency, and config rule application as three separate sequential steps. In `validate_full()`, all three happen in a single loop over documents. Only hierarchy checks and duplicate ID detection are truly separate passes.
**Recommendation:** Restructure the diagram to show the single-pass nature of the main validation loop.

### Finding 7: Agent skills absent from architecture docs

**Severity:** medium
**Location:** `docs/architecture/ARCH-001-overview/` (missing coverage)
**Description:** RFC-002 defines five skill files (`write-rfc`, `create-story`, `create-iteration`, `resolve-context`, `review-iteration`) in `skills/`. The architecture docs have no mention of skills anywhere. The module map has no entry for `skills/`.
**Recommendation:** Add a section or child document covering the skills directory and its role in the agent workflow.

### Finding 8: Graph mode barely documented

**Severity:** medium
**Location:** `docs/architecture/ARCH-005-tui/`
**Description:** RFC-006 describes Graph mode extensively (navigable dependency tree, `j/k` navigation, `h/l` collapse/expand, box-drawing rendering). The architecture docs mention "Graph" with "Relationship visualization" but provide no detail on navigation state, rendering approach, or the `graph_nodes`/`graph_selected` state fields.
**Recommendation:** Add a child document or expand app-state coverage for Graph mode.

### Finding 9: `status --tree` and `--summary` flags missing from CLI reference

**Severity:** medium
**Location:** `docs/architecture/ARCH-004-cli/commands.md`
**Description:** RFC-008 introduces `--tree` and `--summary` flags on the `status` command. The CLI reference shows only `lazyspec status [--json]`.
**Recommendation:** Add `--tree` and `--summary` to the status command documentation.

### Finding 10: Bulk `update` not documented

**Severity:** medium
**Location:** `docs/architecture/ARCH-004-cli/commands.md`
**Description:** RFC-008 specifies that `update` should accept multiple document paths (`paths: Vec<String>`). The CLI reference shows only `lazyspec update <PATH>` with a single argument.
**Recommendation:** Update the command signature if bulk update is implemented.

### Finding 11: Tag editor overlay missing

**Severity:** medium
**Location:** `docs/architecture/ARCH-005-tui/app-state.md`
**Description:** RFC-018 introduces a tag editor overlay on `t` key with autocomplete. The overlay table in app-state has no entry for it.
**Recommendation:** Add tag editor to the overlays table.

### Finding 12: `PreviewTab` state not documented

**Severity:** medium
**Location:** `docs/architecture/ARCH-005-tui/app-state.md`
**Description:** The `App` struct has `preview_tab: PreviewTab` controlling Body/Relations/Details tabs in the preview pane. This user-facing navigation state is not documented anywhere.
**Recommendation:** Add PreviewTab to the navigation state section.

### Finding 13: Legacy `[directories]` config not documented

**Severity:** medium
**Location:** `docs/architecture/ARCH-002-data-model/configuration.md`
**Description:** The `Config` struct supports a legacy `[directories]` TOML section with `rfcs`, `adrs`, `stories`, `iterations` subfields. If `[[types]]` is absent, it falls back to `types_from_directories()`. The config doc only shows the `[[types]]` format.
**Recommendation:** Document the legacy format and fallback behaviour, or note it as deprecated.

### Finding 14: Store struct fields missing from diagrams

**Severity:** medium
**Location:** `docs/architecture/ARCH-001-overview/index.md`, `docs/architecture/ARCH-003-engine/index.md`
**Description:** The Store struct in the C4 container diagram and engine component diagram shows five fields (`docs`, `forward_links`, `reverse_links`, `children`, `parent_of`) but the actual struct also has `root: PathBuf` and `parse_errors: Vec<ParseError>`. Both are structurally important.
**Recommendation:** Add `root` and `parse_errors` to the Store diagrams.

### Finding 15: `expand_cancellable()` missing from engine diagram

**Severity:** medium
**Location:** `docs/architecture/ARCH-003-engine/index.md`
**Description:** The `RefExpander` box shows `expand(content)` and `resolve_ref()`. The actual struct also has `expand_cancellable(content, cancel)` which is the primary method used by the TUI for async expansion.
**Recommendation:** Add `expand_cancellable()` to the RefExpander diagram.

### Finding 16: Event loop diagram missing drain loop

**Severity:** medium
**Location:** `docs/architecture/ARCH-005-tui/event-loop.md` (d2 diagram)
**Description:** After receiving and handling an event, the code drains additional events with `try_recv()` before proceeding. The diagram shows a single `recv -> handle` path without this batching behaviour. Also missing the `AgentFinished` event variant.
**Recommendation:** Add the drain loop to the event loop diagram.

### Finding 17: Lazy body loading not mentioned

**Severity:** low
**Location:** `docs/architecture/ARCH-003-engine/store.md`
**Description:** RFC-001 specifies "Body content is loaded lazily" and the Store "parsing only frontmatter (stops at the second `---`)". The store doc describes loading but does not mention this optimisation.
**Recommendation:** Note the frontmatter-only parsing and lazy body loading.

### Finding 18: Auxiliary crates missing from dependency table

**Severity:** low
**Location:** `docs/architecture/ARCH-001-overview/module-map.md`
**Description:** `tui-markdown`, `image`, `unicode-width`, and `uuid` crates are in Cargo.toml but not in the Key Dependencies table.
**Recommendation:** Add if the table aims to be comprehensive, or note it covers core dependencies only.

### Finding 19: Filter mode interaction details thin

**Severity:** low
**Location:** `docs/architecture/ARCH-005-tui/app-state.md`
**Description:** RFC-006 describes Filters mode in detail (`j/k` field navigation, `h/l` value cycling, filter persistence). The architecture doc gives it a one-line mention. The `filter_focused`, `filter_status`, `filter_tag`, `available_tags` state fields are also undocumented.
**Recommendation:** Expand Filters mode coverage.

### Finding 20: `@ref path#<line>` syntax not backed by any RFC

**Severity:** low
**Location:** `docs/architecture/ARCH-002-data-model/ref-directives.md`
**Description:** RFC-019 specifies `@ref <path>#<symbol>` and `@ref <path>#<symbol>@<sha>`. Line number references (`@ref path#<line>`) appear in the architecture docs and code but are not specified in any RFC.
**Recommendation:** Either add an RFC amendment or note it as an implementation extension.

### Finding 21: Mode-aware help overlay not documented

**Severity:** low
**Location:** `docs/architecture/ARCH-005-tui/app-state.md`
**Description:** RFC-011 specifies context-sensitive help that varies per ViewMode, including a persistent `? help` hint. Help is listed as an overlay but without mode-awareness detail.
**Recommendation:** Note the mode-aware behaviour in the overlay description.

### Finding 22: Threading model prose says "three threads"

**Severity:** low
**Location:** `docs/architecture/ARCH-005-tui/threading-model.md`
**Description:** The prose says "three threads plus optional background workers" but the diagram correctly shows four (main, input, watcher, probe). Minor inconsistency.
**Recommendation:** Align the prose with the diagram count.

### Finding 23: Scroll padding and half-page navigation not documented

**Severity:** low
**Location:** `docs/architecture/ARCH-005-tui/app-state.md`
**Description:** RFC-018 specifies `scrolloff=2` padding and `Ctrl-D`/`Ctrl-U` for half-page jumps. The app-state doc mentions scroll offset fields but not these behaviours.
**Recommendation:** Add scroll padding and half-page navigation to the navigation state section.

### Finding 24: Diagram render workers missing from threading model

**Severity:** low
**Location:** `docs/architecture/ARCH-005-tui/threading-model.md`
**Description:** Diagram rendering spawns background workers similar to expansion workers, producing `DiagramRendered` events. These are not shown in the threading diagram.
**Recommendation:** Add diagram render workers alongside expansion workers.

### Finding 25: `search_selected` state field omitted

**Severity:** low
**Location:** `docs/architecture/ARCH-005-tui/app-state.md`
**Description:** The doc mentions `search_mode`, `search_query`, `search_results` but omits `search_selected: usize`.
**Recommendation:** Add the missing field.

## Summary

22 documents across 5 architecture sections were reviewed against source code, 15+ RFCs, and internal diagram consistency.

**No critical or high severity findings.** The architecture documentation is structurally sound and captures the core design well.

16 medium-severity findings break into two patterns:

- **Stale state** (4 findings): mode switching mechanism, Metrics view mode, validate-ignore contradiction, and the list command flag. These are cases where the docs don't reflect later RFC decisions or actual code behaviour.
- **Missing coverage** (12 findings): agent skills, graph mode, CLI flags, state fields, and diagram struct fields. These are features or components that exist in code/RFCs but are absent from the architecture docs.

9 low-severity findings are mostly thin coverage of interaction details, minor prose inconsistencies, and missing auxiliary information.

Diagrams scored well on readability. All d2 diagrams use valid syntax, consistent colour coding, and appropriate shapes. No rendering issues found.
