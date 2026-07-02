---
title: Custom GitHub label per document type
type: story
status: accepted
author: jkaloger
date: 2026-07-02
tags: []
related:
- implements: RFC-037
---## Problem

RFC-037 fixes the github-issues type filter label as `lazyspec:{type}`, hardcoded in `type_label()` (src/engine/gh.rs:218-220). Every `github-issues` type gets this exact label. A type named `ticket` always shows `lazyspec:ticket` on its issues — no way to use a plainer label like `Ticket`.

## Goal

Let a `github-issues` type declare its own label in config. Default stays `lazyspec:{type}` when unset, so existing configs need no change.

## Design

Add `github_label: Option<String>` to `TypeDef` (src/engine/config.rs:239-266), `#[serde(default)]`. Add a resolver method:

```rust
impl TypeDef {
    pub fn github_label(&self) -> String {
        self.github_label.clone().unwrap_or_else(|| gh::type_label(&self.name))
    }
}
```

(name clash: rename the field or the method — pick one, `github_label()` reads better than the field, so consider `label_override: Option<String>` for the field.)

### Write side — swap every `gh::type_label(&type_def.name)` call for `type_def.github_label()`

- src/cli/init.rs:85 (`ensure_github_labels`, label creation on `init`)
- src/engine/store_dispatch.rs:495 (`materialize_one`)
- src/engine/store_dispatch.rs:897 (issue create on new doc)
- src/engine/store_dispatch.rs:1131 (`delete`, tags deleted issue)
- src/engine/issue_cache.rs:161 (`refresh_stale`, fetch filter)
- src/engine/issue_cache.rs:313 (`fetch_all`, fetch filter)

### Read side — label-to-type matching can't stay a `"lazyspec:"` prefix strip

`extract_type_and_tags` (src/engine/issue_body.rs:169-190) currently strips the `lazyspec:` prefix and matches the suffix against `known_types` (plain name strings). Once labels are arbitrary strings, prefix-stripping can't identify the type label among an issue's other labels — needs an exact match against each known type's resolved label instead.

Plumbing: `Config` (full `TypeDef` list, so full label set) is in scope one level above every call site (`issue_cache.rs` `refresh_stale`/`fetch_all` take `config: &Config`; the three `store_dispatch.rs` sites hold `self.config`) but only bare name strings get extracted before passing down to `parse_issue` / `IssueContext` / `deserialize` / `extract_type_and_tags`. Thread a name+label pair (or a `label -> name` map) through this chain instead of `known_types: &[String]`:

- src/engine/issue_cache.rs:553 (`parse_issue` signature)
- src/engine/issue_cache.rs:201, :352 (its two call sites, `known_types` currently built as bare names)
- src/engine/issue_body.rs:95-96 (`deserialize`, builds `known_type_refs` from `ctx.known_types`)
- src/engine/store_dispatch.rs:268-282, :955-969, :1092-1106 (three direct `IssueContext` builders, `known_types` built via `self.config.documents.types.iter().map(|t| t.name.clone())`)

Also fix the fallback path: src/engine/issue_cache.rs:599 filters tags with `.starts_with("lazyspec:")` when `deserialize` fails — same exact-match-against-resolved-labels fix applies here.

### Validation

A `github_label` override on a non-`github-issues` type is inert (unused by any store) — decide whether `validate` should warn on it or silently ignore. Recommend silently ignore; it's a config directive not a data-integrity concern (consistent with how e.g. `agents` is unused for non-iteration types today).

## Non-goals

- Custom labels for tags (already unprefixed, direct pass-through — untouched).
- Custom colors/descriptions for the label (still `deterministic_color`/`"lazyspec document type: {name}"`).
- `github-milestones` store — no label scheme there at all (native milestone entity), out of scope.

## Acceptance criteria

- `[[types]]` entry with `github_label = "Ticket"` creates/filters/tags issues using `Ticket`, not `lazyspec:ticket`.
- Omitting `github_label` keeps current `lazyspec:{type}` behavior (regression-free default).
- Reading back an issue tagged with a custom label correctly resolves its lazyspec type (round-trip through `extract_type_and_tags`).
- `README.md` documents `github_label` under the `github-issues` store auth section (README.md:536+), matching how other `[[types]]` fields are documented there.
