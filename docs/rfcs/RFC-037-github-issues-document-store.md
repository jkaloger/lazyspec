---
title: GitHub Issues Document Store
type: rfc
status: draft
author: jkaloger
date: 2026-03-27
tags:
- github
- store
- issues
- sync
related:
- related-to: RFC-035
---



## Problem

RFC-035 introduced the `store` field on document types, with `filesystem` and `git-ref` as the initial backends. It explicitly anticipated future backends, naming `github-issues` as a candidate. The store abstraction exists, but there's no specification for how a GitHub Issues backend would actually work.

GitHub Issues is a natural fit for lazyspec documents. Issues have a body (markdown), labels, comments, and a built-in web UI. Teams already use them for tracking work. Backing lazyspec documents onto issues would make document state visible to anyone with repo access, without requiring the lazyspec CLI or TUI.

The gap: there's no defined mapping between lazyspec's document model (frontmatter, relationships, status, tags) and GitHub's issue model (body, labels, state). There's no fetching strategy, no conflict resolution, and no story for how the TUI and CLI interact with a remote API instead of the local filesystem.

## Intent

Add a `github-issues` storage backend that persists lazyspec documents as GitHub Issues. GitHub is the source of truth. The lazyspec CLI and TUI are clients that read from and write to the GitHub API, with a local cache for performance.

The backend:
- Maps lazyspec fields onto native GitHub primitives where possible: title to issue title, tags to labels, status to open/closed state
- Stores fields that have no GitHub equivalent (relationships, author, date) in an HTML comment block in the issue body
- Uses a single `lazyspec:{type}` label per issue for type filtering on API requests
- Derives document status from issue open/closed state for standard lifecycle statuses, reserving frontmatter status for non-lifecycle states (superseded, rejected, etc.)
- Uses a hybrid cache with configurable TTL for reads, with `lazyspec fetch` as a manual refresh
- Enforces optimistic locking via GitHub's `updated_at` timestamps to prevent silent overwrites
- Supports the same relationship model as other backends, with cross-backend links resolving transparently

Any document type can use `store = "github-issues"`. The typical configuration puts iterations there (agents create issues, work them, close them), but RFCs, stories, or ADRs could use it too.

## Design

### Configuration

The `store` field on a type definition selects the backend. GitHub-specific settings live in a `[github]` section:

```toml
[[types]]
name = "iteration"
prefix = "ITERATION"
dir = "docs/iterations"
store = "github-issues"

[[types]]
name = "rfc"
prefix = "RFC"
dir = "docs/rfcs"
store = "filesystem"

[github]
repo = "owner/repo"          # required
cache_ttl = "60s"             # default, per-document freshness
```

@ref src/engine/config.rs#TypeDef

`repo` is the only required field. It can be omitted if the repo is inferrable from `git remote get-url origin`. Auth is delegated to the `gh` CLI (see Authentication section).

### Field Mapping

Lazyspec documents have fields that map onto native GitHub Issue primitives where possible. Fields without a GitHub equivalent live in an HTML comment block in the issue body.

| Lazyspec field | GitHub primitive | Notes |
|---------------|-----------------|-------|
| `title` | Issue title | Direct mapping. Editable on either side. |
| `tags` | Issue labels | Direct mapping, no prefix. `auth` tag = `auth` label. |
| `type` | `lazyspec:{type}` label | e.g. `lazyspec:story`, `lazyspec:iteration`. One per issue. Used for API filtering. |
| `status` | Issue open/closed state | See status mapping below. |
| `body` | Issue body (below the HTML comment) | The visible markdown content. |
| `author` | HTML comment frontmatter | No native equivalent. |
| `date` | HTML comment frontmatter | Creation date (not GitHub's `created_at`, which is the issue creation time). |
| `related` | HTML comment frontmatter | Relationships have no GitHub equivalent. |

### Issue Body Format

The issue body contains an HTML comment block with frontmatter for fields that don't map to GitHub primitives, followed by the visible document markdown.

```html
<!-- lazyspec
---
author: agent-7
date: 2026-03-27
related:
- implements: STORY-075
---
-->

## Task

Refactor the auth middleware to use the new session token format.

### Acceptance criteria

- All endpoints use the new token validator
- Legacy token support removed
- Integration tests pass
```

The HTML comment only contains fields that have no GitHub-native home: `author`, `date`, `related`, and non-lifecycle `status` values (see below). Title, tags, type, and lifecycle status live in their native GitHub locations.

The comment is invisible when viewing the issue on GitHub. Lazyspec parses it on read and reconstructs the full frontmatter by combining the comment fields with the issue title, labels, and open/closed state.

> [!NOTE]
> Humans can edit the issue title, labels, body markdown, and open/closed state freely on GitHub. The HTML comment block should only be edited through lazyspec CLI commands. If someone does edit it, lazyspec parses whatever is there on next fetch.

### Status Mapping

Document status is derived from the issue's open/closed state for standard lifecycle statuses. Non-lifecycle statuses that have no open/closed equivalent are stored in the HTML comment frontmatter.

| Lazyspec status | GitHub state | Frontmatter `status` |
|----------------|-------------|---------------------|
| `draft` | open | _(not set)_ |
| `review` | open | `review` |
| `accepted` | open | `accepted` |
| `in-progress` | open | `in-progress` |
| `complete` | closed | _(not set)_ |
| `rejected` | closed | `rejected` |
| `superseded` | closed | `superseded` |

The reconstruction logic: if the issue is open and frontmatter has no `status`, the document is `draft`. If the issue is closed and frontmatter has no `status`, it's `complete`. If frontmatter has an explicit `status`, that takes precedence regardless of open/closed state.

This means the common operations (draft -> in-progress -> complete) work by simply opening/closing the issue on GitHub, which is the natural GitHub workflow. Non-lifecycle transitions (rejected, superseded) require a lazyspec command to set the frontmatter status before closing.

### Label Sync

Tags map directly to GitHub labels with no prefix. A document with `tags: [auth, refactor]` gets labels `auth` and `refactor` on its issue. This is bidirectional: labels added on GitHub are picked up as tags on the next fetch, and tags added via lazyspec are pushed as labels.

The document type gets a single `lazyspec:{type}` label (e.g. `lazyspec:iteration`, `lazyspec:story`). This is the only prefixed label. It serves two purposes: identifying lazyspec-managed issues in the GitHub UI, and filtering on API requests (`GET /repos/{owner}/{repo}/issues?labels=lazyspec:iteration`) to avoid fetching unrelated issues.

Labels are created automatically if they don't exist on the repo. The `lazyspec:{type}` labels use a deterministic color (hash of type name) for visual consistency.

### Issue Number Mapping

Each lazyspec document ID (e.g. `ITERATION-042`) maps to a GitHub issue number (e.g. `#87`). The mapping is stored in `.lazyspec/issue-map.json`:

```json
{
  "ITERATION-042": { "issue_number": 87, "updated_at": "2026-03-27T10:00:00Z" },
  "ITERATION-043": { "issue_number": 88, "updated_at": "2026-03-27T10:05:00Z" }
}
```

`updated_at` is the GitHub timestamp from the last known state. This is the optimistic lock token.

On `lazyspec create`, a new issue is created via the API and the mapping is recorded. On `lazyspec show ITERATION-042`, the mapping resolves the document ID to an issue number for the API call (or cache lookup).

The map file is gitignored. It's reconstructed from the cache or by scanning issues with the `lazyspec:{type}` label on `lazyspec setup`.

### Hybrid Cache

Reads go through `.lazyspec/cache/{type}/{id}.md`, the same cache directory RFC-035 uses for git-ref documents. The difference is the freshness model.

Each cached document has a timestamp in `cache.lock` recording when it was last fetched from the API. On read:

1. Check cache exists for the requested document
2. If `now - cached_at < cache_ttl`: return the cached file (fast path)
3. If stale: fetch from the GitHub API, update the cache file and `cache.lock`, return the fresh content
4. If the API is unreachable: return stale cache with a warning

`lazyspec fetch` forces a full refresh of all `github-issues` documents regardless of TTL. It paginates through the issues API, filtering by `lazyspec:{type}` labels for each configured type, and rebuilds the cache and issue map.

```
.lazyspec/
  cache/
    iteration/
      ITERATION-042.md
      ITERATION-043.md
  cache.lock
  issue-map.json
```

The TUI refreshes stale documents in the background on its poll cycle. A single batch request per cycle fetches any documents whose cache has expired, avoiding per-document API calls on each render.

#### Cache TTL and Rate Limits

The default TTL of 60 seconds balances freshness with API budget. At 5000 requests/hour (authenticated), a TUI polling every 2 seconds can refresh ~1 document per cycle without hitting limits. For larger projects, increase the TTL or rely on `lazyspec fetch`.

Conditional requests (`If-Modified-Since` / `If-None-Match` headers) reduce bandwidth and count toward GitHub's rate limit at a lower cost. The cache stores ETags alongside timestamps for this purpose.

### Optimistic Locking

GitHub is the source of truth. When lazyspec pushes a change, it must not silently overwrite edits made on GitHub since the last fetch.

The write path:

1. Read the current issue via API (or from fresh cache)
2. Compare the issue's `updated_at` with the value in `issue-map.json`
3. If they match: the issue hasn't changed since we last saw it. Proceed with the update.
4. If they differ: someone edited the issue on GitHub. Reject the push.

```
$ lazyspec update ITERATION-042 --set-status complete
Error: ITERATION-042 has been modified on GitHub since your last fetch.
  Local:  2026-03-27T10:00:00Z
  Remote: 2026-03-27T10:45:00Z
Run `lazyspec fetch` to pull the latest version, then retry.
```

After fetching, the user sees the remote changes in their local cache and can decide whether to proceed. There's no automatic merge. The assumption is that most conflicts are metadata-only (status changes, tag edits) and the right resolution is usually "accept remote, then apply your change on top."

For body content conflicts (two people editing the markdown), the user fetches, reviews the diff, and re-applies their edit. This is the same workflow as git conflicts, just mediated by the API rather than merge commits.

### Write Path

All mutations go through the GitHub API. The CLI never edits cached files directly.

| Operation | API call | Side effects |
|-----------|----------|--------------|
| `lazyspec create` (github-issues type) | `POST /repos/{owner}/{repo}/issues` | Create issue, add to issue map, add labels, write cache |
| `lazyspec update` | `PATCH /repos/{owner}/{repo}/issues/{number}` | Update body/labels, update issue map timestamp, refresh cache |
| `lazyspec delete` | `PATCH /repos/{owner}/{repo}/issues/{number}` (close + remove labels) | Close issue, remove from issue map, delete cache file |

GitHub Issues cannot be truly deleted via the API (only closed), so `lazyspec delete` closes the issue, removes the `lazyspec:{type}` label, and prepends `[DELETED]` to the title. Removing the type label ensures the issue is excluded from future fetches. Tag labels are left in place (they're harmless without the type label).

### TUI Integration

GitHub Issues documents appear in the TUI alongside filesystem and git-ref documents. The TUI doesn't know or care which backend a document uses; it reads from the unified engine.

Keybindings for github-issues documents:

| Key | Action |
|-----|--------|
| `e` | Open document in `$EDITOR`. On editor close, parse the edited content and push to GitHub via API. Optimistic lock check before push. |
| `s` | Cycle status. For lifecycle statuses (draft, in-progress, complete), updates the issue's open/closed state. For non-lifecycle statuses (rejected, superseded), sets the frontmatter status and closes the issue. |
| `Enter` | View document (read from cache, no API call unless stale) |

The `e` flow in detail:

1. Fetch fresh content from API (or use cache if within TTL)
2. Write to a temp file with the full document (HTML comment frontmatter + body)
3. Open in `$EDITOR`
4. On close, parse the temp file. Extract frontmatter from the HTML comment, body from the rest.
5. Optimistic lock check against `updated_at`
6. If clean: push the update to GitHub, refresh cache
7. If conflict: warn the user, offer to fetch and re-edit

Status bar shows a sync indicator for github-issues documents: a timestamp of the last successful fetch, and a warning icon if any cached documents are beyond 2x TTL (suggesting a `lazyspec fetch`).

### CLI Commands

Existing commands gain github-issues awareness transparently:

- `lazyspec list iteration` reads from cache (refreshing stale entries), same output format
- `lazyspec show ITERATION-042` reads from cache or API
- `lazyspec search "auth"` searches cached documents (body + frontmatter)
- `lazyspec context ITERATION-042` follows relationship chains across backends
- `lazyspec validate` checks frontmatter schema, relationship integrity, etc.
- `lazyspec status` includes github-issues documents in the full project view

New commands:

- `lazyspec fetch` manually refreshes all github-issues documents (and git-ref documents per RFC-035)
- `lazyspec push <id>` explicitly pushes local edits to GitHub (for workflows where you want to batch changes before syncing)

### Relationships Across Backends

Same model as RFC-035. A github-issues document can `implement` a filesystem story, or a filesystem RFC can be `related-to` a github-issues iteration. The relationship target is a document ID (e.g. `STORY-075`), and the engine resolves it to the correct backend.

```yaml
related:
- implements: STORY-075
```

The engine looks up `STORY-075` in the document index. If it's a filesystem type, read from the configured directory. If it's a git-ref type, read from the ref cache. If it's a github-issues type, read from the issue cache. The relationship itself is backend-agnostic.

`lazyspec context` renders the full chain:

```
RFC-030 (Git-Based Document Number Reservation)  ← filesystem
  └── STORY-075 (Auth refactor)                   ← filesystem
        └── ITERATION-042 (Implementation)         ← github-issues, #87, in-progress
```

### Authentication

Three modes, selected by `[github].auth`:

The initial implementation delegates all authentication to the `gh` CLI. `gh` handles token storage, refresh, and scope validation. Lazyspec shells out to `gh` for every API operation, so auth "just works" if `gh auth login` has been run.

`lazyspec validate` checks that `gh` is installed and authenticated (`gh auth status`).

Future auth modes (`$GITHUB_TOKEN` for CI, GitHub App for daemon) are deferred until the native HTTP client replaces `gh` CLI.

### Init and Setup

`lazyspec init` with a `github-issues` type:

1. Prompts for `repo` (or reads from `git remote get-url origin`)
2. Validates API access with the configured auth method
3. Creates `lazyspec:{type}` labels for each configured github-issues type if they don't exist
4. Adds `.lazyspec/cache/` and `.lazyspec/issue-map.json` to `.gitignore`

`lazyspec setup` for new clones:

1. Reads `[github]` config from `.lazyspec.toml`
2. Validates auth
3. Runs initial fetch to populate cache and issue map

### Graceful Degradation

| Scenario | Behaviour |
|----------|-----------|
| API unreachable | Reads return stale cache with warning. Writes fail. |
| Auth expired | Clear error message with instructions per auth mode. |
| Issue deleted on GitHub | Detected on fetch. Removed from issue map and cache. Warning logged. |
| Tag label added/removed on GitHub | Synced bidirectionally on next fetch. |
| `lazyspec:{type}` label removed on GitHub | Issue excluded from next fetch. Detected and warned. |
| HTML comment edited on GitHub | Parsed as-is on next fetch. If malformed, validation error. |
| Rate limit exceeded | `Retry-After` header respected. Warn user, suggest increasing TTL. |

### Interaction with RFC-035 Coordination

If coordination is enabled (RFC-035), github-issues documents participate in the same lock system. Locks still live in git refs (`refs/lazyspec/locks/{type}/{id}`), not on GitHub. The lock protects against concurrent lazyspec writes; GitHub's own concurrency is handled by optimistic locking.

An agent claiming `ITERATION-042` (stored as GitHub Issue #87):
1. Acquires the lazyspec lock on `ITERATION-042` (git ref)
2. Fetches the issue from GitHub (cache refresh)
3. Does its work
4. Pushes updates to the issue via API (optimistic lock check)
5. Releases the lazyspec lock

The two locking layers serve different purposes: the lazyspec lock coordinates between lazyspec clients, the optimistic lock coordinates between lazyspec and direct GitHub edits.

## Open Questions

These are out of scope for this RFC but should be addressed in follow-up RFCs.

### Issue import

This RFC covers documents created through lazyspec that are stored as GitHub Issues. The reverse case, issues created directly on GitHub that should become lazyspec documents, is not addressed. A future RFC should explore an import command (`lazyspec import #87`) or an auto-import mode that adopts issues matching certain label conventions. This includes questions around ID assignment (does an imported issue get the next available `ITERATION-*` number?), frontmatter injection into existing issue bodies, and whether import should be one-shot or continuous.

### Metadata durability and GitHub Projects migration

The HTML comment block in the issue body is fragile. Anyone with write access can edit or delete it, intentionally or by accident, and there's no way to protect it. The current design treats this as acceptable because the comment only holds fields without a GitHub-native home (author, date, relationships), and lazyspec validates on fetch.

Long-term, GitHub Projects custom fields are a better home for structured metadata. Fields are typed, visible in the project UI, and not exposed in the issue body for accidental editing. The blockers today are that project fields are scoped to project items (not issues directly), items can be silently removed from projects, and the API is GraphQL-only. A future RFC should revisit this once GitHub's project model matures or if the fragility of the HTML comment proves to be a real problem in practice.

## Stories

Stories are ordered by dependency. The `gh` CLI is the integration layer for all GitHub API operations in the initial implementation. A native HTTP client (reqwest) is a future optimization, not a priority.

> [!NOTE]
> Stories for this RFC will be migrated to GitHub Issues as a test candidate for the github-issues store backend.

1. Issue body format and parsing -- HTML comment frontmatter serialization and deserialization. Round-trip fidelity (parse then serialize produces identical output). Integration with the existing frontmatter parser. Status reconstruction from open/closed state + frontmatter status field.

2. `gh` CLI integration layer -- shell out to `gh` for all GitHub API operations (issue create, edit, list, view, close). JSON output parsing. Auth delegation to `gh auth`. Label management (tag sync, `lazyspec:{type}` label). Error handling for missing `gh`, auth failures, rate limits.

3. Issue CRUD and store dispatch -- `store = "github-issues"` on TypeDef. Route `lazyspec create/update/delete` through `gh` for github-issues types. Issue number mapping in `.lazyspec/issue-map.json`. Optimistic locking via `updated_at` comparison. Status mapping to open/closed state on writes.

4. Hybrid cache and fetch -- `.lazyspec/cache/` for github-issues documents. TTL-based freshness checks on reads. `lazyspec fetch` for manual full refresh via `gh issue list --json`. Cache lock file with timestamps. Conditional refresh (skip unchanged issues).

5. Config, init, and setup -- `[github]` config section (repo, cache_ttl). `lazyspec init` creates `lazyspec:{type}` labels via `gh label create`. `lazyspec setup` runs initial fetch for new clones. `.gitignore` entries for cache and issue map.

6. TUI integration -- github-issues documents in the document list via the unified engine. `e` to edit and push (optimistic lock check). `s` to cycle status (open/close issue). Sync indicator in status bar. Background cache refresh on poll cycle.

7. Cross-backend relationship resolution -- unified document index spanning filesystem, git-ref, and github-issues backends. `lazyspec context` chain rendering across backends. `lazyspec validate` cross-backend relationship checks.

### Deferred

These are not blocked by the above but are lower priority:

- Native HTTP client (`reqwest`) replacing `gh` CLI for performance and tighter error handling
- Background TUI refresh with rate limit awareness
- `lazyspec push <id>` for batched local-to-remote sync
