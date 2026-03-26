---
title: 'Code Quality Remediation: AUDIT-007 Findings'
type: rfc
status: accepted
author: jkaloger
date: 2026-03-23
tags: []
related:
- related-to: AUDIT-007
- related-to: RFC-009
- related-to: RFC-012
---






## Summary

AUDIT-007 reviewed the lazyspec codebase against eight criteria and produced 27 findings. Sixteen are actionable (high or medium severity), seven are low, and four are informational. This RFC proposes concrete remediation for all actionable findings, organised into six work streams.

The codebase is functionally correct. These changes are structural: flattening nesting, extracting helpers, splitting modules, and improving naming. No user-facing behaviour changes.

## Context

RFC-009 (Codebase Quality Baseline) addressed the first round of structural debt: frontmatter deduplication, dead code removal, `FromStr` impls, validation extraction from store.rs. RFC-012 (YAGNI/DRY Cleanup) continued with rendering consolidation, metrics removal, and dual-path JSON cleanup.

This RFC picks up where those left off. The codebase has grown since those cleanups (store.rs is now 604 lines, ui.rs is 1890 lines, app.rs is 2563 lines), and AUDIT-007 identified patterns that the earlier RFCs didn't cover: deep nesting, vague function names, and modules that have accumulated too many concerns.

## Work Streams

### 1. Error Propagation Safety

AUDIT-007 Finding 1 identified 131 instances of `unwrap()`/`expect()` across the codebase. The highest-risk calls are in the sqids numbering pipeline where `.expect()` will panic at runtime if configuration is invalid or encoding fails.

Current pattern in template.rs:

```rust
let sqids = sqids::Sqids::builder()
    .alphabet(alphabet)
    .min_length(sqids_config.min_length)
    .blocklist(HashSet::new())
    .build()
    .expect("valid sqids config");

let id = sqids.encode(&[input]).expect("sqids encode");
```

These calls exist in `template.rs`, `create.rs`, `fix.rs`, and `reservations.rs`. The builder and encode operations can fail for legitimate reasons (bad alphabet, encoding overflow), and the errors should propagate to the caller via `?`.

Proposed change: replace all sqids `.expect()` calls with `?` propagation. The functions already return `Result`, so this is a signature-preserving change. For TUI `.unwrap()` calls on cache lookups (e.g. `filtered_docs_cache`), replace with `.unwrap_or_default()` or match expressions that handle the missing-cache case gracefully.

### 2. Nesting Flattening

Findings 17-21 identified functions at 4-5 levels of nesting that could be flattened with extracted helpers and early returns.

#### 2a. validate_full() (F17)

`validate_full()` is a 217-line function that runs all validation checks in a single scope. The nesting comes from iterating documents, then iterating their relations, then checking conditions on each relation.

Proposed split: extract one function per validation concern.

```rust
@draft fn validate_broken_links(store: &Store) -> Vec<ValidationIssue>
@draft fn validate_parent_links(store: &Store, config: &Config) -> Vec<ValidationIssue>
@draft fn validate_status_consistency(store: &Store) -> Vec<ValidationIssue>
@draft fn validate_duplicate_ids(store: &Store) -> Vec<ValidationIssue>
```

Each helper iterates the store independently, uses early `continue` to skip irrelevant documents, and returns a flat list of issues. `validate_full()` becomes an orchestrator that collects results from each helper and partitions into errors/warnings.

#### 2b. Store::load() (F18)

`Store::load()` combines filesystem traversal, document parsing, virtual doc creation, and link graph building in 196 lines. The nesting comes from iterating type directories, then entries within each, then handling directory-vs-file branching.

Proposed split:

```rust
@draft fn load_type_directory(root: &Path, type_def: &TypeDef) -> Result<Vec<LoadedDoc>>
@draft fn parse_document_entry(path: &Path, type_def: &TypeDef) -> Result<DocMeta>
```

`load()` calls `load_type_directory()` per type, which handles the entry iteration. `parse_document_entry()` handles the read-parse-validate pipeline for a single file. Virtual doc creation stays in `load()` as it depends on the full document set.

#### 2c. resolve_shorthand() (F19)

The qualified and unqualified branches contain duplicated 3-level nested closures for name matching. This overlaps with Finding 2 (duplicated path extraction).

Proposed fix: extract a `canonical_name(doc: &DocMeta) -> Option<&str>` helper that both branches call. This simultaneously eliminates the duplication and flattens the nesting.

```rust
@draft fn canonical_name(doc: &DocMeta) -> Option<&str>
```

The function checks whether the path ends in `index.md` or `.virtual` and returns the parent directory name or the filename accordingly.

#### 2d. extract_gfm_segments() (F20)

A 218-line function managing five interleaved state machines via boolean flags (`in_table`, `in_footnote`, `admonition_kind.is_some()`). The state variables alone occupy 20 lines.

Proposed approach: extract each state machine into a struct that implements a common trait.

```rust
@draft trait GfmExtractor {
    fn try_start(&mut self, event: &Event, offset: usize) -> bool;
    fn feed(&mut self, event: &Event, offset: usize);
    fn try_end(&mut self, event: &Event, offset: usize) -> Option<GfmSegment>;
}

@draft struct TableExtractor { /* table state */ }
@draft struct AdmonitionExtractor { /* admonition state */ }
@draft struct FootnoteExtractor { /* footnote state */ }
```

The main loop iterates pulldown-cmark events and delegates to each extractor. Each extractor owns its state, uses early returns, and stays flat.

#### 2e. draw_preview_content() (F21)

A 159-line rendering function with a match statement over segment types at 4 levels deep. Each arm handles image positioning and wrap-aware Y offsets.

Proposed fix: extract per-segment renderers.

```rust
@draft fn render_markdown_segment(f: &mut Frame, area: Rect, lines: &[Line], scroll: u16)
@draft fn render_diagram_overlay(f: &mut Frame, area: Rect, image: &DynamicImage, y_offset: u16)
```

The match statement in the main loop becomes a one-line dispatch to the appropriate renderer.

### 3. DRY Consolidation

Findings 2-5 identified duplicated logic that should be extracted.

#### 3a. Path extraction (F2)

The block in `store.rs` that resolves a document path to its canonical name appears twice (qualified and unqualified branches of `resolve_shorthand`). Addressed by the `canonical_name()` helper proposed in 2c.

#### 3b. Display name (F3)

`doc_display_name` in `fix.rs` and `extract_id` in `store.rs` both resolve paths by checking for `index.md`. Rather than sharing a function (they have different return types), add a `display_name()` method to `DocMeta` that both can use:

```rust
// In document.rs
impl DocMeta {
    @draft pub fn display_name(&self) -> &str
}
```

This method returns `self.id` (which is already computed during loading), making the ad-hoc path logic in fix.rs unnecessary.

#### 3c. TypeDef construction (F4)

`default_types()` and `types_from_directories()` repeat nearly identical TypeDef construction for rfc, story, iteration, adr. Extract a builder function:

```rust
@draft fn build_type_def(name: &str, dir: &str, prefix: &str, icon: &str) -> TypeDef
```

The function sets `plural` to `format!("{}s", name)` (or the irregular form for "story") and `numbering` to the default strategy. Both call-sites become one-liners.

#### 3d. strip_type_prefix divergence (F5)

`store.rs` matches `is_ascii_alphanumeric() && !is_ascii_uppercase()` (sqids-style). `fix.rs` matches `is_ascii_digit()` only (sequential-style). These are genuinely different operations.

Proposed fix: rename to make the distinction explicit.

```rust
@draft fn strip_type_prefix_sqids(name: &str) -> &str   // in store.rs
@draft fn strip_type_prefix_numeric(name: &str) -> &str  // in fix.rs
```

No consolidation, just clarity. If a future change needs both behaviours, a caller can try one then the other.

### 4. Function Naming

Findings 22-24 identified functions whose names don't convey their scope, forcing comments to compensate.

#### 4a. draw_* renames (F22)

| Current | Proposed | Rationale |
|---------|----------|-----------|
| `draw_preview_content` | `render_document_preview` | Renders metadata, tags, body, and image overlays |
| `draw_relations_content` | `render_relationship_sections` | Builds and renders chain, children, related |
| `draw_fullscreen` | `render_fullscreen_document` | Scrollable document view with image overlays |
| `draw_filters_mode` | `render_filter_panel` | Filter controls, filtered doc list, preview |

These are private functions, so the rename has no public API impact.

#### 4b. collect_* renames (F23)

| Current | Proposed | Rationale |
|---------|----------|-----------|
| `collect_renumber_fixes` | `plan_renumbering` | Plans multi-pass renumbering with reference cascade |
| `collect_all` | `plan_field_and_conflict_fixes` | Plans field fixes and conflict resolution |

"Plan" conveys that these functions produce a list of intended changes without applying them. This matches the dry-run semantics they already support.

#### 4c. walk() rename (F24)

The nested `walk` function inside `rebuild_graph` traverses implementing children. Rename to `traverse_dependency_chain` and move to a standalone function (or method on the graph builder). This also eliminates one level of nesting.

### 5. Module Splits

Findings 6-8 and 25-27 identified six files that have accumulated too many concerns.

#### 5a. tui/ui.rs (1890 lines, F25)

Current state: 27 rendering functions covering layout calculations, panel rendering, overlay dialogs, color mapping, and scrollbar rendering.

Proposed split:

```
src/tui/
├── ui.rs          (orchestrator: draw(), layout selection)
├── ui/
│   ├── colors.rs  (status_color, tag_color)
│   ├── layout.rs  (wrapped_line_count, calculate_image_height)
│   ├── panels.rs  (draw_doc_list, render_document_preview, render_relationship_sections)
│   └── overlays.rs (draw_help, draw_create_form, draw_delete_confirm, draw_status_picker,
│                     draw_link_editor, draw_agent_dialog, draw_search, draw_warnings)
```

> [!NOTE]
> The split uses a `ui/` subdirectory with `ui.rs` as the module root (Rust 2021 module layout). Each submodule receives the functions that belong to its concern. The orchestrator `draw()` function stays in `ui.rs` and dispatches to the submodules.

#### 5b. tui/app.rs (2563 lines, F7)

Current state: App struct with 73 fields, 60+ methods covering document state, form state, caching, event handling, and validation.

Proposed split:

```
src/tui/
├── app.rs         (App struct, core state, public API)
├── app/
│   ├── forms.rs   (CreateForm, DeleteConfirm, StatusPicker, LinkEditor, AgentDialog)
│   ├── cache.rs   (body expansion, diagram rendering, filtered docs cache)
│   ├── keys.rs    (handle_key, key dispatch logic)
│   └── graph.rs   (rebuild_graph, traverse_dependency_chain)
```

The App struct stays in `app.rs` but delegates to submodules via methods. Form structs move to `forms.rs` with their associated methods. The `handle_key` match tree moves to `keys.rs`.

#### 5c. tui/gfm.rs (700 lines, F27)

Current state: parsing (extract_gfm_segments) and terminal rendering mixed in one file.

Proposed split:

```
src/tui/
├── gfm.rs           (re-exports, GfmSegment enum)
├── gfm/
│   ├── parse.rs     (extract_gfm_segments, extractors)
│   └── render.rs    (segment-to-Line conversion, styling)
```

#### 5d. cli/fix.rs (1043 lines, F6)

Current state: field fixing, conflict resolution, renumbering, reference scanning, and output formatting.

Proposed split:

```
src/cli/
├── fix.rs           (run, run_json entry points)
├── fix/
│   ├── fields.rs    (field fix collection and application)
│   ├── conflicts.rs (duplicate ID detection and resolution)
│   ├── renumber.rs  (renumbering orchestration, reference cascade)
│   └── output.rs    (JSON and human-readable output formatting)
```

#### 5e. engine/store.rs (604 lines, F8)

Proposed split:

```
src/engine/
├── store.rs         (Store struct, query API: list, get, resolve_shorthand)
├── store/
│   ├── loader.rs    (load, load_type_directory, parse_document_entry)
│   └── links.rs     (forward_links, reverse_links, related_to, referenced_by)
```

#### 5f. engine/refs.rs (425 lines, F26)

Proposed split:

```
src/engine/
├── refs.rs          (RefExpander, expand, expand_cancellable)
├── refs/
│   ├── code_fence.rs (find_fenced_code_ranges)
│   └── resolve.rs    (resolve_ref, resolve_head_short_sha, language_from_extension)
```

### 6. SOLID Improvements

Findings 9-12 are lower priority but worth addressing for long-term maintainability.

#### 6a. Validation rules (F9)

The current `ValidationIssue` enum requires modifying the enum and all match arms to add a new rule. Introduce a trait:

```rust
@draft trait ValidationRule {
    fn check(&self, store: &Store, config: &Config) -> Vec<ValidationIssue>;
    fn severity(&self) -> Severity; // Error or Warning
}
```

Each rule becomes a struct implementing the trait. `validate_full()` iterates a `Vec<Box<dyn ValidationRule>>`. New rules are added by implementing the trait, without touching existing code.

> [!WARNING]
> This is the most invasive change in the RFC. The current enum-based approach works and is easy to grep. The trait approach adds indirection. Consider whether the rate of new validation rules justifies the abstraction.

#### 6b. Relation types (F10)

`RelationType` is a four-variant enum. Making it data-driven would mean storing relation types in config and parsing them at runtime. The current enum has worked since the beginning and the set of relation types has been stable.

Proposed approach: keep the enum but add a `FromStr`/`Display` round-trip and move the string mapping out of match blocks in link.rs. This addresses the immediate pain (scattered string matching) without the over-engineering risk of a fully data-driven system.

#### 6c. Config decomposition (F11)

The `Config` struct has fields for types, rules, directories, templates, naming, tui, sqids, and reserved ranges. Group related fields into sub-structs:

```rust
@draft struct Config {
    pub documents: DocumentConfig,  // types, naming, numbering
    pub filesystem: FilesystemConfig, // directories, templates
    pub ui: UiConfig,               // tui settings
    pub rules: RulesConfig,         // validation rules
}
```

This is a breaking internal change (all `config.field` becomes `config.documents.field` etc.), but the config is only constructed in one place (`Config::load`) and consumed by value. The blast radius is contained.

#### 6d. Filesystem abstraction (F12)

Direct `std::fs` calls throughout the codebase prevent unit testing of file operations in isolation. Introduce a trait:

```rust
@draft trait FileSystem {
    fn read_to_string(&self, path: &Path) -> Result<String>;
    fn write(&self, path: &Path, contents: &str) -> Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> Result<()>;
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;
}
```

Production code uses `RealFileSystem`. Tests can inject a `MockFileSystem` or `InMemoryFileSystem`. This is a large mechanical change (threading the trait through function signatures) with high payoff for testability.

> [!WARNING]
> This change touches most function signatures in the engine and CLI layers. Consider deferring it until the module splits (stream 5) are complete, so the diff is against the new file structure rather than the old one.

## Ordering and Dependencies

The work streams have dependencies that constrain execution order:

1. Stream 1 (error propagation) has no dependencies. Start here.
2. Stream 2 (nesting flattening) and Stream 3 (DRY) can proceed in parallel.
3. Stream 4 (naming) can proceed in parallel with 2 and 3.
4. Stream 5 (module splits) should follow streams 2-4, since the splits are easier when functions are already extracted and renamed.
5. Stream 6 (SOLID) should follow stream 5, since the trait introductions affect the new module structure.

```
Stream 1 ──────────────────────────────────────────►
Stream 2 ──────────────────────►
Stream 3 ──────────────────────►
Stream 4 ──────────────────────►
                                Stream 5 ──────────►
                                                     Stream 6 ──►
```

## Non-goals

- No new user-facing features
- No public CLI behaviour changes
- No changes to document format or frontmatter schema
- No test coverage expansion (tests are updated where signatures change, but coverage is out of scope)

## Risks

The module splits (stream 5) touch the most files and have the highest merge-conflict risk. Doing them in a single iteration per module (rather than incrementally) keeps the diff coherent but means the PR is large.

Stream 6 (SOLID, especially 6d filesystem abstraction) has the widest blast radius. If the rate of new validation rules and relation types remains low, the abstraction may not pay for itself.

## Stories

1. Engine Safety and Nesting (streams 1, 2a, 2b, 2c) -- replace panicking calls, flatten engine functions
2. DRY Consolidation (stream 3) -- extract helpers, add DocMeta::display_name(), rename strip_type_prefix variants
3. TUI Nesting, Naming, and Structure (streams 2d, 2e, 4a, 4c, 5a, 5b, 5c) -- flatten TUI functions, rename, split ui.rs/app.rs/gfm.rs
4. CLI Fix Restructure (streams 4b, 5d) -- rename collect_* functions, split fix.rs
5. Engine Module Splits (stream 5e, 5f) -- split store.rs, refs.rs
6. SOLID Refactors (stream 6) -- validation traits, config decomposition, filesystem abstraction
