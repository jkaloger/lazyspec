---
title: 'GitHub deep-links: repo coordinates + engine::github_url per backend'
type: story
status: accepted
author: jkaloger
date: 2026-06-30
tags: []
related:
- implements: RFC-052
---## Context

RFC-052 gives every document an outbound link to its GitHub representation so editing happens on github.com. This story adds `engine::github_url(doc, repo_coords) -> Option<Url>`, deriving the deep-link from the document's store backend and the repo coordinates. Coordinates resolve in order: `.lazyspec.toml` `[web]` override, then the `origin` remote. When neither yields owner/repo/branch, deep-links are omitted (the function returns `None`, rendering no link rather than a broken one) and `serve` logs a one-line startup warning. Depends on STORY-176; the page rendering from STORY-177 surfaces the link.

## Acceptance Criteria

- **Given** a `filesystem`-backed document and resolvable repo coordinates
  **When** `engine::github_url` is called
  **Then** it returns a blob URL of the form `/blob/{branch}/{path}`.

- **Given** a `github-issues`-backed document
  **When** `engine::github_url` is called
  **Then** it returns the issue URL.

- **Given** a `github-milestones`-backed document
  **When** `engine::github_url` is called
  **Then** it returns the milestone URL.

- **Given** a `.lazyspec.toml` with a `[web]` owner/repo/branch override
  **When** coordinates are resolved
  **Then** the override takes precedence over the `origin` remote.

- **Given** no `.lazyspec.toml` `[web]` table
  **When** coordinates are resolved
  **Then** owner/repo/branch are derived from the `origin` remote.

- **Given** no remote, a detached HEAD with no branch, or a backend whose `github_native` mapping yields no stable URL
  **When** `engine::github_url` is called
  **Then** it returns `None` (engine unit: no link data produced).

- **Given** repo coordinates that cannot be resolved at startup (no remote / detached HEAD)
  **When** `serve` starts
  **Then** it logs a one-line warning that deep-links are disabled, and continues serving. (Integration with `serve`; depends on STORY-176.)

- **Given** a document page (covered-by STORY-177)
  **When** `github_url` returns `Some`
  **Then** the page shows an outbound "edit on GitHub" link to that URL. (Seam contract verified in STORY-177's page render, not a unit AC of this story.)

## Scope

### In Scope

- `engine::github_url(doc, repo_coords) -> Option<Url>` per backend (blob / issue / milestone).
- Repo-coordinate resolution: `.lazyspec.toml` `[web]` override, then `origin` remote.
- `.lazyspec.toml` `[web]` optional table for owner/repo/branch.
- Graceful `None` + startup warning when coordinates can't resolve.
- Surfacing the link on the document page.

### Out of Scope

- Editing or write-back -- the link hands off to GitHub's own editor (RFC non-goal).
- Multi-remote selection beyond `origin`.
- OAuth (STORY-181); deep-links work on the unauthenticated local view.
