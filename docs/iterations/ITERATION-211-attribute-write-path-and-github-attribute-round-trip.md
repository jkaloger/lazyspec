---
title: Attribute write path and github attribute round-trip
type: iteration
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: STORY-156
---## Changes

### 1. CLI flag `--attr key=value` (clap + dispatch)

`src/cli.rs` `Commands::Update` (line 123) -> add repeatable field:
```
/// Set a custom attribute (repeatable): --attr key=value
#[arg(long = "attr", value_name = "KEY=VALUE")]
attr: Vec<String>,
```
`Vec<String>` -> clap collects each `--attr` occurrence -> AC4 (flag supplied >1x) satisfied by clap natively.

`src/main.rs` `Some(Commands::Update { .. })` (line 182) -> destructure new `attr` field. After existing status/title/body pushes (lines 193-202), parse each `attr` entry:
- split on FIRST `=` -> `(key, value)`; missing `=` -> `bail!("invalid --attr, expected key=value: {raw}")`. Empty key -> bail. Value may contain `=` (split_once, not splitn-all).
- push `(key, value)` into same `updates: Vec<(&str,&str)>` slice already threaded to `run_with_config`.
- Borrow lifetimes: parse into owned `Vec<(String,String)>` BEFORE building `updates`, then push `(&str,&str)` refs into it (mirrors existing `s.as_str()` pattern, lines 195/198/201).

Reserved keys `status|title|body|author` collide with the existing update verbs -> reject an `--attr status=...` form with a bail naming the key (attrs go through the AttrDef path, not the lifecycle path).

### 2. Coercion + validation against declared AttrDefs

New shared helper, `src/cli/update.rs` -> `fn apply_attrs(type_def: &TypeDef, meta: &mut DocMeta, attrs: &[(&str,&str)]) -> Result<()>` (called by BOTH fs + github paths so coercion is single-sourced):
- for each `(key,value)`: look up `type_def.attributes.iter().find(|d| d.name == key)` (`AttrDef`).
- unknown key (no AttrDef) -> bail naming key (foundational slice = declared path only per Scope; dynamic/native attrs are STORY-157/162).
- wrap raw string as `serde_yaml::Value::String(value)`, call `crate::engine::document::coerce_attr(&yaml, def)` -> `Option<AttrValue>`.
  - `None` -> kind mismatch / bad enum option -> `bail!` naming offending key + kind (AC2). e.g. `estimate=notanumber` (Int) and `priority=urgent` (Enum not in `def.values`) both return None.
  - `Some(v)` -> `meta.attributes.insert(key.to_string(), v)`.
- coercion is per-key against its OWN `def.kind` -> AC4 (owner=Str stays string, estimate=3 -> Int) and AC5 (Int emits JSON number) fall out of existing `coerce_attr` + `AttrValue` Serialize (document.rs:236, :198).
- run `AttributeSchemaChecker` over resulting `meta.attributes` before persist -> enforces required-attr rule too; any issue -> bail, NO write (AC2 "left unchanged"). NB `coerce_attr` is `pub(crate)`; `apply_attrs` lives in-crate so access OK.

### 3. Filesystem store: persist coerced AttrValue to frontmatter

`src/engine/fs_ops.rs` `update_document` (line 184). Current loop (lines 202-214) only rewrites EXISTING frontmatter lines -> a brand-new attr key is silently dropped (no append). Fix:
- route `--attr` keys through `apply_attrs` to get typed `AttrValue`, then for each key: if a `key:` line exists -> replace (existing behaviour); else APPEND `key: <serialized>` to `lines`.
- serialize value via `serde_yaml::to_string(&attr_value)` (AttrValue Serialize emits bare scalar: Int->number, Str->string, Date->YYYY-MM-DD) -> reparses correctly via `parse_with_schema` (document.rs:373) -> AC1 (owner=jkaloger reads back) + AC5.
- non-attr keys (status/title) keep current replace-only path.

### 4. github-issues store: round-trip attributes (HTML comment + CACHE)

The github read-back is two-hop: a write pushes to the remote issue body AND mirrors a local cache `.md` under `.lazyspec/cache/<type>/`; `show --json` / `status --json` read the CACHE via the fs parser, NOT the remote body. So attrs must survive BOTH the remote HTML comment (so a LATER update re-reading the remote body does not clobber them) AND the cache file (so `show --json` actually surfaces them). Two distinct sinks, both currently drop attrs.

**(a) Remote HTML comment — clobber-protection on re-read.**
`src/engine/issue_body.rs`:
- `serialize` (line 25): after the `related` block (line 47), emit attributes. `if !doc.attributes.is_empty()` -> push `attributes:` then for each `(k,v)` a nested `  <k>: <serde_yaml scalar>` line. Reuses AttrValue Serialize for typed scalars.
- `CommentFrontmatter` struct (line 150): add `#[serde(default)] attributes: Option<serde_yaml::Mapping>`.
- `deserialize` (line 66): replace `attributes: Default::default()` (line 95) with the parsed map, coercing each entry against AttrDefs (fall back to `AttrValue::Raw` for undeclared, mirroring `parse_with_schema`). `IssueContext` (line 9) carries no AttrDefs -> add `pub attr_defs: Vec<AttrDef>`; populate at the two ctx call sites in `store_dispatch.rs` that re-read the remote body: `update` (line 296) and `set_provenance` (line 366), from `type_def.attributes`.
- IMPORTANT framing: `deserialize` is NOT the `show --json` seam. It runs ONLY when a subsequent `update`/`set_provenance` re-reads the remote body to merge-not-clobber existing attrs (store_dispatch:309). The AC3/AC5 read-back that `show --json` exercises goes through the cache, fixed in (b).

**(b) Cache file — the actual `show --json` read-back seam (load-bearing).**
`src/engine/store_dispatch.rs`:
- `CacheFrontmatter` struct (line ~20) has NO attributes field; `write_cache_file` (line 424) never emits them -> attrs written by `GithubIssuesStore::update` reach the remote + the in-memory `meta` but are DROPPED from the cache `.md`, so `show --json` (reads cache via `DocMeta::parse_with_schema`, loader.rs:55) sees nothing. THIS breaks AC3 + AC5 on the github path.
- Fix: add `attributes: BTreeMap<String, AttrValue>` to `CacheFrontmatter` (or serialize via a map of bare scalars), populate it in `write_cache_file` from `meta.attributes`, emit under an `attributes:` frontmatter key. AttrValue Serialize -> typed scalars -> cache `.md` frontmatter.
- Read side needs NO change: cache load already runs `DocMeta::parse_with_schema(content, schema)` (loader.rs:55) which coerces the `attributes:` block against the type AttrDefs -> typed `AttrValue` -> `doc_to_json` emits JSON number for Int etc. (AC5).

### 5. github-issues store update verb: accept attrs

`src/engine/store_dispatch.rs` `GithubIssuesStore::update` (line 293). The match (lines 312-323) bails on any unknown key (`_ => bail!`). Change: collect non-(status/title/author/body) pairs into an `attrs` vec, then call `apply_attrs(type_def, &mut meta, &attrs)` after the loop, BEFORE `serialize` (line 326). Validation failure -> bail before the `issue_edit` push (line 327) -> no remote mutation on bad input (AC2). `meta` deserialized from remote (line 309) so existing attrs merge. Note: the SAME `meta` (now carrying coerced attrs) flows to `write_cache_file` (line 352) -> with fix 4(b) the cache mirrors the attrs -> `show --json` surfaces them.

## Test Plan

Unit + CLI integration; one check per AC.

- **AC1 (fs string attr persists + reads):** integration test in `src/cli/update.rs` tests (or `tests/`): scaffold fs-backed doc with declared `owner: string` AttrDef -> run update path with `--attr owner=jkaloger` -> assert frontmatter file contains `owner: jkaloger` AND `doc_to_json`/`parse_with_schema` reports `attributes["owner"] == Str("jkaloger")`.
- **AC2 (validation rejects, doc unchanged):** two cases. (a) enum `priority` opts `low|med|high` + `--attr priority=urgent` -> `apply_attrs` returns Err naming `priority`; assert non-zero exit, file byte-identical to pre-state. (b) int `estimate` + `--attr estimate=notanumber` -> Err naming `estimate`, file unchanged. Assert error message contains the offending key.
- **AC3 (github round-trip, BOTH sinks):**
  - HTML comment: in `issue_body.rs` tests, extend `sample_doc` with `attributes` (owner=Str, estimate=Int) + `sample_context` with matching `attr_defs` -> `serialize` then `deserialize` -> assert `meta.attributes` equals input map (typed).
  - Cache + show --json (the real read-back): fake `Gh` client in `store_dispatch.rs` tests -> `GithubIssuesStore::update` with `--attr owner=jkaloger` -> read the written cache `.md`, parse via `DocMeta::parse_with_schema` (or `doc_to_json`) -> assert `attributes["owner"]` present. This is the check that would have caught the dropped-at-cache gap.
- **AC4 (multiple --attr, per-kind coercion):** clap parse test: `["--attr","owner=jkaloger","--attr","estimate=3"]` -> `attr` Vec len 2. apply_attrs -> `attributes["owner"] == Str("jkaloger")` AND `attributes["estimate"] == Int(3)` (NOT Str("3")). Single invocation.
- **AC5 (--json reflects typed value):** after `--attr estimate=3` on a github-backed doc, read cache via `doc_to_json` -> assert emitted `attributes.estimate` is JSON number `3`, not string `"3"` (exercises CacheFrontmatter write + parse_with_schema coercion + AttrValue Serialize). String kind -> JSON string.
- **Edge (parse):** `--attr badpair` (no `=`) -> bail; `--attr k=a=b` -> value `a=b` (split_once on first `=`); `--attr =v` (empty key) -> bail.

## Notes

- **Single coercion seam.** `apply_attrs` is the ONLY place raw strings -> `AttrValue`; fs + github both call it -> AC4/AC5 type fidelity guaranteed identically across stores. Do not coerce inline in `update_document`.
- **Three attribute sinks on the github path.** (1) remote HTML comment (clobber-protection on re-read), (2) cache `.md` frontmatter (the `show --json` read-back), (3) in-memory `meta`. All three must carry attrs; the cache sink (4b) is the one `show --json` reads -> miss it and AC3/AC5 fail despite a correct remote write.
- **HTML-comment format.** Attributes nest under an `attributes:` YAML key inside the existing `<!-- lazyspec ... -->` block (issue_body.rs COMMENT_START/END). `extract_comment` regex (line 171) is format-agnostic -> no regex change. Keep emission deterministic (BTreeMap sorted) for stable diffs.
- **Cache read path is free.** Cache load already calls `DocMeta::parse_with_schema` (loader.rs:55) -> coerces the `attributes:` block against type AttrDefs -> only the WRITE side (`write_cache_file` + `CacheFrontmatter`) needs the new field.
- **Layering.** Clap (cli.rs) -> parse/validate key=value (main.rs) -> coerce+schema-check (apply_attrs) -> persist. fs: -> fs_ops frontmatter. github: -> remote HTML comment (issue_body) + cache .md (write_cache_file). Validation strictly BEFORE any write -> AC2 atomicity (no partial frontmatter, no remote issue_edit, no cache mirror).
- **Last-write-wins.** github path deserializes remote `meta` first (store_dispatch:309) then merges attrs -> concurrent remote edits silently overwritten (per RFC-050 non-goal: no conflict detection).
- **key=value edges.** split on FIRST `=` (split_once) so values containing `=` survive; missing `=` and empty key both bail. Reserved keys (status/title/body/author) rejected as attrs -> they own dedicated verbs.
- **Scope guard.** Unknown key (no matching AttrDef) bails here; dynamic/native attrs (issue_type, PROJECT-n.field) out of scope -> STORY-157/162. No GraphQL, no snapshot validation this slice.