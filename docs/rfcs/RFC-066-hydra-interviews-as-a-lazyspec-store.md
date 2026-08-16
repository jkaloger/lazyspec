---
title: "Hydra interviews as a lazyspec store"
type: rfc
status: accepted
author: "Jack Kaloger"
date: 2026-08-17
tags: []
related: []
---
## Summary

Add a `hydra` store backend so lazyspec reads hydra decision-tree interviews — `.hydra/<slug>.json` — as documents. One lazyspec document per hydra tree, parsed directly from JSON at `Store::load` with no `.lazyspec/cache` mirror, read-only. Every existing surface (CLI, TUI, web) picks it up for free because all three consume `Store`.

Design decisions were made in a hydra interview: `.hydra/hydra-store.json` (20 heads, all answered).

## Motivation

A hydra interview is the design record that precedes an RFC. Today it lives in `.hydra/*.json`, invisible to lazyspec: it does not appear in `list`, cannot be found by `search`, cannot be linked from the RFC it produced, and cannot be read in the TUI or web view. The decisions that shaped a document are one directory away from it and structurally disconnected.

Nothing breaks without this, but the record stays split. Anyone reading `RFC-0xx` has no path back to the rejected options and rationale that produced it — which is the specific thing hydra exists to preserve.

Now, because the store abstraction already supports five backends and the marginal cost of a sixth is one enum variant plus a loader.

## Goals

- `.hydra/*.json` trees appear as documents in `list`, `show`, `status`, `search`, `context` and their `--json` forms.
- A hydra document's body carries the intent, the tree, and every decision with its rationale and rejected alternatives.
- Another document can link to a hydra interview and the inbound link shows on it.
- No `.lazyspec/cache` entry, no sync step, no staleness window.
- Missing `.hydra`, malformed JSON, or a newer hydra schema never breaks an unrelated lazyspec command.
- TUI and web reach parity with the CLI in the same iteration.

## Non-goals

- **Writing to `.hydra`.** lazyspec never cuts, sprouts, reopens or rewords. hydra owns invariants lazyspec does not model — cascade on re-answer, `blocked_by` cycle refusal, cauterise-vs-reopen. A lazyspec write path would duplicate or corrupt them.
- **Invoking the `hydra` binary.** No subprocess. lazyspec parses the JSON itself and renders the tree itself, so hydra is not a runtime dependency.
- **Per-head documents.** A tree is one document, not one document per question.
- **A dedicated TUI mode.** No tree-navigation pane, no per-head selection.
- **New CLI subcommands.** The generic commands are already store-agnostic.
- **Reading `.hydra/HEAD`.** Which tree is active is hydra's cursor, not a lazyspec concern.

## Design

### Store backend

A `StoreBackend::Hydra` variant in `src/engine/config.rs:94`, alongside `Filesystem`, `GithubIssues`, `GithubMilestones`, `GithubProjects`, `GitRef` and `ClickupTasks`.

Every non-filesystem backend today routes to `.lazyspec/cache/<type>/` in `Store::load` (`src/engine/store.rs:72-110`) and parses markdown from there. Hydra does neither. The `.hydra` JSON is already local, already version-controlled and already the source of truth; a markdown mirror could only be stale or redundant. So `Hydra` resolves to `root.join(&type_def.dir)` like `Filesystem` does, and dispatches to a JSON loader instead of `loader::load_type_directory`.

This makes hydra the first backend whose documents are not markdown-on-disk. `DocMeta` already carries `virtual_doc: bool`, so a document with no backing markdown file is an existing concept rather than a new one.

### Discovery

The type definition's existing `dir` field, defaulting to `.hydra`. No new config concept and no upward walk — hydra walks upward because it has no project root; lazyspec has already resolved one, and a second walk could disagree with it.

Every `*.json` in that directory becomes a document. `HEAD` is not read: filtering to the active tree would make real interviews invisible to `list` and `search`, which is the opposite of what a store is for.

### Identity

`ID = "HYDRA-" + slug.to_uppercase()`. The tree `hydra-store` becomes `HYDRA-HYDRA-STORE`; the prefix comes from the type definition as usual.

The uppercasing is not cosmetic. `extract_id_from_name` (`src/engine/store.rs:656`) returns the parts of a name up to and including the first segment that is not all-uppercase, so `HYDRA-hydra-store` truncates to `HYDRA-hydra`. Document lookup itself is exact-match on `DocMeta.id` (`src/engine/store.rs:225`, `:259`) and unaffected, but two sites re-derive an id from the path stem rather than reading `DocMeta.id`:

- `src/engine/graph.rs:446`
- `src/web/render.rs:316`

Given a path of `.hydra/hydra-store.json`, both yield `hydra`. They must resolve through the store instead. This is the one place the feature is not free.

An incremental `HYDRA-001` scheme was rejected: it needs a persisted slug-to-number map (the `issue_map.rs` / `task_map.rs` pattern) to stay stable, for no benefit — hydra slugs are already unique and stable within a directory.

### Body rendering

Assembled from the parsed JSON on each load, never written to disk:

~~~
# <slug>

<intent, as prose>

```
<ASCII tree, rendered by lazyspec>
```

## Decisions

### <question>

<answer>

**Why:** <rationale>

**Rejected:** <rejected[] as a list>

## Open questions

- <question> (ready | blocked by <slugs>)
~~~

Rationale and `rejected[]` are included deliberately: they are the part that stops a future reader re-proposing a dead branch, and dropping them would leave the body a list of conclusions without their reasons. Cauterised heads appear under Decisions with their `cauterised_by` noted.

The ASCII tree is rendered by lazyspec from the parsed heads rather than shelling out to `hydra tree`, so there is no version coupling to an installed binary and no subprocess in the load path.

### Status

`Status` is a newtype over `String` (`src/engine/document.rs:95`), so arbitrary state names are already legal. Status is derived from the tree, not authored:

| tree state | status |
|---|---|
| no heads | `draft` |
| any open head | `in-progress` |
| all heads answered | `complete` |

That is `hydra status` exit 4 versus exit 0. The store owns the value; `advance` cannot move it. Per-head states (`ready`, `blocked`, `reopened`) have no meaning at tree granularity — a tree is many heads at once — so they surface in the body's Open questions section instead.

### Refresh

Read on every `Store::load`. No sync command, no mtime comparison, no invalidation: with no cache there is no staleness window, and the read is a handful of local JSON files. The existing file watch (`src/engine/watch.rs`) extends to the hydra directory so a live TUI reflects a cut made mid-interview.

### Links and validation

Inbound links come free: `build_links` (`src/engine/store.rs:112`) constructs `reverse_links` from all loaded documents, so an RFC declaring `implements HYDRA-HYDRA-STORE` shows on the hydra document with no extra work. Outbound links are not supported — nothing in the hydra JSON names a lazyspec document, so there is no field to source one from.

Hydra documents are exempt from authoring rules (`parent-child`, `relation-existence`). A finding is only useful if someone can act on it, and lazyspec cannot author or repair a read-only document. Dangling-link validation still applies: it fires on the document holding the bad reference, which is an ordinary lazyspec document a user can fix.

### Failure modes

- **No `.hydra` directory** — zero documents, no error. `Store::load` already treats a missing type directory this way (`src/engine/store.rs:82-90`); the type may be configured before the first interview exists.
- **Unparseable or newer-schema JSON** — a `ParseError` into `store.parse_errors`, surfaced the way markdown parse errors already are. Load continues, because one bad tree must not break every command in the repo.
- **`hydra` binary absent** — irrelevant. It is never invoked.

### Configuration

Opt-in, not enabled in the shipped default config — `.hydra` is not universal, and a shipped-on type would put an empty phantom type in every repo.

```toml
[[types]]
name = "hydra"
store = "hydra"
prefix = "HYDRA"      # default
dir = ".hydra"        # default
singleton = false
```

Defaults are chosen so name and store alone suffice.

### Test seam

The existing `FileSystem` trait (`src/engine/fs.rs:24`) exposes `read_to_string`, `read_dir` and `exists` — the whole I/O surface a read-only JSON loader needs — and `Store::load_with_fs` already threads it. Tests supply fixture tree JSON through the existing fake filesystem.

No `HydraOps` trait. `GitRefOps` exists because git is a subprocess; hydra is a filesystem. Dictum 6: one implementation does not need a trait.

## Interfaces

```rust
// src/engine/config.rs — extend
pub enum StoreBackend {
    // ...
    #[serde(rename = "hydra")]
    Hydra,
}

// src/engine/store/hydra.rs — new @draft
pub(crate) fn load_hydra_directory(
    root: &Path,
    dir: &Path,
    type_def: &TypeDef,
    docs: &mut HashMap<PathBuf, DocMeta>,
    parse_errors: &mut Vec<ParseError>,
    fs: &dyn FileSystem,
) -> Result<()>;

fn render_body(tree: &HydraTree) -> String;
fn render_ascii(tree: &HydraTree) -> String;
fn derive_status(tree: &HydraTree) -> Status;
```

No CLI signature changes. `--json` output gains hydra documents through the existing serialisation.

## Decisions (ADRs to emit)

- **No cache materialisation for a local-source backend.** Every prior non-filesystem store mirrors into `.lazyspec/cache`; hydra breaks that pattern because its source is already local. Worth recording as the rule for future local backends.
- **Read-only stores are a legitimate class.** lazyspec has assumed every store is writable. Hydra establishes that a store may reject `create`/`update`/`advance`/`link` outright, and that authoring validation rules do not apply to it.

## Stories

1. **Hydra store backend** — the enum variant, the JSON loader, body rendering, status derivation, config declaration, failure modes. The whole engine change.
2. **Path-derived ID resolution** — fix `graph.rs:446` and `web/render.rs:316` to read `DocMeta.id` rather than re-deriving from the path stem. Independently correct; a prerequisite for hydra ids surviving graph and web.

Story 2 blocks story 1's web and graph acceptance, but both are small enough to land together.

## Risks and tradeoffs

- **Read-only is a hard boundary.** Anyone expecting to answer a head from the TUI will be surprised. Accepted: hydra's invariants are the whole of hydra, and duplicating them is a much larger feature. The error message names the `hydra` command to use instead.
- **`extract_id_from_name`'s heuristic is load-bearing and undocumented.** The uppercasing convention works but is easy to break by adding a lowercase-slugged type later. Mitigated by fixing the two path-derived sites rather than relying on the heuristic.
- **Body is regenerated every load.** Rendering ~20 heads of markdown per tree per invocation is trivial at current scale, but a repo with many large trees pays it on every command. Acceptable until measured otherwise; the body cache (`src/engine/store.rs:52`) already exists if it matters.
- **Duplicating hydra's ASCII renderer.** Two renderers can drift. Accepted over making the hydra binary a runtime dependency with version coupling.
- **The JSON schema is hydra's, not ours.** A future hydra format change breaks parsing. Contained by the parse-error path: a broken tree degrades to one bad document, not a broken repo.

