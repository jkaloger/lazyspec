---
title: "Deeper GitHub-native integration"
type: rfc
status: accepted
author: "jkaloger"
date: 2026-06-25
tags: []
related: []
---## Summary

Extend the `github-issues` store from a dumb-storage mirror (metadata in an HTML comment, status as open/closed) into a **native-binding layer** over GitHub: lazyspec documents bind to GitHub Projects v2 boards, milestones, native issue-types, sub-issues, project field values, and read GitHub issue comments. The binding is bidirectional except where reading-only is the natural semantics (comments). All four GraphQL-only constructs (Projects v2, issue-types, sub-issues, project fields) are reached through one new trait wrapping `gh api graphql`.

## Motivation

The current github-issues integration is deliberately minimal: type and tags are labels, status is open/closed, and every other lazyspec concept (relations, provenance) lives in an HTML comment embedded in the issue body. That keeps GitHub as inert storage but leaves the document model blind to the GitHub-native structure teams actually organize work with: project boards, milestones, issue-types, and sub-issue hierarchies. A lazyspec doc backed by a GitHub issue cannot today answer "what board is this on", "what's its priority field", "what milestone", or "what are its sub-issues" — that state exists on GitHub and is invisible to lazyspec.

Three latent gaps compound this: the github store silently ignores subdirectory-type documents (only `index.md` would sync, children dropped), it does not round-trip document attributes at all, and there is no CLI to write an attribute value. Any native-field work depends on closing all three.

## Goals

- A `GhGraphql` trait wrapping `gh api graphql`, fakeable at the same seam as the existing `Gh*` traits, with no new dependency.
- Native issue-type readable and writable as an orthogonal document attribute (lazyspec type stays the `lazyspec:{type}` label).
- A `github-milestones` store backend: a milestone document binds to a GitHub milestone; issue-docs associate via the native `issue.milestone` field surfaced as a relation.
- Subdirectory documents materialize correctly in the github store, and their structural children bind to native GitHub sub-issues.
- A `github-projects` store backend: a project document binds to a Projects v2 board; issue-docs join boards many-to-many via a membership relation; per-board field values surface as namespaced dynamic attributes synced bidirectionally.
- GitHub issue comments surfaced read-only in `show --json` / `status --json`.
- Native-field schema (board field options, org issue-types) validated offline against a cached snapshot.
- Attribute round-trip and an attribute write CLI path in the github store.

## Non-goals

- **Lifecycle inheritance from a board's Status field.** A doc can be on many boards; choosing a status-authority board is deferred to a later RFC. Board Status is a plain namespaced attribute here; lifecycle stays open/closed.
- **Posting comments.** Comments are read-only this round.
- **Conflict detection on native writes.** Policy is last-write-wins + refresh; optimistic locking for native fields is out of scope.
- **Creating project boards from lazyspec.** Boards are read/associated, not authored. (Milestone authoring is in scope; boards are not.)
- **A direct GraphQL HTTP client (octocrab).** All GitHub access stays behind the `gh` CLI seam.

## Design

The organizing idea is a **native-binding layer**: a lazyspec concept binds to a GitHub-native construct in one of three shapes.

### 1. Native store backends

A document whose home is a GitHub object that is not an issue.

- `github-milestones` — a milestone document maps to a GitHub milestone via the REST Milestones API (`title`, `description`, `due_on` ISO 8601, `state`; `open_issues`/`closed_issues` are read-only, so %complete is computed, not a stored field). Milestone docs are authorable (create/update milestone). Lifecycle maps to milestone open/closed state. Issue→milestone association is `PATCH issues/{n}` with `milestone: <number>`.
- `github-projects` — a project document maps to a Projects v2 board. Read/associate only; boards are not created from lazyspec. The board owns its field schema.

Both extend the `StoreBackend` enum and implement `DocumentStore`. The cache materializes them under `.lazyspec/cache/<type>/` like issues.

### 2. Native-backed relations

A relation whose edge lives in a GitHub API rather than the HTML comment. A `[[relationships]]` entry declares `github_native = "<kind>"`.

- `sub-issue` — structural subdirectory children. A subdirectory doc's `index.md` is the parent issue; each child `.md` (its own typed doc) becomes a native sub-issue via the GraphQL sub-issues API (`addSubIssue` / `removeSubIssue` / `reprioritizeSubIssue`; GA 2025-03-17, the `sub_issues` preview header no longer required). Endpoints are issue-backed and **same-store by construction** — GitHub itself permits same-owner cross-repo sub-issues, so the constraint we rely on is lazyspec's, not GitHub's. This also closes the gap where the github store ignores subdirectory children: the store must now materialize subdirectory parents + children. (Flat parent→child shape stays well within GitHub's ~100-children / 8-nesting limits.)
- `membership` — issue-doc → project doc, many-to-many (multiple relations = multiple boards). Backed by the Projects v2 `addProjectV2ItemById(projectId, contentId)` mutation.

Semantic relations (`implements`, `blocks`, `related-to`) stay comment-backed and unchanged. Native-backed relations are the exception, flagged in config.

### 3. Native attributes

Namespaced dynamic attributes synced to a GitHub field or type. These are not statically declared `AttrDef`s — their schema lives on GitHub.

- `issue_type` — the org's native issue-type (Bug/Task/Feature/custom), orthogonal to lazyspec type. Bidirectional. GA since 2025-03-17 (preview `issue_types` header no longer required). The write is the `updateIssue` mutation with `issueTypeId` (or `null` to clear) — there is no dedicated set-type mutation — so the schema snapshot must cache the org's issue-type **ids**, not just names.
- `PROJECT-n.<field>` — per-board field values. The board id namespaces the key so the same field name on two boards does not collide (`PROJECT-1.Status`, `PROJECT-2.Status`). GitHub field types map to attribute values: single-select → enum, iteration → enum/string, number → int/float, date → date, text → string.

Because the option sets live on GitHub, these attributes are dynamic (carried as `AttrValue`, not declared `AttrDef`s). Validation runs against a cached schema snapshot (below), not config.

### GraphQL access

A new `GhGraphql` trait, implemented on `GhCli` by shelling to `gh api graphql -f query=...`, fakeable for tests at the same seam as `GhIssueReader`/`GhIssueWriter`. The CLI uses the `gh auth login` token automatically — no separate token handling. Projects v2, issue-types, sub-issues, and project fields are GraphQL-only; milestones and comments are REST (`gh issue`/`gh api`) and may stay on the existing reader/writer.

`gh api graphql` does not accept a JSON variables blob — variables are repeated `-f key=value` (string) / `-F key=value` (typed: ints, bools, file refs) flags. The impl flattens its variables argument into `-f`/`-F` flags accordingly; the trait signature should reflect this rather than implying a single JSON payload.

**Project field write is three steps, not two.** (1) Resolve the project node id (`organization|user { projectV2(number:N){id} }`); (2) resolve the target field id and, for single-select/iteration, the option/iteration id; (3) call `updateProjectV2ItemFieldValue` with a `value` object carrying **exactly one** key (`singleSelectOptionId` | `iterationId` | `text` | `number` | `date`) — extra null keys are rejected. Clearing a single-select cannot use an empty string; it requires `clearProjectV2ItemFieldValue`. The store handles set and clear as distinct mutations.

### Schema snapshot and validation

On store refresh, the github layer fetches the field/type schema (board field options, org issue-types) and persists it to `.lazyspec/cache/gh-schema.json`. `validate` reads the snapshot, so offline validation of native attribute values works. Drift is bounded by the cache TTL; a value valid at fetch can still be rejected by GitHub if an option was removed since.

### Comments

GitHub issue comments are fetched read-only and surfaced as a `comments` array (author, body, timestamp) in `show --json` / `status --json`. They are never merged into the authored `--body` and never round-tripped, keeping the body serialization clean.

### Write conflicts

Native mutations push unconditionally, then refresh the cache from GitHub. Concurrent edits are silently overwritten. This is a deliberate simplification; read-before-write conflict detection is deferred.

## Interfaces

- `trait GhGraphql { fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value>; }` @draft — impl on `GhCli` via `gh api graphql`, flattening vars to `-f` (string) / `-F` (typed) flags. (Not a JSON-blob argument; see Design.)
- `enum StoreBackend { Filesystem, GithubIssues, GitRef, GithubMilestones, GithubProjects }` @draft — two new variants.
- `[[relationships]]` gains `github_native: Option<String>` @draft (`"sub-issue"` | `"membership"`).
- Native mutation surface @draft: issue-type via `updateIssue { issueTypeId }`; sub-issues via `addSubIssue` / `removeSubIssue` / `reprioritizeSubIssue`; project membership via `addProjectV2ItemById`; project fields via `updateProjectV2ItemFieldValue` and `clearProjectV2ItemFieldValue`; milestones via REST.
- Attribute write path @draft — `lazyspec update <id> --attr <key>=<value>` (no such flag today); the github store must round-trip attributes through the issue-body HTML comment (currently dropped).
- `show --json` / `status --json` gain a read-only `comments` array @draft.
- `.lazyspec/cache/gh-schema.json` @draft — cached native-field schema snapshot; caches **ids** (issue-type ids, project field ids, single-select option ids, iteration ids), not just display names, since every native write keys off the id.
- Auth requirement @draft: Projects mutations need the `project` scope (reads `read:project`) on the `gh` token; document `gh auth refresh -s project` as the remedy for permission errors.

## Decisions (ADRs to emit)

- **Native-binding layer over the github store.** The github store moves from inert storage to a binding layer with three shapes (store backend, native-backed relation, native attribute). Records why GitHub-native structure enters the document model.
- **GraphQL via `gh api graphql`, not a GraphQL crate.** Keeps a single CLI auth/test seam; no new dependency.
- **Board Status is an attribute, not lifecycle (for now).** Defers status-authority-board selection; documents the open/closed lifecycle staying intact.
- **Last-write-wins for native mutations.** Records the accepted risk of silent overwrite versus the cost of per-field optimistic locking.

## Stories

Foundational first; 1 and 2 unblock the rest.

1. **GraphQL layer + schema snapshot** — `GhGraphql` trait + `gh-schema.json` cache. Unblocks 3, 4, 5, 6.
2. **Attribute write path + github attribute round-trip** — `--attr` write, HTML-comment round-trip in the github store. Unblocks 3, 6.
3. **Native issue-type as attribute** — `updateIssue { issueTypeId }`; snapshot caches type ids. Depends on 1, 2.
4. **Milestones** — `github-milestones` store + association relation (REST). Depends on 1.
5. **Sub-issues** — subdirectory materialization in github store + structural children → native sub-issues. Depends on 1.
6. **Comments read-thru** — read-only `comments` array in `--json`. Depends on 1 only; ships independently of projects.
7. **Project membership + board store** — `github-projects` store + `membership` relation (`addProjectV2ItemById`). Depends on 1, 2.
8. **Per-board field attributes** — namespaced `PROJECT-n.<field>` read/write (three-step id resolution, single-key value object, clear semantics). Depends on 7. Hardest; isolate it.

## Risks and tradeoffs

- **Cache snapshot staleness compounds with last-write-wins.** Offline validation can pass a value against a stale option that no longer exists, and the write can then clobber a concurrent edit. Accepted: the GraphQL mutation error is the real backstop — an invalid id fails at GitHub, and refresh-after-write re-syncs the clobbered field on next read. TTL bounds the window.
- **Last-write-wins data loss.** Concurrent edits to native fields are silently overwritten. Accepted for now; revisit if multi-actor editing becomes common.
- **GraphQL coupling to `gh` CLI.** Tying GraphQL to the `gh` binary inherits its auth and version constraints; mitigated by the trait seam, which keeps the engine testable and the binary swappable. Two operational gotchas to surface in docs: Projects mutations require the `project` token scope (a `gh auth login` without it yields permission errors, fixable via `gh auth refresh -s project`); and on macOS a keyring-lookup timeout can make `gh api` silently send unauthenticated requests (a surprise 403 rate-limit), worked around with `GH_TOKEN="$(gh auth token)" gh api …`.
- **Dynamic attribute schema bypasses config AttrDef.** Native attributes aren't declared in config, so they don't get the static guarantees hand-authored attributes do; the snapshot validation is the compensating control.
- **Scope of the github store grows.** Subdirectory materialization, GraphQL, two new backends, and attribute round-trip enlarge a previously thin store; the foundational stories (1, 2) exist to land the shared machinery before the per-feature stories build on it.
