---
title: "Author github sub-issues through the CLI"
type: iteration
status: complete
author: jkaloger
date: 2026-06-26
tags: []
related:
- implements: STORY-166
---
## Changes

### 1. CLI: `--parent <ID>` arg on `Create` (`src/cli.rs:75`)
- `Commands::Create` variant (`:75`) has `doc_type, title, author, body, body_file, json`. ADD `#[arg(long)] parent: Option<String>` after `author` (before `body`). Doc comment: "Place the new document under a parent doc, as a subdir child."
- `--json` field UNCHANGED -> preserved on create (in scope).

### 2. main.rs: thread `parent` into create dispatch (`src/main.rs:116`)
- `Some(Commands::Create { .. })` arm (`:116`) destructures the variant; ADD `parent`. Both call sites -> `run_json_with_body` (`:128`) and `run_with_body` (`:140`) gain a `parent: Option<&str>` arg (`parent.as_deref()`).
- Lease gate (`check_lease_gate_for_create`, `:125`) UNCHANGED -> keys off `doc_type` not parent.

### 3. create.rs: parent-aware path (`src/cli/create.rs`)
- `run_with_body` (`:40`) signature TODAY `(root, config, store, doc_type, title, author, body, on_progress)`. ADD `parent: Option<&str>` (after `body`). `run` (`:18`) + `run_json` (`:228`) thin wrappers -> add `parent`, default `None` from `run`/`run_json` (those are body-less convenience fns; create-with-parent always flows through `run_with_body`/`run_json_with_body`).
- `parent == None` -> existing behaviour, ALL store branches (`:107` github-issues, `:132` milestones, `:156` projects, `:180` git-ref, `:205` filesystem) untouched.
- `parent == Some(pid)` -> NEW pre-create block, runs BEFORE the store branches:
  1. resolve parent doc: `store.resolve_shorthand(pid)` (mirrors `materialize_subdir` `:296`); not found -> `bail!`.
  2. SAME-STORE GUARD: look up child `type_def.store` (already have `type_def`, `:50`) and parent's type store (`config.type_by_name(parent_meta.doc_type.as_str())?.store`). `child_store != parent_store` -> `bail!` with the SAME wording shape as `reconcile_subissues` (`gh_subissue.rs:51`): "...different stores; lazyspec sub-issues are same-store only". Rejects BEFORE any file/remote mutation (AC3).
  3. PROMOTE flat parent -> subdir: if `parent_meta.path` file name is NOT `index.md` (flat), compute the subdir form. Parent dir = `parent_meta.path.parent()`, dir stem = parent file stem (`TYPE-n-slug`), new dir = `<parent_dir>/<stem>/`, new index = `<new_dir>/index.md`. `fs::create_dir_all(new_dir)`, `fs::rename(old_flat_path -> index.md)`. Idempotent: already-`index.md` parent -> skip promote. This is the inverse of `fs_ops::create_document`'s subdir branch (`fs_ops.rs:146-156`) which writes `<dir_name>/index.md`.
  4. CHILD placement: child must land as a sibling `.md` INSIDE the parent subdir, not in `type_def.dir`. Compute child filename via `template::resolve_filename(&config.documents.naming.pattern, &child_type_def.prefix, title, &parent_subdir, numbering, pre_id)` (same call `create_document` makes, `fs_ops.rs:128`) with `&parent_subdir` as the dir so numbering scans the subdir. Render template + write `<parent_subdir>/<child_filename>`. Reuse `fs_ops` template logic -> EXTRACT a `create_child_in_dir(root, config, child_type_def, target_dir, title, author, body)` helper in `fs_ops.rs` rather than duplicate template/numbering/`render_template` (`fs_ops.rs:128-164`); the existing `create_document` subdir/flat split stays, the new helper writes one `.md` into an explicit dir.
  5. return the child's absolute path (same return contract as the flat path, `:225`).
- WHY pre-create, store-agnostic: the child + promoted parent are authored as plain `.md`/`index.md` on disk in `type_def.dir`. For `github-issues` types the on-disk subdir is the SOURCE tree `materialize_subdir`/`load_source_store` scan (`store_dispatch.rs:333`); the child becomes its own issue + native sub-issue on the next fetch via `reconcile_subdir_subissues` (`fetch.rs:246`). For filesystem types the loader's `load_subdirectory` (`loader.rs:105`) tracks the new `children_of`/`parent_of` edges directly (AC1). The command never calls the GitHub store directly for the child -> store dispatch decides the native binding later.
- `run_json_with_body` (`:250`): ADD `parent` arg, forward to `run_with_body`. Returns the child path's `doc_to_json` as today (`:276`).

### 4. fs_ops: child-into-dir helper (`src/engine/fs_ops.rs`)
- ADD `pub fn create_child_in_dir(root, config, child_type_def, target_dir: &Path, title, author, body) -> Result<PathBuf>`: `fs::create_dir_all(target_dir)`; resolve filename via `template::resolve_filename` against `target_dir` (numbering scans the subdir, isolating child numbering from the flat `dir`); render via `load_template` + `render_template` (private today -> call within module); write `<target_dir>/<filename>`; apply `body` override (the `split_frontmatter` re-write block, create.rs `:218`) if `Some`. Returns absolute path. `create_document` (`:68`) UNCHANGED -> the flat/subdir-parent path keeps its own logic; this helper is the child leaf writer.

### 5. list/show JSON: surface `id` (`src/cli/json.rs:19`)
- `doc_to_json` (`:19`) emits `path,title,type,status,author,date,tags,provenance,related,validate_ignore,attributes` -> NO `id` key. github-backed records therefore read `id: null` downstream (key absent). ADD `"id": doc.id` to the `json!` object (`:20`). `doc.id` is ALWAYS populated by the loader's `extract_id` (`store.rs:440`, `loader.rs:58`) for both fs and cache docs, so this is non-null for every record incl. `.lazyspec/cache/<type>/TYPE-n.md`. Fixes `list --json` (`list.rs:43` -> `doc_to_json_with_family` -> `doc_to_json`) and `show --json` (`show.rs:194`) uniformly (AC4). `--json` shape otherwise unchanged.

### 6. Eliminate transient empty-stem `.md` write (`src/engine/store_dispatch.rs`)
- `write_cache_file` (`:1326`) derives the cache path from `meta.id`: `find_cache_file(cache_dir, &meta.id)` else `cache_dir.join(format!("{}.md", meta.id))` (`:1334`). `meta.id == ""` -> writes `cache_dir/.md` (empty stem), the transient file the story observed; it self-heals only when a later write with the real id lands.
- GUARD: `write_cache_file` -> `if meta.id.is_empty() { bail!("refusing cache write for empty doc id") }` at the top (`:1331`). Empty id signals a placeholder that escaped before id assignment (`GithubIssuesStore::create` builds `placeholder_meta` with `id: String::new()`, `:718`, then replaces it into `doc_meta` with the real id BEFORE the write at `:754` -> correct path never trips the guard). The guard converts a silent stray-file into a hard error -> any future caller that writes pre-id is caught in tests, not on disk (AC4 regression guard for the cache-noise observation).

## Test Plan

- AC1 (promote flat parent + track child) — `tests/integration/cli_child_test.rs` style: `TestFixture` writes a flat `docs/rfcs/RFC-003-multi.md` parent; run `create rfc "Appendix" --parent RFC-003` (filesystem type). Assert: `docs/rfcs/RFC-003-multi/index.md` exists (promoted), flat `.md` gone; child `.md` is a sibling under the subdir; reload `Store::load` -> `store.children_of(index_path)` contains the child and `store.parent_of(child) == index_path`. Reuses the `write_child_doc`/loader seam already exercised by `store_test.rs:431`.
- AC2 (github child becomes native sub-issue end-to-end) — unit over `GithubIssuesStore` with `MockGhClient` (`gh.rs` test_support). Author a flat github-issues parent on disk + run create-with-parent to produce the subdir source tree; then `gh_store.sync_subissues(type_def, parent_id)` (`store_dispatch.rs:578`) -> assert the child appears in `MaterializeResult.children` and a matching `addSubIssue` mutation fired (the same assertion shape as `gh_subissue.rs` `add_sub_issue_called_per_unlinked_child`). Drives the STORY-159 path (`materialize_subdir` -> `to_plan` -> `reconcile_subissues`) from a CLI-authored child.
- AC3 (cross-store parent rejected pre-mutation) — `create <gh-type> "x" --parent <fs-parent-id>` (or inverse). Assert `bail!`, error contains "different stores", and NO file created / NO `MockGhClient` mutation call (`graphql_calls`/`issue_create` count == 0). Mirrors `gh_subissue.rs` `cross_store_child_rejected_before_any_mutation`; the guard is hoisted to the CLI boundary so it trips before `fs::rename`/`issue_create`.
- AC4 (list --json carries id) — `doc_to_json` unit (extend `json.rs` tests): a `DocMeta` with `id: "ISSUE-42"` -> `json["id"] == "ISSUE-42"`, never null. Integration: `list issue --json` over a seeded `.lazyspec/cache/issue/ISSUE-42-*.md` -> every record `["id"]` non-null. Plus `write_cache_file` empty-id guard: call with `meta.id == ""` -> `Err`, and no `cache_dir/.md` exists afterward.
- Regression: existing `cli_child_test.rs`/`store_test.rs` subdir fixtures (built via `write_child_doc`) still load identically; `create` WITHOUT `--parent` unchanged across all five store branches; `gh_subissue.rs` reconcile tests untouched (no semantic change).

## Notes

- Store-agnostic by construction: `create --parent` only manipulates the on-disk doc tree (promote parent to `index.md`, drop child `.md` beside it). The github-native binding is NOT done here — it is `reconcile_subdir_subissues` on the next fetch (`fetch.rs:246`) / the best-effort `sync_subissues` in `GithubIssuesStore::create` (`store_dispatch.rs:760`). This keeps the CLI change small and reuses the settled STORY-159 path.
- Promotion is the inverse of `fs_ops::create_document`'s subdir branch (`fs_ops.rs:146`): that branch writes a fresh `<stem>/index.md` for `subdirectory = true` types; here we `rename` an existing flat `TYPE-n-slug.md` into `<stem>/index.md` so a flat-authored parent can gain children regardless of its type's `subdirectory` flag (`config.rs:238`).
- Same-store guard is duplicated at the CLI boundary (not delegated) ON PURPOSE: `reconcile_subissues` only guards at reconcile time (`gh_subissue.rs:48`), which is post-fetch and post-file-mutation. STORY-166 AC3 requires rejection BEFORE any file/remote write, so the check runs in `create.rs` against `type_def.store` of child vs parent type. Same error wording for consistency.
- Child numbering scans the parent subdir (not the flat `type_def.dir`) via the `resolve_filename` `dir` arg (`template.rs:103`), so child `{n:03}` is local to the subdir and cannot collide with top-level siblings. `[naming] pattern = "{type}-{n:03}-{title}.md"` (`.lazyspec.toml:158`) -> child files are `TYPE-NNN-slug.md` inside the subdir, matching `load_child_markdown_files` (`loader.rs:73`).
- `id` was simply ABSENT from `doc_to_json` (`json.rs:19`), not emitted as null — downstream consumers reading `.id` off a record with no key see null. Adding the key fixes both `list --json` and `show --json` since both funnel through `doc_to_json`. No new field on `DocMeta` (it already has `pub id`, `document.rs:323`).
- Empty-stem `.md`: the only constructor with `id: String::new()` is `GithubIssuesStore::create`'s `placeholder_meta` (`store_dispatch.rs:704`), reassigned to the real id before `write_cache_file` (`:754`). The guard in `write_cache_file` makes an empty id a hard error rather than a stray `cache_dir/.md` that self-heals — defensive, but cheap, and pins the regression.
- Out of scope (per story): no top-level `child` subcommand (folded into `--parent`), no re-parenting/moving existing children, no deep-nesting policy beyond STORY-159, no change to reconcile semantics.
- README: `create` gains `--parent`; update the create usage section of README accordingly (per project rule on CLI-interface changes).
