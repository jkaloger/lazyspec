---
title: External git repo support
type: rfc
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- related-to: RFC-071
reviewed: bb17f1630effa6c943b9fb308098d4cb3ba77469
---

## Summary

Let a type's documents live outside this repo. Two levels, one primitive -- resolve a location to a local path, then hand that path to the existing store. At store level a type declares `store = "git"` with a clone URL, and lazyspec keeps a managed clone under `.lazyspec/cache/`. At config level `.lazyspec.toml` is a one-liner, `extends = "<dir|url>"`, and the whole config plus every doc root comes from there. Local directories need no new backend: `filesystem` with a `dir` outside the project root already resolves.

## Motivation

1. Specs are shared; code repos are not. A team running several services against one set of RFCs has to either duplicate the docs into each repo or give up lazyspec in all but one. Neither is a choice about documentation.
2. The doc root is already a one-line seam. `store.rs:133` is `root.join(&type_def.dir)`, and every store path flows through it. The capability is a resolution question, not a storage question, which is why it does not want a parallel store implementation.
3. `git-ref` solved the adjacent problem and shows the shape. It puts documents under refs of the current repo. What it does not do is put them in another repo's worktree, so a shared-specs repo has no backend today.
4. Nothing stops a shared repo drifting. Once docs are visible to several code repos, a hand-edit in one is invisible to the others until it breaks their validation. Guarding that is out of scope here, but any tool that wants to -- a hook, an editor plugin, a CI check -- needs to know which paths are lazyspec documents, and today it cannot compute that from outside the process (see Design, resolved roots).

## Goals

- A type declaring `store = "git"` with `remote = "<url>"` and optional `branch` reads and writes its documents in a managed clone, and `lazyspec fetch` brings that clone current.
- Every command that writes a document of such a type pushes to the declared remote, and a rejected push surfaces as an error naming the remote, not a silent local-only write.
- `store = "filesystem"` with a `dir` resolving outside the project root works end to end -- `list`, `show`, `validate`, `why` -- with no new backend and no new config key.
- Document paths come out of `list --json` in one shape per type, whether `dir` is relative, absolute, or relative-escaping.
- A `.lazyspec.toml` whose only key is `extends` loads the config at that location, and resolves every type's `dir` against *that* root rather than the local one, while code-facing roots stay local.
- `config --json` reports each type's resolved absolute doc root, for every backend, so a consumer outside the process can match document paths without reimplementing resolution.
- `validate` treats an external document exactly as a local one; no rule assumes a doc path is project-relative.

## Non-goals

- Locking, leasing, or content conflict resolution beyond git's own push rejection. A rejected push is reported; the human re-runs after a fetch. ID allocation is the one exception: it is reserved against the type's remote, because a duplicate ID is not something a re-run can fix.
- Per-document repositories. The repo is a property of the type, so all documents of a type share one location.
- Credential handling. Whatever `git` already does for that URL is what lazyspec does; no token storage, no auth prompts.
- Recursive `extends`. The extended config is read as-is; if it carries its own `extends` that is an error, not a chain to follow.
- Writing to a `filesystem` dir that is a git repo lazyspec does not manage. Files are written; committing them is the human's business, as it is for local docs today.
- Migrating existing documents between stores. Moving docs into a shared repo is a `git mv` and a config edit.

## Design

### Resolution, not a second store

One function answers where a type's documents live:

- `filesystem` -- `root.join(&type_def.dir)`, unchanged. `Path::join` discards `root` when `dir` is absolute, so an out-of-project dir already resolves.
- the five cache-backed stores (`github-issues`, `github-milestones`, `github-projects`, `git-ref`, `clickup-tasks`) -- `root.join(".lazyspec/cache").join(&type_def.name)`, unchanged, ignoring `dir` entirely.
- `git` -- ensure the clone exists under `.lazyspec/cache/<type>/`, then join `dir` against the clone root.

Everything downstream -- template rendering, frontmatter parse, ID assignment, link writing -- reads files under the returned path and is untouched.

What the external-`filesystem` case needs is not a new resolution but a consistent *output* shape. Verified against HEAD: an absolute out-of-project `dir` reports document paths as absolute, while a relative one that escapes the root (`../shared-specs`) reports them as `../shared-specs/RFC-001-x.md`, because `store/loader.rs:75` only falls back to absolute when `strip_prefix` fails. One feature, two path shapes. A missing external directory is worse: `store.rs:136` skips a non-existent doc root silently, so a typo in an absolute `dir` is indistinguishable from a shared repo with no documents in it.

### The git store

`remote` is a clone URL. `branch` is optional and defaults to the remote's default branch.

The clone is managed, not user-visible state: created on first use, brought current by `fetch`, and living under `.lazyspec/cache/`. That directory is gitignored by `ensure_cache_gitignored` (`git_ref_store.rs:16`), which today is private and reached from exactly one call site -- `GitRefStore::create` at `git_ref_store.rs:287`, a git-ref *write*. A project with a `git` type and no git-ref writes never calls it, so the guard must be lifted out of that module and invoked on the clone path.

A write is a file write, a commit, and a push. Push rejection means the remote moved; the error names the remote and the branch, and says to fetch. Two consequences the implementation has to carry:

- **The rejected commit is rolled back.** Left in place, the orphan file is counted by `next_number` (`fs_ops.rs:154`), so the retry allocates *n+1* while the remote's winner holds *n*. `reserve_next` already sets the precedent with `cleanup_local_ref` (`reservation.rs:255`).
- **IDs are reserved against the type's remote.** Push rejection does not allocate an ID: `NumberingStrategy::Incremental` scans the target dir only, so two repos both compute STORY-300 and the loser gets a different ID than it just reported. `reservation::reserve_next` (`reservation.rs:226`) solves this already, but `fs_ops.rs:129` points it at `[numbering.reserved].remote` rather than the type's.

This sits beside `git-ref` rather than inside it. Both are git, and the RFC states the split so the config surface does not read as a duplicate: **`git-ref` stores documents under refs of this repo; `git` stores them as files in another repo's worktree.**

`create --parent` already refuses when parent and child sit in different backends (`ops/create.rs:251`). A `git` type inherits that refusal, but not sufficiently: the guard compares `StoreBackend` discriminants, so two `git` types with different `remote` values pass it. The rule this RFC wants -- a subdir child cannot live in a repo its parent does not -- needs the comparison made on the resolved repo.

### The config override

```toml
extends = "git@github.com:org/shared-specs.git"
```

or

```toml
extends = "../shared-specs"
```

The file holds that key and nothing else; any other key alongside it is an error, because a partial override invites a merge semantics nobody asked for. A URL is resolved by the same clone machinery as the `git` store; a dir is used in place.

The consequence worth stating: `extends` moves the **doc root**, so every type's `dir` in the extended config resolves against that root. A repo that extends another is reading that repo's documents through that repo's own type definitions. This is how a code repo participates in a shared doc set without redeclaring its DAG.

It moves the doc root and nothing else. `Store` already carries two roots -- `store.rs:188` sets `governs_root` from `config.governs.root`, defaulting to `"."` -- and conflating them would point every git question about *code* at the shared spec repo: `why.rs:45`, `validation.rs:985` and `:1138`, `staleness.rs:173`, and the `reviewed` anchors themselves. RFC-068 already solved the docs/code split with `[governs] root`, and Decision 2 makes that key illegal beside `extends`, so the split has to be structural rather than configurable:

| resolves against the **extended** root | resolves against the **local** root |
| --- | --- |
| `[[types]].dir` | `[governs].root` |
| `[templates].dir` | staleness anchors and `reviewed` SHAs |
| | `@ref` expansion |
| | `.lazyspec/cache/` |

A one-key config also has to short-circuit before `parse_inner`, which bails on missing `[[types]]` (`config.rs:1803`) and missing `[[relationships]]` (`config.rs:1810`).

### Resolved roots on config --json

`config --json` currently reports `dir` as the raw config string. It gains a resolved absolute path per type, for every backend -- including the five cache-backed ones, where the answer is `<root>/.lazyspec/cache/<type name>` and the raw `dir` is unused. Nothing in the binary needs this; the engine resolves internally. A consumer outside the process does, and Principle 2 says the answer belongs on the interface rather than in every consumer.

## Interfaces

```rust
@draft pub enum StoreBackend { /* + */ Git }
@draft pub struct GitStoreConfig { pub remote: String, pub branch: Option<String> }
@draft pub fn doc_root(config: &Config, root: &Path, type_def: &TypeDef) -> Result<PathBuf>;
@draft pub struct RawConfig { /* + */ pub extends: Option<String> }
```

`[[types]]` gains `remote` and `branch`, meaningful when `store = "git"`. Top-level `extends`. `config --json` types gain `resolved_dir`. `fetch` covers `git` types. No new subcommand.

## Decisions (ADRs to emit)

1. **The `git` store is remote-only.** A local directory is `filesystem` with a `dir` outside the project root. Two config spellings for one resolution would be a distinction the engine does not make.
2. **`extends` is exclusive.** A config declaring `extends` declares nothing else. The alternative is merge semantics between two configs, and there is no obvious answer for a type declared in both.
3. **External doc roots are absolute on `config --json`.** Consumers outside the process match document paths against them directly rather than reimplementing resolution against a project root that may not be the right one.
4. **`extends` moves the doc root only.** Code-facing roots -- `governs`, staleness anchors, `@ref`, `.lazyspec/cache/` -- stay local. The alternative needs a per-repo `[governs] root` that only the shared config could declare, which breaks the N-services case Decision 2 exists to serve.
5. **A rejected push exits non-zero.** `git-ref` reports an unreachable remote as `PushOutcome::LocalOnly` / `synced: false` at exit 0 (`git_ref_store.rs:138`). The `git` store diverges deliberately: an unreachable remote is a retry, a moved remote is a conflict a human resolves.

## Stories

1. **One resolved doc root per type.** `doc_root` resolution across every existing backend, `resolved_dir` on `config --json`, one path shape out of `list --json` whatever the config spelling, and a warning where a missing external directory currently reads as an empty one.
2. **The `git` store, read path.** `StoreBackend::Git`, managed clone lifecycle, `fetch` coverage, explicit write refusal in the manner of `ClickupTasksStore`.
3. **The `git` store, write path.** Commit and push across every mutating command, rejection rollback, ID reservation against the type's remote.
4. **The `extends` override.** Config loading from a resolved location, the exclusivity error, the doc-root/local-root split, `fetch` for the config clone.

Story 1 pins the contract the rest build on; 2 depends on it, 3 and 4 depend on 2.

The earlier two-story split folded story 1 into story 2. That was wrong: the resolution contract covers six backends that exist today and a path-shape bug that is live now, none of which needs the `git` store to be worth fixing.

## Risks and tradeoffs

- **Nothing guards a hand-edit to a shared document, and this RFC does not add one.** An earlier draft claimed RFC-071's edit guard would fail open on external paths. That was wrong on the facts: no such guard exists -- `hooks/hooks.json` carries a single `UserPromptSubmit` hook, and `CLAUDE_PROJECT_DIR` appears nowhere in the codebase -- and RFC-071 is itself unaccepted with none of its stories written. The exposure is real but it is not a regression: `resolved_dir` is what any future guard would need, which is reason to ship the field, not reason to claim urgency this RFC cannot support.
- **A managed clone can be stale.** Reads go against whatever the clone last fetched, so a doc can look current and not be. Accepted: the same is true of every cached store here, and `fetch` is the existing remedy.
- **Push on every write is slow.** A `create` becomes a network round trip. Accepted over batching, which would mean local commits nothing pushes and a divergence to explain.
- **Two git backends will be confused.** `git` and `git-ref` differ in a way the names only hint at. Mitigated by documenting the split in `--help` and the README rather than by a longer name.
- **`extends` makes a repo's docs invisible in the repo.** Someone reading the code sees a one-line config and no `docs/`. That is the point, and it is still a surprise; the one-liner is self-describing enough to follow.
