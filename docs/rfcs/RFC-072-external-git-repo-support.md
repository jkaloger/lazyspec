---
title: External git repo support
type: rfc
status: review
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- related-to: RFC-071
reviewed: 71b1c52ef65b210c31212ece6098a75d8d7b9f03
---

## Summary

Let a type's documents live outside this repo. Two levels, one primitive -- resolve a location to a local path, then hand that path to the existing store. At store level a type declares `store = "git"` with a clone URL, and lazyspec keeps a managed clone under `.lazyspec/cache/`. At config level `.lazyspec.toml` is a one-liner, `extends = "<dir|url>"`, and the whole config plus every doc root comes from there. Local directories need no new backend: `filesystem` with a `dir` outside the project root already resolves.

## Motivation

1. Specs are shared; code repos are not. A team running several services against one set of RFCs has to either duplicate the docs into each repo or give up lazyspec in all but one. Neither is a choice about documentation.
2. The doc root is already a one-line seam. `store.rs:133` is `root.join(&type_def.dir)`, and every store path flows through it. The capability is a resolution question, not a storage question, which is why it does not want a parallel store implementation.
3. `git-ref` solved the adjacent problem and shows the shape. It puts documents under refs of the current repo. What it does not do is put them in another repo's worktree, so a shared-specs repo has no backend today.
4. Nothing stops a shared repo drifting. Once docs are visible to several code repos, a hand-edit in one is invisible to the others until it breaks their validation. RFC-071's edit guard is the mechanism for this, and it does not currently cover paths outside the project (see Risks).

## Goals

- A type declaring `store = "git"` with `remote = "<url>"` and optional `branch` reads and writes its documents in a managed clone, and `lazyspec fetch` brings that clone current.
- `create`, `update`, `link` and `delete` on such a type push to the declared remote, and a rejected push surfaces as an error naming the remote, not a silent local-only write.
- `store = "filesystem"` with a `dir` resolving outside the project root works end to end -- `list`, `show`, `validate`, `why` -- with no new backend and no new config key.
- A `.lazyspec.toml` whose only key is `extends` loads the config at that location, and resolves every type's `dir` against *that* root rather than the local one.
- `config --json` reports each type's resolved absolute doc root, so a consumer outside the process can match document paths without reimplementing resolution.
- `validate` treats an external document exactly as a local one; no rule assumes a doc path is project-relative.

## Non-goals

- Locking, leasing, or conflict resolution beyond git's own push rejection. A rejected push is reported; the human re-runs after a fetch.
- Per-document repositories. The repo is a property of the type, so all documents of a type share one location.
- Credential handling. Whatever `git` already does for that URL is what lazyspec does; no token storage, no auth prompts.
- Recursive `extends`. The extended config is read as-is; if it carries its own `extends` that is an error, not a chain to follow.
- Writing to a `filesystem` dir that is a git repo lazyspec does not manage. Files are written; committing them is the human's business, as it is for local docs today.
- Migrating existing documents between stores. Moving docs into a shared repo is a `git mv` and a config edit.

## Design

### Resolution, not a second store

One function answers where a type's documents live:

- `filesystem` -- `root.join(&type_def.dir)`, unchanged. `Path::join` discards `root` when `dir` is absolute, so an out-of-project dir already resolves; what this RFC adds for that case is test coverage and the confirmation that no validation rule assumes relativity.
- `git` -- ensure the clone exists under `.lazyspec/cache/<type>/`, then join `dir` against the clone root.

Everything downstream -- template rendering, frontmatter parse, ID assignment, link writing -- reads files under the returned path and is untouched.

### The git store

`remote` is a clone URL. `branch` is optional and defaults to the remote's default branch.

The clone is managed, not user-visible state: created on first use, brought current by `fetch`, and living under `.lazyspec/cache/` which is already gitignored (`git_ref_store.rs` ensures this today). A write is a file write, a commit, and a push. Push rejection means the remote moved; the error names the remote and the branch, and says to fetch.

This sits beside `git-ref` rather than inside it. Both are git, and the RFC states the split so the config surface does not read as a duplicate: **`git-ref` stores documents under refs of this repo; `git` stores them as files in another repo's worktree.**

`create --parent` already refuses when parent and child sit in different backends. A `git` type inherits that refusal, and it is the right one: a subdir child cannot live in a repo its parent does not.

### The config override

```toml
extends = "git@github.com:org/shared-specs.git"
```

or

```toml
extends = "../shared-specs"
```

The file holds that key and nothing else; any other key alongside it is an error, because a partial override invites a merge semantics nobody asked for. A URL is resolved by the same clone machinery as the `git` store; a dir is used in place.

The consequence worth stating: `extends` moves the **root**, so every type's `dir` in the extended config resolves against that root. A repo that extends another is reading that repo's documents through that repo's own type definitions. This is how a code repo participates in a shared doc set without redeclaring its DAG.

### Resolved roots on config --json

`config --json` currently reports `dir` as the raw config string. It gains a resolved absolute path per type. Nothing in the binary needs this -- the engine resolves internally -- but a hook does, and Principle 2 says the answer belongs on the interface rather than in every consumer.

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

## Stories

1. **The `git` store.** `StoreBackend::Git`, `doc_root` resolution, managed clone lifecycle, `fetch` coverage, push-rejection error. Includes the `filesystem`-with-external-dir coverage, since that is where the resolution contract gets pinned.
2. **The `extends` override.** Config loading from a resolved location, the exclusivity error, root-relative `dir` resolution, `resolved_dir` on `config --json`.

Story 2 depends on story 1 for the clone machinery.

## Risks and tradeoffs

- **Write protection is fail-open until RFC-071 lands, and this is the sharp edge.** That RFC's edit guard relativises `tool_input.file_path` against `$CLAUDE_PROJECT_DIR` and matches type dirs under it. An external doc path does not relativise, so every branch falls through to the silent default and a direct agent edit is permitted -- precisely where a hand-edit is most costly, since other repos read the same file. The guard must match against `resolved_dir` instead, which is why `resolved_dir` is in this RFC and not deferred. External stores land with that change or they ship the hole.
- **A managed clone can be stale.** Reads go against whatever the clone last fetched, so a doc can look current and not be. Accepted: the same is true of every cached store here, and `fetch` is the existing remedy.
- **Push on every write is slow.** A `create` becomes a network round trip. Accepted over batching, which would mean local commits nothing pushes and a divergence to explain.
- **Two git backends will be confused.** `git` and `git-ref` differ in a way the names only hint at. Mitigated by documenting the split in `--help` and the README rather than by a longer name.
- **`extends` makes a repo's docs invisible in the repo.** Someone reading the code sees a one-line config and no `docs/`. That is the point, and it is still a surprise; the one-liner is self-describing enough to follow.
