---
title: Module pins and code ownership lookup
type: rfc
status: draft
author: Jack Kaloger
date: 2026-09-04
tags: []
related:
- blocks: RFC-069
---

## Summary

Documents declare the code they govern as coarse globs in frontmatter (`governs`). Lazyspec answers the inverse question, `why <path>`: which documents govern this file. Validation keeps pins honest: a glob that matches nothing is a finding, code in configured scope with no governing document is a finding. Findings become structured objects in `validate --json` so they can carry repair data. First slice of the Pins -> Trust -> Pack chain. Establishes the authored mapping and its lookup only. Does not judge staleness; that is RFC-069.

## Motivation

1. No reverse index from code to documents. An agent changing `src/engine/context/**` greps for the spec, convention or decision that applies and guesses. Lazyspec has document-to-document relations and prose-level `@ref` directives, neither answers "what governs this file".
2. `@ref` is line-level and incidental. It cites code from prose; it does not declare responsibility. Per-file density made RFC-060 citation edges unworkable. Coarse module pins invert that: few globs per document, authored once, stable across refactors.
3. Trust (RFC-069) needs a code-to-document substrate to compute drift. Without pins there is nothing to diff.
4. Dogfood pressure. Unowned-code findings drive authoring here; without them pin density never grows.

## Goals

- `governs` and `reviewed` parse on every document type; empty default; globs compile at load.
- `why <path> --json` lists every document whose glob matches the path, with the matching glob.
- `validate` emits `governs-no-match` for a glob matching zero files and `governs-unowned` for each in-scope file no document governs. Unowned severity is configurable and off by default.
- `validate --json` findings are objects (`rule`, `message`, rule-specific fields), not strings. TUI and web validation panels render `message`.
- Zero-match finding carries rename candidates and a suggested glob when `reviewed` is set; `fix --governs` applies the suggestion.
- `pin <id>` stamps `reviewed` with `HEAD` and keeps its existing `@ref` blob-hash behaviour.
- TUI and web show `governs`/`reviewed` on document detail, carry the new findings in their validation panels, and accept a file path in existing search to list governing documents.

## Non-goals

- Drift, age, staleness bands, badges (RFC-069).
- Symbol-level pins, per-file blob hashes for pins, in-source comments, sidecar maps, CODEOWNERS-style files.
- `@ref` replacement or deprecation.
- Per-type opt-in for pinning.
- A dedicated command for unowned files. Validation is the surface.
- New TUI screens, columns, graph marks.

## Design

### Frontmatter

```yaml
---
governs:
  - src/engine/context/**
reviewed: 0123456789abcdef
---
```

`DocMeta.governs: Vec<String>`, `DocMeta.reviewed: Option<String>`, beside `provenance`. Always parsed, empty default. Globs compiled with `globset` at store load, relative to the configured code root. Pin at module-directory depth, never per file; a module rename is one glob edit, a file rename is nothing.

### Configuration

```toml
[governs]
scope   = ["src/**"]   # files that must be owned
unowned = "warning"    # warning | error; omit to disable (default)
root    = "."          # code root, for a docs-repo split
```

Any type may pin. A bug doc pinning code is a review problem, not a schema problem.

`unowned` defaults off. A repo with no pins and `unowned = "warning"` emits one finding per source file on day one; a project turns it on after seeding pins, and narrows `scope` to the modules it wants owned.

### Validation

Two new `ValidationIssue` variants:

- `GovernsNoMatch { path, glob, renamed, suggested_glob }`, rule `governs-no-match`: a glob matches no file under root. Warning. When `reviewed` is set, `renamed: [{from, to}]` comes from `git diff -M --name-status <reviewed>..HEAD` filtered to paths the old glob matched, and `suggested_glob` is the longest common directory prefix of the `to` paths plus `/**`. Both empty otherwise.
- `GovernsUnowned { file }`, rule `governs-unowned`: a file under `scope` no document's glob matches. Severity from `unowned`.

### Finding shape

`validate --json` today emits `warnings` and `errors` as arrays of strings. Repair data does not fit a string. Every `ValidationIssue` gains a `rule` slug and serialises as an object:

```json
{"rule": "governs-no-match", "message": "...", "path": "docs/specs/SPEC-001-context.md",
 "glob": "src/engine/ctx/**", "renamed": [{"from": "src/engine/ctx/mod.rs", "to": "src/engine/context/mod.rs"}],
 "suggested_glob": "src/engine/context/**"}
```

`message` is the string emitted today. Human `validate` output is unchanged. The TUI validation panel and web validation view render `message`, so their only change is reading a field instead of a string. Breaking change for `validate --json` consumers; lands in story 1 before either new rule.

`fix --governs` rewrites zero-match globs to their `suggested_glob`. It does not touch `reviewed`, so RFC-069 still reports drift after the repair. The rewritten glob is reviewable in the resulting diff.

### Lookup

`why <path> --json` walks the store's compiled globs and returns every match. Engine owns matching; CLI formats. `show` and `show --json` print `governs` and `reviewed`.

`pin <id>` extends the existing verb. Today it pins blob hashes onto `@ref` directives through `certification::compute_blob_hash_for_spec`. It additionally sets `reviewed: <HEAD sha>`, read through `GitRefOps::head`. One verb: "I have reviewed this document against current code."

### Surfaces

TUI and web render `governs`/`reviewed` as frontmatter fields on document detail. New findings flow through the existing validation panel and view. Fuzzy search (TUI `/`, web search box) accepts a file path and lists documents whose globs match it. No new screen.

## Interfaces

```rust
@draft pub struct DocMeta {
    // existing fields
    pub governs: Vec<String>,
    pub reviewed: Option<String>,
}

@draft pub struct GovernsConfig {
    pub scope: Vec<String>,
    pub unowned: Option<Severity>,   // None = off
    pub root: PathBuf,
}

@draft pub enum ValidationIssue {
    // existing variants
    GovernsNoMatch { path: PathBuf, glob: String, renamed: Vec<(String, String)>, suggested_glob: Option<String> },
    GovernsUnowned { file: PathBuf },
}

@draft impl ValidationIssue {
    pub fn rule(&self) -> &'static str;   // one slug per variant
}

@draft pub fn governing(store: &Store, path: &Path) -> Vec<(&DocMeta, &str)>;

@draft trait GitRefOps {
    // existing methods, including read_commit_timestamp
    fn renames(&self, root: &Path, from: &str, to: &str) -> Result<Vec<(String, String)>>;
    fn head(&self, root: &Path) -> Result<String>;
}
```

`GitRefOps` is the git-ref store's client seam, but `read_commit_timestamp` already made it the home for read-only git queries. A second trait for two methods with one caller is principle 6.

```text
lazyspec why src/engine/context/resolve.rs --json
# [{"id":"SPEC-001","type":"spec","title":"Context","status":"accepted","glob":"src/engine/context/**","reviewed":"0123456"}]
lazyspec pin SPEC-001 --json
lazyspec fix --governs --json
lazyspec validate --json | jq '.warnings[] | select(.rule=="governs-unowned") | .file'
```

New dependency: `globset`.

## Decisions (ADRs to emit)

1. **Pins live in frontmatter.** Rejected: CODEOWNERS-style map file (second loader, anchor split from paths; the docs-outside-repo case is handled by `root`), per-directory markers (litters tree, walk cost), in-source comments (`@ref` density problem inverted, language-specific), sidecar per doc (frontmatter with an extra file), instance rows in `.lazyspec.toml` (config holding instance data).
2. **Globs, not directory nodes.** Rejected: code as a store-backed document type with `governs` as a relation. Elegant reuse of relations and edges, but directory granularity cannot express `src/**/*_test.rs`. Deferred as a presentation layer over this design.
3. **No per-type gate.** Rejected: `TypeDef.governs: bool`. One more knob before any evidence of misuse. Unowned findings and review carry the load.
4. **Unowned code is a validation finding, not a command.** `validate` already has `--json`, exits non-zero in CI, and has panels in TUI and web.
5. **Detect, propose, fix for renames.** Rejected: suggest only (leaves the chore), follow renames at query time (hides the stale glob), rename-stable targets like module paths or packages (language-specific).
6. **`pin` is extended, not duplicated.** One verb stamps both `@ref` blob hashes and `reviewed`. Rejected: a new `review <id>` verb (second name for the same act).
7. **Findings are objects with a `rule` slug.** Rejected: string findings with repair data encoded in the message (unparseable), a parallel `repairs` array beside `warnings` (two lists describing one finding), a new `--structured` flag (two shapes for one command).
8. **`why` is a verb, not a flag.** It asks a new question, code to documents, with its own input and output. RFC-070 rejects new verbs for a different output of an existing question; that rule does not apply here. Rejected: `context --for-file` (RFC-064; `context` takes a document and returns a chain, this takes a file and returns a flat list), `search <path>` (search is fuzzy text over titles and bodies; path lookup is exact glob matching).

## Stories

1. **Declare pins and validate them.** Object findings with `rule` in `validate --json`, TUI and web panels reading `message`. Parse `governs`/`reviewed`, load `[governs]`, compile globs, emit `governs-no-match` and `governs-unowned`.
2. **Look up governing documents.** Add `why <path>`, `show` fields, TUI/web frontmatter rendering, and file-path search in TUI and web.
3. **Repair pins after a refactor.** Rename candidates and `suggested_glob` on the finding, `fix --governs`, `pin` stamping `reviewed`.

## Risks and tradeoffs

- **Authoring cost.** Every governed module needs a pin somewhere. Accepted: unowned findings make the gap visible and CI-addressable; pins are a few lines per document.
- **Day-one flood.** Turning `unowned` on in an unpinned repo emits one finding per file under `scope`. Accepted: default off, `scope` narrows the blast radius, and the flood is the adoption pressure once a project opts in.
- **`validate --json` breaks.** Strings become objects. Accepted: `message` preserves the old text, the in-repo consumers (TUI panel, web view, skills) update in story 1, and the shape must change before any finding can carry repair data.
- **Suggested glob is a heuristic.** Longest common prefix over-widens when a module splits. Accepted: the rewrite lands in a reviewable diff; `reviewed` stays put so drift still flags it.
- **`reviewed` is parsed here but only judged in RFC-069.** Accepted: rename detection needs the anchor now; leaving it out would mean a second frontmatter migration.
- **Search overloading.** A path typed into fuzzy search may collide with title matches. Accepted: results are additive; a dedicated screen was rejected as a new surface for one question.
