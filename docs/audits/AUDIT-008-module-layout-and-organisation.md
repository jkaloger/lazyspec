---
title: Module Layout and Organisation
type: audit
status: draft
author: jkaloger
date: 2026-03-24
tags: []
related:
- related-to: RFC-032
---



## Scope

Health check of `src/` module layout. Reviewing directory structure, module declaration patterns, file sizes, naming, and placement of concerns across the three top-level modules (`engine`, `cli`, `tui`).

## Criteria

1. Consistent module declaration pattern across the crate (either `mod.rs` or root-file pattern, not both)
2. Module root files are thin routers, not implementation files
3. Files are sized appropriately (no god modules, no trivial single-function files that could be inlined)
4. Modules are placed in the correct layer (engine concerns in engine, CLI concerns in cli, TUI concerns in tui)
5. No naming collisions or ambiguous module names across layers
6. Sub-module visibility is consistent and intentional

## Findings

### F1: Mixed module declaration patterns

Severity: medium
Location: `src/cli/fix/`, `src/engine/refs/`, `src/engine/store/`, `src/tui/app/`, `src/tui/gfm/`, `src/tui/ui/`

The crate uses two Rust module patterns side by side. Top-level modules and `cli/fix` use the `mod.rs` pattern (`cli/mod.rs`, `cli/fix/mod.rs`). Every second-level split in `engine` and `tui` uses the root-file pattern (`store.rs` + `store/`, `app.rs` + `app/`). The mix is consistent within each subtree but inconsistent across the crate.

Recommendation: pick one pattern and apply it everywhere. The root-file pattern (Rust 2018+) is generally preferred because it avoids a directory full of `mod.rs` tabs in an editor. Migrate `cli/mod.rs` to `cli.rs`, `cli/fix/mod.rs` to `cli/fix.rs`, and `engine/mod.rs` to `engine.rs`.

### F2: cli/fix/mod.rs is a thick module root

Severity: medium
Location: `src/cli/fix/mod.rs` (203 lines)

Every other `mod.rs` / module root in the crate is a thin router (3-11 lines of `pub mod` declarations and possibly a few re-exports). `cli/fix/mod.rs` contains the full `fix` command dispatcher with real logic. This makes it inconsistent with how all other module roots behave.

Recommendation: rename to `cli/fix.rs` (root-file pattern) or extract the dispatcher logic into a sibling file (e.g. `cli/fix/dispatch.rs`) so the module root stays thin.

### F3: Inconsistent sub-module visibility in cli/fix

Severity: low
Location: `src/cli/fix/mod.rs`

Three sub-modules are private (`mod conflicts`, `mod fields`, `mod output`) while `renumber` is `pub mod`. Every other module in the crate declares all children as `pub mod`. The selective visibility here is the only instance of this pattern.

Recommendation: if `renumber` needs to be public for tests or external access, document why. Otherwise make it private to match the other three, or make all four public to match the rest of the crate.

### F4: tui/app.rs uses re-export pattern uniquely

Severity: low
Location: `src/tui/app.rs`

`app.rs` declares its four sub-modules as private (`mod cache; mod forms; mod graph; mod keys;`) and then selectively re-exports with `pub use`. No other module in the crate does this. Everywhere else, sub-modules are `pub mod` and consumers import from the sub-module path directly.

Recommendation: this is arguably the better pattern (it controls the public surface), but it should be deliberate. Either adopt the re-export pattern everywhere for split modules, or switch `app.rs` to `pub mod` to match the rest.

### F5: Ambiguous cache module names

Severity: medium
Location: `src/engine/cache.rs`, `src/tui/app/cache.rs`

Two modules named `cache` exist at different levels. `engine/cache.rs` (154 lines) is a disk cache for rendered content. `tui/app/cache.rs` (152 lines) manages TUI-level view state (expansion requests, diagram rendering, filtered document lists). When navigating the codebase or reading imports, the name collision creates ambiguity.

Recommendation: rename one or both to reflect their purpose. E.g. `engine/disk_cache.rs` and `tui/app/view_cache.rs`, or keep `engine/cache.rs` (it's the "real" cache) and rename the TUI one to `tui/app/expansion.rs` since that's what it actually manages.

### F6: engine/symbols.rs is oversized

Severity: medium
Location: `src/engine/symbols.rs` (583 lines)

Second-largest file in the engine layer. The rest of the engine averages around 200-300 lines per file. At 583 lines, `symbols.rs` likely accumulates multiple concerns.

Recommendation: review contents and consider splitting along concern boundaries (e.g. icon/symbol definitions vs symbol resolution logic), similar to what was done for `store.rs` and `refs.rs`.

### F7: git_status.rs placement

Severity: info
Location: `src/engine/git_status.rs` (254 lines)

`git_status.rs` provides git working-tree status parsing. It is consumed exclusively by `tui/ui/panels.rs` and `tui/ui/overlays.rs` for rendering git sign decorations. It has zero CLI consumers. Whether it belongs in `engine` depends on whether "engine" means "domain model" (in which case git awareness is reasonable) or "document management core" (in which case it's a TUI rendering concern that leaked into the wrong layer).

Recommendation: if git status is intended as a shared capability (e.g. future CLI commands might use it), it's fine in `engine`. If it's purely a TUI decoration, consider moving it to `tui/`. No action needed if the current placement is intentional.

### F8: Trivially small CLI modules

Severity: info
Location: `src/cli/delete.rs` (15 lines), `src/cli/resolve.rs` (18 lines), `src/cli/init.rs` (23 lines), `src/cli/ignore.rs` (26 lines), `src/cli/update.rs` (27 lines)

Five CLI command modules are under 30 lines each. These are thin wrappers that call into the engine and print a result. They're not problematic per se (one file per subcommand is a clean pattern), but if any of them are just forwarding a single function call, they could be inlined into the CLI dispatcher.

Recommendation: no action required. The one-file-per-command pattern is consistent and aids discoverability. Flag only if the number of trivial files becomes a navigation burden.

### F9: tui/mod.rs contains substantial logic

Severity: medium
Location: `src/tui/mod.rs` (308 lines)

Like F2, this module root contains real implementation: the main TUI event loop, terminal setup/teardown, and rendering orchestration. The other two `mod.rs` files (`engine/mod.rs` at 11 lines, `cli/mod.rs` at 202 lines) range from thin to moderate. `tui/mod.rs` at 308 lines is the heaviest.

Recommendation: extract the event loop and terminal lifecycle into a dedicated file (e.g. `tui/event_loop.rs` or `tui/run.rs`), leaving `tui/mod.rs` as a thin router like `engine/mod.rs`.

### F10: TUI lacks functional grouping

Severity: high
Location: `src/tui/`

All TUI modules sit flat under `tui/`, mixing content processing (`gfm`, `diagram`), rendering (`ui`), state management (`app`), and infrastructure (`terminal_caps`, `perf_log`) at the same level. As the TUI grows, this flat layout makes it harder to find where a concern lives and encourages coupling between unrelated pieces.

Recommendation: reorganise into functional groups using the root-file pattern throughout:

```
src/tui/
├── mod.rs                (thin router: pub mod declarations + re-export run())
│
├── content.rs + content/
│   ├── gfm.rs + gfm/       (markdown parsing + widget rendering)
│   └── diagram.rs          (d2/mermaid rendering, cache)
│
├── views.rs + views/
│   ├── panels.rs           (doc list, preview, graph, fullscreen)
│   ├── overlays.rs         (forms, search, help, dialogs)
│   ├── colors.rs
│   ├── layout.rs
│   └── keys.rs             (key dispatch, co-located with views)
│
├── state.rs + state/
│   ├── app.rs              (App struct composing sub-structs)
│   ├── forms.rs            (form state structs)
│   ├── cache.rs            (expansion + filtered docs state)
│   └── graph.rs            (dependency traversal)
│
├── agent.rs                (agent spawning, domain-level)
│
└── infra.rs + infra/
    ├── event_loop.rs       (extracted from current mod.rs)
    ├── terminal_caps.rs    (terminal detection)
    └── perf_log.rs         (debug logging)
```

Key design decisions:

- `keys.rs` is co-located with views rather than state, because key dispatch is inherently a view concern (mapping input to actions on the current view). If every new panel means editing `keys.rs`, that coupling is at least visible within the same module.
- `agent.rs` stays at the top level rather than in `infra/`, since agent spawning is a domain concern (LLM-driven actions) not infrastructure.
- `mod.rs` remains as the module root (Rust requires it), but becomes a thin router once the event loop is extracted to `infra/event_loop.rs`.

## Agreed Target Layouts

### CLI: flat, root-file pattern

The one-file-per-command pattern is already clean. Migrate `cli/mod.rs` to `cli.rs`. No functional regrouping needed.

```
src/cli.rs                (arg definitions, command enum)
src/cli/
├── completions.rs        (shared: shell completion callbacks)
├── context.rs            (command)
├── create.rs             (command)
├── delete.rs             (command)
├── fix.rs + fix/          (command, already split)
├── ignore.rs             (command)
├── init.rs               (command)
├── json.rs               (shared: JSON serialization)
├── link.rs               (command)
├── list.rs               (command)
├── reservations.rs       (command)
├── resolve.rs            (shared: path resolution)
├── search.rs             (command)
├── show.rs               (command)
├── status.rs             (command)
├── style.rs              (shared: terminal styling)
├── update.rs             (command)
└── validate.rs           (command)
```

### Engine: flat, root-file pattern

11 files with clear responsibilities. Grouping at this scale adds depth without benefit. Migrate `engine/mod.rs` to `engine.rs`.

```
src/engine.rs             (thin router)
src/engine/
├── config.rs             (core model)
├── document.rs           (core model)
├── fs.rs                 (infrastructure)
├── cache.rs              (infrastructure)
├── git_status.rs         (infrastructure)
├── reservation.rs        (infrastructure)
├── store.rs + store/      (storage)
├── refs.rs + refs/        (content processing)
├── symbols.rs            (content processing)
├── template.rs           (content processing)
└── validation.rs         (content processing)
```

### TUI: functional grouping, root-file pattern

The TUI has the most modules and the most diverse concerns. Group into `content/`, `views/`, `state/`, and `infra/`.

```
src/tui.rs                (thin router: pub mod declarations + re-export run())
src/tui/
├── content.rs + content/
│   ├── gfm.rs + gfm/       (markdown parsing + widget rendering)
│   └── diagram.rs          (d2/mermaid rendering, cache)
│
├── views.rs + views/
│   ├── panels.rs           (doc list, preview, graph, fullscreen)
│   ├── overlays.rs         (forms, search, help, dialogs)
│   ├── colors.rs
│   ├── layout.rs
│   └── keys.rs             (key dispatch, co-located with views)
│
├── state.rs + state/
│   ├── app.rs              (App struct composing sub-structs)
│   ├── forms.rs            (form state structs)
│   ├── cache.rs            (expansion + filtered docs state)
│   └── graph.rs            (dependency traversal)
│
├── agent.rs                (agent spawning, domain-level)
│
└── infra.rs + infra/
    ├── event_loop.rs       (extracted from current tui/mod.rs)
    ├── terminal_caps.rs    (terminal detection)
    └── perf_log.rs         (debug logging)
```

## Summary

The codebase has clean layer separation (`engine`, `cli`, `tui`) and the recent ITERATION-102 splits addressed the worst god modules. The remaining work falls into two tiers:

Tier 1 (structural): F1 (standardise on root-file pattern crate-wide), F10 (TUI functional grouping), F9 (extract event loop from tui module root). These are the high-impact changes that reshape how the codebase navigates.

Tier 2 (consistency/cleanup): F2 (thick fix module root), F3 (visibility inconsistency in cli/fix), F4 (re-export pattern consistency), F5 (ambiguous cache names), F6 (oversized symbols.rs).

Tier 3 (informational): F7 (git_status placement), F8 (small CLI files). No action required.
