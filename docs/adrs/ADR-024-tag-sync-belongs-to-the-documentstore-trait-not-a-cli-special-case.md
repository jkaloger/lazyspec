---
title: "Tag sync belongs to the DocumentStore trait, not a CLI special-case"
type: adr
status: accepted
author: "jkaloger"
date: 2026-07-14
tags:
- architecture
- store
- tags
related:
- related-to: STORY-207
- related-to: RFC-037
---

## Context

`lazyspec tag add/remove` rewrites frontmatter `tags:` for any backend, but remote propagation lives in a hardcoded special-case in `src/cli/tag.rs`. `push_tags_if_github_backed` guards with `if type_def.store != StoreBackend::GithubIssues { return Ok(()) }` — a not-equal early-return, not a match. Consequences of the current shape:

- **No compile-time coverage.** `StoreBackend` has 6 variants; only `GithubIssues` propagates. `ClickupTasks`, `GitRef`, `GithubMilestones`, `GithubProjects` hit the early-return and silently no-op remote-side. A 7th backend compiles fine and drops tags with zero warning.
- **No trait seam.** Every other write (`create`, `update`, `delete`, `set_provenance`) dispatches through the `DocumentStore` trait (`store_dispatch.rs:44`), so each backend is forced to impl or explicitly `bail!` (see `UnavailableStore`). Tags are the lone write that bypasses this — logic sits in the CLI layer, which per principle 3 should be dispatch + formatting only.
- **Silent failure is a lie.** A ClickUp- or git-ref-backed `tag add` returns `Ok(())`, prints success, and changes only the local cache. The user believes the tag synced.

The indirection is now justified under principle 6: there are ≥2 concrete backends that must propagate tags differently (github label push; git-ref frontmatter re-push into the blob), not one.

## Decision

Add a tag-sync method to the `DocumentStore` trait and route `cli/tag.rs` through store dispatch instead of matching on `StoreBackend`.

```rust
fn sync_tags(
    &mut self,
    type_def: &TypeDef,
    doc_id: &str,
    add: &[String],
    remove: &[String],
) -> Result<()>;
```

Single method with `add`/`remove` slices, mirroring `GhIssueWriter::issue_edit`'s `(labels_add, labels_remove)` shape. Per backend:

- **FilesystemStore** — no-op `Ok(())`. The doc file is the source of truth; the CLI already rewrote its frontmatter.
- **GithubIssuesStore** — `label_ensure` each added label, `issue_edit(labels_add, labels_remove)`, touch cache lock. (The existing `push_tags_if_github_backed` body moves here verbatim.)
- **GitRefStore** — re-serialize the cache frontmatter into the ref blob and push, matching how `update` already persists tags.
- **ClickupTasksStore** — `bail!` with the existing `WRITE_UNIMPLEMENTED` message. Now an explicit, visible "not yet" at the trait seam rather than a silent success.

The CLI keeps the backend-agnostic local frontmatter rewrite, then calls `store.sync_tags(...)` for propagation. The `#[non_exhaustive]`-in-spirit guarantee is the `dyn DocumentStore` dispatch: a new backend cannot be registered without an impl.

Rejected: a `match` over `StoreBackend` in the CLI with a `#[deny]` on non-exhaustiveness. It keeps I/O logic in the CLI layer (violates principle 3) and still lets a backend forget the remote half — the compiler checks the arm exists, not that it does the right thing.

## Consequences

- Adding a backend forces a `sync_tags` decision at compile time — propagate or `bail!` — the same contract as every other store write.
- git-ref tag mutations stop drifting from the pushed ref; ClickUp `tag` stops lying and errors clearly until its write path lands (RFC-056).
- The CLI layer sheds its one piece of backend-specific I/O; `cli/tag.rs` becomes resolve → rewrite → dispatch.
- Cost: one more trait method every backend carries, including the trivial filesystem no-op. Accepted — it is the price of exhaustiveness the current `!=` check silently skips.
