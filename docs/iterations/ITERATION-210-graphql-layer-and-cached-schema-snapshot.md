---
title: GraphQL layer and cached schema snapshot
type: iteration
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: STORY-155
---## Changes

### 1. GhGraphql trait + GqlVar — `src/engine/gh.rs`

Add alongside `GhIssueReader`/`GhIssueWriter`/`GhAuth` (gh.rs:115-157). Vars NOT a JSON blob -> typed enum flattened to flags.

```rust
pub enum GqlVar { Str(String), Int(i64), Bool(bool) }

pub trait GhGraphql {
    fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value>;
}
```

Helper `fn gql_var_flag(v: &GqlVar) -> (&'static str, String)`: `Str -> ("-f", s)`, `Int|Bool -> ("-F", n.to_string())`. `-f` = string (GraphQL String), `-F` = typed (Int/Boolean/null).

### 2. GhCli impl — `src/engine/gh.rs`

`impl GhGraphql for GhCli` reusing existing shell seam `run_gh_checked` (gh.rs:186-196 -> `Command::new("gh")`). Build argv:

- base: `["api", "graphql", "-f", "query=<query>"]`
- each var -> push `flag` then `<key>=<value>` (e.g. `-f owner=foo`, `-F number=5`)
- run -> `serde_json::from_str(&stdout)` -> `Value`. Errors classified by existing `classify_gh_error` -> `GhError::{AuthFailed,ApiError,RateLimited}`.

gh auth token used automatically (gh inherits its own login) -> no token handling, matches existing reader/writer.

Build `-f query=...` as owned `String`s; collect `Vec<String>` then `args(v.iter().map(String::as_str))` (run_gh takes `&[&str]`).

### 3. Fake impl (test seam) — `src/engine/gh.rs` #[cfg(test)] + reused in callers' tests

Extend `MockGhClient` (gh.rs:469-482) -> impl `GhGraphql`. Add fields:

- `graphql_responses: RefCell<Vec<serde_json::Value>>` (canned, popped FIFO per call)
- `graphql_calls: RefCell<Vec<(String, Vec<(String, GqlVar)>)>>` (records query + flattened vars for assertions)

`graphql()` -> record call, pop next canned `Value`, NO `gh` process. Builder `with_graphql_responses(Vec<Value>)`. Same seam as existing `MockReader`/`MockGhClient` -> engine tests inject without network.

### 4. Schema snapshot struct + cache read/write — new `src/engine/gh_schema.rs`

Caches IDS not names (every native write keys off id):

```rust
#[derive(Serialize, Deserialize, Default)]
pub struct GhSchemaSnapshot {
    pub issue_types: Vec<IssueTypeId>,        // {name, id}
    pub project_fields: Vec<ProjectFieldId>,  // {project_number, field_name, id, data_type}
    pub single_select_options: Vec<OptionId>, // {field_id, name, id}
    pub iterations: Vec<IterationId>,         // {field_id, title, id}
    pub fetched_at: String,                   // rfc3339
}
```

Path helper mirrors `IssueCache::cache_dir` (issue_cache.rs:43-45 -> `.lazyspec/cache`):

- `fn snapshot_path(root: &Path) -> PathBuf` -> `root.join(".lazyspec/cache/gh-schema.json")`
- `fn load(root: &Path) -> GhSchemaSnapshot` -> read+`serde_json::from_str`; missing/parse-fail -> `Default` (offline-safe; same `unwrap_or_default` fallback precedent as gh.rs:377-379)
- `fn save(&self, root: &Path) -> Result<()>` -> `create_dir_all(parent)` + `serde_json::to_string_pretty` + `fs::write`

Resolver methods (offline, used by validate/store later): `issue_type_id(name)`, `field_id(project_n, field)`, `option_id(field_id, name)`, `iteration_id(field_id, title)` -> `Option<&str>`.

### 5. Refresh hook (fetch + persist) — `src/engine/gh_schema.rs` + wired in `src/engine/issue_cache.rs`

`fn fetch_snapshot(gh: &dyn GhGraphql, repo: &str) -> Result<GhSchemaSnapshot>`:

- split `repo` -> owner/name
- query org issue-types: `organization(login:$owner){ issueTypes(first:50){nodes{id name}} }` (vars `-f owner=...`)
- query project fields/options/iterations: `...projectV2(number:$n){fields(first:50){nodes{... on ProjectV2SingleSelectField{id name options{id name}} ... on ProjectV2IterationField{id name configuration{iterations{id title}}}}}}`
- map nodes -> snapshot vecs, set `fetched_at = Utc::now().to_rfc3339()`

Hook: in `IssueCache::refresh_stale` / `fetch_all` (issue_cache.rs:109,224 — the refresh entry points), after issue sync, call `fetch_snapshot` and `save`. Snapshot fetch is best-effort -> GraphQL error -> push `RefreshWarning`, keep existing snapshot (offline validation still works). Refresh entry points gain a `&dyn GhGraphql` param; `GhCli` satisfies both reader and graphql at call sites (cli/*, main.rs:104/115; refresh_stale call main.rs:618).

## Test Plan

AC1 (GhCli shells to `gh api graphql`, uses token, returns Value):
- Unit on `gql_var_flag` + argv builder (factor argv into pure `fn build_graphql_args(query, vars) -> Vec<String>`): assert `["api","graphql","-f","query=..."]` prefix + parses stdout via `serde_json::from_str`. No real gh in unit; argv shape is the contract.

AC2 (fake at same seam, no gh process):
- `MockGhClient::with_graphql_responses([json!({...})])` -> call `graphql` -> returns canned `Value`; assert `graphql_calls` recorded; assert zero process spawn (mock has no `Command`).

AC3 (string -> -f, typed -> -F, never JSON blob):
- `build_graphql_args(q, &[("owner",Str), ("number",Int(5)), ("flag",Bool(true))])` -> contains `-f owner=...`, `-F number=5`, `-F flag=true`; assert NO arg contains `variables=` / no JSON blob.

AC4 (refresh fetches + persists ids to gh-schema.json):
- Fake `GhGraphql` returns canned issue-types + project fields JSON. Run refresh hook over TempDir -> assert `.lazyspec/cache/gh-schema.json` exists, parses, contains issue-type **id** (not just name), project field id, single-select option id, iteration id.

AC5 (offline read of snapshot, no network):
- Write snapshot file via `save`. `GhSchemaSnapshot::load(root)` (no `GhGraphql` passed) -> resolvers return ids. Assert zero graphql calls. Missing file -> `Default` (no panic).

Snapshot persist/load round-trip:
- `save` then `load` -> structural equality; `save` creates `.lazyspec/cache/` parent dir.

## Notes

- `gh api graphql` vars are repeated `-f key=val` (String) / `-F key=val` (Int/Bool/null) flags — NOT a single `variables` JSON blob. Trait sig (`&[(&str, GqlVar)]`) enforces this; never serialize a vars object.
- Projects mutations/reads need `project` scope on the gh token. A plain `gh auth login` -> permission error on projectV2 queries. Remedy to document: `gh auth refresh -s project`. Surface in error path + README.
- macOS keyring-lookup timeout -> `gh api` can silently send unauthenticated -> surprise 403 rate-limit. Workaround: `GH_TOKEN=\"$(gh auth token)\" gh api …`. Document; do not bake env mangling into GhCli (keep seam clean).
- Snapshot caches **ids** (issue-type id, project field id, single-select option id, iteration id), not just display names -> native writes (`updateIssue{issueTypeId}`, `updateProjectV2ItemFieldValue`) key off id.
- Layering: `GhGraphql` is an engine seam (gh.rs), same as existing Gh* traits. CLI/TUI never call `gh api graphql` directly -> go through trait. `GhCli` is the only real impl; mocks at the seam.
- No new crate dependency — reuse `std::process::Command` + `serde_json` already in gh.rs. No octocrab.
- Snapshot fetch on refresh is best-effort: GraphQL failure -> `RefreshWarning` + keep prior snapshot. Stale snapshot still serves offline validate; GitHub mutation error is the real backstop for stale ids.
- No TTL/staleness logic on the snapshot this slice — just write on refresh, read on demand (out of scope per story).