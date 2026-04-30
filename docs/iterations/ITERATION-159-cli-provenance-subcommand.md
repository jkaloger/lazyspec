---
title: CLI provenance subcommand
type: iteration
status: accepted
author: agent
date: 2026-04-29
tags: []
related:
- implements: STORY-111
---



## Changes

Backend scope: all three (`filesystem`, `github-issues`, `git-ref`). Provenance writes through `DocumentStore` dispatch.

### CLI surface

1. **New module `src/cli/provenance.rs`**. Mirror `src/cli/reservations.rs`. Define:
   ```rust
   #[derive(Subcommand)]
   pub enum ProvenanceCommand {
       Add { id: String, citation: String, #[arg(long)] json: bool },
       Remove { id: String, citation: String, #[arg(long)] json: bool },
       List { id: Option<String>, #[arg(long)] json: bool },
   }
   ```
   `id` arg uses `add = ArgValueCompleter::new(completions::complete_doc_id)`. Public fns `run_add`, `run_remove`, `run_list`. ACs: 1, 4, 6, 8, 10.

2. **Wire enum** in `src/cli.rs`. Add `pub mod provenance;` near line 15. Add `use crate::cli::provenance::ProvenanceCommand;` near line 25. Add variant after `Reservations`:
   ```rust
   /// Manage document provenance citations
   Provenance {
       #[command(subcommand)]
       command: ProvenanceCommand,
   },
   ```

3. **Dispatch** in `src/main.rs` after `Reservations` arm (~line 341):
   ```rust
   Some(Commands::Provenance { command }) => match command {
       ProvenanceCommand::Add { id, citation, json } =>
           lazyspec::cli::provenance::run_add(&cwd, &store, &config, &id, &citation, json)?,
       ProvenanceCommand::Remove { id, citation, json } =>
           lazyspec::cli::provenance::run_remove(&cwd, &store, &config, &id, &citation, json)?,
       ProvenanceCommand::List { id, json } =>
           lazyspec::cli::provenance::run_list(&store, id.as_deref(), json)?,
   },
   ```

### Engine: list-replacement path through DocumentStore

4. **Trait method** in `src/engine/store_dispatch.rs`. Extend `DocumentStore` trait:
   ```rust
   fn set_provenance(
       &mut self,
       type_def: &TypeDef,
       doc_id: &str,
       provenance: &[String],
   ) -> Result<()>;
   ```
   Whole-list replacement. CLI reads current via `DocMeta`, computes new list, calls this. Justification (principle 6): three concrete impls (fs, gh, git-ref). Indirection paid for.

5. **Filesystem impl** (`FilesystemStore::set_provenance`). Resolve doc path via `Store::load`, call `rewrite_frontmatter(path, &RealFileSystem, |val| { ... })`. Inside closure: get root mapping, set key `provenance` to `Value::Sequence` of `Value::String` entries (or remove key if list empty? KISS: always write the list, even empty — matches existing write paths).

6. **GitHub Issues impl** (`GithubIssuesStore::set_provenance`). Round-trip through `issue_body::deserialize/serialize`:
   - Fetch issue, deserialize, set `meta.provenance = new_list.to_vec()`, serialize, `client.issue_edit(... new_body ...)`. Mirror existing `update` flow including lock check, issue_map touch, cache write.
   - **Extend `CommentFrontmatter`** at `src/engine/issue_body.rs` to include `provenance: Option<Vec<String>>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. Map both ways in serialize/deserialize.
   - **Extend `serialize`** at `src/engine/issue_body.rs:25` to emit `provenance:` block when `!doc.provenance.is_empty()`. Mirror the `related` block style.
   - **Extend `deserialize`** at `src/engine/issue_body.rs:58` so `meta.provenance` reads from `parsed.provenance.unwrap_or_default()` instead of the hardcoded `vec![]` at line 83.
   - **Extend `CacheFrontmatter`** at `src/engine/store_dispatch.rs:~360` to include `provenance: Vec<String>` (mirror `tags`). Pass through in `write_cache_file`.

7. **Git Ref impl** (`GitRefStore::set_provenance`). Cache file is YAML+body. Current `update` does line-based key=value replace; insufficient for list fields. Use YAML-aware mutation: read cache file, run a mutation analogous to `rewrite_frontmatter` but on the cache content string, write back, push commit via existing `git.create_commit`. Implementation:
   - Read cache content, `split_frontmatter`, `serde_yaml::from_str` → `serde_yaml::Value`, set `provenance` key, `serde_yaml::to_string`, recombine, write cache, push commit with `Some(&old_sha)` for fast-forward.
   - Update cache.lock with new sha.

8. **Dispatch in CLI**. `run_add`/`run_remove` resolve doc, look up `type_def` via `config.type_by_name(doc.doc_type.as_str())`, build the appropriate `*Store`, call `set_provenance(type_def, &doc.id, &new_list)`. Mirror `src/cli/update.rs` shape for backend instantiation.

9. **`run_list` no-op for backends**. Reads `DocMeta.provenance` from in-memory `Store`. Backend-agnostic. Single-doc path uses `resolve_shorthand_or_path`. Global path iterates `store.all_docs()`, filters non-empty.

10. **Empty citation** rejected at `run_add` before backend call. Bail with `"citation must not be empty"`.

11. **Citation-not-found on remove** rejected at `run_remove` before backend call. After computing new list (filter first exact match), if length unchanged → bail.

12. **Doc-not-found** surfaces from `resolve_shorthand_or_path` → anyhow error. AC3.

13. **JSON output**. Verify error precedent (read `src/cli/pin.rs` and `src/cli/lease.rs`) before implementing. Match existing pattern: errors via anyhow → stderr + non-zero exit; success in JSON mode prints structured stdout. AC9 is success-shape only. Shapes:
    - `add`: `{ "doc": "<id>", "added": "<citation>", "provenance": [...] }`
    - `remove`: `{ "doc": "<id>", "removed": "<citation>", "provenance": [...] }`
    - `list <id>`: `{ "doc": "<id>", "provenance": [...] }`
    - `list`: `{ "documents": [{ "id", "path", "provenance" }, ...] }`

### Docs

14. **README update**. Add `lazyspec provenance` section with `add`/`remove`/`list` examples. Per CLAUDE.md.

## Test Plan

Unit tests inside touched modules where helpful. Integration tests in new `tests/provenance_cli.rs` covering CLI + filesystem backend end-to-end. Existing `tests/provenance_roundtrip.rs` (engine layer) already covers serde. Real `Store`, `tempfile::TempDir`, behavioural assertions per DICTUM-004.

### CLI behaviour (filesystem backend)

- `add_appends_to_empty` — fresh doc no provenance, `run_add(... "X")` → reload, `meta.provenance == ["X"]`. AC1.
- `add_appends_to_existing` — existing `["A","B"]`, add "C" → `["A","B","C"]`. AC1.
- `add_empty_citation_errors` — `run_add(... "")` → `Err`; doc unchanged byte-for-byte. AC2.
- `add_unresolved_doc_errors` — bogus id → `Err` mentions "not found". AC3.
- `remove_exact_match` — `["A","B","C"]`, remove "B" → `["A","C"]`. AC4.
- `remove_first_match_only_when_duplicates` — `["A","A","B"]`, remove "A" → `["A","B"]`. (Spec consequence; documents behaviour even if AC silent.)
- `remove_missing_errors` — `["A"]`, remove "Z" → `Err`; reload still `["A"]`. AC5.
- `list_single_doc_plain` — `["A","B"]`, plain output → both citations on stdout. AC6.
- `list_single_doc_empty` — no provenance → exit Ok, plain stdout empty. AC7.
- `list_global_groups_by_doc` — three docs, two with provenance → output groups present only for the two. AC8.
- `add_json_shape` — `--json`, parse stdout, assert keys `doc`, `added`, `provenance`. AC9.
- `remove_json_shape` — assert keys `doc`, `removed`, `provenance`. AC9.
- `list_json_with_id` — keys `doc`, `provenance`. AC9.
- `list_json_global` — top-level `documents` array of `{id,path,provenance}`. AC9.
- `shorthand_id_resolves` — `RFC-001` style id matches `show` resolution. AC10.

### GitHub Issues backend

Tests in `src/engine/issue_body.rs::mod tests` (unit, no network):

- `serialize_emits_provenance_block` — `meta.provenance = ["A","B"]` → output contains a `provenance:` YAML block listing both.
- `serialize_omits_provenance_when_empty` — empty list → no `provenance:` key in output.
- `deserialize_reads_provenance` — fixture comment containing `provenance:` list → `meta.provenance` matches.
- `deserialize_missing_provenance_defaults_empty` — fixture without field → empty vec, no error.
- `roundtrip_preserves_provenance` — serialize then deserialize → `provenance` round-trips.

Tests in `src/engine/store_dispatch.rs::mod tests` (unit, fake `GhCli`):

- `gh_set_provenance_pushes_via_issue_edit` — fake gh, set provenance to `["A"]`, assert recorded body contains `provenance:` block; cache file rewritten with same.
- `gh_set_provenance_clears_when_empty` — set to `[]`, assert no `provenance:` block in pushed body.
- `cache_frontmatter_round_trips_provenance` — write cache then re-load via `Store`, `meta.provenance` matches.

### Git Ref backend

Tests in `src/engine/git_ref_store.rs::mod tests` (unit, fake `GitCli`):

- `git_ref_set_provenance_writes_yaml_list` — set to `["A","B"]`, assert pushed cache content YAML has `provenance` sequence with both entries.
- `git_ref_set_provenance_replaces_existing` — existing `["X"]`, set to `["Y","Z"]` → cache reflects `["Y","Z"]`.
- `git_ref_set_provenance_uses_old_sha_for_ff` — verify `create_commit` called with `Some(old_sha)`.

### Tradeoffs called out

- **Whole-list replacement vs append/remove primitive.** Replacement is simpler at the trait seam and matches how `tags` would be handled; race semantics (concurrent edits clobber) match existing `update` paths. AC4 ("citation removed and others preserved") and AC1 ("appended without modifying existing entries") are satisfied because CLI reads-then-writes against the loaded `Store` snapshot. Remote-backend stale-read risk is the same as for `update`; no new pitfall.
- **`provenance: []` vs absent key on empty.** Filesystem writes the empty sequence (mirrors how serde round-trips today). GitHub serializer omits when empty (matches existing `related`/`status` omission style). Asymmetry intentional: GitHub body is human-visible, noise-averse; filesystem is machine-edited.

## Notes

- Engine prerequisites already shipped (ITERATION-158): `DocMeta.provenance: Vec<String>` parsed and validated; `rewrite_frontmatter` preserves opaque YAML.
- New trait method `set_provenance` is justified by three concrete impls (fs, gh, git-ref). Without remote write-through this would be one impl and inline; with it, the trait pays for itself.
- `CommentFrontmatter` (issue body) and `CacheFrontmatter` (cache file) both need `provenance` extensions; otherwise GitHub round-trips lose the field.
- Git-ref `update` currently does line-based YAML replacement which can't insert/replace list-valued keys. New `set_provenance` impl uses serde_yaml::Value to mutate; `update` for scalar keys can stay line-based until refactor pressure exists.
- README placement: after `lazyspec link` or near `lazyspec update`, whichever follows existing structure.
- Doc resolution: re-use `src/cli/resolve.rs::resolve_shorthand_or_path`. AC10.
- JSON error precedent: verify against `src/cli/pin.rs` / `src/cli/lease.rs` during build before committing to a shape.
